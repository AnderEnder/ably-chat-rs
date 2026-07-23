//! Static credentials for the client (ADR-0005).

use base64::{Engine, engine::general_purpose::STANDARD};

/// Static credentials supplied to the client.
///
/// Ably accepts HTTP **Basic** auth with an API key (`keyName:keySecret`) or
/// **Bearer** auth with an Ably Token/JWT. Static only in 0.x — no automatic
/// token refresh (ADR-0005).
#[derive(Clone)]
pub enum Auth {
    /// An Ably API key, `keyName:keySecret`, sent as HTTP Basic.
    ApiKey(String),
    /// An Ably Token/JWT, sent as a Bearer token.
    Token(String),
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

    /// Renders the value for the `Authorization` header.
    pub(crate) fn header_value(&self) -> String {
        match self {
            Auth::ApiKey(k) => format!("Basic {}", STANDARD.encode(k.as_bytes())),
            Auth::Token(t) => format!("Bearer {t}"),
        }
    }
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::ApiKey(_) => f.write_str("Auth::ApiKey(<redacted>)"),
            Auth::Token(_) => f.write_str("Auth::Token(<redacted>)"),
        }
    }
}

// TODO(ADR-0005): reserve `Auth::Provider(Arc<dyn TokenProvider>)` here later
// for token auto-refresh. `#[non_exhaustive]` is intentionally NOT added yet so
// callers can match exhaustively in 0.x; revisit before adding the variant.

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
}
