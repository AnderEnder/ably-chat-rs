#![cfg(feature = "token-issuance")]

use std::sync::Arc;

use ably_chat::KeyTokenProvider;
use ably_chat::prelude::*;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn provider_supplies_bearer_to_chat_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/keys/app.key/requestToken"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"token":"tok-XYZ"}"#, "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/rooms/r/occupancy"))
        .and(header("authorization", "Bearer tok-XYZ"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"connections":1,"presenceMembers":0}"#),
        )
        .mount(&server)
        .await;

    let provider = Arc::new(
        KeyTokenProvider::new("app.key:secret")
            .unwrap()
            .host(server.uri()),
    );
    let client = Client::builder(Auth::provider(provider))
        .host(server.uri())
        .build();
    let occ = client.room("r").occupancy().get().await.unwrap();
    assert_eq!(occ.connections, 1);
}
