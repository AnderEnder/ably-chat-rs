//! Integration tests for sending a message (`POST /messages`).

use ably_chat::{Auth, Client, MessageAction};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CREATED_BODY: &str = r#"{
    "serial": "01726585978590-001@abcdefghij:001",
    "version": {
        "serial": "01726585978590-001@abcdefghij:001",
        "timestamp": 1700000000000,
        "clientId": "alice"
    },
    "text": "hi",
    "clientId": "alice",
    "action": "message.create",
    "metadata": {},
    "headers": {},
    "timestamp": 1700000000000
}"#;

#[tokio::test]
async fn send_posts_text_body_and_returns_created_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/v4/rooms/my-room/messages"))
        .and(header("x-ably-version", "4"))
        .and(body_json(json!({ "text": "hi" })))
        .and(query_param_is_missing("idempotencyKey"))
        .respond_with(ResponseTemplate::new(201).set_body_string(CREATED_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let msg = client.room("my-room").messages().send("hi").await.unwrap();
    assert_eq!(msg.text, "hi");
    assert_eq!(msg.action, MessageAction::Create);
}

#[tokio::test]
async fn send_includes_metadata_headers_and_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/v4/rooms/r/messages"))
        .and(body_json(json!({
            "text": "hi",
            "metadata": { "lang": "en" },
            "headers": { "x-trace": "abc" }
        })))
        .and(query_param("idempotencyKey", "key-1"))
        .respond_with(ResponseTemplate::new(201).set_body_string(CREATED_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let mut metadata = serde_json::Map::new();
    metadata.insert("lang".to_owned(), json!("en"));
    let mut headers = std::collections::BTreeMap::new();
    headers.insert("x-trace".to_owned(), "abc".to_owned());

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    let msg = client
        .room("r")
        .messages()
        .send("hi")
        .metadata(metadata)
        .headers(headers)
        .idempotency_key("key-1")
        .await
        .unwrap();
    assert_eq!(msg.text, "hi");
}
