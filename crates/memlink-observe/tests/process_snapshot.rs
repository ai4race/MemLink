use memlink_observe::capture_process_snapshot;

#[tokio::test]
async fn captures_process_snapshot() {
    let snapshot = capture_process_snapshot("test").await.expect("snapshot");
    assert_eq!(snapshot.process_id, std::process::id());
    assert_eq!(snapshot.note, "test");
}
