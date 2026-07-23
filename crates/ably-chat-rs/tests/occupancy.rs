//! Integration tests for the occupancy read operation. The seam is the HTTP
//! boundary: a `wiremock` server backs the client and we drive the real public
//! API (ADR-0003).

use ably_chat::{Auth, Client};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn get_occupancy_returns_typed_metrics() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/my-room/occupancy"))
        .and(header("x-ably-version", "4"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"connections":3,"presenceMembers":2}"#),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let occ = client.room("my-room").occupancy().get().await.unwrap();
    assert_eq!(occ.connections, 3);
    assert_eq!(occ.presence_members, 2);
}

#[tokio::test]
async fn get_occupancy_maps_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/occupancy"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_string(r#"{"error":{"code":40400,"message":"no","statusCode":404}}"#),
        )
        .mount(&server)
        .await;

    let client = Client::builder(Auth::api_key("app.k:s"))
        .host(server.uri())
        .build();
    let err = client.room("r").occupancy().get().await.unwrap_err();
    assert!(err.is_not_found());
    assert_eq!(err.status(), Some(404));
}
