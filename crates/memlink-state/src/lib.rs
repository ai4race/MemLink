use async_trait::async_trait;
use bytes::Bytes;
use chrono::{Duration as ChronoDuration, Utc};
use memlink_protocol::{AgentId, Checksum, StateFormat, StateId, StateRef, StateTransport};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StateMeta {
    pub producer: AgentId,
    pub format: StateFormat,
    pub shape: Option<Vec<usize>>,
    pub ttl: Option<Duration>,
}

#[async_trait]
pub trait StateStore: Send + Sync {
    async fn put(&self, bytes: Bytes, meta: StateMeta) -> Result<StateRef, StateError>;
    async fn get(&self, state_ref: &StateRef) -> Result<Bytes, StateError>;
    async fn pin(&self, state_id: StateId, ttl: Duration) -> Result<(), StateError>;
    async fn delete_expired(&self) -> Result<usize, StateError>;
}

#[derive(Debug, Error)]
pub enum StateError {
    #[error("state not found: {0}")]
    NotFound(StateId),
    #[error("checksum mismatch for state: {0}")]
    ChecksumMismatch(StateId),
    #[error("state io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
struct StateEntry {
    bytes: Bytes,
    state_ref: StateRef,
}

#[derive(Debug, Clone)]
struct FileStateEntry {
    path: PathBuf,
    state_ref: StateRef,
}

#[derive(Debug)]
pub struct MmapFileStateStore {
    root: PathBuf,
    entries: RwLock<HashMap<StateId, FileStateEntry>>,
}

impl MmapFileStateStore {
    pub async fn open(root: impl AsRef<Path>) -> Result<Arc<Self>, StateError> {
        let root = root.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&root).await?;
        Ok(Arc::new(Self {
            root,
            entries: RwLock::new(HashMap::new()),
        }))
    }

    fn state_path(&self, state_id: StateId) -> PathBuf {
        self.root.join(format!("{state_id}.bin"))
    }
}

#[async_trait]
impl StateStore for MmapFileStateStore {
    async fn put(&self, bytes: Bytes, meta: StateMeta) -> Result<StateRef, StateError> {
        let state_id = Uuid::new_v4();
        let path = self.state_path(state_id);
        tokio::fs::write(&path, &bytes).await?;
        let checksum = Checksum {
            algorithm: "blake3".to_owned(),
            value: hex::encode(blake3::hash(&bytes).as_bytes()),
        };
        let created_at = Utc::now();
        let expires_at = meta
            .ttl
            .and_then(|ttl| ChronoDuration::from_std(ttl).ok())
            .map(|ttl| created_at + ttl);
        let state_ref = StateRef {
            state_id,
            producer: meta.producer,
            format: meta.format,
            shape: meta.shape,
            byte_len: bytes.len() as u64,
            transport: StateTransport::MmapFile,
            checksum,
            created_at,
            expires_at,
        };
        self.entries.write().await.insert(
            state_id,
            FileStateEntry {
                path,
                state_ref: state_ref.clone(),
            },
        );
        Ok(state_ref)
    }

    async fn get(&self, state_ref: &StateRef) -> Result<Bytes, StateError> {
        let path = {
            let entries = self.entries.read().await;
            entries
                .get(&state_ref.state_id)
                .map(|entry| entry.path.clone())
                .unwrap_or_else(|| self.state_path(state_ref.state_id))
        };
        if !path.exists() {
            return Err(StateError::NotFound(state_ref.state_id));
        }
        let bytes = tokio::fs::read(path).await?;
        let checksum = hex::encode(blake3::hash(&bytes).as_bytes());
        if checksum != state_ref.checksum.value {
            return Err(StateError::ChecksumMismatch(state_ref.state_id));
        }
        Ok(Bytes::from(bytes))
    }

    async fn pin(&self, state_id: StateId, ttl: Duration) -> Result<(), StateError> {
        let mut entries = self.entries.write().await;
        let entry = entries
            .get_mut(&state_id)
            .ok_or(StateError::NotFound(state_id))?;
        let ttl = ChronoDuration::from_std(ttl).map_err(|_| StateError::NotFound(state_id))?;
        entry.state_ref.expires_at = Some(Utc::now() + ttl);
        Ok(())
    }

    async fn delete_expired(&self) -> Result<usize, StateError> {
        let now = Utc::now();
        let expired = {
            let entries = self.entries.read().await;
            entries
                .iter()
                .filter(|(_, entry)| {
                    entry
                        .state_ref
                        .expires_at
                        .is_some_and(|expires_at| expires_at <= now)
                })
                .map(|(state_id, entry)| (*state_id, entry.path.clone()))
                .collect::<Vec<_>>()
        };
        let mut deleted = 0;
        for (state_id, path) in expired {
            match tokio::fs::remove_file(path).await {
                Ok(()) => deleted += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => deleted += 1,
                Err(error) => return Err(StateError::Io(error)),
            }
            self.entries.write().await.remove(&state_id);
        }
        Ok(deleted)
    }
}

#[derive(Debug, Default)]
pub struct InMemoryStateStore {
    entries: RwLock<HashMap<StateId, StateEntry>>,
}

impl InMemoryStateStore {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[async_trait]
impl StateStore for InMemoryStateStore {
    async fn put(&self, bytes: Bytes, meta: StateMeta) -> Result<StateRef, StateError> {
        let state_id = Uuid::new_v4();
        let checksum = Checksum {
            algorithm: "blake3".to_owned(),
            value: hex::encode(blake3::hash(&bytes).as_bytes()),
        };
        let created_at = Utc::now();
        let expires_at = meta
            .ttl
            .and_then(|ttl| ChronoDuration::from_std(ttl).ok())
            .map(|ttl| created_at + ttl);
        let state_ref = StateRef {
            state_id,
            producer: meta.producer,
            format: meta.format,
            shape: meta.shape,
            byte_len: bytes.len() as u64,
            transport: StateTransport::InMemory,
            checksum,
            created_at,
            expires_at,
        };
        self.entries.write().await.insert(
            state_id,
            StateEntry {
                bytes,
                state_ref: state_ref.clone(),
            },
        );
        Ok(state_ref)
    }

    async fn get(&self, state_ref: &StateRef) -> Result<Bytes, StateError> {
        let entries = self.entries.read().await;
        let entry = entries
            .get(&state_ref.state_id)
            .ok_or(StateError::NotFound(state_ref.state_id))?;
        let checksum = hex::encode(blake3::hash(&entry.bytes).as_bytes());
        if checksum != state_ref.checksum.value {
            return Err(StateError::ChecksumMismatch(state_ref.state_id));
        }
        Ok(entry.bytes.clone())
    }

    async fn pin(&self, state_id: StateId, ttl: Duration) -> Result<(), StateError> {
        let mut entries = self.entries.write().await;
        let entry = entries
            .get_mut(&state_id)
            .ok_or(StateError::NotFound(state_id))?;
        let ttl = ChronoDuration::from_std(ttl).map_err(|_| StateError::NotFound(state_id))?;
        entry.state_ref.expires_at = Some(Utc::now() + ttl);
        Ok(())
    }

    async fn delete_expired(&self) -> Result<usize, StateError> {
        let now = Utc::now();
        let mut entries = self.entries.write().await;
        let before = entries.len();
        entries.retain(|_, entry| {
            entry
                .state_ref
                .expires_at
                .is_none_or(|expires_at| expires_at > now)
        });
        Ok(before - entries.len())
    }
}

pub fn deterministic_embedding(text: &str, dims: usize) -> Vec<f32> {
    let mut vector = vec![0.0; dims.max(1)];
    for token in text.split(|character: char| !character.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        let hash = blake3::hash(token.to_lowercase().as_bytes());
        let bytes = hash.as_bytes();
        for (index, value) in vector.iter_mut().enumerate() {
            let byte = bytes[index % bytes.len()] as f32;
            *value += (byte / 255.0) - 0.5;
        }
    }
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

pub fn embedding_to_bytes(embedding: &[f32]) -> Bytes {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for value in embedding {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Bytes::from(bytes)
}

pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
