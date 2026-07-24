//! Ably JWT minting (ADR-0012, SPEC §13.2). Feature `jwt`. SERVER-SIDE ONLY:
//! this carries your API secret and must never run in client-shipped code.

use crate::error::{Error, Result};

/// An Ably API key split into its name (`appId.keyId`) and secret, for signing
/// Ably JWTs. `Debug` redacts the secret.
#[derive(Clone)]
pub struct SigningKey {
    name: String,
    // Not yet read until `mint_ably_jwt` is implemented for real in Task 2.2.
    #[allow(dead_code)]
    secret: String,
}

impl SigningKey {
    /// Parse a full API key `appId.keyId:keySecret`.
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

/// Inputs for minting an Ably JWT (Task 2.2).
pub struct TokenParams; // replaced in Task 2.2

/// Mint an Ably JWT (Task 2.2).
pub fn mint_ably_jwt(_key: &SigningKey, _params: &TokenParams) -> Result<String> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
