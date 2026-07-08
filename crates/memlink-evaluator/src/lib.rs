use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use memlink_protocol::{ExperimentId, MessageKind, RunMode, TaskId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    AgentRegistered {
        agent_id: String,
        role: String,
        capability_count: usize,
    },
    MessageMetric {
        from: String,
        to: String,
        kind: MessageKind,
        text_chars: u64,
        encoded_bytes: u64,
        state_bytes: u64,
    },
    StateTransfer {
        state_id: Uuid,
        format: String,
        byte_len: u64,
        producer: String,
        consumer: String,
    },
    MemoryQuery {
        query: String,
        hit_count: usize,
        adopted_count: usize,
    },
    MemoryWritten {
        memory_id: Uuid,
        topic: String,
        source_agent: String,
    },
    TaskFinished {
        mode: RunMode,
        success: bool,
        duration_ms: u64,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: Uuid,
    pub experiment_id: ExperimentId,
    pub task_id: TaskId,
    pub created_at: DateTime<Utc>,
    pub kind: EventKind,
}

impl Event {
    pub fn new(experiment_id: ExperimentId, task_id: TaskId, kind: EventKind) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            experiment_id,
            task_id,
            created_at: Utc::now(),
            kind,
        }
    }
}

#[async_trait]
pub trait EvaluatorSink: Send + Sync {
    async fn record(&self, event: Event) -> Result<()>;
}

#[derive(Debug)]
pub struct JsonlEvaluator {
    path: PathBuf,
    file: Mutex<File>,
}

impl JsonlEvaluator {
    pub async fn open(path: impl AsRef<Path>) -> Result<Arc<Self>> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        Ok(Arc::new(Self {
            path,
            file: Mutex::new(file),
        }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[async_trait]
impl EvaluatorSink for JsonlEvaluator {
    async fn record(&self, event: Event) -> Result<()> {
        let mut file = self.file.lock().await;
        let line = serde_json::to_string(&event)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub mode: Option<RunMode>,
    pub task_count: u64,
    pub success_count: u64,
    pub message_count: u64,
    pub text_chars: u64,
    pub encoded_bytes: u64,
    pub state_transfer_count: u64,
    pub state_bytes: u64,
    pub duration_ms: u64,
    pub memory_queries: u64,
    pub memory_queries_with_hits: u64,
    pub memory_hits: u64,
    pub adopted_memory_hits: u64,
}

impl MetricsSummary {
    pub fn memory_hit_rate(&self) -> f64 {
        if self.memory_queries == 0 {
            0.0
        } else {
            self.memory_queries_with_hits as f64 / self.memory_queries as f64
        }
    }

    pub fn effective_reuse_rate(&self) -> f64 {
        if self.memory_hits == 0 {
            0.0
        } else {
            self.adopted_memory_hits as f64 / self.memory_hits as f64
        }
    }
}

pub async fn summarize_events(
    path: impl AsRef<Path>,
    mode: Option<RunMode>,
) -> Result<MetricsSummary> {
    let path = path.as_ref();
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read event log {}", path.display()))?;
    let mut summary = MetricsSummary {
        mode,
        ..MetricsSummary::default()
    };
    for (index, line) in content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let event: Event = serde_json::from_str(line)
            .with_context(|| format!("parse event log {} line {}", path.display(), index + 1))?;
        match event.kind {
            EventKind::MessageMetric {
                text_chars,
                encoded_bytes,
                state_bytes,
                ..
            } => {
                summary.message_count += 1;
                summary.text_chars += text_chars;
                summary.encoded_bytes += encoded_bytes;
                summary.state_bytes += state_bytes;
            }
            EventKind::StateTransfer { .. } => {
                summary.state_transfer_count += 1;
            }
            EventKind::MemoryQuery {
                hit_count,
                adopted_count,
                ..
            } => {
                summary.memory_queries += 1;
                summary.memory_queries_with_hits += u64::from(hit_count > 0);
                summary.memory_hits += hit_count as u64;
                summary.adopted_memory_hits += adopted_count as u64;
            }
            EventKind::TaskFinished {
                success,
                duration_ms,
                ..
            } => {
                summary.task_count += 1;
                summary.success_count += u64::from(success);
                summary.duration_ms += duration_ms;
            }
            EventKind::AgentRegistered { .. }
            | EventKind::MemoryWritten { .. }
            | EventKind::Error { .. } => {}
        }
    }
    Ok(summary)
}

pub fn comparison_report(text: &MetricsSummary, structured: &MetricsSummary) -> String {
    let text_saving = saving(structured.text_chars, text.text_chars);
    let byte_saving = saving(structured.encoded_bytes, text.encoded_bytes);
    let latency_improvement = saving(structured.duration_ms, text.duration_ms);
    format!(
        "# MemLink Experiment Report\n\n| Metric | Text | Structured | Improvement |\n| --- | ---: | ---: | ---: |\n| Tasks | {} | {} | - |\n| Success | {} | {} | - |\n| Messages | {} | {} | - |\n| Text chars | {} | {} | {:.2}% |\n| Encoded bytes | {} | {} | {:.2}% |\n| State transfers | {} | {} | - |\n| State bytes | {} | {} | - |\n| Duration ms | {} | {} | {:.2}% |\n| Memory queries with hits | {} | {} | - |\n| Memory hit rate | {:.2}% | {:.2}% | - |\n| Effective reuse rate | {:.2}% | {:.2}% | - |\n\nStructured mode carries large intermediate artifacts as `StateRef` handles and reuses SQLite-backed memories across linked tasks.\n",
        text.task_count,
        structured.task_count,
        text.success_count,
        structured.success_count,
        text.message_count,
        structured.message_count,
        text.text_chars,
        structured.text_chars,
        text_saving * 100.0,
        text.encoded_bytes,
        structured.encoded_bytes,
        byte_saving * 100.0,
        text.state_transfer_count,
        structured.state_transfer_count,
        text.state_bytes,
        structured.state_bytes,
        text.duration_ms,
        structured.duration_ms,
        latency_improvement * 100.0,
        text.memory_queries_with_hits,
        structured.memory_queries_with_hits,
        text.memory_hit_rate() * 100.0,
        structured.memory_hit_rate() * 100.0,
        text.effective_reuse_rate() * 100.0,
        structured.effective_reuse_rate() * 100.0,
    )
}

pub fn saving(current: u64, baseline: u64) -> f64 {
    if baseline == 0 {
        0.0
    } else {
        1.0 - current as f64 / baseline as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_report_uses_encoded_bytes_baseline_for_byte_saving() {
        let text = MetricsSummary {
            task_count: 1,
            success_count: 1,
            message_count: 1,
            text_chars: 100,
            encoded_bytes: 400,
            duration_ms: 100,
            ..MetricsSummary::default()
        };
        let structured = MetricsSummary {
            task_count: 1,
            success_count: 1,
            message_count: 1,
            text_chars: 90,
            encoded_bytes: 100,
            duration_ms: 100,
            ..MetricsSummary::default()
        };

        let report = comparison_report(&text, &structured);

        assert!(report.contains("| Encoded bytes | 400 | 100 | 75.00% |"));
    }

    #[tokio::test]
    async fn summarize_events_reports_missing_log_file() {
        let path =
            std::env::temp_dir().join(format!("memlink-missing-events-{}.jsonl", Uuid::new_v4()));

        let error = summarize_events(&path, Some(RunMode::Structured))
            .await
            .expect_err("missing event log should fail");

        assert!(
            error
                .to_string()
                .contains(&format!("read event log {}", path.display()))
        );
    }
}
