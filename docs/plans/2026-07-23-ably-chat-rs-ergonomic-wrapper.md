# ably-chat-rs Ergonomic Wrapper Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the hand-written ergonomic `ably_chat` client as a workspace crate layered over the generated `ably-chat-openapi` bindings.

**Architecture:** A two-crate Cargo workspace (ADR-0002). The ergonomic crate owns one async `reqwest` dispatch layer and its own forward-compatible domain types (ADR-0003), because the generated functions discard the `Link` headers pagination needs and the generated `MessageAction` enum hard-fails on unknown values. The generated crate is re-exported as `ably_chat::raw` (ADR-0004). Public surface is a room-scoped handle chain with `IntoFuture` builders (ADR-0010); history/versions paginate via `Page<T>` + `Stream` over RFC 5988 `Link` headers (ADR-0009); one typed `Error` carries the Ably envelope (ADR-0008).

**Tech Stack:** Rust 2024 (MSRV 1.85), `reqwest` 0.13 (async; `native-tls`/`rustls` features), `serde`/`serde_json`, `thiserror` 2, `futures` 0.3, `base64` 0.22; dev-deps `tokio` 1, `wiremock` 0.6.

**Test seam (single, highest):** the HTTP boundary. Every integration test starts a `wiremock` server and points the client's base host at it, then drives the real public API. Nothing internal is mocked. Assert on the request wiremock received and the typed value/error returned.

**Reference docs:** [SPEC.md](../SPEC.md) (normative contract), [PRD.md](../PRD.md), [adr/](../adr/), the wire contract [openapi/ably-chat-rest.yaml](../../openapi/ably-chat-rest.yaml).

**Conventions for the executor:**
- TDD strictly: failing test → run-fail → minimal impl → run-pass → commit.
- Run tests from the workspace root; scope with `-p ably-chat-rs`.
- Commit after every green step. Use Conventional Commits.
- Do not edit anything under `crates/ably-chat-openapi/src/` by hand (it is generated).

---

## Phase 0 — Restructure into the workspace

Not TDD (structural). Gate: `cargo check --workspace` is green and `ably_chat::raw` resolves.

### Task 1: Move the generated crate and create the ergonomic skeleton

**Files:**
- Create: `Cargo.toml` (workspace root, replacing the current package manifest)
- Move: `src/` → `crates/ably-chat-openapi/src/`
- Create: `crates/ably-chat-openapi/Cargo.toml`
- Move: `LICENSE-APACHE`, `LICENSE-MIT` copies into each crate
- Create: `crates/ably-chat-rs/Cargo.toml`, `crates/ably-chat-rs/src/lib.rs`

**Step 1: Move the generated sources**

```bash
cd /Users/andrii/work/ably-chat-api
mkdir -p crates/ably-chat-openapi crates/ably-chat-rs/src
git mv src crates/ably-chat-openapi/src 2>/dev/null || mv src crates/ably-chat-openapi/src
mv .openapi-generator crates/ably-chat-openapi/.openapi-generator
mv .openapi-generator-ignore crates/ably-chat-openapi/.openapi-generator-ignore
cp LICENSE-APACHE LICENSE-MIT crates/ably-chat-openapi/
cp LICENSE-APACHE LICENSE-MIT crates/ably-chat-rs/
```

**Step 2: Write the workspace root `Cargo.toml`**

Replace the entire current root `Cargo.toml` with:

```toml
[workspace]
resolver = "2"
members = ["crates/ably-chat-rs", "crates/ably-chat-openapi"]

[workspace.package]
version = "0.1.0"
authors = ["Andrii Radyk <andrii@natter.co>"]
license = "MIT OR Apache-2.0"
edition = "2024"
rust-version = "1.85"
# TODO: set before publish
# repository = "https://github.com/<you>/ably-chat-rs"
```

**Step 3: Write `crates/ably-chat-openapi/Cargo.toml`** (rename the generated crate)

```toml
[package]
name = "ably-chat-openapi"
description = "Unofficial generated OpenAPI bindings for the Ably Chat REST API (v4). Not affiliated with or endorsed by Ably. Prefer the `ably-chat-rs` crate."
readme = "README.md"
keywords = ["ably", "chat", "rest", "openapi"]
categories = ["api-bindings"]
version.workspace = true
authors.workspace = true
license.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
name = "ably_chat_openapi"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_repr = "0.1"
url = "2"
reqwest = { version = "0.13", default-features = false, features = ["json", "multipart"] }

[features]
default = ["native-tls"]
native-tls = ["reqwest/native-tls"]
rustls = ["reqwest/rustls"]
```

**Step 4: Write `crates/ably-chat-rs/Cargo.toml`**

```toml
[package]
name = "ably-chat-rs"
description = "Unofficial, ergonomic Rust client for the Ably Chat REST API (v4). Not affiliated with or endorsed by Ably."
readme = "README.md"
documentation = "https://docs.rs/ably-chat-rs"
keywords = ["ably", "chat", "rest", "client", "sdk"]
categories = ["api-bindings", "web-programming::http-client"]
version.workspace = true
authors.workspace = true
license.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
name = "ably_chat"

[dependencies]
ably-chat-openapi = { version = "0.1.0", path = "../ably-chat-openapi" }
reqwest = { version = "0.13", default-features = false, features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
futures = "0.3"
base64 = "0.22"
chrono = { version = "0.4", optional = true, default-features = false, features = ["std"] }
time = { version = "0.3", optional = true }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
wiremock = "0.6"

[features]
default = ["native-tls"]
native-tls = ["reqwest/native-tls", "ably-chat-openapi/native-tls"]
rustls = ["reqwest/rustls", "ably-chat-openapi/rustls"]
chrono = ["dep:chrono"]
time = ["dep:time"]

[package.metadata.docs.rs]
all-features = true
```

**Step 5: Write `crates/ably-chat-rs/src/lib.rs` skeleton**

```rust
//! Unofficial, ergonomic Rust client for the Ably Chat REST API (v4).
//!
//! Not affiliated with or endorsed by Ably.

/// Low-level generated bindings. Escape hatch; NOT covered by the pre-1.0
/// stability guarantee and may change on regeneration.
pub mod raw {
    pub use ably_chat_openapi::*;
}

// Ergonomic layer (filled in by later phases).
mod error;
pub use error::{Error, ErrorInfo, Result};
```

For the skeleton to compile, also create an empty `crates/ably-chat-rs/src/error.rs` containing a placeholder that Task 5 replaces:

```rust
//! placeholder; replaced in Phase 1
#[derive(Debug, thiserror::Error)]
#[error("placeholder")]
pub struct Error;
pub struct ErrorInfo;
pub type Result<T> = std::result::Result<T, Error>;
```

**Step 6: Verify the workspace compiles**

Run: `cargo check --workspace`
Expected: PASS — both `ably-chat-openapi` and `ably-chat-rs` compile.

**Step 7: Commit**

```bash
git add -A
git commit -m "refactor: restructure into two-crate workspace (ADR-0002)"
```

### Task 1b: CI/CD workflows

The workflows are committed under `.github/workflows/` (adapted from
`work/sendgrid`; library-only, Linux + macOS **native** runners, no Windows, no
cross-compilation):

- `ci.yml` — `lint` (fmt/clippy/docs/feature-matrix), `msrv` (1.85),
  `codegen-gate` (regenerate the OpenAPI crate source from the spec and `diff`),
  and `test` on `[ubuntu-latest, ubuntu-24.04-arm, macos-latest, macos-15-intel]`.
- `release.yml` — on a `v*.*.*` tag, `cargo publish --workspace` to crates.io
  (no binaries — this is a library). Needs the `CARGO_TOKEN` secret.
- `drift.yml` — weekly: opens an issue when `@ably/chat` on npm is newer than
  `openapi/upstream.lock`.

**Step 1: Verify the codegen gate reproduces the committed source** (verified
clean at plan-authoring time)

```bash
npx --yes @openapitools/openapi-generator-cli generate \
  -i openapi/ably-chat-rest.yaml -g rust -o /tmp/gen \
  --additional-properties=packageName=ably-chat-openapi,packageVersion=0.1.0,supportAsync=true,library=reqwest
diff -ru crates/ably-chat-openapi/src /tmp/gen/src
```
Expected: no output (clean). `src/` is `packageName`-independent, so the Task 1
*move* is sufficient — no regeneration needed.

**Step 2: Commit**

```bash
git add .github openapi/upstream.lock
git commit -m "ci: linux+macos native CI, crates.io release, upstream-drift (from sendgrid)"
```

---

## Phase 1 — Core: types, error, client, dispatch

TDD. Gate: `cargo test -p ably-chat-rs` green; unit tests for auth encoding, error parsing, enum serde, retry predicate.

### Task 2: Identifier newtypes & Timestamp

**Files:**
- Create: `crates/ably-chat-rs/src/types.rs`
- Modify: `crates/ably-chat-rs/src/lib.rs` (add `mod types; pub use types::*;`)
- Test: inline `#[cfg(test)]` in `types.rs`

**Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serial_roundtrips_through_json() {
        let s: Serial = serde_json::from_str("\"01abc@def:001\"").unwrap();
        assert_eq!(s.as_str(), "01abc@def:001");
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"01abc@def:001\"");
    }
    #[test]
    fn timestamp_is_epoch_millis() {
        let t: Timestamp = serde_json::from_str("1700000000000").unwrap();
        assert_eq!(t.as_millis(), 1_700_000_000_000);
    }
}
```

**Step 2: Run to fail** — `cargo test -p ably-chat-rs types::` → FAIL (types missing).

**Step 3: Implement**

```rust
use serde::{Deserialize, Serialize};

macro_rules! string_newtype {
    ($(#[$m:meta])* $name:ident, ord = $ord:tt) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn as_str(&self) -> &str { &self.0 }
            pub fn into_string(self) -> String { self.0 }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
        }
        impl From<String> for $name { fn from(s: String) -> Self { Self(s) } }
        impl From<&str> for $name { fn from(s: &str) -> Self { Self(s.to_owned()) } }
        impl std::borrow::Borrow<str> for $name { fn borrow(&self) -> &str { &self.0 } }
    };
}

// Serial is region-scoped: intentionally NOT Ord (ADR-0007).
string_newtype!(
    /// A message's unique, region-scoped identifier.
    Serial, ord = false
);
// RoomName may be ordered/keyed.
string_newtype!(
    /// The name of a chat room.
    RoomName, ord = false
);

/// Milliseconds since the Unix epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);
impl Timestamp {
    pub fn as_millis(&self) -> i64 { self.0 }
    #[cfg(feature = "chrono")]
    pub fn to_chrono(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::from_timestamp_millis(self.0)
    }
}
impl From<i64> for Timestamp { fn from(v: i64) -> Self { Self(v) } }
```

(Note: the `ord` macro arm is a marker for the executor; `Serial`/`RoomName` deliberately omit `Ord`. If a keyed id later needs `Ord`, add a separate derive.)

**Step 4: Run to pass** — `cargo test -p ably-chat-rs types::` → PASS.

**Step 5: Commit** — `git commit -am "feat(types): Serial/RoomName newtypes and epoch-ms Timestamp (ADR-0007)"`

### Task 3: Forward-compatible enums

**Files:** Modify `crates/ably-chat-rs/src/types.rs`; inline tests.

**Step 1: Failing test**

```rust
#[test]
fn unknown_action_is_captured_not_rejected() {
    let a: MessageAction = serde_json::from_str("\"message.future\"").unwrap();
    assert_eq!(a, MessageAction::Other("message.future".into()));
    let c: MessageAction = serde_json::from_str("\"message.create\"").unwrap();
    assert_eq!(c, MessageAction::Create);
}
```

**Step 2: Run to fail.**

**Step 3: Implement** (the `from`/`into = String` pattern gives forward-compat + serde):

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum MessageAction { Create, Update, Delete, Other(String) }
impl From<String> for MessageAction {
    fn from(s: String) -> Self {
        match s.as_str() {
            "message.create" => Self::Create,
            "message.update" => Self::Update,
            "message.delete" => Self::Delete,
            _ => Self::Other(s),
        }
    }
}
impl From<MessageAction> for String {
    fn from(a: MessageAction) -> String {
        match a {
            MessageAction::Create => "message.create".into(),
            MessageAction::Update => "message.update".into(),
            MessageAction::Delete => "message.delete".into(),
            MessageAction::Other(s) => s,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Forwards, Backwards }   // query-only; serialize as lowercase str in dispatch

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ReactionType { Unique, Distinct, Multiple, Other(String) }
// From<String>/Into<String> as above with "unique"/"distinct"/"multiple".
```

**Step 4: Run to pass. Step 5: Commit** `feat(types): forward-compatible enums with Other variant`.

### Task 4: Message, MessageVersion, Occupancy, reaction summaries

**Files:** Modify `crates/ably-chat-rs/src/types.rs`; inline test deserializing a sample wire message.

**Step 1: Failing test** — paste a representative message JSON (from the OpenAPI examples) and assert fields deserialize, including `timestamp` as `Timestamp`, `action` as enum, absent `reactions` → default empty.

**Step 3: Implement** serde structs matching the wire contract (`#[serde(rename_all = "camelCase")]` because the wire uses `clientId`, `presenceMembers`, etc.):

```rust
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub serial: Serial,
    pub version: MessageVersion,
    pub text: String,
    pub client_id: String,
    pub action: MessageAction,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub user_claim: Option<String>,
    pub timestamp: Timestamp,
    #[serde(default)]
    pub reactions: ReactionSummary,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageVersion {
    pub serial: Serial,
    pub timestamp: Timestamp,
    #[serde(default)] pub client_id: Option<String>,
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub metadata: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Occupancy { pub connections: u64, #[serde(rename = "presenceMembers")] pub presence_members: u64 }

pub type Metadata = serde_json::Map<String, serde_json::Value>;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReactionSummary {
    #[serde(default)] pub unique: BTreeMap<String, ClientIdList>,
    #[serde(default)] pub distinct: BTreeMap<String, ClientIdList>,
    #[serde(default)] pub multiple: BTreeMap<String, ClientIdCounts>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ClientIdList { pub total: u64, #[serde(default)] pub client_ids: Vec<String>, #[serde(default)] pub clipped: bool }
#[derive(Clone, Debug, Deserialize)]
pub struct ClientIdCounts { pub total: u64, #[serde(default)] pub client_ids: BTreeMap<String, u64>, #[serde(default, rename = "totalUnidentified")] pub total_unidentified: u64, #[serde(default)] pub clipped: bool }
```

**Step 4/5:** run to pass; commit `feat(types): Message/Occupancy/reaction summary wire types`.

### Task 5: Error model (ADR-0008)

**Files:** Replace `crates/ably-chat-rs/src/error.rs`; inline tests.

**Step 1: Failing tests**

```rust
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
```

**Step 3: Implement**

```rust
use serde::Deserialize;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    pub code: i64,
    #[serde(default)] pub message: String,
    #[serde(default)] pub status_code: u16,
    #[serde(default)] pub href: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("failed to decode response: {0}")]
    Decode(String),
    #[error("Ably API error {}: {message}", .info.code, message = .info.message)]
    Api { status: u16, info: ErrorInfo },
}

#[derive(Deserialize)]
struct Envelope { error: ErrorInfo }

impl Error {
    pub(crate) fn from_api_body(status: u16, body: &[u8]) -> Self {
        let info = serde_json::from_slice::<Envelope>(body)
            .map(|e| e.error)
            .unwrap_or(ErrorInfo { code: 0, message: String::from_utf8_lossy(body).into_owned(), status_code: status, href: None });
        Error::Api { status, info }
    }
    pub fn status(&self) -> Option<u16> {
        match self { Error::Api { status, .. } => Some(*status), Error::Transport(e) => e.status().map(|s| s.as_u16()), _ => None }
    }
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Transport(e) => e.is_timeout() || e.is_connect(),
            Error::Api { status, .. } => *status == 429 || (500..=599).contains(status),
            Error::Decode(_) => false,
        }
    }
    pub fn is_not_found(&self) -> bool { matches!(self, Error::Api { info, .. } if info.code == 40400) }
    pub fn is_rejected_by_rule(&self) -> bool { matches!(self, Error::Api { info, .. } if info.code == 42211) }
    pub fn is_rejected_by_moderation(&self) -> bool { matches!(self, Error::Api { info, .. } if info.code == 42213) }
}
```

**Step 4/5:** run to pass; commit `feat(error): typed Error with Ably envelope + retryability (ADR-0008)`.

### Task 6: Auth + ClientConfig + ClientBuilder (ADR-0005)

**Files:** Create `crates/ably-chat-rs/src/config.rs`; add `mod config; pub use config::*;` to `lib.rs`; inline tests.

**Step 1: Failing tests**

```rust
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
}
```

**Step 3: Implement**

```rust
use base64::{Engine, engine::general_purpose::STANDARD};

#[derive(Clone)]
pub enum Auth { ApiKey(String), Token(String) }
impl Auth {
    pub fn api_key(k: impl Into<String>) -> Self { Auth::ApiKey(k.into()) }
    pub fn token(t: impl Into<String>) -> Self { Auth::Token(t.into()) }
    pub(crate) fn header_value(&self) -> String {
        match self {
            Auth::ApiKey(k) => format!("Basic {}", STANDARD.encode(k.as_bytes())),
            Auth::Token(t) => format!("Bearer {t}"),
        }
    }
}
impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Auth::ApiKey(_) => f.write_str("Auth::ApiKey(<redacted>)"), Auth::Token(_) => f.write_str("Auth::Token(<redacted>)") }
    }
}
// TODO(ADR-0005): reserve `Auth::Provider(Arc<dyn TokenProvider>)` here later.
```

**Step 4/5:** run to pass; commit `feat(config): Auth (Basic/Bearer) with redacting Debug (ADR-0005)`.

### Task 7: Client, ClientBuilder, and the dispatch layer (ADR-0003, ADR-0006)

**Files:** Create `crates/ably-chat-rs/src/client.rs`, `crates/ably-chat-rs/src/dispatch.rs`; wire into `lib.rs`; integration test `crates/ably-chat-rs/tests/dispatch.rs` (wiremock).

**Step 1: Failing integration test** (the seam in action)

```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};
use ably_chat::{Client, Auth};

#[tokio::test]
async fn sends_version_and_auth_headers_and_maps_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/chat/v4/rooms/r/occupancy"))
        .and(header("x-ably-version", "4"))
        .and(header("authorization", "Basic YXBwLms6cw=="))
        .respond_with(ResponseTemplate::new(404).set_body_string(r#"{"error":{"code":40400,"message":"no","statusCode":404}}"#))
        .mount(&server).await;

    let client = Client::builder(Auth::api_key("app.k:s")).host(server.uri()).build();
    let err = client.room("r").occupancy().get().await.unwrap_err();
    assert!(err.is_not_found());
}
```

(This test also depends on Tasks 8/11; if executing strictly, first write a thinner dispatch-only test hitting an internal `dispatch` helper, then let Task 11 replace it with the public-API version above. Prefer the public-API version once `occupancy` exists.)

**Step 3: Implement dispatch + client**

`client.rs`:
```rust
use std::sync::Arc;
use crate::config::Auth;
use crate::room::Room;

#[derive(Clone)]
pub struct Client { pub(crate) inner: Arc<Inner> }
pub(crate) struct Inner {
    pub(crate) http: reqwest::Client,
    pub(crate) base: String,       // e.g. https://rest.ably.io
    pub(crate) auth_header: String,
    pub(crate) max_retries: u32,
}
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("base", &self.inner.base).field("auth", &"<redacted>").finish()
    }
}
impl Client {
    pub fn builder(auth: Auth) -> ClientBuilder { ClientBuilder::new(auth) }
    pub fn room(&self, name: impl Into<crate::types::RoomName>) -> Room { Room::new(self.clone(), name.into()) }
}

pub struct ClientBuilder { auth: Auth, host: String, http: Option<reqwest::Client>, timeout: Option<std::time::Duration>, max_retries: u32 }
impl ClientBuilder {
    pub fn new(auth: Auth) -> Self { Self { auth, host: "https://rest.ably.io".into(), http: None, timeout: None, max_retries: 3 } }
    pub fn host(mut self, h: impl Into<String>) -> Self { self.host = h.into(); self }
    pub fn http_client(mut self, c: reqwest::Client) -> Self { self.http = Some(c); self }
    pub fn timeout(mut self, d: std::time::Duration) -> Self { self.timeout = Some(d); self }
    pub fn max_retries(mut self, n: u32) -> Self { self.max_retries = n; self }
    pub fn build(self) -> Client {
        let http = self.http.unwrap_or_else(|| {
            let mut b = reqwest::Client::builder();
            if let Some(t) = self.timeout { b = b.timeout(t); }
            b.build().expect("reqwest client")
        });
        Client { inner: Arc::new(Inner { http, base: self.host.trim_end_matches('/').to_string(), auth_header: self.auth.header_value(), max_retries: self.max_retries }) }
    }
}
```

`dispatch.rs` (owns the request loop, retry predicate, error mapping, and returns status+headers+bytes):
```rust
use reqwest::{Method, header::HeaderMap};
use crate::error::{Error, Result};

pub(crate) struct RawResponse { pub status: u16, pub headers: HeaderMap, pub body: bytes::Bytes }

impl crate::client::Inner {
    fn retry_eligible(method: &Method, has_idempotency_key: bool) -> bool {
        matches!(*method, Method::GET | Method::DELETE) || has_idempotency_key
    }

    pub(crate) async fn send(
        &self, method: Method, path: &str, query: &[(&str, String)], body: Option<serde_json::Value>, has_idem: bool,
    ) -> Result<RawResponse> {
        let url = format!("{}{}", self.base, path);
        let eligible = Self::retry_eligible(&method, has_idem);
        let mut attempt = 0;
        loop {
            let mut req = self.http.request(method.clone(), &url)
                .header("X-Ably-Version", "4")
                .header(reqwest::header::AUTHORIZATION, &self.auth_header);
            if !query.is_empty() { req = req.query(query); }
            if let Some(b) = &body { req = req.json(b); }
            let resp = req.send().await;
            match resp {
                Ok(r) => {
                    let status = r.status().as_u16();
                    if (status == 429 || (500..=599).contains(&status)) && eligible && attempt < self.max_retries {
                        attempt += 1; continue; // (respect Retry-After: sleep parsed value; use ~0 in tests)
                    }
                    let headers = r.headers().clone();
                    let bytes = r.bytes().await?;
                    if (200..300).contains(&status) { return Ok(RawResponse { status, headers, body: bytes }); }
                    return Err(Error::from_api_body(status, &bytes));
                }
                Err(e) => { if e.is_timeout() && eligible && attempt < self.max_retries { attempt += 1; continue; } return Err(e.into()); }
            }
        }
    }
}
```

(Add `bytes = "1"` to deps, or use `Vec<u8>` instead of `bytes::Bytes`.) Add a unit test for `retry_eligible` covering GET (true), POST without key (false), POST with key (true).

**Step 4:** `cargo test -p ably-chat-rs` → green (once Task 8/11 land the public path). **Step 5:** commit `feat(client): builder + dispatch with retry-safety predicate (ADR-0003/0006)`.

---

## Phase 2 — Read operations + pagination

### Task 8: Handle chain skeleton (Room / Messages / Reactions / Occupancy handles)

**Files:** Create `room.rs`, `messages.rs`, `reactions.rs`, `occupancy.rs`; wire into `lib.rs`.

Implement cheap `Arc`-backed handles (they just carry `Client` + `RoomName` [+ `Serial` where relevant]). No operations yet. Commit `feat: room-scoped handle chain (ADR-0010)`.

Pattern for every handle:
```rust
#[derive(Clone)]
pub struct Room { client: Client, room: RoomName }
impl Room {
    pub(crate) fn new(client: Client, room: RoomName) -> Self { Self { client, room } }
    pub fn messages(&self) -> Messages { Messages { client: self.client.clone(), room: self.room.clone() } }
    pub fn occupancy(&self) -> OccupancyHandle { OccupancyHandle { client: self.client.clone(), room: self.room.clone() } }
}
```

### Task 9: Occupancy (`GET /occupancy`) — the first end-to-end operation

**Files:** `occupancy.rs`; test `tests/occupancy.rs`.

**Step 1: Failing test** (wiremock returns `{"connections":3,"presenceMembers":2}`; assert `occ.connections == 3`).
**Step 3: Implement** an `IntoFuture` builder that calls `inner.send(GET, "/chat/v4/rooms/{room}/occupancy", …)` then `serde_json::from_slice::<Occupancy>`; map decode failure to `Error::Decode`.

Reusable path builder helper in `dispatch.rs`:
```rust
pub(crate) fn room_path(room: &str, suffix: &str) -> String {
    format!("/chat/v4/rooms/{}{}", urlencoding::encode(room), suffix)
}
```
`IntoFuture` builder shape (reused by all read ops):
```rust
pub struct GetOccupancy { client: Client, room: RoomName }
impl std::future::IntoFuture for GetOccupancy {
    type Output = Result<Occupancy>;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send>>;
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let r = self.client.inner.send(Method::GET, &room_path(self.room.as_str(), "/occupancy"), &[], None, false).await?;
            serde_json::from_slice(&r.body).map_err(|e| Error::Decode(e.to_string()))
        })
    }
}
```
**Step 4/5:** run to pass; commit `feat(occupancy): get room occupancy`.

### Task 10: Get a single message (`GET /messages/{serial}`)

Same builder pattern as Task 9; path `…/messages/{serial}`; returns `Message`. Test: 200 with a full message body → typed `Message`; 404 → `err.is_not_found()`. Commit `feat(messages): get message by serial`.

### Task 11: Client-reactions (`GET …/client-reactions`)

Builder with optional `.client_id(..)` → query `forClientId`; returns `ReactionSummary`. Test asserts the query param is forwarded and the summary parses. Commit `feat(reactions): fetch client reactions`.

### Task 12: Pagination — `Page<T>` + `Stream` (ADR-0009)

**Files:** `pagination.rs`; test `tests/pagination.rs`.

**Step 1: Failing test** — wiremock mounts page 1 with header `Link: <{server}/chat/v4/rooms/r/messages?cont=2>; rel="next"` and a 1-item array, page 2 with a 1-item array and no next. Assert `history().into_stream()` yields 2 items; assert manual `next()` returns page 2 then `None`.

**Step 3: Implement** `Page<T>` holding `items: Vec<T>`, parsed cursors from the `Link` header, and a `Client`; `next()` issues a GET to the absolute `next` URL; `into_stream()` uses `futures::stream::unfold`. Parse RFC 5988: split on `,`, each `<url>; rel="next"`. Commit `feat(pagination): Page<T> + Stream over Link headers`.

### Task 13: History + Versions

`Messages::history()` returns a builder with `.start/.end/.direction/.limit/.from_serial` (maps `direction` to `forwards`/`backwards`, default `backwards`) then `.await` → first `Page<Message>` (or `.into_stream()`). `Messages::versions(serial)` → `Page<Message>` for `…/versions`. Tests: filters land in the query; default direction is `backwards`; a two-page history streams fully. Commit `feat(messages): history and versions (paginated, default newest-first)`.

---

## Phase 3 — Write operations (idempotency + retry)

### Task 14: Send message (`POST /messages`)

**Step 1: Failing test** — wiremock expects `POST …/messages`, JSON body `{"text":"hi","metadata":{...}}`, returns 201 + created message; assert returned `Message.text == "hi"`. Second test: with `.idempotency_key("k")`, assert query `idempotencyKey=k` is present.
**Step 3:** builder `send(text).metadata(..).headers(..).idempotency_key(..)`; POST with `has_idem = key.is_some()`; body built from set fields only. Commit `feat(messages): send message with metadata/headers/idempotency`.

### Task 15: Update message (`PUT /messages/{serial}`)

Builder `update(serial, text).metadata(..).headers(..).description(..).idempotency_key(..)`; body `{ "message": { text, metadata?, headers? }, description?, metadata? }`. Doc comment MUST state full-replace semantics. Test asserts body shape + returned updated message. Commit `feat(messages): update (full-replace) message`.

### Task 16: Delete message (`POST …/delete`)

Builder `delete(serial).description(..).metadata(..).idempotency_key(..)`; POST to `…/messages/{serial}/delete`. Test asserts the `/delete` path (not HTTP DELETE) and a returned `message.delete` action. Commit `feat(messages): soft-delete message`.

---

## Phase 4 — Reactions write

### Task 17: Send reaction (`POST …/reactions`) — never retried

Builder `reactions().send(serial, name).kind(ReactionType).count(u64)`; body `{ type, name, count? }`; `has_idem = false` (ADR-0006 — must not retry). Returns `()`. Test: body shape for each type; assert (via wiremock `expect(1)`) that a 503 is **not** retried. Commit `feat(reactions): send reaction (non-retried)`.

### Task 18: Delete reaction (`DELETE …/reactions`)

Builder `reactions().delete(serial).kind(..).name(..)`; query `type` (required), `name` (required for distinct/multiple). Client-side validation returns an `Error` (not a request) when `name` missing for non-`unique`. Test both the happy path and the validation error. Commit `feat(reactions): delete reaction with name-required rule`.

---

## Phase 5 — Ergonomic polish

### Task 19: Prelude, re-exports, doctests

**Files:** `crates/ably-chat-rs/src/prelude.rs`; crate-level doc example in `lib.rs`.
- Re-export `Client`, `Auth`, `Room`, `Error`, `Result`, key types.
- Add a crate-level `//!` example (annotate `no_run`) showing build → room → send → history stream.
- Ensure every handle is `Send + Sync` (add a `fn assert_send_sync<T: Send + Sync>() {}` compile test).

Run: `cargo test -p ably-chat-rs --doc` → PASS. Run: `cargo clippy -p ably-chat-rs --all-features -- -D warnings` → clean. Commit `docs: prelude, re-exports, doctests; clippy clean`.

---

## Phase 6 — Docs, features, publish prep

### Task 20: Feature-matrix + README + dry-run

- Build each combo:
  - `cargo check -p ably-chat-rs --no-default-features --features rustls`
  - `cargo check -p ably-chat-rs --all-features`
- Reframe `crates/ably-chat-rs/README.md` crate-first (install `cargo add ably-chat-rs`, `use ably_chat`, a usage snippet, the `raw` escape hatch, the unofficial + no-realtime + no-room-CRUD notes, dual licence). Update the repo root `README.md` to point at both crates.
- **Confirm the SPEC §3 open question** against a live endpoint (bare object vs 1-element array) and record the result in the README caveat; adjust deserialization only if it is an array.
- Verify packaging without publishing (a full `cargo publish --dry-run` of
  `ably-chat-rs` cannot resolve its unpublished path-dep until
  `ably-chat-openapi` is on crates.io — chicken-and-egg):
  - `cargo publish -p ably-chat-openapi --dry-run` (this one can dry-run)
  - `cargo package -p ably-chat-rs --allow-dirty` (packages + build-verifies via the path dep)
- Set `repository`/`homepage` in the workspace manifest.
- The real release is **tag-triggered**: `release.yml` runs `cargo publish
  --workspace` on a `v*.*.*` tag (publishes `ably-chat-openapi` then
  `ably-chat-rs`; needs `CARGO_TOKEN`). Bump `[workspace.package] version`
  before tagging.

Gate: openapi dry-run + ergonomic `cargo package` succeed; `--all-features` docs build; live singleton-body check recorded. Commit `chore: publish prep (features, README, packaging)`.

---

## Final verification (whole plan)

Run from the workspace root:
- `cargo test --workspace --all-features` → all green
- `cargo clippy --workspace --all-features -- -D warnings` → clean
- `cargo doc --workspace --all-features --no-deps` → builds

Any deferred item (e.g. the live singleton-body confirmation) MUST be recorded explicitly, not silently skipped.

CI (`.github/workflows/ci.yml`) mirrors these gates on every push: fmt, clippy
(`-D warnings`), docs, the feature matrix, MSRV 1.85, the codegen drift gate, and
build+test across the Linux + macOS native matrix. Green CI is the definition of
done for each phase once the repo has a remote.
