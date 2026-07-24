//! Static credentials for the client (ADR-0005).

use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD};
use futures::future::BoxFuture;

use crate::error::Result;

/// Supplies a currently-valid Bearer credential (Ably Token string or Ably JWT),
/// refreshed on demand. The returned string is the raw token — the client adds
/// the `Bearer ` prefix. Implementations MUST be cheap to call when cached.
///
/// An implementation MUST NOT call back into the same [`Client`](crate::Client)
/// it authenticates in order to obtain its token: the client's cache mutex is
/// non-reentrant and is held across this call, so doing so would deadlock.
pub trait TokenProvider: Send + Sync {
    /// Fetch a currently-valid token.
    fn token(&self) -> BoxFuture<'_, Result<String>>;
}

/// Static credentials supplied to the client.
///
/// Ably accepts HTTP **Basic** auth with an API key (`keyName:keySecret`) or
/// **Bearer** auth with an Ably Token/JWT. `Auth::Provider` refreshes on
/// demand via a caller-supplied [`TokenProvider`] (ADR-0005).
#[derive(Clone)]
#[non_exhaustive]
pub enum Auth {
    /// An Ably API key, `keyName:keySecret`, sent as HTTP Basic.
    ApiKey(String),
    /// An Ably Token/JWT, sent as a Bearer token.
    Token(String),
    /// A caller-supplied provider that yields (and refreshes) Bearer credentials.
    Provider(Arc<dyn TokenProvider>),
}

impl Auth {
    /// Constructs API-key (HTTP Basic) credentials from a `keyName:keySecret`.
    pub fn api_key(key: impl Into<String>) -> Self {
        Auth::ApiKey(key.into())
    }

    /// Constructs Bearer-token credentials from an Ably Token/JWT.
    pub fn token(token: impl Into<String>) -> Self {
        Auth::Token(token.into())
    }

    /// Constructs credentials backed by a refreshing [`TokenProvider`].
    pub fn provider(p: Arc<dyn TokenProvider>) -> Self {
        Auth::Provider(p)
    }

    /// Renders the value for the `Authorization` header.
    pub(crate) fn header_value(&self) -> String {
        match self {
            Auth::ApiKey(k) => format!("Basic {}", STANDARD.encode(k.as_bytes())),
            Auth::Token(t) => format!("Bearer {t}"),
            Auth::Provider(_) => unreachable!(
                "Auth::Provider carries no static header; see AuthState::Provider in client.rs"
            ),
        }
    }
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::ApiKey(_) => f.write_str("Auth::ApiKey(<redacted>)"),
            Auth::Token(_) => f.write_str("Auth::Token(<redacted>)"),
            Auth::Provider(_) => f.write_str("Auth::Provider(<dyn TokenProvider>)"),
        }
    }
}

/// Splits a full Ably API key `appId.keyId:keySecret` into its name and secret.
///
/// Shared by the `jwt` and `token-issuance` features, which both need the halves
/// separately (the key name serves as the JWT `kid` / HTTP Basic username, the
/// secret as the signing key / Basic password).
#[cfg(any(feature = "jwt", feature = "token-issuance"))]
pub(crate) fn split_api_key(api_key: &str) -> crate::error::Result<(&str, &str)> {
    let (name, secret) = api_key.split_once(':').ok_or_else(|| {
        crate::error::Error::InvalidRequest("API key must be `keyName:keySecret`".into())
    })?;
    if name.is_empty() || secret.is_empty() {
        return Err(crate::error::Error::InvalidRequest(
            "API key name and secret must be non-empty".into(),
        ));
    }
    Ok((name, secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_becomes_basic_header() {
        let a = Auth::api_key("app.key:secret");
        assert_eq!(a.header_value(), "Basic YXBwLmtleTpzZWNyZXQ=");
    }

    #[test]
    fn token_becomes_bearer_header() {
        assert_eq!(Auth::token("tok123").header_value(), "Bearer tok123");
    }

    #[test]
    fn debug_redacts_credentials() {
        let dbg = format!("{:?}", Auth::api_key("app.key:secret"));
        assert!(!dbg.contains("secret"));
        let dbg = format!("{:?}", Auth::token("tok123"));
        assert!(!dbg.contains("tok123"));
    }

    #[test]
    fn provider_debug_redacts() {
        use futures::future::BoxFuture;
        use std::sync::Arc;

        struct P;
        impl crate::config::TokenProvider for P {
            fn token(&self) -> BoxFuture<'_, crate::error::Result<String>> {
                Box::pin(async { Ok("jwt-abc".to_string()) })
            }
        }
        let a = Auth::provider(Arc::new(P));
        let dbg = format!("{a:?}");
        assert!(dbg.contains("Provider"));
        assert!(!dbg.contains("jwt-abc"));
    }
}
