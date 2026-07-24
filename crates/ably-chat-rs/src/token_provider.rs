//! Server-side `TokenProvider` that mints Ably Tokens via the platform
//! `requestToken` endpoint (feature `token-issuance`). SERVER-SIDE ONLY: holds
//! the API secret. See ADR-0012 item 5 and SPEC §13.

use std::time::Duration;

use crate::error::{Error, Result};

// `BoxFuture`/`TokenProvider` land with the `TokenProvider` impl in Task 4.3.

/// Mints Ably Tokens by calling `POST /keys/{keyName}/requestToken` with an
/// unsigned `TokenParams` body under HTTP Basic auth. Pair with
/// [`Auth::provider`](crate::Auth::provider) for automatic use + refresh.
#[derive(Clone)]
pub struct KeyTokenProvider {
    key_name: String,
    // Not read until the `TokenProvider` impl lands in Task 4.3.
    #[allow(dead_code)]
    key_secret: String,
    host: String,
    capability: Option<String>,
    client_id: Option<String>,
    ttl: Option<Duration>,
    http: reqwest::Client,
}

impl KeyTokenProvider {
    /// New provider from a full API key `appId.keyId:keySecret`.
    pub fn new(api_key: impl AsRef<str>) -> Result<Self> {
        let s = api_key.as_ref();
        let (name, secret) = s
            .split_once(':')
            .ok_or_else(|| Error::InvalidRequest("API key must be `keyName:keySecret`".into()))?;
        if name.is_empty() || secret.is_empty() {
            return Err(Error::InvalidRequest(
                "API key name and secret must be non-empty".into(),
            ));
        }
        Ok(Self {
            key_name: name.to_owned(),
            key_secret: secret.to_owned(),
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
            .field("key_name", &self.key_name)
            .field("key_secret", &"<redacted>")
            .field("host", &self.host)
            .field("client_id", &self.client_id)
            .finish()
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
}
