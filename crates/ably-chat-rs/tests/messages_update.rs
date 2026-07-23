//! Integration tests for updating (full-replace) a message (`PUT /messages/{serial}`).

use ably_chat::{Auth, Client, MessageAction};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const UPDATED_BODY: &str = r#"{
    "serial": "01726585978590-001@abcdefghij:001",
    "version": {
        "serial": "01726585978590-002@abcdefghij:001",
        "timestamp": 1700000001000,
        "clientId": "alice",
        "description": "typo fix"
    },
    "text": "new text",
    "clientId": "alice",
    "action": "message.update",
    "metadata": {},
    "headers": {},
    "timestamp": 1700000001000
}"#;

#[tokio::test]
async fn update_nests_content_under_message_and_returns_updated() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/chat/v4/rooms/my-room/messages/msg-1"))
        .and(header("x-ably-version", "4"))
        .and(body_json(json!({ "message": { "text": "new text" } })))
        .and(query_param_is_missing("idempotencyKey"))
        .respond_with(ResponseTemplate::new(200).set_body_string(UPDATED_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let msg = client
        .room("my-room")
        .messages()
        .update("msg-1", "new text")
        .await
        .unwrap();
    assert_eq!(msg.text, "new text");
    assert_eq!(msg.action, MessageAction::Update);
}

#[tokio::test]
async fn update_carries_metadata_headers_description_and_idempotency() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/chat/v4/rooms/r/messages/msg-1"))
        .and(body_json(json!({
            "message": {
                "text": "new text",
                "metadata": { "lang": "en" },
                "headers": { "x-trace": "abc" }
            },
            "description": "typo fix"
        })))
        .and(query_param("idempotencyKey", "key-9"))
        .respond_with(ResponseTemplate::new(200).set_body_string(UPDATED_BODY))
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
        .update("msg-1", "new text")
        .metadata(metadata)
        .headers(headers)
        .description("typo fix")
        .idempotency_key("key-9")
        .await
        .unwrap();
    assert_eq!(msg.action, MessageAction::Update);
}
