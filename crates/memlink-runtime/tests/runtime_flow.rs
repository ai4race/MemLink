use memlink_evaluator::{JsonlEvaluator, summarize_events};
use memlink_memory::SqliteMemoryStore;
use memlink_protocol::RunMode;
use memlink_runtime::{Runtime, TaskSpec};
use memlink_state::InMemoryStateStore;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn structured_runtime_emits_state_transfers_and_memory_hits() {
    let id = Uuid::new_v4();
    let db = PathBuf::from(format!("/tmp/memlink-runtime-{id}.sqlite"));
    let events = PathBuf::from(format!("/tmp/memlink-runtime-{id}.jsonl"));
    let state = InMemoryStateStore::shared();
    let memory = Arc::new(SqliteMemoryStore::open(&db).expect("open sqlite"));
    let evaluator = JsonlEvaluator::open(&events).await.expect("open evaluator");
    let runtime = Runtime::new(state, memory, evaluator);

    let task = TaskSpec {
        id: Some("test".to_owned()),
        group: "knowledge".to_owned(),
        topic: "StateRef shared memory".to_owned(),
        prompt: "Explain structured memory reuse".to_owned(),
        tags: vec!["stateref".to_owned()],
        code: None,
    };

    runtime
        .run_task(Uuid::new_v4(), RunMode::Structured, task.clone())
        .await
        .expect("first run");
    runtime
        .run_task(Uuid::new_v4(), RunMode::Structured, task)
        .await
        .expect("second run");

    let summary = summarize_events(&events, Some(RunMode::Structured))
        .await
        .expect("summary");
    assert_eq!(summary.task_count, 2);
    assert!(summary.state_transfer_count > 0);
    assert!(summary.memory_queries_with_hits > 0);
    let _ = std::fs::remove_file(db);
    let _ = std::fs::remove_file(events);
}
