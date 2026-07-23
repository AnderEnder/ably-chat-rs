//! Integration tests for paginated message history and versions.

use ably_chat::{Auth, Client, Direction};
use futures::StreamExt;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn message_json(serial: &str) -> String {
    format!(
        r#"{{"serial":"{serial}","version":{{"serial":"{serial}","timestamp":1}},
           "text":"t","clientId":"a","action":"message.create",
           "metadata":{{}},"headers":{{}},"timestamp":1}}"#
    )
}

#[tokio::test]
async fn history_defaults_to_backwards_and_limit_100() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/messages"))
        .and(query_param("direction", "backwards"))
        .and(query_param("limit", "100"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(format!("[{}]", message_json("m1"))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    let page = client.room("r").messages().history().await.unwrap();
    assert_eq!(page.items().len(), 1);
    assert_eq!(page.items()[0].serial.as_str(), "m1");
}

#[tokio::test]
async fn history_forwards_all_filters_into_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/messages"))
        .and(query_param("start", "1700000000000"))
        .and(query_param("end", "1700003600000"))
        .and(query_param("direction", "forwards"))
        .and(query_param("limit", "50"))
        .and(query_param("fromSerial", "01ts@abc:001"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    let page = client
        .room("r")
        .messages()
        .history()
        .start(1_700_000_000_000_i64)
        .end(1_700_003_600_000_i64)
        .direction(Direction::Forwards)
        .limit(50)
        .from_serial("01ts@abc:001")
        .await
        .unwrap();
    assert!(page.items().is_empty());
}

#[tokio::test]
async fn history_streams_across_two_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/messages"))
        .and(query_param_is_missing("cont"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("Link", "</chat/v4/rooms/r/messages?cont=2>; rel=\"next\"")
                .set_body_string(format!("[{}]", message_json("m1"))),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/messages"))
        .and(query_param("cont", "2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(format!("[{}]", message_json("m2"))),
        )
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    let serials: Vec<String> = client
        .room("r")
        .messages()
        .history()
        .into_stream()
        .map(|r| r.unwrap().serial.as_str().to_owned())
        .collect()
        .await;
    assert_eq!(serials, vec!["m1", "m2"]);
}

#[tokio::test]
async fn versions_returns_a_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/messages/msg-1/versions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "[{},{}]",
            message_json("msg-1"),
            message_json("msg-1")
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("k:s"))
        .host(server.uri())
        .build();
    let page = client.room("r").messages().versions("msg-1").await.unwrap();
    assert_eq!(page.items().len(), 2);
    assert!(!page.has_next());
}
