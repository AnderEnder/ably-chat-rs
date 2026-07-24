//! The `Client` handle, its builder, and the shared inner state (ADR-0003).

use std::sync::Arc;

use futures::lock::Mutex;

use crate::config::{Auth, TokenProvider};
use crate::error::Result;

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
    /// Resolved auth: a fixed header, or a provider + cached header.
    pub(crate) auth: AuthState,
    pub(crate) max_retries: u32,
}

/// Resolved auth: a fixed header for static creds, or a provider + cached header.
pub(crate) enum AuthState {
    Static(String),
    Provider {
        provider: Arc<dyn TokenProvider>,
        cache: Mutex<Option<String>>,
    },
}

impl Inner {
    /// The `Authorization` header value. For a provider, returns the cached
    /// header. The provider is called only when nothing is cached yet, or
    /// when `stale` names the exact header value that a caller just had
    /// rejected: if another task already refreshed the cache since then, its
    /// value is reused instead of minting a redundant token.
    ///
    /// `stale`: `None` for a normal request (use whatever is cached, or fetch
    /// if empty); `Some(header)` when retrying after `header` was rejected as
    /// a token error.
    pub(crate) async fn auth_header(&self, stale: Option<&str>) -> Result<String> {
        match &self.auth {
            AuthState::Static(h) => Ok(h.clone()),
            AuthState::Provider { provider, cache } => {
                // The mutex is deliberately held across `provider.token()`
                // below (not just this cache read): that is what makes
                // concurrent refreshes single-flighted. The first caller to
                // take the lock fetches and caches; every other caller that
                // arrives while the fetch is in flight blocks here, then
                // observes the freshly cached value and reuses it. The guard
                // is released long before the outbound HTTP send, which
                // happens in `dispatch::send_url` after this returns.
                let mut guard = cache.lock().await;
                if let Some(cached) = guard.as_ref()
                    && stale != Some(cached.as_str())
                {
                    return Ok(cached.clone());
                }
                let header = format!("Bearer {}", provider.token().await?);
                *guard = Some(header.clone());
                Ok(header)
            }
        }
    }
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
        let auth = match self.auth {
            Auth::Provider(p) => AuthState::Provider {
                provider: p,
                cache: Mutex::new(None),
            },
            a @ (Auth::ApiKey(_) | Auth::Token(_)) => AuthState::Static(a.header_value()),
        };
        Client {
            inner: Arc::new(Inner {
                http,
                base: self.host.trim_end_matches('/').to_string(),
                auth,
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

    #[tokio::test]
    async fn provider_auth_header_resolves_and_caches() {
        use crate::config::TokenProvider;
        use futures::future::BoxFuture;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Counting(Arc<AtomicUsize>);
        impl TokenProvider for Counting {
            fn token(&self) -> BoxFuture<'_, crate::error::Result<String>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok("tok-1".to_string()) })
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let client = Client::builder(crate::config::Auth::provider(Arc::new(Counting(
            calls.clone(),
        ))))
        .build();

        let h1 = client.inner.auth_header(None).await.unwrap();
        let h2 = client.inner.auth_header(None).await.unwrap();
        assert_eq!(h1, "Bearer tok-1");
        assert_eq!(h2, "Bearer tok-1");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "second call served from cache"
        );

        // forced refresh of the (still-current) cached value re-fetches
        client
            .inner
            .auth_header(Some("Bearer tok-1"))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn forced_refresh_is_single_flighted() {
        use crate::config::TokenProvider;
        use futures::future::BoxFuture;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Rotating(Arc<AtomicUsize>);
        impl TokenProvider for Rotating {
            fn token(&self) -> BoxFuture<'_, crate::error::Result<String>> {
                let n = self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { Ok(format!("t{}", n + 1)) })
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let client = Client::builder(crate::config::Auth::provider(Arc::new(Rotating(
            calls.clone(),
        ))))
        .build();

        let first = client.inner.auth_header(None).await.unwrap();
        assert_eq!(first, "Bearer t1");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Two requests concurrently discover the same stale token: exactly ONE refresh.
        let (a, b) = futures::join!(
            client.inner.auth_header(Some("Bearer t1")),
            client.inner.auth_header(Some("Bearer t1")),
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "second caller must reuse the refreshed token"
        );
        assert_eq!(a, "Bearer t2");
        assert_eq!(b, "Bearer t2");
    }
}
