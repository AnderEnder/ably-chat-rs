# Design: an ergonomic `ably_chat` wrapper over the generated Ably Chat REST client

**Status:** proposed · **Target crate:** `ably-chat-rs` (import path `ably_chat`) · **Ably Chat REST API v4** · **MSRV 1.85 / edition 2024**

This document specifies a hand-written, idiomatic Rust facade layered over the OpenAPI-generated mechanical client that currently lives at `src/apis/` and `src/models/`. It is grounded in the actual generated code and the actual spec (`openapi/ably-chat-rest.yaml`), and it maps **all ten** REST operations to an ergonomic, room-scoped API.

Every wire shape referenced below (`raw::models::*` fields, enum renames, `i32`/`i64` widths, `Box<_>` wrappers, `Option` vs required) was confirmed against the generated code in `src/models/*` and the spec. Treat that as a snapshot: re-verify these shapes whenever the generated crate is regenerated.

---

## 1. Overview & design principles

The generated crate is a faithful but *mechanical* client: every operation is a free function taking a `&Configuration`, a positional `x_ably_version: Option<&str>`, `Box<...>`-wrapped bodies, closed enums, `i32` counts, raw `String` identifiers, and one `Error<SpecificOpError>` per operation. It works, but it leaks generator artifacts into every call site and is fragile against server evolution. The wrapper turns it into something a Rust developer would enjoy using and that mirrors the `@ably/chat` JS SDK's handle-chain spirit.

**Principles**

1. **Own the call layer; convert, don't re-expose.** The public surface returns *owned* domain types (`ably_chat::Message`, `Serial`, `Occupancy`, …), never `raw::models::*`. This is not gold-plating — three independent requirements each force the wrapper to own the HTTP call for body-bearing responses:
   - **Pagination.** `getMessages` / `getMessageVersions` return a bare JSON array and pagination lives *only* in RFC 5988 `Link` response headers (`rel="first"|"current"|"next"`); the generated fns call `resp.text()` and discard the headers. Streamed history/versions is a hard requirement, so these must be re-issued by the wrapper.
   - **Retryability / `Retry-After` / fallback-host rotation.** Each generated fn bakes `configuration.base_path` into the URL and owns build+execute, dropping response headers. Retry, host rotation, and `Retry-After` parsing cannot live in a delegate or in `reqwest-middleware`; they must live in the wrapper's own executor.
   - **Forward-compatibility.** `MessageAction` is generated as a *closed* enum (`message.create|update|delete`, no fallback). A future server action (`message.something`) fails inside `serde_json::from_str::<Message>` — *before* any `From<generated::Message>` conversion could run. A conversion layer therefore cannot restore forward-compat; only owning deserialization can.

   Since the first two are forced by the task's own requirements, owning deserialization everywhere a body comes back is nearly free, and it is the *only* thing that simultaneously buys newtyped ids, an `Other(String)` action variant, `#[non_exhaustive]` response types, and feature-gated datetime.

2. **"Layered over" means layering *policy*, not calling every generated fn.** The generated crate remains the regeneratable mechanical foundation and transport container (`Configuration`, `urlencode`) plus an opt-in `raw::` escape hatch. The wrapper layers auth mapping, RSC15 host resolution, retry/backoff, pagination, ergonomic room-scoped handles, and forward-compatible domain types on top.

3. **Ergonomics mirror the JS SDK.** `client.room("x").messages().send("hi").await`, `.history()`, `.reactions().send(serial, "👍")`, `.occupancy().await`. Handles are cheap, `Arc`-backed, `Clone`, `'static + Send + Sync`.

4. **One `Error`, one `Result`.** The per-op `Error<SendMessageError>` generics never surface. A single `#[non_exhaustive] enum Error` carries the Ably envelope, HTTP status, chat-specific codes, and retryability.

5. **Bare `.await` is the uniform terminal.** Optional-parameter calls return builders that `impl IntoFuture`, so they end in `.await` — never `.send().await`. This is the only convention uniform with the zero-optional plain methods (`get(serial).await`, `occupancy().await`), and it avoids the `messages().send("hi").send().await` stutter (two operations are literally named *send*).

6. **Secrets never leak.** Credentials live in `secrecy::SecretString`; the public `Client`'s `Debug` is hand-written (forwarding to a redacting `Inner`), so a key can never be printed into logs.

7. **No provisioning API.** See [§10](#10-there-is-no-createdelete-room-or-channel). Rooms are channel-backed and implicit; `client.room(name)` performs zero I/O.

---

## 2. Crate layout & module tree

### 2.1 Topology: a two-crate workspace

The generated crate today *is* `ably-chat-rs` with lib name `ably_chat` — the exact name the wrapper must publish under. Resolve the collision by demoting the generated output to its own crate and letting the wrapper own the public name.

```
ably-chat-api/                       (workspace root)
├── Cargo.toml                       # [workspace] members = ["crates/*"]
└── crates/
    ├── ably-chat-openapi/           # the generated mechanical client (renamed)
    │   ├── Cargo.toml               # package = "ably-chat-openapi", lib = "ably_chat_openapi"
    │   └── src/                     # <- today's src/apis, src/models, unchanged, regeneratable
    └── ably-chat/                   # the hand-written wrapper (public face)
        ├── Cargo.toml               # package = "ably-chat-rs", lib = "ably_chat"
        └── src/ ...
```

The wrapper depends on the generated crate privately, under its real package name (no dependency alias — an alias named `raw` would collide with the wrapper's own `mod raw`):

```toml
# crates/ably-chat/Cargo.toml
[dependencies]
ably-chat-openapi = { version = "0.1", path = "../ably-chat-openapi", default-features = false }
```

The escape hatch is a *module*, not a renamed dependency (see `raw.rs` below). Regenerate `ably-chat-openapi` freely; the wrapper is the stable public surface. Publishing a generated "raw" crate to crates.io alongside the ergonomic one is normal (cf. `k8s-openapi` + `kube`).

> **Fallback if a single published crate is mandatory:** move the generated output under a private `mod generated;` (not `pub`) inside the wrapper crate and expose it only via `pub mod raw`. Do **not** interleave hand-written modules with generated files under `.openapi-generator-ignore` — that forces you to hand-manage the generated `lib.rs` visibility on every regeneration.

### 2.2 Wrapper module tree

```
ably_chat/                           # crate ably-chat-rs, lib `ably_chat`
├── lib.rs                           # crate docs, re-exports, #![cfg_attr(docsrs, ...)]
├── client.rs                        # Client, ClientBuilder, Inner, Auth, host resolution
├── executor.rs                      # apply_common(), dispatch() (one HTTP call path), retry + fallback rotation
├── room.rs                          # Room handle
├── messages.rs                      # Messages handle + SendMessage/UpdateMessage/DeleteMessage/History/Versions builders
├── reactions.rs                     # Reactions handle + Reaction / ReactionKey enums + ReactionSummary builder
├── occupancy.rs                     # occupancy accessor
├── page.rs                          # Page<T>, Links, RFC 5988 parsing, Stream adaptors
├── types.rs                         # OWNED domain types (Message, Serial, Timestamp, ...) + From<raw::models::*>
├── error.rs                         # Error, ApiError, ChatErrorCode, Result
├── prelude.rs                       # lean glob-import surface (incl. StreamExt/TryStreamExt/BoxStream)
└── raw.rs                           # `mod raw` — re-exports from the ably_chat_openapi dependency crate
```

`raw.rs` is the whole escape hatch, and it is a module (`mod raw;` in `lib.rs`) re-exporting from the dependency crate by its real name — so there is no crate-vs-module name collision:

```rust
// raw.rs
pub use ably_chat_openapi::{apis, models};
pub use ably_chat_openapi::apis::configuration::Configuration;
```

All internal call sites (`raw::apis::urlencode`, `raw::models::SendMessageRequest`, …) resolve through this module.

**Re-export vs wrap decision table**

| Generated item | Wrapper treatment | Why |
|---|---|---|
| `apis::*_api::*` free fns | **hidden** (used only to seed `raw::`) | replaced by room-scoped handles + owned executor |
| `apis::configuration::Configuration` | **wrapped** (built internally), re-exposed only under `raw::` | secrets, host resolution, retry live in `ClientBuilder` |
| `apis::Error<T>`, per-op `*Error` enums | **hidden**, collapsed | one public `Error` keeps the surface stable across regen |
| `models::*Request` (Send/Update/Delete/SendReaction) | **hidden** behind builders/enums | exactly one way to construct each call |
| `models::Message`, `MessageVersion`, `MessageReactions`, `ClientIdList`, `ClientIdCounts`, `Occupancy` | **re-modelled** as owned `types::*` with `From<raw::models::*>` | forward-compat, newtypes, `u64`, no `Box`, `#[non_exhaustive]` |
| `models::MessageReactionType` | **re-modelled** as `ReactionType` (de-stutter), kept closed | serialize-only value |
| `models::MessageAction` | **re-modelled** with an `Other(String)` fallback | closed generated enum hard-fails on unknown server values |
| `apis::urlencode` | **reused** internally | correct path-segment encoding |

---

## 3. Client construction & configuration

### 3.1 The builder

```rust
use std::{sync::Arc, time::Duration};
use secrecy::{ExposeSecret, SecretString};

/// O(1)-clone handle; pass by value, store in structs, spawn freely.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

// Public types eagerly implement Debug (C-DEBUG); forward through the Arc to the
// redacting Inner so the credential can never be printed.
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.inner, f)
    }
}

struct Inner {
    http: reqwest::Client,     // reqwest is already Arc-backed; never re-wrap or rebuild per call
    auth: Auth,
    hosts: Vec<String>,        // full `https://host` URLs, primary first
    user_agent: String,
    retry: RetryConfig,
}

// Hand-written, redacting Debug. `Auth` holds a SecretString, so even a derived Debug would
// be safe today — but making redaction explicit keeps it safe if a plaintext field is ever added.
impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("hosts", &self.hosts)
            .field("retry", &self.retry)
            .field("credential", &"<redacted>")
            .finish_non_exhaustive()
    }
}

enum Auth {
    /// Ably API key -> HTTP Basic (keyName = username, keySecret = password).
    ApiKey { key_name: String, key_secret: SecretString },
    /// Ably Token / Ably JWT / external JWT -> `Authorization: Bearer`.
    Bearer(SecretString),
}

/// `#[non_exhaustive]` + builder-style setters so fields can grow without a breaking change.
/// Read the fields directly; construct via `RetryConfig::default().with_*(..)`.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_backoff: Duration,
}
impl RetryConfig {
    pub fn with_max_retries(mut self, n: u32) -> Self { self.max_retries = n; self }
    pub fn with_base_backoff(mut self, d: Duration) -> Self { self.base_backoff = d; self }
}
impl Default for RetryConfig {
    fn default() -> Self {
        Self { max_retries: 3, base_backoff: Duration::from_millis(100) }
    }
}
```

```rust
#[derive(Default)]
pub struct ClientBuilder {
    auth: Option<Auth>,
    environment: Option<String>,
    rest_host: Option<String>,          // explicit override -> DISABLES default public fallbacks
    fallback_hosts: Option<Vec<String>>,
    http_client: Option<reqwest::Client>,
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    user_agent: Option<String>,
    retry: RetryConfig,
}

impl Client {
    pub fn builder() -> ClientBuilder { ClientBuilder::default() }

    /// One-liner shortcut for an API key in `keyName:keySecret` form.
    pub fn new(api_key: &str) -> Result<Self, BuildError> {
        Self::builder().key(api_key)?.build()
    }

    /// Pure construction — no I/O, no attach, no lifecycle.
    pub fn room(&self, name: impl Into<String>) -> Room {
        Room { client: self.clone(), name: Arc::from(name.into()) }
    }
}

impl ClientBuilder {
    /// Ably API key `keyName:keySecret` -> HTTP Basic (split on the FIRST ':').
    pub fn key(mut self, key: &str) -> Result<Self, BuildError> {
        let (name, secret) = key.split_once(':').ok_or(BuildError::MalformedApiKey)?;
        self.auth = Some(Auth::ApiKey {
            key_name: name.to_owned(),
            key_secret: SecretString::from(secret.to_owned()),
        });
        Ok(self)
    }

    /// Ably Token or JWT -> `Authorization: Bearer`.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(Auth::Bearer(SecretString::from(token.into())));
        self
    }

    pub fn environment(mut self, env: impl Into<String>) -> Self { self.environment = Some(env.into()); self }
    pub fn rest_host(mut self, host: impl Into<String>) -> Self { self.rest_host = Some(host.into()); self }
    pub fn fallback_hosts(mut self, hosts: Vec<String>) -> Self { self.fallback_hosts = Some(hosts); self }

    /// Inject your own transport. WINS over `.timeout`/`.connect_timeout`/`.user_agent`,
    /// which only shape a client the wrapper would otherwise build.
    pub fn http_client(mut self, c: reqwest::Client) -> Self { self.http_client = Some(c); self }
    pub fn timeout(mut self, d: Duration) -> Self { self.timeout = Some(d); self }
    pub fn connect_timeout(mut self, d: Duration) -> Self { self.connect_timeout = Some(d); self }
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self { self.user_agent = Some(ua.into()); self }
    pub fn retry(mut self, r: RetryConfig) -> Self { self.retry = r; self }

    pub fn build(self) -> Result<Client, BuildError> {
        let auth = self.auth.ok_or(BuildError::MissingCredential)?;

        // Host resolution (Ably RSC15): explicit host or custom environment DISABLES the
        // default public fallbacks (never fail a private/sandbox cluster over to production).
        let (primary, fallbacks) = match (self.rest_host, self.environment) {
            (Some(host), _)   => (host, self.fallback_hosts.unwrap_or_default()),
            (None, Some(env)) => (format!("{env}-rest.ably.io"), self.fallback_hosts.unwrap_or_default()),
            (None, None)      => (
                DEFAULT_HOST.to_owned(),
                self.fallback_hosts.unwrap_or_else(|| DEFAULT_FALLBACKS.iter().map(|h| h.to_string()).collect()),
            ),
        };
        let mut hosts = vec![ensure_https(&primary)];
        hosts.extend(fallbacks.iter().map(|h| ensure_https(h)));

        let user_agent = self.user_agent
            .unwrap_or_else(|| concat!("ably-chat-rs/", env!("CARGO_PKG_VERSION")).to_owned());

        // Transport: caller-supplied client wins and bypasses timeout/UA options by design.
        let http = match self.http_client {
            Some(c) => c,
            None => {
                let mut b = reqwest::Client::builder().user_agent(&user_agent);
                if let Some(t) = self.timeout { b = b.timeout(t); }
                if let Some(t) = self.connect_timeout { b = b.connect_timeout(t); }
                b.build().map_err(BuildError::Transport)?
            }
        };

        Ok(Client { inner: Arc::new(Inner { http, auth, hosts, user_agent, retry: self.retry }) })
    }
}

const DEFAULT_HOST: &str = "rest.ably.io";
const DEFAULT_FALLBACKS: [&str; 5] = [
    "a.ably-realtime.com", "b.ably-realtime.com", "c.ably-realtime.com",
    "d.ably-realtime.com", "e.ably-realtime.com",
];
pub(crate) const ABLY_VERSION: &str = "4";

fn ensure_https(host: &str) -> String {
    if host.starts_with("http://") || host.starts_with("https://") {
        host.to_owned()
    } else {
        format!("https://{host}")
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    #[error("a credential (API key or token) is required")]
    MissingCredential,
    #[error("API key must be in `keyName:keySecret` form")]
    MalformedApiKey,
    #[error("failed to build the HTTP transport")]
    Transport(#[source] reqwest::Error),
}
```

### 3.2 Usage

```rust
use ably_chat::{Client, RetryConfig};
use std::time::Duration;

// Simplest: API key -> HTTP Basic. X-Ably-Version: 4 is injected centrally.
let chat = Client::new("appId.keyId:keySecret")?;

// Full control.
let chat = Client::builder()
    .token(std::env::var("ABLY_TOKEN")?)     // Bearer token / JWT
    .environment("sandbox")                  // -> https://sandbox-rest.ably.io, fallbacks disabled
    .timeout(Duration::from_secs(10))
    .retry(RetryConfig::default().with_max_retries(5).with_base_backoff(Duration::from_millis(50)))
    .build()?;

// Bring your own transport (owns the connection pool + runtime); timeout/UA above are ignored.
let http = reqwest::Client::builder().pool_max_idle_per_host(32).build()?;
let chat = Client::builder().key("app.key:secret")?.http_client(http).build()?;
```

### 3.3 TLS features

TLS backend is chosen at **compile time**. Mirror the generated crate's existing manifest and forward to both crates. reqwest 0.13 renamed the 0.12-era `rustls-tls` to `rustls`; use the new names.

```toml
[features]
default    = ["native-tls"]                                            # matches the existing manifest
native-tls = ["ably-chat-openapi/native-tls", "reqwest/native-tls"]
rustls     = ["ably-chat-openapi/rustls",     "reqwest/rustls"]        # pulls aws-lc-rs (C toolchain + cmake)
# Optional pure-Rust / musl path if you switch to rustls:
# ring     = ["ably-chat-openapi/rustls",     "reqwest/rustls", "reqwest/rustls-no-provider"]
chrono     = ["dep:chrono"]
time       = ["dep:time"]
```

`default-features = false` on reqwest means HTTPS is unavailable unless a backend is enabled — that is why `native-tls` is the default. If a downstream wants `rustls`, they set `default-features = false` and enable it; document this no-default story. reqwest's runtime `tls_backend_*()` selectors only pick among backends already compiled in.

---

## 4. Public API surface — all ten endpoints

### 4.1 Handle chain

```
Client
 └─ .room(name) -> Room
      ├─ .messages() -> Messages
      │    ├─ .send(text)            -> SendMessage      (IntoFuture -> Message)      [sendMessage]
      │    ├─ .get(serial)           -> async  -> Message                             [getMessage]
      │    ├─ .update(serial, text)  -> UpdateMessage    (IntoFuture -> Message)      [updateMessage]
      │    ├─ .delete(serial)        -> DeleteMessage    (IntoFuture -> Message)      [deleteMessage]
      │    ├─ .history()             -> History          (IntoFuture -> Page<Message>, .into_stream(), .into_pages())  [getMessages]
      │    ├─ .versions(serial)      -> Versions         (IntoFuture -> Page<Message>, .into_stream(), .into_pages())  [getMessageVersions]
      │    └─ .reactions() -> Reactions
      │         ├─ .send(serial, into Reaction)  -> async -> ()                       [sendMessageReaction]
      │         ├─ .delete(serial, ReactionKey)  -> async -> ()                       [deleteMessageReaction]
      │         │    (+ .delete_unique / .delete_distinct conveniences)
      │         └─ .summary(serial)              -> ReactionSummary (IntoFuture -> MessageReactions)  [getClientReactions]
      └─ .occupancy() -> async -> Occupancy                                           [getOccupancy]
```

All ten operations route through one internal executor (`§4.8`); `raw::apis::*` is reserved for the escape hatch.

Because these handles are cheap `Clone`s, bind the one you use repeatedly instead of re-typing the chain:

```rust
let messages = room.messages();
messages.send("hi").await?;
let page = messages.history().await?;
```

### 4.2 Handles

```rust
use reqwest::Method;

#[derive(Clone)]
pub struct Room { client: Client, name: Arc<str> }

impl Room {
    pub fn name(&self) -> &str { &self.name }
    pub fn messages(&self) -> Messages { Messages { room: self.clone() } }

    /// [getOccupancy] — zero optional inputs, so a plain async method.
    pub async fn occupancy(&self) -> Result<Occupancy> {
        self.client
            .execute_json(Method::GET, |base| {
                format!("{base}/chat/v4/rooms/{}/occupancy", raw::apis::urlencode(&self.name))
            }, RequestBody::none())
            .await
    }
}

#[derive(Clone)]
pub struct Messages { room: Room }

impl Messages {
    /// [getMessage] — zero optionals, plain async, bare `.await`.
    pub async fn get(&self, serial: impl Into<Serial>) -> Result<Message> {
        let serial = serial.into();
        self.room.client
            .execute_json(Method::GET, |base| {
                format!("{base}/chat/v4/rooms/{}/messages/{}",
                        raw::apis::urlencode(self.room.name()), raw::apis::urlencode(serial.as_str()))
            }, RequestBody::none())
            .await
    }

    pub fn send(&self, text: impl Into<String>) -> SendMessage { SendMessage::new(self.clone(), text.into()) }
    pub fn update(&self, serial: impl Into<Serial>, text: impl Into<String>) -> UpdateMessage {
        UpdateMessage::new(self.clone(), serial.into(), text.into())
    }
    pub fn delete(&self, serial: impl Into<Serial>) -> DeleteMessage { DeleteMessage::new(self.clone(), serial.into()) }
    pub fn history(&self) -> History { History::new(self.clone()) }
    pub fn versions(&self, serial: impl Into<Serial>) -> Versions { Versions::new(self.clone(), serial.into()) }
    pub fn reactions(&self) -> Reactions { Reactions { messages: self.clone() } }
}
```

Every `serial` / `client_id` parameter is `impl Into<Serial>` / `impl Into<ClientId>`. Because the id newtypes implement `From<&Serial>` (a cheap clone) as well as `From<&str>`/`From<String>` (see [§5.1](#51-identifiers)), a `Serial` you already hold works borrowed (`update(&msg.serial, …)`), owned, or as a bare string — with no `.clone()`/`.as_str()` noise at the call site.

### 4.3 `sendMessage` — builder terminating in bare `.await`

```rust
use std::collections::HashMap;
use futures::future::BoxFuture;

#[must_use = "a request does nothing until awaited"]
pub struct SendMessage {
    messages: Messages,
    text: String,
    metadata: Option<Metadata>,
    headers: Option<HashMap<String, String>>,
    idempotency_key: Option<String>,
}

impl SendMessage {
    pub fn metadata(mut self, m: impl Into<Metadata>) -> Self { self.metadata = Some(m.into()); self }
    pub fn headers(mut self, h: HashMap<String, String>) -> Self { self.headers = Some(h); self }
    /// Per-entry adder so callers rarely build a HashMap by hand.
    pub fn header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.headers.get_or_insert_with(HashMap::new).insert(k.into(), v.into());
        self
    }
    pub fn idempotency_key(mut self, k: impl Into<String>) -> Self { self.idempotency_key = Some(k.into()); self }
}

impl IntoFuture for SendMessage {
    type Output = Result<Message>;
    type IntoFuture = BoxFuture<'static, Result<Message>>;
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let body = raw::models::SendMessageRequest {
                text: self.text,
                metadata: self.metadata.map(Metadata::into_wire),  // Option<HashMap<String, Value>>
                headers: self.headers,
            };
            // Auto-mint a stable idempotency key so fallback-retry of this POST cannot duplicate.
            let idem = self.idempotency_key
                .or_else(|| self.messages.room.client.auto_idempotency_key());
            self.messages.room.client
                .execute_json(Method::POST, |base| {
                    format!("{base}/chat/v4/rooms/{}/messages",
                            raw::apis::urlencode(self.messages.room.name()))
                }, RequestBody::json(&body)?.idempotency(idem))
                .await
        })
    }
}
```

```rust
// Usage
let msg: Message = room.messages()
    .send("hello world")
    .header("lang", "en")
    .metadata(Metadata::try_from(serde_json::json!({ "priority": "high" }))?)  // TryFrom rejects non-objects
    .await?;
println!("{} @ {}", msg.serial, msg.timestamp.as_millis());
```

### 4.4 `updateMessage` and `deleteMessage`

`updateMessage` has **two** metadata concepts of **different types**, which is why `.metadata()` is reserved exclusively for message JSON metadata across *all* builders and the string-map is always `.operation_metadata()`:

- `message.metadata` — open JSON (`Metadata`), set by `.metadata()`.
- top-level *operation* metadata — a string map (`OperationMetadata`), set by `.operation_metadata()`.

> **updateMessage is a full replace, not a partial patch.** The spec (`updateMessage`, "All fields under `message` are replaced; any omitted `message` field is reset to empty") means an unset `.metadata()` or `.headers()` **wipes** the message's existing values server-side. To edit text while preserving metadata/headers, refetch with `Messages::get`, then re-supply them. This is surfaced again on the setters' doc-comments below.

```rust
#[must_use = "a request does nothing until awaited"]
pub struct UpdateMessage {
    messages: Messages,
    serial: Serial,
    text: String,                                  // required (message.text)
    msg_metadata: Option<Metadata>,                // message.metadata (open JSON)
    msg_headers: Option<HashMap<String, String>>,  // message.headers
    description: Option<String>,                   // top-level op description
    op_metadata: Option<OperationMetadata>,        // top-level op metadata (STRING map)
    idempotency_key: Option<String>,
}

impl UpdateMessage {
    /// Sets `message.metadata`. FULL REPLACE: if you omit this, the server clears any existing
    /// message metadata. Preserve it by refetching and re-supplying.
    pub fn metadata(mut self, m: impl Into<Metadata>) -> Self { self.msg_metadata = Some(m.into()); self }
    /// Sets `message.headers`. FULL REPLACE, same wipe-on-omit behaviour as `metadata`.
    pub fn headers(mut self, h: HashMap<String, String>) -> Self { self.msg_headers = Some(h); self }
    pub fn description(mut self, d: impl Into<String>) -> Self { self.description = Some(d.into()); self }
    /// Operation metadata (a string map), distinct from `message.metadata`.
    pub fn operation_metadata(mut self, m: impl Into<OperationMetadata>) -> Self { self.op_metadata = Some(m.into()); self }
    pub fn operation_metadata_entry(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.op_metadata.get_or_insert_with(OperationMetadata::new).insert(k.into(), v.into());
        self
    }
    pub fn idempotency_key(mut self, k: impl Into<String>) -> Self { self.idempotency_key = Some(k.into()); self }
}

impl IntoFuture for UpdateMessage {
    type Output = Result<Message>;
    type IntoFuture = BoxFuture<'static, Result<Message>>;
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let body = raw::models::UpdateMessageRequest {
                message: Box::new(raw::models::UpdateMessageRequestMessage {
                    text: self.text,
                    metadata: self.msg_metadata.map(Metadata::into_wire),
                    headers: self.msg_headers,
                }),
                description: self.description,
                metadata: self.op_metadata.map(op_metadata_into_wire),  // BTreeMap -> HashMap<String,String>
            };
            let idem = self.idempotency_key.or_else(|| self.messages.room.client.auto_idempotency_key());
            self.messages.room.client
                .execute_json(Method::PUT, |base| {
                    format!("{base}/chat/v4/rooms/{}/messages/{}",
                            raw::apis::urlencode(self.messages.room.name()),
                            raw::apis::urlencode(self.serial.as_str()))
                }, RequestBody::json(&body)?.idempotency(idem))
                .await
        })
    }
}
```

`deleteMessage` is a **soft delete** — a `POST` to `/delete`, returning the new `message.delete` version (typed `-> Message`, not `-> ()`). Its body is optional; because we own the call we always send a concrete `{}` (never a literal JSON `null`).

```rust
#[must_use = "a request does nothing until awaited"]
pub struct DeleteMessage {
    messages: Messages,
    serial: Serial,
    description: Option<String>,
    op_metadata: Option<OperationMetadata>,     // operation metadata (STRING map)
    idempotency_key: Option<String>,
}
impl DeleteMessage {
    pub fn description(mut self, d: impl Into<String>) -> Self { self.description = Some(d.into()); self }
    /// Operation metadata (a string map). Named `operation_metadata` on every builder;
    /// `.metadata()` is reserved for message JSON metadata, which delete does not have.
    pub fn operation_metadata(mut self, m: impl Into<OperationMetadata>) -> Self { self.op_metadata = Some(m.into()); self }
    pub fn operation_metadata_entry(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.op_metadata.get_or_insert_with(OperationMetadata::new).insert(k.into(), v.into());
        self
    }
    pub fn idempotency_key(mut self, k: impl Into<String>) -> Self { self.idempotency_key = Some(k.into()); self }
}
impl IntoFuture for DeleteMessage {
    type Output = Result<Message>;
    type IntoFuture = BoxFuture<'static, Result<Message>>;
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            // Always a concrete object so we never POST `null`.
            let body = raw::models::DeleteMessageRequest {
                description: self.description,
                metadata: self.op_metadata.map(op_metadata_into_wire),
            };
            let idem = self.idempotency_key.or_else(|| self.messages.room.client.auto_idempotency_key());
            self.messages.room.client
                .execute_json(Method::POST, |base| {
                    format!("{base}/chat/v4/rooms/{}/messages/{}/delete",
                            raw::apis::urlencode(self.messages.room.name()),
                            raw::apis::urlencode(self.serial.as_str()))
                }, RequestBody::json(&body)?.idempotency(idem))
                .await
        })
    }
}
```

```rust
// Usage
let edited = room.messages()
    .update(&msg.serial, "hello (edited)")   // &Serial works: From<&Serial> is a cheap clone
    .description("fix typo")
    .await?;
assert_eq!(edited.action, MessageAction::Update);

let deleted = room.messages().delete(&msg.serial).description("spam").await?;
assert_eq!(deleted.action, MessageAction::Delete);
```

### 4.5 `getMessages` (history) and `getMessageVersions` — paginated

`Direction` pins its wire form to the spec's lowercase enum (`[forwards, backwards]`) via `as_query()`, never a derived/`Debug` rendering. The wrapper defaults `direction` to `Backwards` (recent-first), matching both the JS SDK and the spec's own `default: backwards` — so a zero-config `history().await` behaves identically across SDKs.

```rust
use futures::stream::BoxStream;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Forwards, Backwards }
impl Direction {
    /// Exact wire form; never `Debug`/derive.
    pub(crate) fn as_query(self) -> &'static str {
        match self { Self::Forwards => "forwards", Self::Backwards => "backwards" }
    }
}

#[must_use = "a request does nothing until awaited or streamed"]
pub struct History {
    messages: Messages,
    start: Option<Timestamp>,
    end: Option<Timestamp>,
    direction: Option<Direction>,
    limit: Option<u32>,
    from_serial: Option<Serial>,
}
impl History {
    /// Earliest timestamp to include (inclusive). Accepts a `Timestamp`, a bare epoch-ms `i64`,
    /// or (with the `chrono` feature) a `DateTime<Utc>` — all via `Into<Timestamp>`.
    pub fn start(mut self, t: impl Into<Timestamp>) -> Self { self.start = Some(t.into()); self }
    /// Latest timestamp to include (exclusive).
    pub fn end(mut self, t: impl Into<Timestamp>) -> Self { self.end = Some(t.into()); self }
    pub fn direction(mut self, d: Direction) -> Self { self.direction = Some(d); self }
    pub fn backwards(self) -> Self { self.direction(Direction::Backwards) }
    pub fn forwards(self) -> Self { self.direction(Direction::Forwards) }
    /// Maximum messages per PAGE (spec: 1..=1000, default 100). This caps the page, NOT the
    /// total: `into_stream()` follows `next` to end-of-history. For a bounded total use
    /// `into_stream().take(n)`.
    pub fn page_size(mut self, n: u32) -> Self { self.limit = Some(n); self }
    pub fn from_serial(mut self, s: impl Into<Serial>) -> Self { self.from_serial = Some(s.into()); self }

    /// One page (entry point for manual paging). Rotates fallback hosts on the FIRST request.
    pub async fn page(self) -> Result<Page<Message>> {
        let direction = self.direction.unwrap_or(Direction::Backwards);   // JS/spec parity
        let (start, end, limit, from) = (self.start, self.end, self.limit, self.from_serial.clone());
        let room = self.messages.room.clone();
        let resp = room.client.dispatch(Method::GET, |base| {
            let mut url = format!("{base}/chat/v4/rooms/{}/messages?direction={}",
                raw::apis::urlencode(room.name()), direction.as_query());
            if let Some(l) = limit { url.push_str(&format!("&limit={l}")); }
            if let Some(s) = start { url.push_str(&format!("&start={}", s.as_millis())); }
            if let Some(e) = end   { url.push_str(&format!("&end={}", e.as_millis())); }
            if let Some(f) = &from { url.push_str("&fromSerial="); url.push_str(&raw::apis::urlencode(f.as_str())); }
            url
        }, RequestBody::none()).await?;
        page_from(&room.client, resp)
    }

    /// Auto-follows rel="next" and yields individual messages. `'static + Send` -> spawnable.
    pub fn into_stream(self) -> BoxStream<'static, Result<Message>> { /* self.page() then Page::into_stream — §7 */ }
    /// Page-granularity stream (preserves per-page metadata/backpressure).
    pub fn into_pages(self) -> BoxStream<'static, Result<Page<Message>>> { /* §7 */ }
}
impl IntoFuture for History {
    type Output = Result<Page<Message>>;
    type IntoFuture = BoxFuture<'static, Result<Page<Message>>>;
    fn into_future(self) -> Self::IntoFuture { Box::pin(self.page()) }
}
```

`Versions` is identical minus the query setters (it carries a `serial`, has no filters, and is unaffected by the `direction` default; it is still paginated, so it exposes the same three terminals plus `IntoFuture`).

```rust
// Bare `.await` -> first page (defaults to backwards / recent-first):
let page: Page<Message> = room.messages().history().page_size(50).await?;
for m in page.items() { println!("{}: {}", m.client_id, m.text); }

// Transparent item stream (spawnable because it is 'static + Send).
// The prelude re-exports StreamExt/TryStreamExt, so try_next/take/try_collect work out of the box.
use ably_chat::prelude::*;
let mut stream = room.messages().history().forwards().into_stream();
while let Some(msg) = stream.try_next().await? {
    println!("{}", msg.text);
}

// Bounded pull without paginating an entire room:
let recent: Vec<Message> = room.messages().history()
    .into_stream()      // backwards by default
    .take(500)
    .try_collect()
    .await?;

// Version history of one message:
let versions: Vec<Message> = room.messages()
    .versions(&msg.serial)
    .into_stream()
    .try_collect()
    .await?;
```

### 4.6 Reactions

`sendMessageReaction`, `deleteMessageReaction`, `getClientReactions`. The `type`/`name`/`count` coupling is encoded in the type system so illegal states are unrepresentable.

```rust
/// Send: `name` required for all types; `count` applies only to `multiple`.
pub enum Reaction {
    Unique(String),
    Distinct(String),
    Multiple { name: String, count: u32 },
}
impl Reaction {
    pub fn unique(name: impl Into<String>) -> Self { Self::Unique(name.into()) }
    pub fn distinct(name: impl Into<String>) -> Self { Self::Distinct(name.into()) }
    pub fn multiple(name: impl Into<String>, count: u32) -> Self { Self::Multiple { name: name.into(), count } }
}
// A bare string is a DISTINCT reaction — the JS SDK's default reaction type.
impl From<&str>   for Reaction { fn from(name: &str)   -> Self { Self::Distinct(name.to_owned()) } }
impl From<String> for Reaction { fn from(name: String) -> Self { Self::Distinct(name) } }

/// Delete key: `name` required only for distinct/multiple, absent for unique. No count.
/// Named for intent (the *key* identifying which reaction to remove), distinct from `Reaction`.
pub enum ReactionKey {
    Unique,
    Distinct(String),
    Multiple(String),
}

#[derive(Clone)]
pub struct Reactions { messages: Messages }

impl Reactions {
    /// [sendMessageReaction] -> 201, no body. Accepts `"👍"` (distinct), or an explicit
    /// `Reaction::unique/distinct/multiple`.
    pub async fn send(&self, serial: impl Into<Serial>, reaction: impl Into<Reaction>) -> Result<()> {
        let serial = serial.into();
        let (ty, name, count) = match reaction.into() {
            Reaction::Unique(n)                => (ReactionType::Unique, n, None),
            Reaction::Distinct(n)              => (ReactionType::Distinct, n, None),
            Reaction::Multiple { name, count } => (ReactionType::Multiple, name, Some(count as i32)),
        };
        let body = raw::models::SendMessageReactionRequest { r#type: ty.into_wire(), name, count };
        // POST, no idempotency key -> not auto-retried on ambiguous failures (see §4.8):
        // a `multiple` reaction increments a counter, so a blind retry could double-count.
        self.messages.room.client
            .execute_empty(Method::POST, |base| {
                format!("{base}/chat/v4/rooms/{}/messages/{}/reactions",
                        raw::apis::urlencode(self.messages.room.name()), raw::apis::urlencode(serial.as_str()))
            }, RequestBody::json(&body)?)
            .await
    }

    /// [deleteMessageReaction] -> 204, no body. `type` is a required query param, `name` optional.
    /// DELETE is idempotent, so it is safe to auto-retry.
    pub async fn delete(&self, serial: impl Into<Serial>, key: ReactionKey) -> Result<()> {
        let serial = serial.into();
        let (ty, name) = match key {
            ReactionKey::Unique      => (ReactionType::Unique, None),
            ReactionKey::Distinct(n) => (ReactionType::Distinct, Some(n)),
            ReactionKey::Multiple(n) => (ReactionType::Multiple, Some(n)),
        };
        self.messages.room.client
            .execute_empty(Method::DELETE, |base| {
                // `type=` serializes to the lowercase MessageReactionType wire form.
                let mut url = format!("{base}/chat/v4/rooms/{}/messages/{}/reactions?type={}",
                    raw::apis::urlencode(self.messages.room.name()),
                    raw::apis::urlencode(serial.as_str()),
                    ty.as_query());
                if let Some(n) = &name { url.push_str("&name="); url.push_str(&raw::apis::urlencode(n)); }
                url
            }, RequestBody::none())
            .await
    }

    pub async fn delete_unique(&self, serial: impl Into<Serial>) -> Result<()> {
        self.delete(serial, ReactionKey::Unique).await
    }
    pub async fn delete_distinct(&self, serial: impl Into<Serial>, name: impl Into<String>) -> Result<()> {
        self.delete(serial, ReactionKey::Distinct(name.into())).await
    }

    /// [getClientReactions] -> the message's reaction summary, optionally narrowed to one client.
    pub fn summary(&self, serial: impl Into<Serial>) -> ReactionSummary {
        ReactionSummary { reactions: self.clone(), serial: serial.into(), for_client_id: None }
    }
}

#[must_use = "a request does nothing until awaited"]
pub struct ReactionSummary { reactions: Reactions, serial: Serial, for_client_id: Option<ClientId> }
impl ReactionSummary {
    /// Optional filter narrowing the summary to a single client's reactions.
    pub fn for_client_id(mut self, id: impl Into<ClientId>) -> Self { self.for_client_id = Some(id.into()); self }
}
impl IntoFuture for ReactionSummary {
    type Output = Result<MessageReactions>;
    type IntoFuture = BoxFuture<'static, Result<MessageReactions>>;
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let ReactionSummary { reactions, serial, for_client_id } = self;
            reactions.messages.room.client
                .execute_json(Method::GET, |base| {
                    let mut url = format!("{base}/chat/v4/rooms/{}/messages/{}/client-reactions",
                        raw::apis::urlencode(reactions.messages.room.name()),
                        raw::apis::urlencode(serial.as_str()));
                    if let Some(id) = &for_client_id {
                        url.push_str("?forClientId=");
                        url.push_str(&raw::apis::urlencode(id.as_str()));
                    }
                    url
                }, RequestBody::none())
                .await
        })
    }
}
```

```rust
// Usage
room.messages().reactions().send(&msg.serial, "👍").await?;                       // defaults to distinct
room.messages().reactions().send(&msg.serial, Reaction::multiple("🔥", 3)).await?;
room.messages().reactions().delete_distinct(&msg.serial, "👍").await?;

let summary: MessageReactions = room.messages()
    .reactions()
    .summary(&msg.serial)
    .for_client_id("user-123")
    .await?;
// Borrow<str> lets you look up by the bare emoji, no ReactionName wrapping:
if let Some(list) = summary.distinct.get("👍") {
    println!("{} clients", list.total);
}
```

### 4.7 Occupancy

```rust
let occ: Occupancy = room.occupancy().await?;
println!("connections={} present={}", occ.connections, occ.presence_members);
```

### 4.8 The one call path

Every operation funnels through a single executor (`dispatch`) that applies auth + `X-Ably-Version` + user-agent, retries transient failures with backoff, and maps the Ably error envelope. This is the same machinery pagination requires; keeping it single-path means the retry and error-mapping *machinery* is shared by all ten endpoints.

**The retry *path* is uniform; the retry-*safety policy* and host rotation are not** — one predicate governs both, and by design it does not retry non-idempotent reaction sends on ambiguous failures (below). Non-paginated calls rotate through fallback hosts across attempts. Paginated follow-ups (`getMessages`/`getMessageVersions` beyond the first page) stay on the region that issued the cursor — pagination cursors and serials are region-scoped, so failing a `next` link over to another region is meaningless. The first page of a paginated call *does* rotate.

**Idempotency.** Per the spec, `idempotencyKey` is a *query* parameter on exactly three endpoints — `sendMessage`, `updateMessage`, `deleteMessage` — and there is no idempotency mechanism on reactions or reads. The wrapper auto-mints a UUID key for those three writes (unless the caller supplied one), and reuses the **same** key across every retry attempt, so fallback-retry cannot duplicate a write.

**Retry-safety predicate.** A request is safe to retry on an *ambiguous* failure iff its HTTP method is idempotent (`GET`/`DELETE`) or it carries an idempotency key. Connect-phase transport failures (the request provably never reached the server) are always retried and may rotate hosts — safe even for non-idempotent writes. Consequences:
- `sendMessage`/`updateMessage`/`deleteMessage`: carry a key -> retried on all transient failures.
- `getMessage`/`history`/`versions`/`occupancy`/`summary`: `GET` -> retried.
- `deleteMessageReaction`: `DELETE`, idempotent -> retried.
- `sendMessageReaction`: `POST`, no key -> **not** retried on ambiguous failures (timeout, 5xx). A `multiple` reaction increments a counter, so this avoids double-counting. It is still retried on connect-phase failures (which cannot double-count); net semantics are **at-least-once**.

```rust
use reqwest::{Method, StatusCode, header::{CONTENT_TYPE, LINK, RETRY_AFTER, HeaderMap}};
use std::time::Duration;
use url::Url;

/// Internal request-body descriptor. Built via `none()` / `json()`; the JSON variant is
/// serialized eagerly so the executor can cheaply clone the bytes per retry attempt.
pub(crate) enum RequestBody {
    None,
    Json { bytes: Vec<u8>, idempotency_key: Option<String> },
}
impl RequestBody {
    pub(crate) fn none() -> Self { Self::None }
    pub(crate) fn json<B: serde::Serialize + ?Sized>(value: &B) -> Result<Self> {
        Ok(Self::Json { bytes: serde_json::to_vec(value)?, idempotency_key: None })
    }
    pub(crate) fn idempotency(mut self, key: Option<String>) -> Self {
        if let Self::Json { idempotency_key, .. } = &mut self { *idempotency_key = key; }
        self
    }
    fn idempotency_key(&self) -> Option<&str> {
        match self { Self::Json { idempotency_key, .. } => idempotency_key.as_deref(), Self::None => None }
    }
}

/// Everything downstream needs from a response, snapshotted BEFORE the body is consumed.
pub(crate) struct HttpResponse { pub status: StatusCode, pub headers: HeaderMap, pub body: String, pub url: Url }

impl Client {
    pub(crate) fn http(&self) -> &reqwest::Client { &self.inner.http }

    /// Applies everything the generated fns apply inline (auth, version, UA) so a hand-written
    /// request is never sent unauthenticated. `raw::apis::*` is not on this path.
    pub(crate) fn apply_common(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let rb = rb
            .header("X-Ably-Version", ABLY_VERSION)
            .header(reqwest::header::USER_AGENT, &self.inner.user_agent);
        match &self.inner.auth {
            Auth::ApiKey { key_name, key_secret } =>
                rb.basic_auth(key_name, Some(key_secret.expose_secret())),
            Auth::Bearer(tok) => rb.bearer_auth(tok.expose_secret()),
        }
    }

    /// Mint a stable UUID idempotency key for retryable writes when the caller omitted one.
    pub(crate) fn auto_idempotency_key(&self) -> Option<String> {
        (self.inner.retry.max_retries > 0).then(|| uuid::Uuid::new_v4().to_string())
    }

    /// One attempt against a fully-formed base URL. Appends `idempotencyKey`, sets body + headers,
    /// and snapshots status/headers/final-url before consuming the body (so the error path and the
    /// paginator both keep the headers the generated client would have discarded).
    async fn try_once(&self, method: &Method, url: &str, body: &RequestBody) -> Result<HttpResponse> {
        let mut full = url.to_owned();
        if let Some(key) = body.idempotency_key() {
            full.push(if full.contains('?') { '&' } else { '?' });
            full.push_str("idempotencyKey=");
            full.push_str(&raw::apis::urlencode(key));
        }
        let mut rb = self.apply_common(self.inner.http.request(method.clone(), &full));
        if let RequestBody::Json { bytes, .. } = body {
            rb = rb.header(CONTENT_TYPE, "application/json").body(bytes.clone());
        }
        let resp = rb.send().await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let url = resp.url().clone();
        let body = resp.text().await?;          // consume LAST, after snapshotting
        Ok(HttpResponse { status, headers, body, url })
    }

    /// The single retrying/rotating path. The `url` closure maps a base host -> full URL, so
    /// per-attempt host rotation happens by calling it with each host in turn. (Paginated
    /// follow-ups pass a closure that ignores the host, pinning the region.)
    pub(crate) async fn dispatch<F>(&self, method: Method, url: F, body: RequestBody) -> Result<HttpResponse>
    where F: Fn(&str) -> String {
        let retry_safe = matches!(method, Method::GET | Method::DELETE) || body.idempotency_key().is_some();
        let max = self.inner.retry.max_retries;
        let mut last: Option<Error> = None;
        for attempt in 0..=max {
            let host = &self.inner.hosts[(attempt as usize) % self.inner.hosts.len()];
            let mut retry_after = None;
            match self.try_once(&method, &url(host), &body).await {
                Ok(resp) if resp.status.is_success() => return Ok(resp),
                Ok(resp) if retry_safe && is_transient_status(resp.status) => {
                    retry_after = parse_retry_after(&resp.headers);
                    last = Some(error_from(&resp));
                }
                Ok(resp) => return Err(error_from(&resp)),   // non-retryable status, or unsafe method
                Err(e) if e.is_connect() => last = Some(e),  // never sent -> safe to rotate/retry
                Err(e) if retry_safe && e.is_retryable() => last = Some(e),
                Err(e) => return Err(e),                     // ambiguous + unsafe -> surface
            }
            if attempt < max { backoff(&self.inner.retry, retry_after).await; }  // never sleep after the last try
        }
        Err(last.expect("the loop body runs at least once"))
    }

    /// GET/POST/PUT/DELETE with a JSON response.
    pub(crate) async fn execute_json<T, F>(&self, method: Method, url: F, body: RequestBody) -> Result<T>
    where T: serde::de::DeserializeOwned, F: Fn(&str) -> String {
        let resp = self.dispatch(method, url, body).await?;
        if !resp.status.is_success() { return Err(error_from(&resp)); }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Calls that return 201/204 with no body (reaction send/delete).
    pub(crate) async fn execute_empty<F>(&self, method: Method, url: F, body: RequestBody) -> Result<()>
    where F: Fn(&str) -> String {
        let resp = self.dispatch(method, url, body).await?;
        if !resp.status.is_success() { return Err(error_from(&resp)); }
        Ok(())
    }
}

fn is_transient_status(s: StatusCode) -> bool { matches!(s.as_u16(), 408 | 429 | 500 | 502 | 503 | 504) }

/// Shared error mapping: an Ably envelope -> `Error::Api`, anything else -> `Error::UnexpectedResponse`.
pub(crate) fn error_from(resp: &HttpResponse) -> Error {
    match ApiError::from_response(resp.status, &resp.headers, &resp.body) {
        Some(api) => Error::Api(api),
        None => Error::UnexpectedResponse {
            status: resp.status,
            message: "non-envelope error response".to_owned(),
            body: resp.body.clone(),
        },
    }
}

/// Exponential backoff honouring a server `Retry-After` when present. (Sleep impl elided; on
/// Tokio this is `tokio::time::sleep`. Never called after the final attempt — see `dispatch`.)
async fn backoff(cfg: &RetryConfig, retry_after: Option<Duration>) { /* ... */ }
```

---

## 5. Domain types & newtypes

Defined in `types.rs`, deserialized directly from the wire (never via a `From<generated>` that would fire the closed-enum failure first). Each also gets a `From<raw::models::*>` for the escape-hatch/bootstrap path.

### 5.1 Identifiers

`Serial` deliberately does **not** implement `Ord`/`PartialOrd`: a serial is chronological only *within a single region*, so ordering operators would be a silent cross-region correctness footgun. The id types that are used as map keys (`ClientId`, `ReactionName`, `RoomName`) opt into ordering via the `ord` arm; their `Ord` delegates to the inner string (a stable key order, not a chronology claim) and is consistent with the `Borrow<str>` used for lookups.

```rust
use serde::{Deserialize, Serialize};

macro_rules! newtype_id {
    ($name:ident) => { newtype_id!(@base $name); };
    ($name:ident, ord) => {
        newtype_id!(@base $name);
        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
        }
        impl Ord for $name {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering { self.0.cmp(&other.0) }
        }
    };
    (@base $name:ident) => {
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
        impl From<&str>   for $name { fn from(s: &str)   -> Self { Self(s.to_owned()) } }
        impl From<&$name> for $name { fn from(s: &$name) -> Self { s.clone() } }   // lets `&serial` satisfy Into
        impl AsRef<str>   for $name { fn as_ref(&self) -> &str { &self.0 } }
        impl std::borrow::Borrow<str> for $name { fn borrow(&self) -> &str { &self.0 } }  // map lookups by &str
    };
}

newtype_id!(Serial);            // opaque, region-scoped token; NO Ord (chronological within one region only)
newtype_id!(RoomName, ord);     // permissive: any string (rooms are implicit / channel-backed)
newtype_id!(ClientId, ord);
newtype_id!(ReactionName, ord); // emoji / name; used as a serde/BTreeMap key
```

`impl Into<Serial>` parameters therefore accept `&str`, `String`, `Serial`, and `&Serial` uniformly (three distinct `From` impls, no inference ambiguity), and `BTreeMap<ReactionName, _>::get("👍")` works via `Borrow<str>`.

### 5.2 Timestamp — transparent epoch-ms `i64`, additive datetime features

The wire uses `i64` milliseconds since the Unix epoch. The zero-dep core matches the wire byte-for-byte; `chrono`/`time` are purely additive, so enabling them is never a wire or semver break.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);      // milliseconds since the Unix epoch
impl Timestamp {
    pub fn as_millis(self) -> i64 { self.0 }
}
impl From<i64> for Timestamp { fn from(ms: i64) -> Self { Self(ms) } }   // lets `.start(1_700_000_000_000)` work

#[cfg(feature = "chrono")]
impl TryFrom<Timestamp> for chrono::DateTime<chrono::Utc> {   // ms -> datetime is FALLIBLE (range)
    type Error = Timestamp;
    fn try_from(t: Timestamp) -> std::result::Result<Self, Self::Error> {
        chrono::DateTime::from_timestamp_millis(t.0).ok_or(t)
    }
}
#[cfg(feature = "chrono")]
impl From<chrono::DateTime<chrono::Utc>> for Timestamp {       // datetime -> ms is INFALLIBLE
    fn from(dt: chrono::DateTime<chrono::Utc>) -> Self { Self(dt.timestamp_millis()) }
}
```

With the `chrono` feature, `history().start(some_datetime)` type-checks because `DateTime<Utc>: Into<Timestamp>`.

### 5.3 Enums — direction-aware forward-compat

`MessageAction` is a **response** value Ably may extend, so it tolerates and preserves unknowns. `ReactionType` is **serialize-only** (request body + `?type=` query) and stays closed — adding `Other` would let callers construct an unsendable value.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageAction { Create, Update, Delete, Other(String) }

impl MessageAction {
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Create => "message.create",
            Self::Update => "message.update",
            Self::Delete => "message.delete",
            Self::Other(s) => s,
        }
    }
}
impl<'de> Deserialize<'de> for MessageAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Ok(match String::deserialize(d)?.as_str() {   // normalize; never yields Other("message.create")
            "message.create" => Self::Create,
            "message.update" => Self::Update,
            "message.delete" => Self::Delete,
            other => Self::Other(other.to_owned()),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionType { Unique, Distinct, Multiple }   // no Default: a defaulted reaction type is a footgun
impl ReactionType {
    /// Lowercase wire form for the `?type=` query param.
    pub(crate) fn as_query(self) -> &'static str {
        match self { Self::Unique => "unique", Self::Distinct => "distinct", Self::Multiple => "multiple" }
    }
    pub(crate) fn into_wire(self) -> raw::models::MessageReactionType {
        match self {
            Self::Unique   => raw::models::MessageReactionType::Unique,
            Self::Distinct => raw::models::MessageReactionType::Distinct,
            Self::Multiple => raw::models::MessageReactionType::Multiple,
        }
    }
}
```

### 5.4 Metadata — opaque JSON, no generics

Message metadata is arbitrary JSON (`{ String -> Value }`); operation/version metadata is a string map. Preserve the split; never make the core types generic over a metadata parameter. Conversions are explicit about the object requirement: the infallible path takes a JSON object, and arbitrary `Value` goes through a fallible `TryFrom` that rejects non-objects loudly (no silent data loss).

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Metadata(pub serde_json::Map<String, serde_json::Value>);
impl Metadata {
    /// Wire shape for `SendMessageRequest`/`UpdateMessageRequestMessage` (`HashMap<String, Value>`).
    pub(crate) fn into_wire(self) -> std::collections::HashMap<String, serde_json::Value> {
        self.0.into_iter().collect()
    }
    /// Escape hatch for callers who know the shape.
    pub fn deserialize_as<T: serde::de::DeserializeOwned>(&self) -> serde_json::Result<T> {
        serde_json::from_value(serde_json::Value::Object(self.0.clone()))
    }
}
/// Canonical, infallible: a JSON object IS the metadata shape.
impl From<serde_json::Map<String, serde_json::Value>> for Metadata {
    fn from(m: serde_json::Map<String, serde_json::Value>) -> Self { Self(m) }
}
/// Fallible: arbitrary JSON must be an object. Non-objects are rejected — never silently dropped.
impl TryFrom<serde_json::Value> for Metadata {
    type Error = MetadataError;
    fn try_from(v: serde_json::Value) -> std::result::Result<Self, Self::Error> {
        match v {
            serde_json::Value::Object(m) => Ok(Self(m)),
            other => Err(MetadataError(other)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("metadata must be a JSON object")]
pub struct MetadataError(pub serde_json::Value);

/// Operation/version metadata is a STRING map on the wire, read and written the same way.
pub type OperationMetadata = std::collections::BTreeMap<String, String>;

/// Wire shape for the operation-metadata fields (`HashMap<String, String>`).
pub(crate) fn op_metadata_into_wire(m: OperationMetadata) -> std::collections::HashMap<String, String> {
    m.into_iter().collect()
}
```

### 5.5 Reaction summaries — default-empty maps, `u64` counts

The wire counts are `i32` and non-negative by definition; the owned types widen them to `u64` (a principled non-negative choice) and default missing sub-maps to empty maps, so callers do map lookups rather than `Option<HashMap>` gymnastics. The wrapper's `From<raw::models::*>` widens `i32 -> u64` and flattens `Option<HashMap> -> BTreeMap`.

```rust
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct MessageReactions {
    #[serde(default)] pub unique:   std::collections::BTreeMap<ReactionName, ClientIdList>,
    #[serde(default)] pub distinct: std::collections::BTreeMap<ReactionName, ClientIdList>,
    #[serde(default)] pub multiple: std::collections::BTreeMap<ReactionName, ClientIdCounts>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct ClientIdList {
    pub total: u64,
    #[serde(rename = "clientIds", default)] pub client_ids: Vec<ClientId>,
    #[serde(default)] pub clipped: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct ClientIdCounts {
    pub total: u64,
    #[serde(rename = "clientIds", default)] pub client_ids: std::collections::BTreeMap<ClientId, u64>,
    #[serde(rename = "totalUnidentified", default)] pub total_unidentified: u64,
    #[serde(default)] pub clipped: bool,
    #[serde(rename = "totalClientIds", default)] pub total_client_ids: u64,
}
```

### 5.6 Message, MessageVersion, Occupancy

No `Box` artifacts, no derived `Default` (a zeroed `Message` is meaningless), `#[non_exhaustive]` on every response struct, and **never** `#[serde(deny_unknown_fields)]` (silently ignoring new fields is the forward-compat default we want). On the wire `metadata`/`headers` are always present on `Message`; `#[serde(default)]` is a harmless belt-and-braces.

```rust
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct Message {
    pub serial: Serial,
    pub version: MessageVersion,             // not Box<_>
    pub text: String,
    #[serde(rename = "clientId")] pub client_id: ClientId,
    pub action: MessageAction,
    #[serde(default)] pub metadata: Metadata,
    #[serde(default)] pub headers: std::collections::BTreeMap<String, String>,
    pub timestamp: Timestamp,
    #[serde(rename = "userClaim", default)] pub user_claim: Option<String>,
    #[serde(default)] pub reactions: Option<MessageReactions>,  // None == not fetched (distinct from empty)
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct MessageVersion {
    pub serial: Serial,
    pub timestamp: Timestamp,
    #[serde(rename = "clientId", default)] pub client_id: Option<ClientId>,
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub metadata: Option<OperationMetadata>,   // string map, matches the write side
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct Occupancy {
    pub connections: u64,                                        // wire i32, widened
    #[serde(rename = "presenceMembers")] pub presence_members: u64,
}
```

---

## 6. Error handling

One public `#[non_exhaustive] enum Error` plus `pub type Result<T> = std::result::Result<T, Error>`. Retryability is computed from the HTTP status **wherever it exists**, including for bodies that failed envelope parsing (transient 5xx/429 from Ably's edge often arrive as HTML or empty bodies).

```rust
use std::time::Duration;
use reqwest::{StatusCode, header::{HeaderMap, RETRY_AFTER}};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Server returned an Ably envelope `{ "error": { code, message, statusCode, href? } }`.
    #[error(transparent)]
    Api(#[from] ApiError),

    /// A response arrived but wasn't what we expected (e.g. an edge 5xx with an HTML/empty body).
    #[error("unexpected response (HTTP {status}): {message}")]
    UnexpectedResponse { status: StatusCode, message: String, body: String },

    /// Transport failure: DNS, connect, TLS, timeout, redirect, body stream.
    #[error("HTTP transport error")]
    Transport(#[from] reqwest::Error),

    /// Failed to (de)serialize a request or response body.
    #[error("(de)serialization error")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Ably error {code} (HTTP {http_status}): {message}")]
#[non_exhaustive]
pub struct ApiError {
    pub code: i32,                 // e.g. 40400; leading digits mirror the HTTP status (code/100 == status)
    pub message: String,
    pub status_code: Option<i32>,  // echoed in the BODY; may be absent/inconsistent — prefer http_status
    pub href: Option<String>,
    pub http_status: StatusCode,   // authoritative (from the response line)
    pub retry_after: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChatErrorCode { NotFound, RejectedByRule, Moderation }
impl ChatErrorCode {
    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            40400 => Some(Self::NotFound),        // room/message not found (HTTP 404)
            42211 => Some(Self::RejectedByRule),  // rejected before publish by a room rule (422)
            42213 => Some(Self::Moderation),      // rejected by moderation (422)
            _ => None,                            // do NOT enumerate Ably's thousands of codes
        }
    }
}

impl ApiError {
    /// PRIMARY path: build from a live response (headers survive, as pagination/Retry-After need).
    /// Returns None when the body is not an Ably envelope -> caller emits UnexpectedResponse.
    pub fn from_response(http_status: StatusCode, headers: &HeaderMap, body: &str) -> Option<Self> {
        let env = serde_json::from_str::<raw::models::ErrorResponse>(body).ok()?;
        let info = *env.error;   // ErrorResponse.error is Box<ErrorInfo>
        Some(Self {
            code: info.code, message: info.message,
            status_code: Some(info.status_code), href: info.href,
            http_status, retry_after: parse_retry_after(headers),
        })
    }
    pub fn chat_code(&self) -> Option<ChatErrorCode> { ChatErrorCode::from_code(self.code) }
    pub fn is_not_found(&self) -> bool { self.code == 40400 || self.http_status == StatusCode::NOT_FOUND }
    pub fn is_rejected_by_rule(&self) -> bool { self.code == 42211 }
    pub fn is_moderation(&self) -> bool { self.code == 42213 }
    /// Transient CLASS — safe to retry the HTTP exchange (with a stable idempotency key), NOT a
    /// promise the caller may blindly re-invoke a mutation.
    pub fn is_retryable(&self) -> bool {
        matches!(self.http_status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
    }
}

impl Error {
    pub fn http_status(&self) -> Option<StatusCode> {
        match self {
            Error::Api(e) => Some(e.http_status),
            Error::UnexpectedResponse { status, .. } => Some(*status),
            Error::Transport(e) => e.status(),
            Error::Serialization(_) => None,
        }
    }
    pub fn chat_code(&self) -> Option<ChatErrorCode> {
        if let Error::Api(e) = self { e.chat_code() } else { None }
    }
    pub fn retry_after(&self) -> Option<Duration> {
        if let Error::Api(e) = self { e.retry_after } else { None }
    }
    /// A transport error that provably never reached the server (connection setup failed) —
    /// safe to retry/rotate even for a non-idempotent write. Used by the executor's fast path.
    pub fn is_connect(&self) -> bool {
        matches!(self, Error::Transport(e) if e.is_connect())
    }
    /// Transient across ALL shapes — a non-envelope 503 lands in UnexpectedResponse and still
    /// reports true, so the executor retries exactly the errors it should.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Api(e) => e.is_retryable(),
            Error::UnexpectedResponse { status, .. } => matches!(status.as_u16(), 408|429|500|502|503|504),
            Error::Transport(e) => e.is_timeout() || e.is_connect(),
            Error::Serialization(_) => false,
        }
    }
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    // Ably sends delta-seconds; the HTTP-date form is intentionally not parsed here (guard, don't assume 0).
    headers.get(RETRY_AFTER)?.to_str().ok()?.trim().parse::<u64>().ok().map(Duration::from_secs)
}
```

```rust
// Ergonomic matching at call sites:
match room.messages().get("does-not-exist").await {
    Ok(m) => { /* ... */ }
    Err(e) if e.chat_code() == Some(ChatErrorCode::NotFound) => eprintln!("no such message"),
    Err(Error::Transport(e)) => eprintln!("network: {e}"),
    Err(e) => return Err(e),
}
```

---

## 7. Pagination as async Streams

The engine is a single `try_unfold` over `Option<Url>`; each step GETs the URL **as-is** (the `next` link already bakes in the cursor and every original query param), parses body + `Link` headers, and advances. Streams own an `Arc`-backed `Client` so they are `'static + Send` (spawnable), rather than borrowing the client. Follow-ups reuse the executor's retry (idempotent `GET`), pinned to the cursor's region.

```rust
use std::sync::Arc;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use futures::stream::BoxStream;
use reqwest::{Method, header::LINK};
use url::Url;

#[derive(Clone, Default)]
pub struct Links { pub first: Option<Url>, pub current: Option<Url>, pub next: Option<Url> }

pub struct Page<T> { items: Vec<T>, links: Links, client: Client }

impl<T> Page<T> {
    pub fn items(&self) -> &[T] { &self.items }
    pub fn into_items(self) -> Vec<T> { self.items }
    pub fn has_next(&self) -> bool { self.links.next.is_some() }
}

impl<T: serde::de::DeserializeOwned + Send + 'static> Page<T> {
    /// Manual page-by-page.
    pub async fn next_page(&self) -> Result<Option<Page<T>>> {
        match &self.links.next {
            Some(u) => Ok(Some(fetch_page(&self.client, u.clone()).await?)),
            None => Ok(None),
        }
    }
    pub fn into_stream(self) -> BoxStream<'static, Result<T>> {
        self.into_pages().map_ok(|p| stream::iter(p.into_items().into_iter().map(Ok))).try_flatten().boxed()
    }
    pub fn into_pages(self) -> BoxStream<'static, Result<Page<T>>> {
        let tail = self.links.next.clone();
        let client = self.client.clone();
        stream::once(async move { Ok(self) })
            .chain(match tail {
                Some(u) => page_stream(client, u).boxed(),
                None => stream::empty().boxed(),
            })
            .boxed()
    }
}

/// Turn a raw response into a Page. Resolves `Link` targets against the response's final URL,
/// so it works whether the first page rotated hosts or a follow-up stayed on-region.
pub(crate) fn page_from<T: serde::de::DeserializeOwned>(client: &Client, resp: HttpResponse) -> Result<Page<T>> {
    if !resp.status.is_success() { return Err(error_from(&resp)); }
    let links = parse_links(resp.headers.get_all(LINK), &resp.url)?;
    let items: Vec<T> = serde_json::from_str(&resp.body)?;   // body is a bare JSON array
    Ok(Page { items, links, client: client.clone() })
}

/// Fetch one page at an absolute URL. The closure ignores the host, so `dispatch` retries the
/// SAME url on transient failures (idempotent GET) without rotating region.
async fn fetch_page<T: serde::de::DeserializeOwned>(client: &Client, url: Url) -> Result<Page<T>> {
    let resp = client.dispatch(Method::GET, |_base| url.to_string(), RequestBody::none()).await?;
    page_from(client, resp)
}

fn page_stream<T>(client: Client, first: Url) -> impl Stream<Item = Result<Page<T>>> + Send + 'static
where T: serde::de::DeserializeOwned + Send + 'static {
    stream::try_unfold(Some(first), move |next| {
        let client = client.clone();
        async move {
            let Some(url) = next else { return Ok(None) };
            let page = fetch_page::<T>(&client, url).await?;
            // Stop on absent `next` OR an empty page (safety net against an infinite loop).
            let advance = if page.items.is_empty() { None } else { page.links.next.clone() };
            Ok(Some((page, advance)))
        }
    })
}

/// RFC 5988: `<path-absolute-url>; rel="next"`, resolved against the request URL. A malformed
/// link from an edge/proxy is server-controlled input: skip it, never panic.
fn parse_links<'h, I>(values: I, base: &Url) -> Result<Links>
where I: IntoIterator<Item = &'h reqwest::header::HeaderValue> {
    let mut links = Links::default();
    for v in values {
        let Ok(s) = v.to_str() else { continue };
        for part in s.split(',') {
            if let Some((target, rel)) = parse_one(part) {
                let abs = match base.join(target) { Ok(u) => u, Err(_) => continue };  // skip bad links
                match rel {
                    "next" => links.next = Some(abs),
                    "first" => links.first = Some(abs),
                    "current" => links.current = Some(abs),
                    _ => {}
                }
            }
        }
    }
    Ok(links)
}
fn parse_one(s: &str) -> Option<(&str, &str)> {
    let (uri, params) = s.trim().split_once('>')?;
    let uri = uri.trim_start().strip_prefix('<')?;
    let rel = params.split(';').filter_map(|p| p.trim().strip_prefix("rel=")).map(|r| r.trim_matches('"')).next()?;
    Some((uri, rel))
}
```

Notes captured in the design:
- **`get_all("link")`, not `get`** — the spec emits one `Link` header per relation (`first`/`current`/`next`); `get` would frequently miss `rel="next"` and silently truncate to one page.
- **Resolve the target with `base.join(target)`** — the link is path-absolute (`</chat/v4/rooms/x/messages?...&cont=...>; rel="next"`, per the spec's own example); never string-concatenate, and never panic on a malformed value.
- **Cursor pagination is serial** — `buffer_unordered` cannot speed it up; keep the stream lazy (a dropped stream needs no cleanup).
- **`page_size` caps a page, not the total** — the item stream runs to the end of history unless the caller applies `.take(n)`.
- **One `Err` then terminate** — `try_unfold` gives this for free; never retry the same failing URL in a loop.
- **No pinning needed** — `BoxStream<'a, T>` is `Pin<Box<dyn Stream + Send + 'a>>`, which is `Unpin` and (here) `Send + 'static`. Iterate it with `let mut stream = …; stream.try_next().await?` and spawn it as-is; no `tokio::pin!`/`pin_mut!`.

---

## 8. Cross-cutting concerns

### 8.1 Runtime-agnosticism
reqwest 0.13 pulls hyper -> tokio 1.x transitively, so the SDK is de-facto Tokio-backed. The achievable, idiomatic goal is *no tokio API lock-in in our own surface*: never call `tokio::spawn`/`tokio::time` on the public path, don't re-export `#[tokio::main]`, don't gate on runtime flavor, and let callers inject their own `reqwest::Client` (owning the pool + runtime). The internal `backoff` sleep is the one Tokio touch-point. Document "requires a Tokio 1.x runtime." No transport-trait abstraction — overkill for a thin SDK.

### 8.2 Feature flags (all additive)
- `default = ["native-tls"]` (matches the existing manifest); alternative `rustls` forwards to reqwest's 0.13 `rustls` (note: pulls `aws-lc-rs`, needing a C toolchain + cmake — offer a `ring` path via `rustls-no-provider` for musl/cross builds if you switch).
- `chrono`, `time` — additive epoch-ms conversions (`dep:` optional deps).
- Consider a `stream` gate for `futures`/pagination if you want the streaming surface optional; otherwise keep `futures` a core dep.
- Beware default-feature unification: a downstream wanting a non-default TLS backend must set `default-features = false`.

### 8.3 MSRV & semver
MSRV is forced to **1.85** (edition 2024 + reqwest 0.13). Keep `rust-version = "1.85"`, pin a CI job on exactly that toolchain, and treat any raise as a minor (0.x) bump with a CHANGELOG note. Under 0.x, breaking = bump MINOR, additive/fix = bump PATCH. Run `cargo-semver-checks` in CI. `#[non_exhaustive]` goes on wrapper-owned **response and error types only** (and `RetryConfig`) — never on request builders (it would block literal construction, the opposite of request ergonomics). Because we own (not re-export) response types, the public surface stays stable across regeneration. The public `IntoFuture`/stream methods deliberately return `futures::{BoxFuture, BoxStream}`, and `Url`/`reqwest` types appear in the surface: this couples the crate's semver to those crates' majors. That is an accepted, documented trade-off for a thin HTTP SDK (boxing the futures keeps method signatures nameable and spawnable); a `futures`/`reqwest` major bump is a deliberate breaking release.

### 8.4 Docs
```toml
[package.metadata.docs.rs]
features = ["chrono", "time"]        # explicit; avoid all-features dragging OpenSSL onto the docs builder
rustdoc-args = ["--cfg", "docsrs"]
```
`#![cfg_attr(docsrs, feature(doc_cfg))]` + `#[cfg_attr(docsrs, doc(cfg(feature = "...")))]` for feature badges; `#![deny(rustdoc::broken_intra_doc_links)]`. Every doctest must compile: wrap awaiting examples in `# #[tokio::main] async fn main() { ... }` and mark network-touching examples ```` ```no_run ````. Pull the crate-level example from the README via `#![doc = include_str!("../README.md")]` with ```` ```no_run ````/```` ```ignore ```` fences.

### 8.5 Testing & mocking
Use **wiremock** (async-first, fresh `MockServer` per test -> parallel-safe). Point the client's host at the mock server (`.rest_host(server.uri())`) and assert:
- `X-Ably-Version: 4` header on every request,
- HTTP Basic (`keyName:keySecret`) vs Bearer auth,
- query params: `idempotencyKey` (only on the three message writes), `direction=backwards` by default and lowercase `forwards`/`backwards`, `limit`, `fromSerial`, and reaction `type=`/`name=` (lowercase `unique`/`distinct`/`multiple`),
- that a served `Link: <...>; rel="next"` header drives the pagination-follow loop (the one behaviour the generated client cannot express),
- that `sendMessageReaction` with `type=multiple` is NOT auto-retried after an ambiguous 5xx (at-least-once), while `sendMessage` IS retried and carries a stable `idempotencyKey` across attempts,
- that a served `{"error":{...}}` envelope maps to `Error::Api` with the right `code`/`http_status`, and an HTML 503 maps to a retryable `Error::UnexpectedResponse`.

```rust
#[tokio::test]
async fn history_follows_link_header() {
    let server = wiremock::MockServer::start().await;
    // page 1: Link rel="next" -> page 2; page 2: no Link -> end
    // ... register mocks ...
    let chat = Client::builder().key("k:s").unwrap().rest_host(server.uri()).build().unwrap();
    let all: Vec<_> = chat.room("r").messages().history().into_stream().try_collect().await.unwrap();
    assert_eq!(all.len(), /* items across both pages */ 3);
}
```

Also `raw`-level unit tests for `From<raw::models::*>` conversions, the `MessageAction::Other` round-trip, and `Metadata::try_from` rejecting a non-object `Value`.

---

## 9. Phased implementation plan

The plan bootstraps against the generated crate for a fast working surface, then migrates body-bearing calls onto the owned executor. This honours "layered over the generated crate" as a real trajectory.

**M0 — Workspace split (foundation).** Rename the generated crate to `ably-chat-openapi` (lib `ably_chat_openapi`); create `crates/ably-chat` (`package = "ably-chat-rs"`, lib `ably_chat`) depending on it under its real name. Add `thiserror`, `secrecy`, `uuid`, `futures`, `url`. Wire `mod raw` (re-exporting from `ably_chat_openapi`). CI: build/test/clippy/fmt on 1.85, `cargo-semver-checks`.

**M1 — Client & auth.** `Client`/`ClientBuilder`, `Auth` (Basic/Bearer), `SecretString`, forwarding+redacting `Debug`, host resolution (RSC15), injectable transport, TLS feature forwarding. Bootstrap: build a `raw::Configuration` and prove one delegated call end-to-end against wiremock.

**M2 — Error model.** `Error`/`ApiError`/`ChatErrorCode`/`Result`, envelope parsing, retryability (incl. `is_connect`), `Retry-After`. Blanket `From<raw::apis::Error<T>>` for the bootstrap/delegated path.

**M3 — Domain types.** All owned `types::*` with direct `Deserialize`, newtypes (Serial without `Ord`; keyed ids with `ord`), `Timestamp` (+ `chrono`/`time`), `MessageAction::Other`, `Metadata` (`From<Map>`/`TryFrom<Value>`), `OperationMetadata` alias, `u64` counts, `From<raw::models::*>`. Round-trip tests.

**M4 — Simple operations (delegate -> convert).** `get`, `send`, `update`, `delete`, `occupancy`, `reactions.send`/`delete`, `summary` via `raw::apis::*` then convert to owned types. Ships an ergonomic surface fast. `IntoFuture` builders; `Reaction`/`ReactionKey` enums.

**M5 — Owned executor.** `apply_common`, `try_once`, `dispatch` (single path), fallback-host rotation, exponential backoff honouring `Retry-After`, stable auto-idempotency key, the retry-safety predicate. Migrate all M4 body-bearing calls onto it so retry/rotation/error-mapping are uniform. `sendMessageReaction` runs on the same path but is naturally protected: POST-without-key is not retry-safe, so `multiple` cannot double-count (documented at-least-once).

**M6 — Pagination.** `Page<T>`, RFC 5988 parsing (`get_all`, panic-free `base.join`), `try_unfold` engine, `History`/`Versions` builders with `.await` / `.into_stream()` / `.into_pages()`, first-page rotation + region-pinned follow-ups. wiremock `Link`-header tests.

**M7 — Polish & release.** Prelude (incl. `StreamExt`/`TryStreamExt`/`BoxStream` re-exports), docs.rs metadata, doctests, README via `include_str!`, `#[non_exhaustive]` audit, `cargo-semver-checks` gate, `cargo publish` for both crates (raw first).

---

## 10. There is NO create/delete room (or channel)

The spec is explicit (`openapi/ably-chat-rest.yaml`): there are intentionally **no** endpoints for creating or deleting rooms — Chat rooms are channel-backed and implicit, not provisioned via REST. The wrapper must not invent one.

Consequences baked into this design:
- `client.room(name)` is **pure construction** — no I/O, no HTTP, no attach/lifecycle, no registry lookup. It just stores an `Arc<str>` name alongside a cloned `Client`.
- There is **no** `rooms()` collection accessor, no `create_room`, no `delete_room`, no channel provisioning — a `rooms.get()`-style collection would imply a registry that does not exist.
- `RoomName` is deliberately permissive (any string); rooms materialize server-side on first use.
- The only ten operations are the message, reaction, and occupancy calls enumerated in [§4](#4-public-api-surface--all-ten-endpoints). Nothing else is in scope for the REST surface (presence, typing, room reactions, and live subscriptions are realtime/pub-sub features, out of scope here).
