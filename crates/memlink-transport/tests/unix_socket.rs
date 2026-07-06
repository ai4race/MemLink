use memlink_protocol::{AgentId, Message, MessageKind, Payload, Target};
use memlink_transport::{Transport, TransportFrame, UnixSocketTransport};
use uuid::Uuid;

#[tokio::test]
async fn sends_message_over_unix_socket() {
    let path = std::env::temp_dir().join(format!("memlink-{}.sock", Uuid::new_v4()));
    let server = UnixSocketTransport::bind(&path).await.expect("bind socket");
    let client = UnixSocketTransport::new(&path);
    let message = Message::new(
        AgentId::new("planner"),
        Target::Runtime,
        MessageKind::ActionResult,
        Payload::Text("ok".to_owned()),
        vec![],
    );
    let expected_id = message.message_id;
    let send = tokio::spawn(async move {
        client
            .send(TransportFrame {
                message,
                state_refs: vec![],
            })
            .await
    });
    let frame = server.accept_one().await.expect("accept frame");
    send.await.expect("join send").expect("send frame");
    assert_eq!(frame.message.message_id, expected_id);
    let _ = std::fs::remove_file(path);
}
