//! The crate's single public error type and its `Result` alias (ADR-0008).

use serde::Deserialize;

/// The result type returned by every fallible operation in this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// The Ably error envelope carried by a non-2xx API response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    /// Ably-specific error code (e.g. `40400` for message-not-found).
    pub code: i64,
    /// Human-readable error message.
    #[serde(default)]
    pub message: String,
    /// The HTTP status code reported inside the envelope.
    #[serde(default)]
    pub status_code: u16,
    /// URL to Ably documentation for this error code, if provided.
    #[serde(default)]
    pub href: Option<String>,
}

/// The single error type surfaced by this crate.
///
/// `#[non_exhaustive]` so new variants can be added without a breaking change.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// An HTTP transport failure from `reqwest` (connection, timeout, TLS, ...).
    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),
    /// The response body could not be decoded into the expected type.
    #[error("failed to decode response: {0}")]
    Decode(String),
    /// A request was rejected by client-side validation before being sent
    /// (e.g. a `distinct`/`multiple` reaction delete missing its `name`).
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// A non-2xx response carrying the Ably error envelope.
    #[error("Ably API error {}: {message}", .info.code, message = .info.message)]
    Api {
        /// The HTTP status code of the response.
        status: u16,
        /// The parsed Ably error envelope.
        info: ErrorInfo,
    },
}

#[derive(Deserialize)]
struct Envelope {
    error: ErrorInfo,
}

impl Error {
    /// Builds an [`Error::Api`] from a non-2xx status and response body,
    /// parsing the Ably envelope when present and otherwise preserving the raw
    /// body as the message.
    pub(crate) fn from_api_body(status: u16, body: &[u8]) -> Self {
        let info = serde_json::from_slice::<Envelope>(body)
            .map(|e| e.error)
            .unwrap_or_else(|_| ErrorInfo {
                code: 0,
                message: String::from_utf8_lossy(body).into_owned(),
                status_code: status,
                href: None,
            });
        Error::Api { status, info }
    }

    /// The HTTP status code associated with this error, if any.
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Api { status, .. } => Some(*status),
            Error::Transport(e) => e.status().map(|s| s.as_u16()),
            Error::Decode(_) | Error::InvalidRequest(_) => None,
        }
    }

    /// The parsed Ably error envelope, if this is an API error.
    pub fn info(&self) -> Option<&ErrorInfo> {
        match self {
            Error::Api { info, .. } => Some(info),
            _ => None,
        }
    }

    /// Whether retrying the request that produced this error may succeed.
    ///
    /// Transport timeouts/connect failures, HTTP `429`, and `5xx` are retryable;
    /// decode failures and other API errors are not. The dispatch layer only
    /// actually retries requests that are also idempotency-safe (ADR-0006).
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Transport(e) => e.is_timeout() || e.is_connect(),
            Error::Api { status, .. } => *status == 429 || (500..=599).contains(status),
            Error::Decode(_) | Error::InvalidRequest(_) => false,
        }
    }

    /// Ably code `40400`: the message was not found.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::Api { info, .. } if info.code == 40400)
    }

    /// Ably code `42211`: the operation was rejected by a room rule.
    pub fn is_rejected_by_rule(&self) -> bool {
        matches!(self, Error::Api { info, .. } if info.code == 42211)
    }

    /// Ably code `42213`: the operation was rejected by moderation.
    pub fn is_rejected_by_moderation(&self) -> bool {
        matches!(self, Error::Api { info, .. } if info.code == 42213)
    }

    /// Whether this is an Ably token error (HTTP `401`, code in `[40140, 40150)`)
    /// that a configured `TokenProvider` should renew on (spec RSA4b).
    pub fn is_token_error(&self) -> bool {
        matches!(self, Error::Api { status: 401, info } if (40140..40150).contains(&info.code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ably_envelope() {
        let body = r#"{"error":{"code":40400,"message":"not found","statusCode":404}}"#;
        let e = Error::from_api_body(404, body.as_bytes());
        assert_eq!(e.status(), Some(404));
        assert!(e.is_not_found());
        assert!(!e.is_retryable());
    }

    #[test]
    fn server_error_is_retryable() {
        let e = Error::from_api_body(503, b"{}");
        assert!(e.is_retryable());
    }

    #[test]
    fn rate_limit_is_retryable() {
        let e = Error::from_api_body(429, b"{}");
        assert!(e.is_retryable());
    }

    #[test]
    fn non_json_body_is_preserved_as_message() {
        let e = Error::from_api_body(500, b"upstream boom");
        assert_eq!(e.status(), Some(500));
        assert!(e.is_retryable());
        assert_eq!(e.info().map(|i| i.message.as_str()), Some("upstream boom"));
    }

    #[test]
    fn token_error_range() {
        // 401 + code in [40140,40150) -> token error (renewable).
        for code in [40140, 40141, 40142, 40143, 40149] {
            let body = format!(r#"{{"error":{{"code":{code},"message":"x","statusCode":401}}}}"#);
            assert!(
                Error::from_api_body(401, body.as_bytes()).is_token_error(),
                "code {code}"
            );
        }
        // Not a token error: wrong status, or code outside the range.
        assert!(
            !Error::from_api_body(403, br#"{"error":{"code":40140,"statusCode":403}}"#)
                .is_token_error()
        );
        assert!(
            !Error::from_api_body(401, br#"{"error":{"code":40150,"statusCode":401}}"#)
                .is_token_error()
        );
        assert!(
            !Error::from_api_body(401, br#"{"error":{"code":40400,"statusCode":401}}"#)
                .is_token_error()
        );
    }

    #[test]
    fn code_predicates() {
        assert!(
            Error::from_api_body(
                422,
                br#"{"error":{"code":42211,"message":"x","statusCode":422}}"#
            )
            .is_rejected_by_rule()
        );
        assert!(
            Error::from_api_body(
                422,
                br#"{"error":{"code":42213,"message":"x","statusCode":422}}"#
            )
            .is_rejected_by_moderation()
        );
    }
}
