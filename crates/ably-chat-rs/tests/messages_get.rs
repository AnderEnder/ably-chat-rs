//! Integration tests for fetching a single message by serial.

use ably_chat::{Auth, Client, MessageAction};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MESSAGE_BODY: &str = r#"{
    "serial": "01726585978590-001@abcdefghij:001",
    "version": {
        "serial": "01726585978590-001@abcdefghij:001",
        "timestamp": 1700000000000,
        "clientId": "alice"
    },
    "text": "hello",
    "clientId": "alice",
    "action": "message.create",
    "metadata": {},
    "headers": {},
    "timestamp": 1700000000000
}"#;

#[tokio::test]
async fn get_message_returns_typed_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/my-room/messages/msg-1"))
        .and(header("x-ably-version", "4"))
        .respond_with(ResponseTemplate::new(200).set_body_string(MESSAGE_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let msg = client
        .room("my-room")
        .messages()
        .get("msg-1")
        .await
        .unwrap();
    assert_eq!(msg.serial.as_str(), "01726585978590-001@abcdefghij:001");
    assert_eq!(msg.text, "hello");
    assert_eq!(msg.client_id, "alice");
    assert_eq!(msg.action, MessageAction::Create);
    assert_eq!(msg.timestamp.as_millis(), 1_700_000_000_000);
}

#[tokio::test]
async fn get_message_not_found_maps_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/messages/missing"))
        .respond_with(
            ResponseTemplate::new(404).set_body_string(
                r#"{"error":{"code":40400,"message":"not found","statusCode":404}}"#,
            ),
        )
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let err = client
        .room("r")
        .messages()
        .get("missing")
        .await
        .unwrap_err();
    assert!(err.is_not_found());
}
