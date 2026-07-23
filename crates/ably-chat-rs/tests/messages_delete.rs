//! Integration tests for soft-deleting a message (`POST /messages/{serial}/delete`).

use ably_chat::{Auth, Client, MessageAction};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DELETED_BODY: &str = r#"{
    "serial": "01726585978590-001@abcdefghij:001",
    "version": {
        "serial": "01726585978590-003@abcdefghij:001",
        "timestamp": 1700000002000,
        "clientId": "alice"
    },
    "text": "",
    "clientId": "alice",
    "action": "message.delete",
    "metadata": {},
    "headers": {},
    "timestamp": 1700000002000
}"#;

#[tokio::test]
async fn delete_posts_to_delete_subresource_and_returns_delete_action() {
    let server = MockServer::start().await;
    // A soft delete is a POST to `/delete`, never an HTTP DELETE. Matching on
    // POST + the `/delete` suffix with `expect(1)` proves both.
    Mock::given(method("POST"))
        .and(path("/chat/v4/rooms/my-room/messages/msg-1/delete"))
        .and(header("x-ably-version", "4"))
        .and(query_param_is_missing("idempotencyKey"))
        .respond_with(ResponseTemplate::new(200).set_body_string(DELETED_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let msg = client
        .room("my-room")
        .messages()
        .delete("msg-1")
        .await
        .unwrap();
    assert_eq!(msg.action, MessageAction::Delete);
}

#[tokio::test]
async fn delete_carries_description_metadata_and_idempotency() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/delete"))
        .and(body_json(json!({
            "description": "spam",
            "metadata": { "reason": "abuse" }
        })))
        .and(query_param("idempotencyKey", "key-3"))
        .respond_with(ResponseTemplate::new(200).set_body_string(DELETED_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert("reason".to_owned(), "abuse".to_owned());

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    let msg = client
        .room("r")
        .messages()
        .delete("msg-1")
        .description("spam")
        .metadata(metadata)
        .idempotency_key("key-3")
        .await
        .unwrap();
    assert_eq!(msg.action, MessageAction::Delete);
}
