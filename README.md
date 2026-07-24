# Ably Chat REST API — OpenAPI specification

[![build status](https://github.com/AnderEnder/ably-chat-rs/workflows/Build/badge.svg)](https://github.com/AnderEnder/ably-chat-rs/actions)
[![release status](https://github.com/AnderEnder/ably-chat-rs/workflows/Release/badge.svg)](https://github.com/AnderEnder/ably-chat-rs/actions)
[![crates.io](https://img.shields.io/crates/v/ably-chat-rs.svg)](https://crates.io/crates/ably-chat-rs)
[![docs.rs](https://docs.rs/ably-chat-rs/badge.svg)](https://docs.rs/ably-chat-rs)

An OpenAPI 3.0.3 specification for the [Ably Chat](https://ably.com/docs/chat)
REST API, plus the Rust SDK generated and hand-built from it. This repo is a
two-crate Cargo workspace:

- **[`crates/ably-chat-rs`](crates/ably-chat-rs)** (`ably_chat`) — the ergonomic
  client most users want.
- **[`crates/ably-chat-openapi`](crates/ably-chat-openapi)**
  (`ably_chat_openapi`) — the generated bindings it is built on.

Both are unofficial, not affiliated with or endorsed by Ably.

- Specs: [`openapi/ably-chat-rest.yaml`](openapi/ably-chat-rest.yaml) (Chat REST)
  and [`openapi/ably-auth-rest.yaml`](openapi/ably-auth-rest.yaml) (platform token
  issuance/revocation + server time)
- OpenAPI version: **3.0.3** (broadest Rust generator support)
- Validated with: `@redocly/cli lint` ✅

## Provenance

Ably does **not** publish an official OpenAPI document for the Chat REST API.
Their official specs repo, [`ably/open-specs`](https://github.com/ably/open-specs),
ships `definitions/platform-v1.yaml`, but that covers only the generic pub/sub
REST API (`/channels`, `/push`, `/keys`, `/stats`, `/time`) — it contains no
`/chat/*` endpoints.

This spec was therefore **reconstructed** from two authoritative sources:

1. **The `@ably/chat` JavaScript SDK, v1.4.0** — specifically its REST layer,
   `src/core/chat-api.ts` (endpoints, HTTP verbs, path/query params, request
   bodies) and `src/core/rest-types.ts` (response schemas). This is ground
   truth for the exact HTTP contract the client speaks.
2. **Ably's documented REST conventions** — host, Basic/Bearer authentication,
   the `X-Ably-Version` header, RFC 5988 `Link`-header pagination, and the
   standard `{ "error": { code, message, statusCode, href } }` envelope
   (cross-checked against `platform-v1.yaml`).

### Scope

The spec covers the endpoints the client SDK exercises. It is **not** an
exhaustive description of every server capability, and the create/update/delete
response bodies are modelled as full `Message` objects because the SDK
unconditionally reads those fields off the responses. If you observe the server
returning a partial body for those operations, relax the `required` lists on
the `Message` schema accordingly.

### One assumption to confirm at integration time: singleton bodies

The SDK reads `response.items[0]` for the single-resource operations
(`getMessage`, `sendMessage`, `updateMessage`, `deleteMessage`, `getOccupancy`,
`getClientReactions`). Because ably-js normalizes a bare-object wire body
`{...}` into a 1-element array `[{...}]`, the SDK source alone **cannot** tell
you which the server actually sends.

This spec models those responses as **bare objects** (only the genuinely
paginated `getMessages` / `getMessageVersions` are arrays). That follows Ably's
own documented REST convention: in `platform-v1.yaml` the analogous
`getMetadataOfChannel` (single GET) and `publishMessagesToChannel` (POST) both
return bare objects, with arrays reserved for enumeration/history. It is a
strong corroboration, not a wire capture. Confirm with one live call before
relying on the generated client:

```bash
curl -sS "https://rest.ably.io/chat/v4/rooms/my-room/occupancy" \
  -H "X-Ably-Version: 4" -u "{keyName}:{keySecret}" | head -c1
# '{'  → bare object (matches this spec)
# '['  → 1-element array (wrap the affected response schemas in `type: array`)
```

## Endpoints

Base path prefix: `/chat/v4/rooms/{roomName}`

| Method | Path | operationId | Purpose |
| ------ | ---- | ----------- | ------- |
| GET    | `/messages`                          | `getMessages`           | Paginated message history |
| POST   | `/messages`                          | `sendMessage`           | Send a message |
| GET    | `/messages/{serial}`                 | `getMessage`            | Get a single message |
| PUT    | `/messages/{serial}`                 | `updateMessage`         | Edit a message |
| POST   | `/messages/{serial}/delete`          | `deleteMessage`         | Soft-delete a message (POST, not HTTP DELETE) |
| GET    | `/messages/{serial}/versions`        | `getMessageVersions`    | Paginated version history of a message |
| POST   | `/messages/{serial}/reactions`       | `sendMessageReaction`   | Add a reaction |
| DELETE | `/messages/{serial}/reactions`       | `deleteMessageReaction` | Remove a reaction |
| GET    | `/messages/{serial}/client-reactions`| `getClientReactions`    | A single client's reactions on a message |
| GET    | `/occupancy`                         | `getOccupancy`          | Room occupancy metrics |

### There is no "create room / delete room" (or channel)

Chat rooms are channel-backed and **implicit**. A room comes into existence the
first time a client publishes/attaches/reacts to it, and disappears on its own
once idle (persisted history simply expires per your app's TTL). There is no
provisioning endpoint — confirmed both by the SDK and by Ably's official
`platform-v1.yaml`, where `/channels` and `/channels/{channel_id}` expose
**GET only** (enumeration and metadata), with no create/delete operation
anywhere in the API.

## Conventions

- **Host:** `https://rest.ably.io` (configurable via the `host` server
  variable; fallback/regional hosts may apply to your account).
- **Auth:** HTTP Basic with an API key (`keyName:keySecret`) **or** Bearer with
  an Ably Token / JWT.
- **Version:** `X-Ably-Version: 4` header (the SDK sends `4`; `?v=4` also works).
- **Timestamps:** integers, **milliseconds since the Unix epoch** (`int64`) —
  *not* RFC 3339 date-time strings.
- **Pagination:** `GET /messages` and `GET .../versions` return a JSON **array**
  body; page links come back as RFC 5988 `Link` response headers (`first`,
  `current`, `next`).
- **Idempotency:** create/update/delete accept an optional `idempotencyKey`
  query param.

## Generating a Rust SDK

The spec is written to be codegen-friendly (explicit `operationId`s, epoch-ms
integers, `additionalProperties` typed for maps, minimal spurious `nullable`).
Two common options:

### The Rust SDK (this repository)

This repo is a two-crate Cargo workspace
([ADR-0002](docs/adr/0002-workspace-topology.md)):

| Crate | Import path | Role |
| ----- | ----------- | ---- |
| [`crates/ably-chat-rs`](crates/ably-chat-rs) | `ably_chat` | **Start here.** Hand-written, ergonomic, forward-compatible client. |
| [`crates/ably-chat-openapi`](crates/ably-chat-openapi) | `ably_chat_openapi` | Generated OpenAPI bindings; re-exported as `ably_chat::raw` (escape hatch). |
| [`crates/ably-auth-openapi`](crates/ably-auth-openapi) | `ably_auth_openapi` | Generated bindings for the platform token endpoints (`/keys/.../requestToken`, `/keys/.../revokeTokens`, `/time`). |

All three crates are unofficial and dual-licensed `MIT OR Apache-2.0`. Most users depend on
`ably-chat-rs`; see its [crate README](crates/ably-chat-rs/README.md) for
install and usage.

The generated crates' `src/` is regenerated from their specs and **must not be
hand-edited** (a CI codegen gate diffs each against a fresh regeneration). They
were produced with:

```bash
npx @openapitools/openapi-generator-cli generate \
  -i openapi/ably-chat-rest.yaml \
  -g rust \
  -o crates/ably-chat-openapi \
  --additional-properties=packageName=ably-chat-openapi,packageVersion=0.1.0,supportAsync=true,library=reqwest

npx @openapitools/openapi-generator-cli generate \
  -i openapi/ably-auth-rest.yaml \
  -g rust \
  -o crates/ably-auth-openapi \
  --additional-properties=packageName=ably-auth-openapi,packageVersion=0.1.0,supportAsync=true,library=reqwest
```

The ergonomic wrapper layered over this generated crate is designed in
[`docs/rust-wrapper-design.md`](docs/rust-wrapper-design.md) and specified in
[`docs/SPEC.md`](docs/SPEC.md).

### progenitor (Rust-native, generates a typed reqwest client)

```bash
cargo install cargo-progenitor
cargo progenitor -i openapi/ably-chat-rest.yaml -o generated/ably-chat-rest --name ably-chat-rest
```

### Codegen notes

- Model `timestamp` as `i64` epoch-ms; do not let the generator coerce it to a
  date type.
- `metadata` is an open object (`serde_json::Value` / `HashMap<String, Value>`);
  `headers` is `HashMap<String, String>`.
- Reaction summary maps (`unique`/`distinct`/`multiple`) are keyed by reaction
  name (emoji) → aggregation object.
- Authentication is injected via the standard `Authorization` header — supply
  the API key as Basic credentials or a token as Bearer.

## Regenerating / re-validating

```bash
npx @redocly/cli@latest lint openapi/ably-chat-rest.yaml
```

The reconstruction tracks `@ably/chat@1.4.0`. When bumping to a newer SDK,
re-check `src/core/chat-api.ts` and `src/core/rest-types.ts` for endpoint,
param, or schema changes.

## License

Dual-licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT)
at your option. The same `MIT OR Apache-2.0` terms apply to both published
crates (`ably-chat-rs` and `ably-chat-openapi`). This is an unofficial
project, not affiliated with or endorsed by Ably.
