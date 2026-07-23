# ably-chat-rs — specification

Normative contract for the `ably-chat-rs` crate (import path `ably_chat`), an
unofficial Rust client for the Ably Chat REST API v4. Requirement keywords
(MUST / SHOULD / MAY) are used in the RFC 2119 sense. Rationale for the choices
here lives in the [ADRs](adr/); code-level detail lives in
[`rust-wrapper-design.md`](rust-wrapper-design.md); the wire contract is
[`../openapi/ably-chat-rest.yaml`](../openapi/ably-chat-rest.yaml).

## 1. Scope & non-goals

- In scope: the ten REST operations (message send/get/update/delete, history,
  versions, reactions send/delete/client-reactions, occupancy).
- Out of scope (no REST endpoint; realtime transport only): subscribing to live
  messages, presence, typing indicators, room reactions, live reaction
  summaries. See [ADR-0001](adr/0001-rest-only-scope.md).
- There is **no** create-room / delete-room / create-channel / delete-channel
  operation and the crate MUST NOT expose one. Rooms are implicit.

## 2. Terminology

- **Room** — a named chat room (`roomName`), created implicitly on first use.
- **Serial** — a message's unique, region-scoped identifier. Not globally
  ordered.
- **Action** — `message.create` | `message.update` | `message.delete`.
- **Reaction type** — `unique` | `distinct` | `multiple`.

## 3. Transport & conventions

- **Base URL:** `https://rest.ably.io` by default; the host MUST be configurable.
- **Version:** every request MUST send `X-Ably-Version: 4`.
- **Auth:** HTTP Basic (API key `keyName:keySecret`) or Bearer (Ably Token/JWT).
  See [ADR-0005](adr/0005-authentication.md).
- **Timestamps:** integer milliseconds since the Unix epoch (`i64`), never
  RFC 3339 strings. See [ADR-0007](adr/0007-domain-modeling.md).
- **Pagination:** `getMessages` and `getMessageVersions` return a JSON array body
  and paginate via RFC 5988 `Link` headers. All other operations return a single
  JSON object.
- **Errors:** non-2xx responses carry `{ "error": { code, message, statusCode,
  href? } }`. See [ADR-0008](adr/0008-error-model.md).
- **Open assumption:** the single-resource responses are modelled as bare JSON
  objects. This is corroborated by Ably convention but not wire-captured; it MUST
  be confirmed against a live endpoint before 1.0 (see the repo README caveat).

## 4. Crate layout

Two-crate workspace ([ADR-0002](adr/0002-workspace-topology.md)):

- `ably-chat-rs` (`use ably_chat`) — this specification's subject.
- `ably-chat-openapi` (`use ably_chat_openapi`) — generated bindings,
  re-exported by the ergonomic crate as `ably_chat::raw` (low-level escape hatch,
  unstable; [ADR-0004](adr/0004-generated-code-as-dependency.md)).

## 5. Client & configuration

- A `Client` MUST be constructed via a builder. Required input: credentials
  (API key **or** token). Optional: host, request timeout, a caller-supplied
  `reqwest::Client`.
- Credentials MUST be modelled as an enum (`ApiKey` | `Token`). Static only in
  0.x; no auto-refresh.
- `Client` MUST be cheap to `Clone` (`Arc`-backed) and `Send + Sync`.
- `Client`'s `Debug` MUST redact credentials.
- TLS backend selection MUST be via additive Cargo features
  (`native-tls` default, `rustls` optional).

## 6. Public API surface

Handle chain ([ADR-0010](adr/0010-ergonomic-surface.md)):
`Client` → `client.room(name) -> Room` → `Room::messages() -> Messages` /
`Room::occupancy() -> Occupancy`; `Messages::reactions() -> Reactions`.

Each operation is a builder terminating in a bare `.await` (`IntoFuture`).
Handles are cheap `Arc`-backed `Clone`. Every fallible call returns
`ably_chat::Result<T>`.

### 6.1 Messages

| Operation | Chain (conceptual) | HTTP | Returns | Retry-safe |
| --- | --- | --- | --- | --- |
| Send | `room.messages().send(text).metadata(..).headers(..).idempotency_key(..)` | `POST /messages` | `Message` | only with idempotency key |
| Get | `room.messages().get(serial)` | `GET /messages/{serial}` | `Message` | yes (GET) |
| Update | `room.messages().update(serial, text).metadata(..).headers(..).description(..).idempotency_key(..)` | `PUT /messages/{serial}` | `Message` | only with idempotency key |
| Delete | `room.messages().delete(serial).description(..).metadata(..).idempotency_key(..)` | `POST /messages/{serial}/delete` | `Message` | only with idempotency key |
| History | `room.messages().history().start(..).end(..).direction(..).limit(..).from_serial(..)` | `GET /messages` | `Page<Message>` / `Stream` | yes (GET) |
| Versions | `room.messages().versions(serial)` | `GET /messages/{serial}/versions` | `Page<Message>` / `Stream` | yes (GET) |

- `update` replaces the whole message body: omitted `text`/`metadata`/`headers`
  are reset to empty. This MUST be documented prominently.
- `delete` is a soft delete via `POST …/delete` (not HTTP `DELETE`).
- History default direction MUST be **backwards** (newest first), matching the
  JS SDK; default `limit` 100.

### 6.2 Reactions

| Operation | Chain | HTTP | Returns | Retry-safe |
| --- | --- | --- | --- | --- |
| Send | `room.messages().reactions().send(serial, name).kind(..).count(..)` | `POST /messages/{serial}/reactions` | `()` | **no** |
| Delete | `room.messages().reactions().delete(serial).kind(..).name(..)` | `DELETE /messages/{serial}/reactions` | `()` | yes (DELETE) |
| Client reactions | `room.messages().reactions().for_client(serial).client_id(..)` | `GET /messages/{serial}/client-reactions` | reaction summary | yes (GET) |

- `send` reaction MUST NOT be retried (no idempotency key; `multiple` counts each
  call) — [ADR-0006](adr/0006-retry-policy.md).
- `delete` reaction: `name` is required for `distinct`/`multiple`, optional for
  `unique`; the client SHOULD enforce this before the request.
- `count` applies only to `multiple`; defaults to 1.

### 6.3 Occupancy

| Operation | Chain | HTTP | Returns | Retry-safe |
| --- | --- | --- | --- | --- |
| Get | `room.occupancy().get()` | `GET /occupancy` | `Occupancy` | yes (GET) |

## 7. Domain types

Owned by the ergonomic crate ([ADR-0007](adr/0007-domain-modeling.md)):

- `Serial(String)`, `RoomName(String)` newtypes. `Serial` MUST NOT implement
  `Ord` (region-scoped). Ids used as map keys keep `Ord`/`Hash`.
- `Timestamp` — transparent epoch-ms `i64`; `chrono`/`time` conversions behind
  additive features.
- `MessageAction`, `Direction`, `ReactionType` — enums that MUST carry an
  `Other`/unknown fallback; an unknown wire value MUST NOT fail deserialization.
- `Metadata` — opaque JSON map; `Headers` — `String → String` map.
- `Message` { serial, version, text, client_id, action, metadata, headers,
  user_claim?, timestamp, reactions? }.
- `MessageVersion` { serial, timestamp, client_id?, description?, metadata? }.
- `Occupancy` { connections, presence_members }.
- Reaction summaries — maps keyed by reaction name; `unique`/`distinct` →
  { total, client_ids, clipped }, `multiple` → per-client `u64` counts. Absent
  groups default to empty.

## 8. Error model

One public `#[non_exhaustive]` `Error` ([ADR-0008](adr/0008-error-model.md)) with
transport / decode / api / invalid-request variants. The api variant carries
`ErrorInfo` { code, message, status_code, href? }; the invalid-request variant is
raised client-side for pre-flight validation (e.g. deleting a `distinct`/`multiple`
reaction without a name) before any request is sent. It MUST expose `status() -> Option<u16>`
and `is_retryable() -> bool`, and SHOULD offer predicates for codes 40400
(not found), 42211 (rejected by rule), 42213 (moderation). `Result<T>` alias
provided.

## 9. Pagination

`Page<T>` ([ADR-0009](adr/0009-pagination-streaming.md)) holds the page items plus
parsed `Link` cursors. It MUST offer `next() -> Result<Option<Page<T>>>` and
`into_stream() -> impl Stream<Item = Result<T>>` that follows `next` until
exhausted. The dispatch layer reads the `Link` headers the generated functions
discard.

## 10. Concurrency

All handles MUST be `Send + Sync` and cheap to `Clone`. The client wraps shared
state in `Arc`. No global mutable state.

## 11. Feature flags & runtime

- All features MUST be additive.
- TLS: `native-tls` (default) | `rustls`.
- Datetime: optional `chrono`, `time`.
- The crate SHOULD be runtime-agnostic (no hard tokio dependency beyond what
  `reqwest` requires); tests MAY use tokio.

## 12. Stability & versioning

- Crate starts at `0.x`; breaking changes bump the minor.
- `ably_chat::raw` is explicitly outside the stability story and tracks the
  generated crate.
- MSRV `1.85`, edition `2024`, dual `MIT OR Apache-2.0`
  ([ADR-0011](adr/0011-packaging.md)).
