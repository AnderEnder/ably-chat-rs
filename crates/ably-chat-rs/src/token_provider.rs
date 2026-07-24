//! Server-side `TokenProvider` that mints Ably Tokens via the platform
//! `requestToken` endpoint (feature `token-issuance`). SERVER-SIDE ONLY: holds
//! the API secret. See ADR-0012 item 5 and SPEC §13.

use std::time::Duration;

use futures::future::BoxFuture;

use crate::config::TokenProvider;
use crate::error::{Error, Result};

/// Mints Ably Tokens by calling `POST /keys/{keyName}/requestToken` with an
/// unsigned `TokenParams` body under HTTP Basic auth. Pair with
/// [`Auth::provider`](crate::Auth::provider) for automatic use + refresh.
#[derive(Clone)]
pub struct KeyTokenProvider {
    name: String,
    secret: String,
    host: String,
    capability: Option<String>,
    client_id: Option<String>,
    ttl: Option<Duration>,
    http: reqwest::Client,
}

impl KeyTokenProvider {
    /// New provider from a full API key `appId.keyId:keySecret`.
    pub fn new(api_key: impl AsRef<str>) -> Result<Self> {
        let (name, secret) = crate::config::split_api_key(api_key.as_ref())?;
        Ok(Self {
            name: name.to_owned(),
            secret: secret.to_owned(),
            host: "https://rest.ably.io".to_owned(),
            capability: None,
            client_id: None,
            ttl: None,
            http: reqwest::Client::new(),
        })
    }
    /// Restrict issued tokens to this capability (a JSON string; with the
    /// `capabilities` feature, build it via `Capability::to_capability_string`).
    pub fn capability(mut self, cap: impl Into<String>) -> Self {
        self.capability = Some(cap.into());
        self
    }
    /// Bind issued tokens to a `clientId`.
    pub fn client_id(mut self, id: impl Into<String>) -> Self {
        self.client_id = Some(id.into());
        self
    }
    /// Requested token TTL (default: Ably's 60 minutes).
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }
    /// Override the platform host (defaults to `https://rest.ably.io`).
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into().trim_end_matches('/').to_owned();
        self
    }
    /// Supply a preconfigured `reqwest::Client`.
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http = client;
        self
    }
}

impl std::fmt::Debug for KeyTokenProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyTokenProvider")
            .field("key_name", &self.name)
            .field("key_secret", &"<redacted>")
            .field("host", &self.host)
            .field("client_id", &self.client_id)
            .finish()
    }
}

use ably_auth_openapi::apis::authentication_api;
use ably_auth_openapi::apis::configuration::Configuration;
use ably_auth_openapi::models::{RequestTokenRequest, TokenParams};

impl TokenProvider for KeyTokenProvider {
    fn token(&self) -> BoxFuture<'_, Result<String>> {
        Box::pin(async move {
            let mut cfg = Configuration::new();
            cfg.base_path = self.host.clone();
            cfg.client = self.http.clone();
            cfg.basic_auth = Some((self.name.clone(), Some(self.secret.clone())));

            let params = TokenParams {
                ttl: self.ttl.map(|d| d.as_millis() as i64),
                capability: self.capability.clone(),
                client_id: self.client_id.clone(),
            };
            let body = RequestTokenRequest::TokenParams(Box::new(params));

            // x-ably-version omitted (None): the platform API applies its default.
            match authentication_api::request_token(&cfg, &self.name, body, None).await {
                Ok(details) => Ok(details.token),
                Err(e) => Err(map_auth_error(e)),
            }
        })
    }
}

/// Maps an `ably-auth-openapi` error into this crate's `Error`.
fn map_auth_error(
    e: ably_auth_openapi::apis::Error<authentication_api::RequestTokenError>,
) -> Error {
    use ably_auth_openapi::apis::Error as AuthErr;
    match e {
        AuthErr::Reqwest(re) => Error::from(re), // -> Error::Transport
        AuthErr::ResponseError(rc) => {
            Error::from_api_body(rc.status.as_u16(), rc.content.as_bytes())
        }
        other => Error::Decode(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_redacts_secret() {
        let p = KeyTokenProvider::new("app.key:supersecret")
            .unwrap()
            .client_id("user-1")
            .ttl(Duration::from_secs(3600));
        let dbg = format!("{p:?}");
        assert!(
            !dbg.contains("supersecret"),
            "secret must be redacted: {dbg}"
        );
        assert!(dbg.contains("KeyTokenProvider"));
    }

    #[test]
    fn rejects_malformed_key() {
        assert!(KeyTokenProvider::new("no-colon").is_err());
    }

    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn mints_token_via_request_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/keys/app.key/requestToken"))
            // The body is the unsigned `TokenParams` shape and `ttl` is in
            // MILLISECONDS (1h -> 3_600_000), which is what Ably expects.
            .and(body_partial_json(serde_json::json!({"ttl": 3_600_000})))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"token":"tok-XYZ","keyName":"app.key"}"#,
                "application/json",
            ))
            .mount(&server)
            .await;
        let p = KeyTokenProvider::new("app.key:secret")
            .unwrap()
            .host(server.uri())
            .ttl(Duration::from_secs(3600));
        assert_eq!(p.token().await.unwrap(), "tok-XYZ");
    }

    #[tokio::test]
    async fn maps_request_token_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/keys/app.key/requestToken"))
            .respond_with(ResponseTemplate::new(401).set_body_string(
                r#"{"error":{"code":40100,"message":"bad key","statusCode":401}}"#,
            ))
            .mount(&server)
            .await;
        let p = KeyTokenProvider::new("app.key:secret")
            .unwrap()
            .host(server.uri());
        let err = p.token().await.unwrap_err();
        assert_eq!(err.status(), Some(401));
        // Status and body are mapped independently; pin the body half too.
        assert_eq!(err.info().unwrap().code, 40100);
    }
}
