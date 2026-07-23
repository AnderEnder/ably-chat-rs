//! Integration tests for deleting a message reaction (`DELETE /reactions`).

use ably_chat::{Auth, Client, Error, ReactionType};
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn delete_unique_sends_type_query_without_name() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/reactions"))
        .and(header("x-ably-version", "4"))
        .and(query_param("type", "unique"))
        .and(query_param_is_missing("name"))
        .respond_with(ResponseTemplate::new(204))
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
        .delete("msg-1")
        .kind(ReactionType::Unique)
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_distinct_sends_type_and_name_queries() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/reactions"))
        .and(query_param("type", "distinct"))
        .and(query_param("name", "\u{1f44d}"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    // `distinct` is the default kind, so `.kind(..)` is omitted here.
    client
        .room("r")
        .messages()
        .reactions()
        .delete("msg-1")
        .name("\u{1f44d}")
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_distinct_without_name_fails_before_any_request() {
    let server = MockServer::start().await;
    // If the client-side rule fails to short-circuit, this would receive a
    // request; `expect(0)` proves nothing was sent.
    Mock::given(method("DELETE"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/reactions"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    let err = client
        .room("r")
        .messages()
        .reactions()
        .delete("msg-1")
        .kind(ReactionType::Distinct)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidRequest(_)));
    // A client-side rejection has no HTTP status (it never reached the server).
    assert_eq!(err.status(), None);
}
