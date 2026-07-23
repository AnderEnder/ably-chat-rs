//! Integration tests for sending a message reaction (`POST /reactions`).

use ably_chat::{Auth, Client, ReactionType};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn send_defaults_to_distinct_and_posts_type_and_name() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/reactions"))
        .and(header("x-ably-version", "4"))
        .and(body_json(
            json!({ "type": "distinct", "name": "\u{1f44d}" }),
        ))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    client
        .room("r")
        .messages()
        .reactions()
        .send("msg-1", "\u{1f44d}")
        .await
        .unwrap();
}

#[tokio::test]
async fn send_unique_posts_unique_type() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/reactions"))
        .and(body_json(json!({ "type": "unique", "name": "\u{1f44d}" })))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    client
        .room("r")
        .messages()
        .reactions()
        .send("msg-1", "\u{1f44d}")
        .kind(ReactionType::Unique)
        .await
        .unwrap();
}

#[tokio::test]
async fn send_multiple_includes_count() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/reactions"))
        .and(body_json(
            json!({ "type": "multiple", "name": "\u{1f389}", "count": 5 }),
        ))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    client
        .room("r")
        .messages()
        .reactions()
        .send("msg-1", "\u{1f389}")
        .kind(ReactionType::Multiple)
        .count(5)
        .await
        .unwrap();
}

#[tokio::test]
async fn send_is_not_retried_on_server_error() {
    let server = MockServer::start().await;
    // A reaction send carries no idempotency key, so a `multiple` reaction would
    // double-count on retry (ADR-0006). `expect(1)` proves the 503 is not retried
    // even though the default `max_retries` is 3.
    Mock::given(method("POST"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/reactions"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    let err = client
        .room("r")
        .messages()
        .reactions()
        .send("msg-1", "\u{1f44d}")
        .await
        .unwrap_err();
    assert_eq!(err.status(), Some(503));
}
