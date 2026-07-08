use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use uuid::Uuid;

pub type MessageId = Uuid;
pub type StateId = Uuid;
pub type MemoryId = Uuid;
pub type TaskId = Uuid;
pub type ExperimentId = Uuid;
pub type Timestamp = DateTime<Utc>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Text,
    Structured,
}

impl Display for RunMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Structured => write!(f, "structured"),
        }
    }
}

impl std::str::FromStr for RunMode {
    type Err = ProtocolError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "text" => Ok(Self::Text),
            "structured" => Ok(Self::Structured),
            other => Err(ProtocolError::InvalidRunMode(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    /// Check compatibility: major must match exactly, minor must be >= remote minor.
    /// Returns Ok(()) if compatible, or a description of the incompatibility.
    pub fn compatible_with(&self, remote: &ProtocolVersion) -> Result<(), String> {
        if self.major != remote.major {
            return Err(format!(
                "major version mismatch: local={} remote={}",
                self.major, remote.major
            ));
        }
        if self.minor < remote.minor {
            return Err(format!(
                "local minor {} older than remote minor {}",
                self.minor, remote.minor
            ));
        }
        Ok(())
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self { major: 1, minor: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    pub experiment_id: ExperimentId,
    pub task_id: TaskId,
    pub parent_message_id: Option<MessageId>,
    pub step: u32,
}

impl TraceContext {
    pub fn new(experiment_id: ExperimentId, task_id: TaskId) -> Self {
        Self {
            experiment_id,
            task_id,
            parent_message_id: None,
            step: 0,
        }
    }

    pub fn child(&self, parent_message_id: MessageId) -> Self {
        let mut next = self.clone();
        next.parent_message_id = Some(parent_message_id);
        next.step += 1;
        next
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl Display for AgentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Retriever,
    Executor,
    Summarizer,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Agent(AgentId),
    Role(AgentRole),
    Runtime,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Handshake,
    CapabilityAdvertise,
    ActionRequest,
    ActionResult,
    StateTransfer,
    MemoryQuery,
    MemoryHit,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    PlanTask,
    SearchMemory,
    SearchExternal,
    ExtractEvidence,
    ExecuteTool,
    Summarize,
    StoreMemory,
    EvaluateRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateFormat {
    EmbeddingF32,
    TaskGraphJson,
    EvidencePackJson,
    ToolOutputJson,
    TextUtf8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateTransport {
    InMemory,
    MmapFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checksum {
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateRef {
    pub state_id: StateId,
    pub producer: AgentId,
    pub format: StateFormat,
    pub shape: Option<Vec<usize>>,
    pub byte_len: u64,
    pub transport: StateTransport,
    pub checksum: Checksum,
    pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostHint {
    pub expected_ms: u64,
    pub text_chars: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub agent_id: AgentId,
    pub role: AgentRole,
    pub action: ActionType,
    pub input_schema: String,
    pub output_schema: String,
    pub accepted_state_formats: Vec<StateFormat>,
    pub cost_hint: CostHint,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageMetrics {
    pub text_chars: u64,
    pub encoded_bytes: u64,
    pub state_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub action: ActionType,
    pub params: BTreeMap<String, String>,
    pub required_capability: Option<ActionType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action: ActionType,
    pub success: bool,
    pub result: BTreeMap<String, String>,
    pub memory_candidates: Vec<MemoryCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub topic: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub keywords: Vec<String>,
    pub evidence_refs: Vec<StateRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHitPayload {
    pub hits: Vec<MemoryHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHit {
    pub memory_id: MemoryId,
    pub topic: String,
    pub summary: String,
    pub score: f32,
    pub reason: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub diagnostics_ref: Option<StateRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Payload {
    Empty,
    Text(String),
    Capabilities(Vec<Capability>),
    ActionRequest(ActionRequest),
    ActionResult(ActionResult),
    MemoryQuery {
        query: String,
        tags: Vec<String>,
        limit: usize,
    },
    MemoryHit(MemoryHitPayload),
    Error(ErrorPayload),
}

impl Payload {
    pub fn text_chars(&self) -> u64 {
        match self {
            Self::Text(value) => value.chars().count() as u64,
            Self::Empty => 0,
            Self::Capabilities(capabilities) => capabilities
                .iter()
                .map(|capability| {
                    chars(&capability.input_schema) + chars(&capability.output_schema)
                })
                .sum(),
            Self::ActionRequest(request) => request.params.values().map(|value| chars(value)).sum(),
            Self::ActionResult(result) => {
                result
                    .result
                    .values()
                    .map(|value| chars(value))
                    .sum::<u64>()
                    + result
                        .memory_candidates
                        .iter()
                        .map(memory_candidate_chars)
                        .sum::<u64>()
            }
            Self::MemoryQuery { query, tags, .. } => {
                chars(query) + tags.iter().map(|tag| chars(tag)).sum::<u64>()
            }
            Self::MemoryHit(payload) => payload.hits.iter().map(memory_hit_chars).sum(),
            Self::Error(error) => chars(&error.code) + chars(&error.message),
        }
    }
}

fn chars(value: &str) -> u64 {
    value.chars().count() as u64
}

fn memory_candidate_chars(candidate: &MemoryCandidate) -> u64 {
    chars(&candidate.topic)
        + chars(&candidate.summary)
        + candidate.tags.iter().map(|tag| chars(tag)).sum::<u64>()
        + candidate
            .keywords
            .iter()
            .map(|keyword| chars(keyword))
            .sum::<u64>()
}

fn memory_hit_chars(hit: &MemoryHit) -> u64 {
    chars(&hit.topic)
        + chars(&hit.summary)
        + chars(&hit.reason)
        + hit.tags.iter().map(|tag| chars(tag)).sum::<u64>()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_id: MessageId,
    pub from: AgentId,
    pub to: Target,
    pub kind: MessageKind,
    pub payload: Payload,
    pub state_refs: Vec<StateRef>,
    pub metrics: MessageMetrics,
    pub created_at: Timestamp,
}

impl Message {
    pub fn new(
        from: AgentId,
        to: Target,
        kind: MessageKind,
        payload: Payload,
        state_refs: Vec<StateRef>,
    ) -> Self {
        let mut message = Self {
            message_id: Uuid::new_v4(),
            from,
            to,
            kind,
            payload,
            state_refs,
            metrics: MessageMetrics::default(),
            created_at: Utc::now(),
        };
        message.refresh_metrics();
        message
    }

    pub fn refresh_metrics(&mut self) {
        self.metrics.text_chars = self.payload.text_chars();
        let mut measured = self.clone();
        measured.metrics.encoded_bytes = 0;
        measured.metrics.state_bytes = 0;
        self.metrics.encoded_bytes = postcard::to_allocvec(&measured)
            .map(|bytes| bytes.len() as u64)
            .unwrap_or(0);
        self.metrics.state_bytes = self
            .state_refs
            .iter()
            .map(|state_ref| state_ref.byte_len)
            .sum();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolEnvelope {
    pub version: ProtocolVersion,
    pub trace: TraceContext,
    pub message: Message,
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid run mode: {0}")]
    InvalidRunMode(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_payload_text_chars_count_string_values_not_json_scaffolding() {
        let payload = Payload::ActionRequest(ActionRequest {
            action: ActionType::PlanTask,
            params: BTreeMap::from([
                ("topic".to_owned(), "StateRef".to_owned()),
                ("prompt".to_owned(), "reuse memory".to_owned()),
            ]),
            required_capability: Some(ActionType::PlanTask),
        });

        assert_eq!(payload.text_chars(), 20);
    }

    #[test]
    fn message_encoded_byte_metric_is_stable_across_refreshes() {
        let mut message = Message::new(
            AgentId::new("planner"),
            Target::Runtime,
            MessageKind::ActionResult,
            Payload::Text("x".repeat(512)),
            vec![],
        );
        let first = message.metrics.encoded_bytes;

        message.refresh_metrics();

        assert_eq!(message.metrics.encoded_bytes, first);
    }
}
