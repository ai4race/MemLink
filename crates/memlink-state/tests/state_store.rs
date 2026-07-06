use bytes::Bytes;
use memlink_protocol::{AgentId, StateFormat, StateTransport};
use memlink_state::{
    InMemoryStateStore, MmapFileStateStore, StateMeta, StateStore, deterministic_embedding,
    embedding_to_bytes,
};
use std::time::Duration;

#[tokio::test]
async fn stores_embedding_state_ref_with_checksum() {
    let store = InMemoryStateStore::default();
    let embedding = deterministic_embedding("StateRef evidence pack", 16);
    let state_ref = store
        .put(
            embedding_to_bytes(&embedding),
            StateMeta {
                producer: AgentId::new("retriever"),
                format: StateFormat::EmbeddingF32,
                shape: Some(vec![16]),
                ttl: Some(Duration::from_secs(60)),
            },
        )
        .await
        .expect("put state");

    assert_eq!(state_ref.byte_len, 64);
    assert_eq!(state_ref.producer, AgentId::new("retriever"));
    assert_eq!(
        store.get(&state_ref).await.expect("get state"),
        Bytes::from(embedding_to_bytes(&embedding))
    );
}

#[tokio::test]
async fn stores_large_state_in_mmap_file_backend() {
    let root = std::env::temp_dir().join(format!("memlink-state-{}", uuid::Uuid::new_v4()));
    let store = MmapFileStateStore::open(&root)
        .await
        .expect("open mmap store");
    let bytes = Bytes::from(vec![7_u8; 128 * 1024]);
    let state_ref = store
        .put(
            bytes.clone(),
            StateMeta {
                producer: AgentId::new("executor"),
                format: StateFormat::ToolOutputJson,
                shape: None,
                ttl: Some(Duration::from_millis(1)),
            },
        )
        .await
        .expect("put file state");

    assert_eq!(state_ref.transport, StateTransport::MmapFile);
    assert_eq!(state_ref.byte_len, bytes.len() as u64);
    assert_eq!(store.get(&state_ref).await.expect("get file state"), bytes);

    tokio::time::sleep(Duration::from_millis(5)).await;
    assert_eq!(store.delete_expired().await.expect("delete expired"), 1);
    assert!(store.get(&state_ref).await.is_err());
    let _ = std::fs::remove_dir_all(root);
}
