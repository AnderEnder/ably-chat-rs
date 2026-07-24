//! The single async HTTP dispatch layer (ADR-0003).
//!
//! Owns request construction, the version/auth headers, the retry-safety
//! predicate (ADR-0006), and error mapping. It returns the raw status,
//! response headers, and body bytes so higher layers can read the `Link`
//! headers that pagination needs (ADR-0009).

use reqwest::Method;
use reqwest::header::HeaderMap;

use crate::error::{Error, Result};

/// A raw HTTP response: status, headers (for pagination `Link`s), and body.
#[derive(Debug)]
pub(crate) struct RawResponse {
    /// The 2xx status code. Part of the raw-response contract and asserted in
    /// dispatch tests; typed operations only consume `headers`/`body` because
    /// non-2xx statuses are already mapped to [`Error`].
    #[allow(dead_code)]
    pub status: u16,
    pub headers: HeaderMap,
    pub body: bytes::Bytes,
}

/// Decodes a JSON response body into `T`, mapping any deserialization failure
/// to [`Error::Decode`]. Shared by every operation that returns a typed value.
pub(crate) fn decode_json<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|e| Error::Decode(e.to_string()))
}

/// Builds a room-scoped request path with the room name URL-encoded.
///
/// `suffix` is appended verbatim after `/chat/v4/rooms/{room}` (e.g.
/// `"/occupancy"` or `"/messages"`).
pub(crate) fn room_path(room: &str, suffix: &str) -> String {
    format!("/chat/v4/rooms/{}{}", urlencoding::encode(room), suffix)
}

/// Builds a message-scoped request path with the room name and serial
/// URL-encoded. `suffix` is appended after `.../messages/{serial}`.
pub(crate) fn message_path(room: &str, serial: &str, suffix: &str) -> String {
    format!(
        "/chat/v4/rooms/{}/messages/{}{}",
        urlencoding::encode(room),
        urlencoding::encode(serial),
        suffix
    )
}

impl crate::client::Inner {
    /// A request is retry-eligible only if it is inherently idempotent
    /// (`GET`/`DELETE`) or carries an idempotency key (ADR-0006).
    fn retry_eligible(method: &Method, has_idempotency_key: bool) -> bool {
        matches!(*method, Method::GET | Method::DELETE) || has_idempotency_key
    }

    /// Sends a request to `path` (relative to the base host), retrying only
    /// retry-eligible requests on transient failures up to `max_retries`.
    ///
    /// Returns the raw response on any 2xx status and maps every non-2xx status
    /// to [`Error::Api`].
    pub(crate) async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<serde_json::Value>,
        has_idem: bool,
    ) -> Result<RawResponse> {
        let url = format!("{}{}", self.base, path);
        self.send_url(method, url, query, body, has_idem).await
    }

    /// Like [`send`](Self::send) but takes an already-built absolute `url`.
    ///
    /// Used to follow RFC 5988 `next` pagination links (ADR-0009), which are
    /// full URLs rather than base-relative paths.
    pub(crate) async fn send_url(
        &self,
        method: Method,
        url: String,
        query: &[(&str, String)],
        body: Option<serde_json::Value>,
        has_idem: bool,
    ) -> Result<RawResponse> {
        let eligible = Self::retry_eligible(&method, has_idem);
        let mut attempt = 0;
        let mut auth_refreshed = false;
        loop {
            let auth = self.auth_header(None).await?;
            let mut req = self
                .http
                .request(method.clone(), &url)
                .header("X-Ably-Version", "4")
                .header(reqwest::header::AUTHORIZATION, &auth);
            if !query.is_empty() {
                req = req.query(query);
            }
            if let Some(b) = &body {
                req = req.json(b);
            }

            match req.send().await {
                Ok(r) => {
                    let status = r.status().as_u16();
                    // TODO(ADR-0006): honor `Retry-After` here (needs a
                    // runtime-agnostic timer); currently retries immediately.
                    if (status == 429 || (500..=599).contains(&status))
                        && eligible
                        && attempt < self.max_retries
                    {
                        attempt += 1;
                        continue;
                    }
                    let headers = r.headers().clone();
                    let bytes = r.bytes().await?;
                    if (200..300).contains(&status) {
                        return Ok(RawResponse {
                            status,
                            headers,
                            body: bytes,
                        });
                    }
                    let err = Error::from_api_body(status, &bytes);
                    // Provider-backed token error: refresh once and retry the
                    // request once (spec RSA4b — exactly one extra attempt).
                    // Deliberately NOT gated on `eligible` (ADR-0006): a 401
                    // means the request was rejected by the auth layer before
                    // it could have any side effect, so retrying it — even a
                    // non-idempotent POST — is always safe.
                    if !auth_refreshed
                        && err.is_token_error()
                        && matches!(&self.auth, crate::client::AuthState::Provider { .. })
                    {
                        auth_refreshed = true;
                        // Refresh only if the cache still holds the value that
                        // was just rejected; propagate provider errors.
                        self.auth_header(Some(auth.as_str())).await?;
                        continue;
                    }
                    return Err(err);
                }
                Err(e) => {
                    if e.is_timeout() && eligible && attempt < self.max_retries {
                        attempt += 1;
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{Client, Inner};
    use crate::config::Auth;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn path_helpers_url_encode_segments() {
        assert_eq!(
            room_path("my room", "/occupancy"),
            "/chat/v4/rooms/my%20room/occupancy"
        );
        // Serials carry `@` and `:`, which MUST be percent-encoded in the path.
        assert_eq!(
            message_path("r", "01ts-001@abc:001", "/versions"),
            "/chat/v4/rooms/r/messages/01ts-001%40abc%3A001/versions"
        );
    }

    #[test]
    fn retry_eligible_predicate() {
        assert!(Inner::retry_eligible(&Method::GET, false));
        assert!(Inner::retry_eligible(&Method::DELETE, false));
        assert!(!Inner::retry_eligible(&Method::POST, false));
        assert!(Inner::retry_eligible(&Method::POST, true));
        assert!(!Inner::retry_eligible(&Method::PUT, false));
        assert!(Inner::retry_eligible(&Method::PUT, true));
    }

    #[tokio::test]
    async fn sends_version_and_auth_headers_and_maps_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/occupancy"))
            .and(header("x-ably-version", "4"))
            .and(header("authorization", "Basic YXBwLms6cw=="))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_string(r#"{"error":{"code":40400,"message":"no","statusCode":404}}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = Client::builder(Auth::api_key("app.k:s"))
            .host(server.uri())
            .build();
        let err = client
            .inner
            .send(Method::GET, "/chat/v4/rooms/r/occupancy", &[], None, false)
            .await
            .unwrap_err();
        assert!(err.is_not_found());
        assert_eq!(err.status(), Some(404));
    }

    #[tokio::test]
    async fn success_returns_status_and_body_bytes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/occupancy"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"connections":3,"presenceMembers":2}"#),
            )
            .mount(&server)
            .await;

        let client = Client::builder(Auth::api_key("k:s"))
            .host(server.uri())
            .build();
        let r = client
            .inner
            .send(Method::GET, "/chat/v4/rooms/r/occupancy", &[], None, false)
            .await
            .unwrap();
        assert_eq!(r.status, 200);
        let occ: crate::types::Occupancy = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(occ.connections, 3);
        assert_eq!(occ.presence_members, 2);
    }

    #[tokio::test]
    async fn provider_refreshes_once_on_token_error_then_succeeds() {
        use crate::config::{Auth, TokenProvider};
        use futures::future::BoxFuture;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let server = MockServer::start().await;
        // First call (Bearer stale) → 401 token error; second (Bearer fresh) → 200.
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/occupancy"))
            .and(header("authorization", "Bearer stale"))
            .respond_with(ResponseTemplate::new(401).set_body_string(
                r#"{"error":{"code":40142,"message":"expired","statusCode":401}}"#,
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/occupancy"))
            .and(header("authorization", "Bearer fresh"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"connections":1,"presenceMembers":0}"#),
            )
            .mount(&server)
            .await;

        struct Rotating(Arc<AtomicUsize>);
        impl TokenProvider for Rotating {
            fn token(&self) -> BoxFuture<'_, crate::error::Result<String>> {
                let n = self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { Ok(if n == 0 { "stale" } else { "fresh" }.to_string()) })
            }
        }
        let client = Client::builder(Auth::provider(Arc::new(Rotating(Arc::new(
            AtomicUsize::new(0),
        )))))
        .host(server.uri())
        .build();
        let r = client
            .inner
            .send(Method::GET, "/chat/v4/rooms/r/occupancy", &[], None, false)
            .await;
        assert!(r.is_ok(), "should succeed after one refresh: {r:?}");
    }

    #[tokio::test]
    async fn provider_does_not_loop_on_persistent_token_error() {
        use crate::config::{Auth, TokenProvider};
        use futures::future::BoxFuture;
        use std::sync::Arc;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/occupancy"))
            .respond_with(ResponseTemplate::new(401).set_body_string(
                r#"{"error":{"code":40142,"message":"expired","statusCode":401}}"#,
            ))
            .expect(2) // original + exactly one retry
            .mount(&server)
            .await;

        struct Always;
        impl TokenProvider for Always {
            fn token(&self) -> BoxFuture<'_, crate::error::Result<String>> {
                Box::pin(async { Ok("t".to_string()) })
            }
        }
        let client = Client::builder(Auth::provider(Arc::new(Always)))
            .host(server.uri())
            .build();
        let err = client
            .inner
            .send(Method::GET, "/chat/v4/rooms/r/occupancy", &[], None, false)
            .await
            .unwrap_err();
        assert_eq!(err.status(), Some(401));
        assert!(err.is_token_error());
    }

    #[tokio::test]
    async fn static_auth_does_not_retry_on_token_error() {
        use crate::config::Auth;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chat/v4/rooms/r/occupancy"))
            .respond_with(ResponseTemplate::new(401).set_body_string(
                r#"{"error":{"code":40142,"message":"expired","statusCode":401}}"#,
            ))
            .expect(1) // exactly one hit: static auth must not retry
            .mount(&server)
            .await;

        let client = Client::builder(Auth::api_key("app.k:s"))
            .host(server.uri())
            .build();
        let err = client
            .inner
            .send(Method::GET, "/chat/v4/rooms/r/occupancy", &[], None, false)
            .await
            .unwrap_err();
        assert_eq!(err.status(), Some(401));
        assert!(err.is_token_error());
    }
}
