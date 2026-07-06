use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use memlink_evaluator::{EvaluatorSink, Event, EventKind};
use memlink_memory::{MemoryQuery, MemoryReuseEvent, MemoryStore, MemoryUnit};
use memlink_protocol::*;
use memlink_sandbox::{RestrictedProcessSandbox, Sandbox, SandboxLanguage, SandboxRequest};
use memlink_state::{StateMeta, StateStore, deterministic_embedding, embedding_to_bytes};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: Option<String>,
    pub group: String,
    pub topic: String,
    pub prompt: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSuite {
    pub tasks: Vec<TaskSpec>,
}

#[derive(Clone)]
pub struct AgentContext {
    pub trace: TraceContext,
    pub mode: RunMode,
    pub state: Arc<dyn StateStore>,
    pub memory: Arc<dyn MemoryStore>,
    pub evaluator: Arc<dyn EvaluatorSink>,
    pub sandbox: Option<Arc<dyn Sandbox>>,
}

#[async_trait]
pub trait Agent: Send + Sync {
    fn id(&self) -> AgentId;
    fn role(&self) -> AgentRole;
    fn capabilities(&self) -> Vec<Capability>;
    async fn handle(&self, ctx: AgentContext, msg: Message) -> Result<Message>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutcome {
    pub task_id: TaskId,
    pub mode: RunMode,
    pub answer: String,
    pub memory_hits: usize,
    pub state_transfers: usize,
    pub duration_ms: u64,
}

pub struct Runtime {
    agents: Vec<Arc<dyn Agent>>,
    state: Arc<dyn StateStore>,
    memory: Arc<dyn MemoryStore>,
    evaluator: Arc<dyn EvaluatorSink>,
    sandbox: Option<Arc<dyn Sandbox>>,
}

impl Runtime {
    pub fn new(
        state: Arc<dyn StateStore>,
        memory: Arc<dyn MemoryStore>,
        evaluator: Arc<dyn EvaluatorSink>,
    ) -> Self {
        Self {
            agents: default_agents(),
            state,
            memory,
            evaluator,
            sandbox: Some(Arc::new(RestrictedProcessSandbox::default())),
        }
    }

    pub fn without_sandbox(mut self) -> Self {
        self.sandbox = None;
        self
    }

    pub async fn run_task(
        &self,
        experiment_id: ExperimentId,
        mode: RunMode,
        task: TaskSpec,
    ) -> Result<TaskOutcome> {
        let task_id = Uuid::new_v4();
        let trace = TraceContext::new(experiment_id, task_id);
        let start = Instant::now();
        self.handshake(&trace, mode).await?;

        let planner = self.agent(AgentRole::Planner)?;
        let retriever = self.agent(AgentRole::Retriever)?;
        let executor = self.agent(AgentRole::Executor)?;
        let summarizer = self.agent(AgentRole::Summarizer)?;

        let plan_request = self.message_from_task(mode, &task, &planner.id())?;
        self.record_message(&trace, &plan_request).await?;
        let plan = planner
            .handle(self.context(trace.clone(), mode), plan_request)
            .await?;
        self.record_message(&trace, &plan).await?;

        let retrieval_request = self.next_request(
            &plan,
            planner.id(),
            retriever.id(),
            ActionType::SearchMemory,
            &task,
            mode,
            vec![],
        )?;
        self.record_message(&trace, &retrieval_request).await?;
        let retrieval = retriever
            .handle(self.context(trace.clone(), mode), retrieval_request)
            .await?;
        self.record_message(&trace, &retrieval).await?;
        let memory_hits = match &retrieval.payload {
            Payload::MemoryHit(payload) => payload.hits.len(),
            _ => 0,
        };

        let mut executor_result = Message::new(
            executor.id(),
            Target::Agent(summarizer.id()),
            MessageKind::ActionResult,
            Payload::ActionResult(ActionResult {
                action: ActionType::ExecuteTool,
                success: true,
                result: BTreeMap::from([(
                    "result".to_owned(),
                    "no tool execution requested".to_owned(),
                )]),
                memory_candidates: vec![],
            }),
            vec![],
        );
        if task.code.is_some() {
            let exec_request = self.next_request(
                &retrieval,
                retriever.id(),
                executor.id(),
                ActionType::ExecuteTool,
                &task,
                mode,
                retrieval.state_refs.clone(),
            )?;
            self.record_message(&trace, &exec_request).await?;
            executor_result = executor
                .handle(self.context(trace.clone(), mode), exec_request)
                .await?;
            self.record_message(&trace, &executor_result).await?;
        }

        let mut state_refs = retrieval.state_refs.clone();
        state_refs.extend(executor_result.state_refs.clone());
        let summary_request = self.summary_request(
            &task,
            mode,
            &summarizer.id(),
            vec![plan, retrieval, executor_result],
            state_refs,
        )?;
        self.record_message(&trace, &summary_request).await?;
        let summary = summarizer
            .handle(self.context(trace.clone(), mode), summary_request)
            .await?;
        self.record_message(&trace, &summary).await?;

        for state_ref in &summary.state_refs {
            self.evaluator
                .record(Event::new(
                    experiment_id,
                    task_id,
                    EventKind::StateTransfer {
                        state_id: state_ref.state_id,
                        format: format!("{:?}", state_ref.format),
                        byte_len: state_ref.byte_len,
                        producer: state_ref.producer.to_string(),
                        consumer: "summarizer".to_owned(),
                    },
                ))
                .await?;
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        self.evaluator
            .record(Event::new(
                experiment_id,
                task_id,
                EventKind::TaskFinished {
                    mode,
                    success: true,
                    duration_ms,
                },
            ))
            .await?;
        Ok(TaskOutcome {
            task_id,
            mode,
            answer: extract_result(&summary),
            memory_hits,
            state_transfers: summary.state_refs.len(),
            duration_ms,
        })
    }

    fn context(&self, trace: TraceContext, mode: RunMode) -> AgentContext {
        AgentContext {
            trace,
            mode,
            state: self.state.clone(),
            memory: self.memory.clone(),
            evaluator: self.evaluator.clone(),
            sandbox: self.sandbox.clone(),
        }
    }

    fn agent(&self, role: AgentRole) -> Result<Arc<dyn Agent>> {
        self.agents
            .iter()
            .find(|agent| agent.role() == role)
            .cloned()
            .with_context(|| format!("agent not found: {role:?}"))
    }

    async fn handshake(&self, trace: &TraceContext, mode: RunMode) -> Result<()> {
        for agent in &self.agents {
            let capabilities = agent.capabilities();
            self.evaluator
                .record(Event::new(
                    trace.experiment_id,
                    trace.task_id,
                    EventKind::AgentRegistered {
                        agent_id: agent.id().to_string(),
                        role: format!("{:?}", agent.role()),
                        capability_count: capabilities.len(),
                    },
                ))
                .await?;
            let message = Message::new(
                AgentId::new("runtime"),
                Target::Agent(agent.id()),
                MessageKind::CapabilityAdvertise,
                Payload::Capabilities(capabilities),
                vec![],
            );
            if mode == RunMode::Structured {
                self.record_message(trace, &message).await?;
            }
        }
        Ok(())
    }

    fn message_from_task(
        &self,
        mode: RunMode,
        task: &TaskSpec,
        planner_id: &AgentId,
    ) -> Result<Message> {
        let payload = match mode {
            RunMode::Text => Payload::Text(format!(
                "Please plan this task. group={} topic={} tags={} prompt={} code={}",
                task.group,
                task.topic,
                task.tags.join(","),
                task.prompt,
                task.code.clone().unwrap_or_default()
            )),
            RunMode::Structured => Payload::ActionRequest(ActionRequest {
                action: ActionType::PlanTask,
                params: task_params(task),
                required_capability: Some(ActionType::PlanTask),
            }),
        };
        Ok(Message::new(
            AgentId::new("runtime"),
            Target::Agent(planner_id.clone()),
            MessageKind::ActionRequest,
            payload,
            vec![],
        ))
    }

    fn next_request(
        &self,
        _previous: &Message,
        from: AgentId,
        to: AgentId,
        action: ActionType,
        task: &TaskSpec,
        mode: RunMode,
        state_refs: Vec<StateRef>,
    ) -> Result<Message> {
        let payload = match mode {
            RunMode::Text => Payload::Text(format!(
                "Execute {:?} for topic '{}'. Full context: {}. Tags: {}. Previous state is inlined in text mode.",
                action,
                task.topic,
                task.prompt,
                task.tags.join(", ")
            )),
            RunMode::Structured => Payload::ActionRequest(ActionRequest {
                action,
                params: task_params(task),
                required_capability: Some(action),
            }),
        };
        Ok(Message::new(
            from,
            Target::Agent(to),
            MessageKind::ActionRequest,
            payload,
            state_refs,
        ))
    }

    fn summary_request(
        &self,
        task: &TaskSpec,
        mode: RunMode,
        summarizer_id: &AgentId,
        messages: Vec<Message>,
        state_refs: Vec<StateRef>,
    ) -> Result<Message> {
        let payload = match mode {
            RunMode::Text => Payload::Text(format!(
                "Summarize task '{}' using {} prior messages. Text mode keeps evidence inline but avoids StateRef handles. Prior summaries: {}",
                task.topic,
                messages.len(),
                compact_message_summaries(&messages)
            )),
            RunMode::Structured => Payload::ActionRequest(ActionRequest {
                action: ActionType::Summarize,
                params: task_params(task),
                required_capability: Some(ActionType::Summarize),
            }),
        };
        Ok(Message::new(
            AgentId::new("runtime"),
            Target::Agent(summarizer_id.clone()),
            MessageKind::ActionRequest,
            payload,
            state_refs,
        ))
    }

    async fn record_message(&self, trace: &TraceContext, message: &Message) -> Result<()> {
        self.evaluator
            .record(Event::new(
                trace.experiment_id,
                trace.task_id,
                EventKind::MessageMetric {
                    from: message.from.to_string(),
                    to: format!("{:?}", message.to),
                    kind: message.kind,
                    text_chars: message.metrics.text_chars,
                    encoded_bytes: message.metrics.encoded_bytes,
                    state_bytes: message.metrics.state_bytes,
                },
            ))
            .await?;
        for state_ref in &message.state_refs {
            self.evaluator
                .record(Event::new(
                    trace.experiment_id,
                    trace.task_id,
                    EventKind::StateTransfer {
                        state_id: state_ref.state_id,
                        format: format!("{:?}", state_ref.format),
                        byte_len: state_ref.byte_len,
                        producer: state_ref.producer.to_string(),
                        consumer: format!("{:?}", message.to),
                    },
                ))
                .await?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct PlannerAgent;

#[async_trait]
impl Agent for PlannerAgent {
    fn id(&self) -> AgentId {
        AgentId::new("planner")
    }
    fn role(&self) -> AgentRole {
        AgentRole::Planner
    }
    fn capabilities(&self) -> Vec<Capability> {
        vec![capability(self.id(), self.role(), ActionType::PlanTask)]
    }

    async fn handle(&self, _ctx: AgentContext, msg: Message) -> Result<Message> {
        let params = params_from_message(&msg);
        let topic = params
            .get("topic")
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned());
        let mut result = BTreeMap::new();
        result.insert("plan".to_owned(), format!("1) search reusable memory for {topic}; 2) collect evidence; 3) summarize and persist memory"));
        result.insert("next".to_owned(), "retriever".to_owned());
        Ok(Message::new(
            self.id(),
            Target::Role(AgentRole::Retriever),
            MessageKind::ActionResult,
            Payload::ActionResult(ActionResult {
                action: ActionType::PlanTask,
                success: true,
                result,
                memory_candidates: vec![],
            }),
            vec![],
        ))
    }
}

#[derive(Debug, Default)]
struct RetrieverAgent;

#[async_trait]
impl Agent for RetrieverAgent {
    fn id(&self) -> AgentId {
        AgentId::new("retriever")
    }
    fn role(&self) -> AgentRole {
        AgentRole::Retriever
    }
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            capability(self.id(), self.role(), ActionType::SearchMemory),
            capability(self.id(), self.role(), ActionType::ExtractEvidence),
        ]
    }

    async fn handle(&self, ctx: AgentContext, msg: Message) -> Result<Message> {
        let params = params_from_message(&msg);
        let prompt = params.get("prompt").cloned().unwrap_or_default();
        let topic = params
            .get("topic")
            .cloned()
            .unwrap_or_else(|| prompt.clone());
        let tags = split_csv(params.get("tags").map(String::as_str).unwrap_or_default());
        let embedding = deterministic_embedding(&format!("{topic} {prompt}"), 32);
        let hits = ctx
            .memory
            .search(MemoryQuery {
                query: format!("{topic} {prompt}"),
                tags: tags.clone(),
                embedding: embedding.clone(),
                limit: 5,
            })
            .await?;
        let adopted = hits.iter().filter(|hit| hit.score >= 0.20).count();
        ctx.evaluator
            .record(Event::new(
                ctx.trace.experiment_id,
                ctx.trace.task_id,
                EventKind::MemoryQuery {
                    query: topic.clone(),
                    hit_count: hits.len(),
                    adopted_count: adopted,
                },
            ))
            .await?;
        for hit in hits.iter().filter(|hit| hit.score >= 0.20) {
            ctx.memory
                .record_reuse(MemoryReuseEvent {
                    memory_id: hit.memory_id,
                    task_id: ctx.trace.task_id,
                    adopted: true,
                    reason: hit.reason.clone(),
                    created_at: Utc::now(),
                })
                .await?;
        }

        let evidence = serde_json::json!({
            "topic": topic,
            "prompt": prompt,
            "memory_hits": hits,
            "fresh_evidence": synthetic_evidence(&params),
        });
        let mut state_refs = Vec::new();
        let payload = if ctx.mode == RunMode::Structured {
            let evidence_ref = ctx
                .state
                .put(
                    Bytes::from(serde_json::to_vec(&evidence)?),
                    StateMeta {
                        producer: self.id(),
                        format: StateFormat::EvidencePackJson,
                        shape: None,
                        ttl: Some(Duration::from_secs(3600)),
                    },
                )
                .await?;
            let embedding_ref = ctx
                .state
                .put(
                    embedding_to_bytes(&embedding),
                    StateMeta {
                        producer: self.id(),
                        format: StateFormat::EmbeddingF32,
                        shape: Some(vec![embedding.len()]),
                        ttl: Some(Duration::from_secs(3600)),
                    },
                )
                .await?;
            state_refs.push(evidence_ref);
            state_refs.push(embedding_ref);
            Payload::MemoryHit(MemoryHitPayload {
                hits: compact_memory_hits(&hits),
            })
        } else {
            Payload::Text(format!(
                "Retrieved evidence and memories inline: {}",
                serde_json::to_string(&evidence)?
            ))
        };
        Ok(Message::new(
            self.id(),
            Target::Role(AgentRole::Summarizer),
            MessageKind::MemoryHit,
            payload,
            state_refs,
        ))
    }
}

#[derive(Debug, Default)]
struct ExecutorAgent;

#[async_trait]
impl Agent for ExecutorAgent {
    fn id(&self) -> AgentId {
        AgentId::new("executor")
    }
    fn role(&self) -> AgentRole {
        AgentRole::Executor
    }
    fn capabilities(&self) -> Vec<Capability> {
        vec![capability(self.id(), self.role(), ActionType::ExecuteTool)]
    }

    async fn handle(&self, ctx: AgentContext, msg: Message) -> Result<Message> {
        let params = params_from_message(&msg);
        let code = params.get("code").cloned().unwrap_or_default();
        let line_count = code.lines().count();
        let todo_count = code.matches("TODO").count();
        let sandbox_result = if let Some(sandbox) = &ctx.sandbox {
            let escaped = serde_json::to_string(&code)?;
            let analysis_code = format!(
                "code = {escaped}\nprint({{\"line_count\": len(code.splitlines()), \"todo_count\": code.count(\"TODO\")}})"
            );
            let request = SandboxRequest {
                code: analysis_code,
                language: SandboxLanguage::Python,
                input_refs: msg.state_refs.clone(),
                timeout_ms: 2_000,
                max_output_bytes: 8 * 1024,
            };
            match sandbox.execute(request).await {
                Ok(result) => serde_json::json!({
                    "available": true,
                    "success": result.success,
                    "summary": result.summary,
                    "stdout": result.stdout,
                    "duration_ms": result.duration_ms,
                }),
                Err(error) => serde_json::json!({ "available": false, "error": error.to_string() }),
            }
        } else {
            serde_json::json!({ "available": false, "error": "sandbox disabled" })
        };
        let result = serde_json::json!({ "line_count": line_count, "todo_count": todo_count, "sandbox": sandbox_result, "strategy": "count structural indicators in restricted CodeAct sandbox and suggest focused fixes" });
        let mut state_refs = Vec::new();
        let payload = if ctx.mode == RunMode::Structured {
            let state_ref = ctx
                .state
                .put(
                    Bytes::from(serde_json::to_vec(&result)?),
                    StateMeta {
                        producer: self.id(),
                        format: StateFormat::ToolOutputJson,
                        shape: None,
                        ttl: Some(Duration::from_secs(3600)),
                    },
                )
                .await?;
            state_refs.push(state_ref);
            Payload::ActionResult(ActionResult {
                action: ActionType::ExecuteTool,
                success: true,
                result: BTreeMap::from([("tool_output_ref".to_owned(), "attached".to_owned())]),
                memory_candidates: vec![],
            })
        } else {
            Payload::Text(format!("Tool output inline: {result}"))
        };
        Ok(Message::new(
            self.id(),
            Target::Role(AgentRole::Summarizer),
            MessageKind::ActionResult,
            payload,
            state_refs,
        ))
    }
}

#[derive(Debug, Default)]
struct SummarizerAgent;

#[async_trait]
impl Agent for SummarizerAgent {
    fn id(&self) -> AgentId {
        AgentId::new("summarizer")
    }
    fn role(&self) -> AgentRole {
        AgentRole::Summarizer
    }
    fn capabilities(&self) -> Vec<Capability> {
        vec![capability(self.id(), self.role(), ActionType::Summarize)]
    }

    async fn handle(&self, ctx: AgentContext, msg: Message) -> Result<Message> {
        let params = params_from_message(&msg);
        let topic = params
            .get("topic")
            .cloned()
            .unwrap_or_else(|| "general".to_owned());
        let prompt = params.get("prompt").cloned().unwrap_or_default();
        let tags = split_csv(params.get("tags").map(String::as_str).unwrap_or_default());
        let mut evidence_fragments = Vec::new();
        for state_ref in &msg.state_refs {
            if matches!(
                state_ref.format,
                StateFormat::EvidencePackJson | StateFormat::ToolOutputJson
            ) {
                if let Ok(bytes) = ctx.state.get(state_ref).await {
                    evidence_fragments.push(String::from_utf8_lossy(&bytes).to_string());
                }
            }
        }
        let summary = if ctx.mode == RunMode::Structured {
            format!(
                "Structured answer for '{topic}': reused state_refs={}, evidence_items={}, prompt='{}'",
                msg.state_refs.len(),
                evidence_fragments.len(),
                prompt
            )
        } else {
            let text = match &msg.payload {
                Payload::Text(value) => value.as_str(),
                _ => "",
            };
            format!(
                "Text answer for '{topic}': summarized {} inline characters for prompt='{}'",
                text.chars().count(),
                prompt
            )
        };
        let embedding =
            deterministic_embedding(&format!("{topic} {summary} {}", tags.join(" ")), 32);
        let memory = MemoryUnit {
            memory_id: Uuid::new_v4(),
            source_agent: self.id(),
            created_at: Utc::now(),
            task_topic: topic.clone(),
            summary: summary.clone(),
            tags: tags.clone(),
            keywords: keywords(&format!("{topic} {prompt} {summary}")),
            embedding,
            evidence_refs: msg.state_refs.clone(),
        };
        let memory_id = ctx.memory.put(memory).await?;
        ctx.evaluator
            .record(Event::new(
                ctx.trace.experiment_id,
                ctx.trace.task_id,
                EventKind::MemoryWritten {
                    memory_id,
                    topic: topic.clone(),
                    source_agent: self.id().to_string(),
                },
            ))
            .await?;
        let result = BTreeMap::from([
            ("answer".to_owned(), summary),
            ("memory_id".to_owned(), memory_id.to_string()),
        ]);
        Ok(Message::new(
            self.id(),
            Target::Runtime,
            MessageKind::ActionResult,
            Payload::ActionResult(ActionResult {
                action: ActionType::Summarize,
                success: true,
                result,
                memory_candidates: vec![],
            }),
            msg.state_refs,
        ))
    }
}

pub fn default_agents() -> Vec<Arc<dyn Agent>> {
    vec![
        Arc::new(PlannerAgent),
        Arc::new(RetrieverAgent),
        Arc::new(ExecutorAgent),
        Arc::new(SummarizerAgent),
    ]
}

fn capability(agent_id: AgentId, role: AgentRole, action: ActionType) -> Capability {
    Capability {
        agent_id,
        role,
        action,
        input_schema: "json-object".to_owned(),
        output_schema: "json-object-or-state-ref".to_owned(),
        accepted_state_formats: vec![
            StateFormat::EmbeddingF32,
            StateFormat::EvidencePackJson,
            StateFormat::ToolOutputJson,
        ],
        cost_hint: CostHint {
            expected_ms: 10,
            text_chars: 256,
        },
    }
}

fn task_params(task: &TaskSpec) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    params.insert("group".to_owned(), task.group.clone());
    params.insert("topic".to_owned(), task.topic.clone());
    params.insert("prompt".to_owned(), task.prompt.clone());
    params.insert("tags".to_owned(), task.tags.join(","));
    if let Some(code) = &task.code {
        params.insert("code".to_owned(), code.clone());
    }
    params
}

fn params_from_message(msg: &Message) -> BTreeMap<String, String> {
    match &msg.payload {
        Payload::ActionRequest(request) => request.params.clone(),
        Payload::Text(text) => BTreeMap::from([
            ("prompt".to_owned(), text.clone()),
            ("topic".to_owned(), infer_topic(text)),
        ]),
        _ => BTreeMap::new(),
    }
}

fn infer_topic(text: &str) -> String {
    text.split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn keywords(text: &str) -> Vec<String> {
    let mut words = BTreeSet::new();
    for word in text
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
    {
        if word.len() > 2 {
            words.insert(word.to_owned());
        }
        if words.len() >= 12 {
            break;
        }
    }
    words.into_iter().collect()
}

fn synthetic_evidence(params: &BTreeMap<String, String>) -> Vec<String> {
    let topic = params.get("topic").map(String::as_str).unwrap_or("task");
    vec![
        format!("{topic}: structured protocol keeps action and parameters machine-readable"),
        format!("{topic}: StateRef avoids re-sending large evidence payloads"),
        format!("{topic}: shared memory enables linked follow-up reuse"),
    ]
}

fn compact_message_summaries(messages: &[Message]) -> String {
    messages
        .iter()
        .map(|message| match &message.payload {
            Payload::Text(text) => text.chars().take(240).collect::<String>(),
            Payload::ActionResult(result) => format!("{:?}:{}", result.action, result.success),
            Payload::MemoryHit(hit) => format!("memory_hits={}", hit.hits.len()),
            Payload::ActionRequest(request) => format!("request={:?}", request.action),
            Payload::Capabilities(capabilities) => format!("capabilities={}", capabilities.len()),
            Payload::MemoryQuery { query, .. } => format!("memory_query={query}"),
            Payload::Error(error) => format!("error={}", error.code),
            Payload::Empty => "empty".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn compact_memory_hits(hits: &[MemoryHit]) -> Vec<MemoryHit> {
    hits.iter()
        .take(3)
        .map(|hit| MemoryHit {
            memory_id: hit.memory_id,
            topic: hit.topic.chars().take(80).collect(),
            summary: hit.summary.chars().take(96).collect(),
            score: hit.score,
            reason: hit.reason.clone(),
            tags: hit.tags.clone(),
        })
        .collect()
}

fn extract_result(message: &Message) -> String {
    match &message.payload {
        Payload::ActionResult(result) => result.result.get("answer").cloned().unwrap_or_default(),
        Payload::Text(text) => text.clone(),
        _ => String::new(),
    }
}
