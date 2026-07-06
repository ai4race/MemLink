use async_trait::async_trait;
use memlink_protocol::{Message, StateRef};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportFrame {
    pub message: Message,
    pub state_refs: Vec<StateRef>,
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, frame: TransportFrame) -> Result<(), TransportError>;
}

#[derive(Debug, Clone)]
pub struct UnixSocketTransport {
    path: PathBuf,
}

impl UnixSocketTransport {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub async fn bind(path: impl AsRef<Path>) -> Result<UnixSocketServer, TransportError> {
        let path = path.as_ref();
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let listener = UnixListener::bind(path)?;
        Ok(UnixSocketServer { listener })
    }
}

#[async_trait]
impl Transport for UnixSocketTransport {
    async fn send(&self, frame: TransportFrame) -> Result<(), TransportError> {
        let mut stream = UnixStream::connect(&self.path).await?;
        let mut bytes = serde_json::to_vec(&frame)?;
        bytes.push(b'\n');
        stream.write_all(&bytes).await?;
        stream.shutdown().await?;
        Ok(())
    }
}

pub struct UnixSocketServer {
    listener: UnixListener,
}

impl UnixSocketServer {
    pub async fn accept_one(&self) -> Result<TransportFrame, TransportError> {
        let (stream, _) = self.listener.accept().await?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        Ok(serde_json::from_str(&line)?)
    }
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("transport json error: {0}")]
    Json(#[from] serde_json::Error),
}
