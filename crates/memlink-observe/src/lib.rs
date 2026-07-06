use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationSnapshot {
    pub snapshot_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub process_id: u32,
    pub socket_count: usize,
    pub open_fd_count: Option<usize>,
    pub note: String,
}

pub async fn capture_process_snapshot(
    note: impl Into<String>,
) -> Result<ObservationSnapshot, ObserveError> {
    Ok(ObservationSnapshot {
        snapshot_id: Uuid::new_v4(),
        created_at: Utc::now(),
        process_id: std::process::id(),
        socket_count: count_unix_sockets().await.unwrap_or(0),
        open_fd_count: count_open_fds().await.ok(),
        note: note.into(),
    })
}

pub async fn write_snapshot(
    path: impl AsRef<Path>,
    snapshot: &ObservationSnapshot,
) -> Result<(), ObserveError> {
    if let Some(parent) = path.as_ref().parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(snapshot)?;
    tokio::fs::write(path, json).await?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum ObserveError {
    #[error("observe io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("observe json error: {0}")]
    Json(#[from] serde_json::Error),
}

async fn count_unix_sockets() -> Result<usize, ObserveError> {
    #[cfg(target_os = "linux")]
    {
        let content = tokio::fs::read_to_string("/proc/net/unix").await?;
        Ok(content.lines().skip(1).count())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(0)
    }
}

async fn count_open_fds() -> Result<usize, ObserveError> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let mut entries = tokio::fs::read_dir("/proc/self/fd").await?;
        let mut count = 0;
        while entries.next_entry().await?.is_some() {
            count += 1;
        }
        Ok(count)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        Ok(0)
    }
}
