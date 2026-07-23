# ADR-0010: Handle-chain surface with `IntoFuture` builders

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk

## Context

The public surface should mirror the ergonomics of the Ably Chat JS SDK
(`chatClient.rooms.get('x'); room.messages.send(...)`) while staying idiomatic
Rust, and it must leave room for future realtime methods
([ADR-0001](0001-rest-only-scope.md)).

## Decision

A **handle chain**: `Client` → `client.room(name)` → `Room` with `.messages()`
and `.occupancy()`; `Messages` → `.reactions()`. Operations are **builders that
terminate in a bare `.await`** via `IntoFuture` — e.g.
`room.messages().send("hi").metadata(m).await`. Handles are cheap **`Arc`-backed
`Clone`**, `Send + Sync`. `Client` has a **redacting `Debug`** that never prints
credentials.

## Consequences

- Discoverable, hard-to-misuse, close to the JS SDK's shape.
- Bare-`.await` builders read naturally; the cost is some `IntoFuture` type
  machinery per operation.
- Cheap-to-clone handles pass freely across tasks.

## Alternatives considered

- **Free functions taking a client** — rejected: less discoverable.
- **Explicit `.send().await` terminal** — viable, but bare `.await` is cleaner;
  chosen per the research.
