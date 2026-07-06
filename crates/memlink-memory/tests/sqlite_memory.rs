use chrono::Utc;
use memlink_memory::{MemoryQuery, MemoryStore, MemoryUnit, SqliteMemoryStore};
use memlink_protocol::AgentId;
use rusqlite::Connection;
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
            quality_score: 0.5,
            reuse_count: 0i64,
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
    assert!(hits[0].score > 0.5);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn migrates_existing_memory_schema() {
    let path = PathBuf::from(format!("/tmp/memlink-memory-{}.sqlite", Uuid::new_v4()));
    let connection = Connection::open(&path).expect("open old sqlite");
    connection
        .execute_batch(
            r#"
            CREATE TABLE memories (
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
            "#,
        )
        .expect("create old schema");
    drop(connection);

    let store = SqliteMemoryStore::open(&path).expect("open migrated sqlite");
    store
        .put(MemoryUnit {
            memory_id: Uuid::new_v4(),
            source_agent: AgentId::new("summarizer"),
            created_at: Utc::now(),
            task_topic: "migration".to_owned(),
            summary: "schema migration works".to_owned(),
            tags: vec!["migration".to_owned()],
            keywords: vec!["schema".to_owned()],
            embedding: vec![1.0, 0.0],
            evidence_refs: vec![],
            quality_score: 0.5,
            reuse_count: 0,
        })
        .await
        .expect("put migrated memory");
    let _ = std::fs::remove_file(path);
}
