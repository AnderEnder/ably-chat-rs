# PRD: `ably-chat-rs` — an ergonomic Rust client for the Ably Chat REST API

Status: ready-for-agent · Synthesized from [ADRs](adr/), [SPEC.md](SPEC.md),
[PLAN.md](PLAN.md), and [rust-wrapper-design.md](rust-wrapper-design.md).
This PRD is authoritative for *what* and *why*; the SPEC is the normative
contract and the ADRs record the decisions.

## Problem Statement

I'm building chat features in a Rust service on top of Ably, but there is no
official Ably Chat SDK for Rust. Today I have to hand-roll HTTP calls against the
Ably Chat REST API myself: base64 Basic auth or Bearer tokens, the
`X-Ably-Version` header, RFC 5988 `Link`-header pagination for history,
millisecond-epoch timestamps, the `{ "error": { … } }` envelope, and the
`unique`/`distinct`/`multiple` reaction rules. It is slow, repetitive, and easy
to get subtly wrong (e.g. double-counting a `multiple` reaction on a naive
retry, or breaking when the server returns an unfamiliar message action).

## Solution

An unofficial, idiomatic Rust crate, **`ably-chat-rs`** (import path
`ably_chat`), that wraps the Ably Chat REST API v4 behind a typed, ergonomic
client. It mirrors the shape of the Ably Chat JS SDK — room-scoped handles
(`client.room("x").messages()…`), builders that end in a bare `.await`, streamed
history — while staying idiomatic Rust: newtypes, forward-compatible enums,
one typed error, and async `Stream` pagination. For anything the ergonomic layer
doesn't cover, a low-level `ably_chat::raw` escape hatch exposes the generated
bindings. The crate targets **server-side** use (API key or token). Realtime
features and room provisioning are explicitly not offered because the REST API
has no such endpoints.

## User Stories

1. As a Rust backend developer, I want to construct a chat client from an Ably
   API key, so that my server can call the Chat REST API with Basic auth.
2. As a developer using token auth, I want to construct the client from an Ably
   Token/JWT, so that I can use Bearer credentials.
3. As a developer, I want the client builder to accept a custom host, so that I
   can target a non-default Ably environment or a local mock server.
4. As a developer, I want to supply my own `reqwest::Client`, so that I can reuse
   my connection pool, proxy, and TLS configuration.
5. As a developer, I want to set a request timeout on the client, so that calls
   fail fast under network trouble.
6. As a security-conscious developer, I want the client's `Debug` output to
   redact credentials, so that I never leak a key into logs.
7. As a developer, I want the client and all handles to be cheap to clone and
   `Send + Sync`, so that I can share them freely across async tasks.
8. As a developer, I want to obtain a room handle by name without any
   provisioning call, so that I can start working with a room immediately.
9. As a developer, I want to send a text message to a room, so that a user's
   message is published.
10. As a developer, I want to attach metadata and headers when sending a message,
    so that I can carry app-specific context.
11. As a developer, I want to pass an idempotency key on a send, so that a safe
    retry cannot create a duplicate message.
12. As a developer, I want the send call to return the created message (with its
    serial and timestamp), so that I can reference it afterwards.
13. As a developer, I want to fetch a single message by its serial, so that I can
    display or inspect it.
14. As a developer, I want to update (edit) a message, so that I can correct or
    change its content.
15. As a developer, I want the update to clearly document that it replaces the
    whole message body (omitted fields reset to empty), so that I am not
    surprised by lost metadata.
16. As a developer, I want to delete a message, so that it is soft-deleted (a
    `message.delete` version) and stops showing as active.
17. As a developer, I want to query a room's message history, so that new
    joiners get context.
18. As a developer, I want to filter history by time window, limit, direction,
    and starting serial, so that I can page precisely.
19. As a developer, I want history to default to newest-first, so that behaviour
    matches the Ably Chat JS SDK.
20. As a developer, I want to stream history as an async `Stream` that
    transparently follows pagination, so that I can iterate all messages without
    handling `Link` headers myself.
21. As a developer, I want a manual page-by-page option too, so that I can
    control fetching and back-pressure.
22. As a developer, I want to fetch all versions of a message, so that I can show
    its edit/delete history.
23. As a developer, I want to add a reaction to a message by name (e.g. an
    emoji), so that users can react.
24. As a developer, I want to choose the reaction type (`unique`, `distinct`,
    `multiple`), so that aggregation behaves as intended.
25. As a developer, I want to set a count for a `multiple` reaction, so that
    vote-style reactions work.
26. As a developer, I want a reaction send to never be silently retried, so that
    a `multiple` reaction cannot be double-counted on a transient failure.
27. As a developer, I want to remove a reaction, with the client enforcing that a
    name is required for `distinct`/`multiple` and optional for `unique`, so that
    I get an early, clear error instead of a server rejection.
28. As a developer, I want to fetch a specific client's reactions on a message,
    so that I can tell whether the current user reacted when the summary is
    clipped.
29. As a developer, I want to read a room's occupancy (connections, presence
    members), so that I can show how busy a room is.
30. As a developer, I want one typed error to match on, so that I don't juggle a
    different error type per operation.
31. As a developer, I want the error to expose the HTTP status and the Ably error
    code/message, so that I can react to specific conditions.
32. As a developer, I want helpers for the notable Chat codes — message not found
    (40400), rejected by a room rule (42211), rejected by moderation (42213) —
    so that I can branch without memorising numbers.
33. As a developer, I want the client to know which errors are retryable, so that
    transient failures are retried safely and non-idempotent ones are not.
34. As a developer, I want message timestamps as an epoch-ms value that I can
    optionally convert to `chrono`/`time`, so that I choose my datetime crate.
35. As a developer, I want message actions and reaction types modelled as enums
    that tolerate unknown values, so that a future server value doesn't break
    deserialization of the whole message.
36. As a developer, I want a low-level `raw` escape hatch, so that I'm not blocked
    if the ergonomic layer lacks an operation.
37. As a developer, I want the crate to be clearly labelled unofficial, so that I
    understand it is not supported by Ably.
38. As a developer, I want dual `MIT OR Apache-2.0` licensing, so that it fits
    the Rust ecosystem's norms.
39. As a developer, I want the crate to be runtime-agnostic and TLS-configurable
    via features, so that it fits my stack (tokio/other, native-tls/rustls).
40. As a developer new to the crate, I want to understand upfront that there is no
    create/delete room and no realtime (subscribe/presence/typing), so that I
    don't expect features the REST API cannot provide.

## Implementation Decisions

References to ADRs are authoritative; this section summarises. No file paths or
code are pinned here (they drift); type shapes are described in prose.

- **Workspace topology** ([ADR-0002](adr/0002-workspace-topology.md)): a Cargo
  workspace with two published member crates sharing the `ably-chat-rs` stem —
  `ably-chat-rs` (ergonomic, import `ably_chat`) depending on
  `ably-chat-openapi` (generated, import `ably_chat_openapi`).
- **Own the call layer** ([ADR-0003](adr/0003-own-the-call-layer.md)): the
  ergonomic crate owns one async HTTP dispatch function over `reqwest` and its
  own domain types. It does not call the generated API functions (they discard
  the `Link` headers pagination needs) nor reuse the generated model structs
  (their closed `MessageAction` enum hard-fails on unknown values).
- **Generated code as `raw`** ([ADR-0004](adr/0004-generated-code-as-dependency.md)):
  the generated crate is re-exported as `ably_chat::raw`, an unstable escape
  hatch; regeneration replaces that crate wholesale.
- **Authentication** ([ADR-0005](adr/0005-authentication.md)): a credentials enum
  (`ApiKey` | `Token`) on the client builder; static credentials only in 0.x; a
  `TokenProvider` extension point is reserved for later.
- **Retry safety** ([ADR-0006](adr/0006-retry-policy.md)): a request is retried
  only if it is `GET`/`DELETE` or carries an idempotency key; `Retry-After` is
  honoured.
- **Domain modelling** ([ADR-0007](adr/0007-domain-modeling.md)): `Serial`
  (no `Ord`) and `RoomName` newtypes; a transparent epoch-ms `Timestamp` with
  optional `chrono`/`time` conversions; forward-compatible enums with an `Other`
  fallback; opaque `Metadata`; default-empty reaction-summary maps.
- **Error model** ([ADR-0008](adr/0008-error-model.md)): one `#[non_exhaustive]`
  `Error` (transport / decode / api) carrying `ErrorInfo { code, message,
  status_code, href? }`, with `status()`, `is_retryable()`, and code predicates;
  a crate `Result<T>` alias.
- **Pagination** ([ADR-0009](adr/0009-pagination-streaming.md)): a `Page<T>` with
  `next()` and `into_stream()`; the dispatch layer reads the `Link` cursors.
- **Ergonomic surface** ([ADR-0010](adr/0010-ergonomic-surface.md)): the handle
  chain `Client → Room → Messages/Occupancy`, `Messages → Reactions`; each
  operation is a builder that implements `IntoFuture` (bare `.await`); handles are
  `Arc`-backed `Clone`.
- **Transport seam / configuration:** the client builder exposes the base host;
  in production it defaults to `rest.ably.io`, and in tests it is pointed at a
  mock server. Every request carries `X-Ably-Version: 4`.
- **API contract:** the ten operations map to the verbs/paths in
  [`../openapi/ably-chat-rest.yaml`](../openapi/ably-chat-rest.yaml); note the
  delete-message operation is `POST …/messages/{serial}/delete`, not HTTP
  `DELETE`.
- **Packaging** ([ADR-0011](adr/0011-packaging.md)): both crates unofficial, dual
  `MIT OR Apache-2.0`, edition 2024, MSRV 1.85, starting at 0.x; published in
  dependency order (`ably-chat-openapi` then `ably-chat-rs`).

## Testing Decisions

- **What makes a good test here:** it exercises *external behaviour* through the
  public `ably_chat` API and asserts on the HTTP request sent and the typed value
  or error returned — never on internal structure. Deserialization, pagination,
  retry, and error mapping are observed through their public effects.
- **The seam (one, highest point):** the HTTP boundary. Tests configure the
  client's base host to a local **`wiremock`** server (the existing config seam —
  no new abstraction) and drive the real public API end-to-end. The only faked
  component is the remote Ably server. A `Transport` trait / mocking `reqwest`
  internals was rejected as a lower, lower-fidelity seam.
- **Modules under test:** the public surface — client/auth, messages
  (send/get/update/delete/history/versions), reactions
  (send/delete/client-reactions), occupancy, pagination, and errors.
- **Representative cases:** Basic/Bearer auth header encoding and the version
  header; send returns a `Message`; update performs a full-body replace; delete
  yields a `message.delete` action; two-page history followed via the stream and
  via manual `next()`; message versions; reaction send with each type and a
  `multiple` count; the name-required validation for `distinct`/`multiple`
  deletes; occupancy; error-envelope mapping and `is_retryable()`; a retry
  asserted by wiremock request count with near-zero backoff (no clock seam
  needed); idempotency-key passthrough on message writes.
- **Prior art:** none in-repo — this is greenfield (the generated crate ships no
  tests). `wiremock` is the idiomatic Rust prior art for HTTP-boundary mocking;
  doctests cover the usage examples.

## Out of Scope

- **Realtime features** — subscribing to live messages, presence, typing
  indicators, room reactions, and live reaction summaries. These use Ably's
  WebSocket transport and have no REST endpoint.
- **Room/channel provisioning** — there is no create/delete room or channel
  operation; rooms are implicit.
- **Token auto-refresh** (`authCallback`-style) — deferred; only static
  credentials in 0.x.
- **Non-`reqwest` transports.**
- **Stability of `ably_chat::raw`** — it tracks the generated crate and is not
  covered by the pre-1.0 story.

## Further Notes

- **Provenance:** the REST contract was reconstructed from the `@ably/chat`
  JavaScript SDK v1.4.0 plus Ably's documented REST conventions; there is no
  official Ably OpenAPI document for Chat.
- **Open question to resolve before 1.0** (SPEC §3): whether the single-resource
  responses are bare JSON objects or 1-element arrays on the wire. Modelled as
  bare objects (Ably-convention-corroborated); confirm against a live endpoint.
- **Unofficial & naming:** both crates state they are not affiliated with Ably;
  the bare `ably-chat` name is deliberately avoided on trademark grounds.
- **Delivery:** built in the phases of [PLAN.md](PLAN.md), each gated on
  `cargo check`/`cargo test`; Phase 0 restructures the current single generated
  crate into the workspace.
