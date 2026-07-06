use chrono::Utc;
use memlink_memory::{MemoryQuery, MemoryStore, MemoryUnit, SqliteMemoryStore};
use memlink_protocol::AgentId;
use std::path::PathBuf;
use uuid::Uuid;

#[tokio::test]
async fn searches_keyword_tag_and_semantic_memory() {
    let path = PathBuf::from(format!("/tmp/memlink-memory-{}.sqlite", Uuid::new_v4()));
    let store = SqliteMemoryStore::open(&path).expect("open sqlite");
    let embedding = vec![1.0, 0.0, 0.0, 0.0];
    store
        .put(MemoryUnit {
            memory_id: Uuid::new_v4(),
            source_agent: AgentId::new("summarizer"),
            created_at: Utc::now(),
            task_topic: "StateRef shared memory".to_owned(),
            summary: "Structured protocol reuses evidence through StateRef".to_owned(),
            tags: vec!["stateref".to_owned(), "summary".to_owned()],
            keywords: vec!["state".to_owned(), "memory".to_owned()],
            embedding: embedding.clone(),
            evidence_refs: vec![],
        })
        .await
        .expect("put memory");

    let hits = store
        .search(MemoryQuery {
            query: "StateRef memory".to_owned(),
            tags: vec!["stateref".to_owned()],
            embedding,
            limit: 3,
        })
        .await
        .expect("search memory");

    assert_eq!(hits.len(), 1);
    assert!(hits[0].score > 0.8);
    let _ = std::fs::remove_file(path);
}
