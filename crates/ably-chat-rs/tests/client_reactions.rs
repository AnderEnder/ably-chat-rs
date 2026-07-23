//! Integration tests for fetching a single client's reactions on a message.

use ably_chat::{Auth, Client};
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SUMMARY_BODY: &str = r#"{
    "unique": {"👍": {"total": 1, "clientIds": ["user-123"], "clipped": false}},
    "distinct": {},
    "multiple": {"🎉": {"total": 3, "clientIds": {"user-123": 3}, "totalUnidentified": 0}}
}"#;

#[tokio::test]
async fn client_reactions_forwards_for_client_id_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/client-reactions"))
        .and(header("x-ably-version", "4"))
        .and(query_param("forClientId", "user-123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SUMMARY_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let summary = client
        .room("r")
        .messages()
        .reactions()
        .for_client("msg-1")
        .client_id("user-123")
        .await
        .unwrap();
    assert_eq!(summary.unique["\u{1f44d}"].total, 1);
    assert_eq!(summary.unique["\u{1f44d}"].client_ids, vec!["user-123"]);
    assert_eq!(summary.multiple["\u{1f389}"].client_ids["user-123"], 3);
    assert!(summary.distinct.is_empty());
}

#[tokio::test]
async fn client_reactions_omits_query_when_unset() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/client-reactions"))
        .and(query_param_is_missing("forClientId"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SUMMARY_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let summary = client
        .room("r")
        .messages()
        .reactions()
        .for_client("msg-1")
        .await
        .unwrap();
    assert_eq!(summary.unique["\u{1f44d}"].total, 1);
}
