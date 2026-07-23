# ADR-0007: Domain modeling & forward compatibility

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk

## Context

The generated types are spec-shaped: epoch-millisecond `i64` timestamps, closed
enums, and generated structs. The ergonomic crate ([ADR-0003](0003-own-the-call-layer.md))
wants idiomatic, forward-compatible types that survive server evolution.

## Decision

The ergonomic crate defines its own domain types:

- **Identifier newtypes** — `Serial`, `RoomName`. `Serial` deliberately does
  **not** implement `Ord` (serials are region-scoped, not globally ordered);
  identifiers used as map keys keep `Ord`/`Hash`.
- **`Timestamp`** — transparent epoch-ms `i64`; conversions to `chrono`/`time`
  behind additive, optional features.
- **Forward-compatible enums** — `MessageAction`, `Direction`, `ReactionType`
  each carry an `Other`/unknown fallback so an unexpected wire value never fails
  deserialization.
- **`Metadata`** — opaque JSON (`serde_json` map), no generics.
- **Reaction summaries** — default-empty maps with `u64` counts.

Conversion happens at the boundary (`From`/`TryFrom` from raw/generated shapes).

## Consequences

- Survives unknown enum values and additive fields — no hard deserialization
  failures (the core reason for owning the call layer).
- Ergonomic, strongly-typed public surface.
- Some conversion boilerplate at the dispatch boundary.

## Alternatives considered

- **Re-export generated structs directly** — rejected: closed enums (fail on
  unknown values) and non-idiomatic shapes.
