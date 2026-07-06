use async_trait::async_trait;
use chrono::{DateTime, Utc};
use memlink_protocol::{AgentId, MemoryHit, MemoryId, StateRef};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::task;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUnit {
    pub memory_id: MemoryId,
    pub source_agent: AgentId,
    pub created_at: DateTime<Utc>,
    pub task_topic: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub keywords: Vec<String>,
    pub embedding: Vec<f32>,
    pub evidence_refs: Vec<StateRef>,
}

#[derive(Debug, Clone)]
pub struct MemoryQuery {
    pub query: String,
    pub tags: Vec<String>,
    pub embedding: Vec<f32>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReuseEvent {
    pub memory_id: MemoryId,
    pub task_id: Uuid,
    pub adopted: bool,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn put(&self, memory: MemoryUnit) -> Result<MemoryId, MemoryError>;
    async fn search(&self, query: MemoryQuery) -> Result<Vec<MemoryHit>, MemoryError>;
    async fn record_reuse(&self, event: MemoryReuseEvent) -> Result<(), MemoryError>;
}

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("task join error: {0}")]
    Join(#[from] task::JoinError),
}

#[derive(Debug, Clone)]
pub struct SqliteMemoryStore {
    path: PathBuf,
}

impl SqliteMemoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MemoryError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let store = Self { path };
        store.init()?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, MemoryError> {
        Ok(Connection::open(&self.path)?)
    }

    fn init(&self) -> Result<(), MemoryError> {
        let connection = self.connect()?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                memory_id TEXT PRIMARY KEY,
                source_agent TEXT NOT NULL,
                created_at TEXT NOT NULL,
                task_topic TEXT NOT NULL,
                summary TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                keywords_json TEXT NOT NULL,
                embedding_json TEXT NOT NULL,
                evidence_refs_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS memory_tags (
                memory_id TEXT NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY(memory_id, tag)
            );
            CREATE TABLE IF NOT EXISTS memory_keywords (
                memory_id TEXT NOT NULL,
                keyword TEXT NOT NULL,
                PRIMARY KEY(memory_id, keyword)
            );
            CREATE TABLE IF NOT EXISTS memory_reuse_events (
                event_id TEXT PRIMARY KEY,
                memory_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                adopted INTEGER NOT NULL,
                reason TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn put(&self, memory: MemoryUnit) -> Result<MemoryId, MemoryError> {
        let store = self.clone();
        task::spawn_blocking(move || {
            let mut connection = store.connect()?;
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT OR REPLACE INTO memories VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    memory.memory_id.to_string(),
                    memory.source_agent.0,
                    memory.created_at.to_rfc3339(),
                    memory.task_topic,
                    memory.summary,
                    serde_json::to_string(&memory.tags)?,
                    serde_json::to_string(&memory.keywords)?,
                    serde_json::to_string(&memory.embedding)?,
                    serde_json::to_string(&memory.evidence_refs)?,
                ],
            )?;
            transaction.execute(
                "DELETE FROM memory_tags WHERE memory_id = ?1",
                params![memory.memory_id.to_string()],
            )?;
            transaction.execute(
                "DELETE FROM memory_keywords WHERE memory_id = ?1",
                params![memory.memory_id.to_string()],
            )?;
            for tag in &memory.tags {
                transaction.execute(
                    "INSERT OR IGNORE INTO memory_tags VALUES (?1, ?2)",
                    params![memory.memory_id.to_string(), tag],
                )?;
            }
            for keyword in &memory.keywords {
                transaction.execute(
                    "INSERT OR IGNORE INTO memory_keywords VALUES (?1, ?2)",
                    params![memory.memory_id.to_string(), keyword],
                )?;
            }
            transaction.commit()?;
            Ok(memory.memory_id)
        })
        .await?
    }

    async fn search(&self, query: MemoryQuery) -> Result<Vec<MemoryHit>, MemoryError> {
        let store = self.clone();
        task::spawn_blocking(move || {
            let connection = store.connect()?;
            let mut statement = connection.prepare(
                "SELECT memory_id, task_topic, summary, tags_json, keywords_json, embedding_json FROM memories ORDER BY created_at DESC",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?;
            let query_terms = terms(&query.query);
            let mut hits = Vec::new();
            for row in rows {
                let (memory_id, topic, summary, tags_json, keywords_json, embedding_json) = row?;
                let tags: Vec<String> = serde_json::from_str(&tags_json)?;
                let keywords: Vec<String> = serde_json::from_str(&keywords_json)?;
                let embedding: Vec<f32> = serde_json::from_str(&embedding_json)?;
                let tag_score = if query.tags.is_empty() {
                    0.0
                } else {
                    query.tags.iter().filter(|tag| tags.contains(tag)).count() as f32 / query.tags.len() as f32
                };
                let searchable = format!("{} {} {} {}", topic, summary, tags.join(" "), keywords.join(" ")).to_lowercase();
                let keyword_score = if query_terms.is_empty() {
                    0.0
                } else {
                    query_terms.iter().filter(|term| searchable.contains(term.as_str())).count() as f32 / query_terms.len() as f32
                };
                let semantic_score = cosine(&query.embedding, &embedding).max(0.0);
                let score = keyword_score * 0.45 + tag_score * 0.25 + semantic_score * 0.30;
                if score > 0.05 {
                    let reason = format!("keyword={keyword_score:.2}, tag={tag_score:.2}, semantic={semantic_score:.2}");
                    hits.push(MemoryHit {
                        memory_id: Uuid::parse_str(&memory_id).unwrap_or_else(|_| Uuid::nil()),
                        topic,
                        summary,
                        score,
                        reason,
                        tags,
                    });
                }
            }
            hits.sort_by(|left, right| right.score.total_cmp(&left.score));
            hits.truncate(query.limit);
            Ok(hits)
        })
        .await?
    }

    async fn record_reuse(&self, event: MemoryReuseEvent) -> Result<(), MemoryError> {
        let store = self.clone();
        task::spawn_blocking(move || {
            let connection = store.connect()?;
            connection.execute(
                "INSERT INTO memory_reuse_events VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    Uuid::new_v4().to_string(),
                    event.memory_id.to_string(),
                    event.task_id.to_string(),
                    i64::from(event.adopted),
                    event.reason,
                    event.created_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await?
    }
}

impl SqliteMemoryStore {
    pub async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryUnit>, MemoryError> {
        let store = self.clone();
        task::spawn_blocking(move || {
            let connection = store.connect()?;
            connection
                .query_row(
                    "SELECT source_agent, created_at, task_topic, summary, tags_json, keywords_json, embedding_json, evidence_refs_json FROM memories WHERE memory_id = ?1",
                    params![memory_id.to_string()],
                    |row| {
                        let created_at = row.get::<_, String>(1)?;
                        let parsed_at = DateTime::parse_from_rfc3339(&created_at)
                            .map(|value| value.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                        Ok(MemoryUnit {
                            memory_id,
                            source_agent: AgentId(row.get(0)?),
                            created_at: parsed_at,
                            task_topic: row.get(2)?,
                            summary: row.get(3)?,
                            tags: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                            keywords: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                            embedding: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                            evidence_refs: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                        })
                    },
                )
                .optional()
                .map_err(MemoryError::from)
        })
        .await?
    }
}

fn terms(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|part| part.len() > 1)
        .map(ToOwned::to_owned)
        .collect()
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let dot = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}
