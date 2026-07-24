# ADR-0001: REST-only scope, shaped for future realtime

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk
- Realtime delivery is planned in a separate repository:
  [ably-realtime-rs](https://github.com/AnderEnder/ably-realtime-rs)

## Context

`ably-chat-rs` is generated from the Ably Chat **REST** surface (~10 operations:
message send/get/edit/delete/history, reactions, occupancy). The interactive
Chat features — subscribing to live messages, presence, typing indicators, room
reactions, live reaction summaries — travel over Ably's realtime/WebSocket
transport and have **no REST endpoints**. The crate name `ably-chat-rs` risks
implying a full Chat SDK.

## Decision

Ship a **REST-only** client. Document the scope plainly (realtime is not
supported). Shape the public surface as a **handle chain**
(`client.room("x").messages()…`) so realtime features could be added later as
new methods without a breaking redesign.

## Consequences

- Honest expectations: users are told upfront what the crate cannot do.
- No `subscribe`/presence/typing APIs; those require the JS SDK today.
- The handle-chain shape (see [ADR-0010](0010-ergonomic-surface.md)) leaves room
  to grow.

## Alternatives considered

- **Imply a full SDK** — rejected; would strand users expecting `subscribe`.
- **Rename to signal REST-only** (e.g. `ably-chat-rest-rs`) — rejected; the name
  is already chosen (see [ADR-0011](0011-packaging.md)); documentation carries
  the scope instead.
