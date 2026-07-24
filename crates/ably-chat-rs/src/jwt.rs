//! Ably JWT minting (ADR-0012, SPEC §13.2). Feature `jwt`. SERVER-SIDE ONLY:
//! this carries your API secret and must never run in client-shipped code.

use crate::error::{Error, Result};

/// An Ably API key split into its name (`appId.keyId`) and secret, for signing
/// Ably JWTs. `Debug` redacts the secret.
#[derive(Clone)]
pub struct SigningKey {
    name: String,
    secret: String,
}

impl SigningKey {
    /// Parse a full API key `appId.keyId:keySecret`.
    pub fn new(api_key: impl AsRef<str>) -> Result<Self> {
        let (name, secret) = crate::config::split_api_key(api_key.as_ref())?;
        Ok(Self {
            name: name.to_owned(),
            secret: secret.to_owned(),
        })
    }

    /// The key name (`appId.keyId`), used as the JWT `kid`.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningKey")
            .field("name", &self.name)
            .field("secret", &"<redacted>")
            .finish()
    }
}

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Inputs for minting an Ably JWT. `capability` is the capability JSON **string**
/// (with the `capabilities` feature, produce it via `Capability::to_capability_string`).
pub struct TokenParams {
    capability: String,
    client_id: Option<String>,
    revocation_key: Option<String>,
    ttl: Duration,
}

impl TokenParams {
    /// New params with the given capability JSON string and a default 1-hour TTL.
    pub fn new(capability: impl Into<String>) -> Self {
        Self {
            capability: capability.into(),
            client_id: None,
            revocation_key: None,
            ttl: Duration::from_secs(3600),
        }
    }
    /// Bind the token to a `clientId` (`x-ably-clientId`).
    pub fn client_id(mut self, id: impl Into<String>) -> Self {
        self.client_id = Some(id.into());
        self
    }
    /// Set an `x-ably-revocation-key` so the JWT can be revoked by that key.
    pub fn revocation_key(mut self, key: impl Into<String>) -> Self {
        self.revocation_key = Some(key.into());
        self
    }
    /// Time-to-live (default 1 hour; keep ≤ 1 hour if the key has revocable tokens).
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }
}

#[derive(serde::Serialize)]
struct Claims<'a> {
    iat: i64,
    exp: i64,
    #[serde(rename = "x-ably-capability")]
    capability: &'a str,
    #[serde(rename = "x-ably-clientId", skip_serializing_if = "Option::is_none")]
    client_id: Option<&'a str>,
    #[serde(
        rename = "x-ably-revocation-key",
        skip_serializing_if = "Option::is_none"
    )]
    revocation_key: Option<&'a str>,
}

/// Mint an HS256 Ably JWT signed with the key secret. No network call.
///
/// SERVER-SIDE ONLY — never run where the API secret could reach a client.
pub fn mint_ably_jwt(key: &SigningKey, params: &TokenParams) -> Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| Error::InvalidRequest(format!("system clock before epoch: {e}")))?
        .as_secs() as i64;
    let claims = Claims {
        iat: now,
        exp: now + params.ttl.as_secs() as i64,
        capability: &params.capability,
        client_id: params.client_id.as_deref(),
        revocation_key: params.revocation_key.as_deref(),
    };
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
    header.kid = Some(key.name.clone());
    jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(key.secret.as_bytes()),
    )
    .map_err(|e| Error::InvalidRequest(format!("JWT signing failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use std::time::Duration;

    #[test]
    fn parses_and_redacts() {
        let k = SigningKey::new("app.keyid:supersecret").unwrap();
        assert_eq!(k.name(), "app.keyid");
        let dbg = format!("{k:?}");
        assert!(
            !dbg.contains("supersecret"),
            "secret must be redacted: {dbg}"
        );
    }

    #[test]
    fn rejects_malformed_key() {
        assert!(SigningKey::new("no-colon-here").is_err());
    }

    fn decode_part(part: &str) -> serde_json::Value {
        let bytes = URL_SAFE_NO_PAD.decode(part).expect("valid base64url");
        serde_json::from_slice(&bytes).expect("valid json")
    }

    #[test]
    fn mints_jwt_with_ably_header_and_claims() {
        let key = SigningKey::new("app.keyid:secret").unwrap();
        let params = TokenParams::new(r#"{"sports":["history","publish"]}"#)
            .client_id("user-123")
            .revocation_key("grp-7")
            .ttl(Duration::from_secs(3600));
        let jwt = mint_ably_jwt(&key, &params).unwrap();

        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "header.payload.signature");

        let header = decode_part(parts[0]);
        assert_eq!(header["alg"], "HS256");
        assert_eq!(header["typ"], "JWT");
        assert_eq!(header["kid"], "app.keyid");

        let claims = decode_part(parts[1]);
        assert_eq!(
            claims["x-ably-capability"],
            r#"{"sports":["history","publish"]}"#
        );
        assert_eq!(claims["x-ably-clientId"], "user-123");
        assert_eq!(claims["x-ably-revocation-key"], "grp-7");
        let iat = claims["iat"].as_i64().unwrap();
        let exp = claims["exp"].as_i64().unwrap();
        assert_eq!(exp - iat, 3600);
    }

    #[test]
    fn omits_optional_claims_when_unset() {
        let key = SigningKey::new("app.keyid:secret").unwrap();
        let jwt = mint_ably_jwt(&key, &TokenParams::new("{}")).unwrap();
        let claims = decode_part(jwt.split('.').nth(1).unwrap());
        assert!(claims.get("x-ably-clientId").is_none());
        assert!(claims.get("x-ably-revocation-key").is_none());
    }
}
