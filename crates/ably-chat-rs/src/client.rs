//! The `Client` handle, its builder, and the shared inner state (ADR-0003).

use std::sync::Arc;

use crate::config::Auth;

/// The entry point to the Ably Chat REST API.
///
/// Cheap to `Clone` (`Arc`-backed) and `Send + Sync`. Its `Debug`
/// representation never prints credentials.
#[derive(Clone)]
pub struct Client {
    pub(crate) inner: Arc<Inner>,
}

/// Shared, immutable client state behind an `Arc`.
pub(crate) struct Inner {
    pub(crate) http: reqwest::Client,
    /// Base host with any trailing slash trimmed, e.g. `https://rest.ably.io`.
    pub(crate) base: String,
    /// Prebuilt `Authorization` header value.
    pub(crate) auth_header: String,
    pub(crate) max_retries: u32,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base", &self.inner.base)
            .field("auth", &"<redacted>")
            .finish()
    }
}

impl Client {
    /// Starts building a client with the given credentials.
    pub fn builder(auth: Auth) -> ClientBuilder {
        ClientBuilder::new(auth)
    }

    /// Returns a handle to the named chat room.
    ///
    /// Rooms are implicit: this neither creates nor deletes anything
    /// server-side.
    pub fn room(&self, name: impl Into<crate::types::RoomName>) -> crate::room::Room {
        crate::room::Room::new(self.clone(), name.into())
    }
}

/// Builder for [`Client`]. Requires credentials; host, timeout, retry budget,
/// and a caller-supplied `reqwest::Client` are optional.
pub struct ClientBuilder {
    auth: Auth,
    host: String,
    http: Option<reqwest::Client>,
    timeout: Option<std::time::Duration>,
    max_retries: u32,
}

impl ClientBuilder {
    /// Creates a builder with the default host and retry budget.
    pub fn new(auth: Auth) -> Self {
        Self {
            auth,
            host: "https://rest.ably.io".into(),
            http: None,
            timeout: None,
            max_retries: 3,
        }
    }

    /// Overrides the base host (e.g. to point at a test server).
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Supplies a preconfigured `reqwest::Client` (overrides `timeout`).
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http = Some(client);
        self
    }

    /// Sets the per-request timeout used when this builder constructs the
    /// `reqwest::Client` itself.
    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Sets the maximum number of retries for retry-eligible requests
    /// (ADR-0006). Defaults to `3`.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    /// Builds the [`Client`].
    ///
    /// # Panics
    ///
    /// Panics only if the underlying `reqwest::Client` cannot be constructed
    /// (e.g. the platform TLS backend fails to initialise).
    pub fn build(self) -> Client {
        let http = self.http.unwrap_or_else(|| {
            let mut b = reqwest::Client::builder();
            if let Some(t) = self.timeout {
                b = b.timeout(t);
            }
            b.build().expect("failed to build reqwest client")
        });
        Client {
            inner: Arc::new(Inner {
                http,
                base: self.host.trim_end_matches('/').to_string(),
                auth_header: self.auth.header_value(),
                max_retries: self.max_retries,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_debug_redacts_credentials() {
        let client = Client::builder(Auth::api_key("app.key:supersecret"))
            .host("https://example.test")
            .build();
        let dbg = format!("{client:?}");
        assert!(!dbg.contains("supersecret"));
        assert!(!dbg.contains("YXBw")); // no base64 of the key either
        assert!(dbg.contains("https://example.test"));
    }

    #[test]
    fn host_trailing_slash_is_trimmed() {
        let client = Client::builder(Auth::token("t"))
            .host("https://example.test/")
            .build();
        assert_eq!(client.inner.base, "https://example.test");
    }
}
