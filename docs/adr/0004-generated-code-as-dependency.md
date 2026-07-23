# ADR-0004: Generated code as a dependency crate, re-exported as `raw`

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk
- Supersedes: the earlier in-crate `pub mod rest` escape hatch

## Context

Given the workspace ([ADR-0002](0002-workspace-topology.md)) and that the
ergonomic layer owns its own call path ([ADR-0003](0003-own-the-call-layer.md)),
the generated crate `ably-chat-rs-openapi` is not on the ergonomic critical path.
It is still valuable: complete, spec-faithful, and regenerable — a useful escape
hatch when the ergonomic API has a gap.

## Decision

The ergonomic crate `ably-chat-rs` **depends on** `ably-chat-rs-openapi` and
re-exports it as **`pub mod raw`** — a low-level escape hatch. The `raw` module
is **not covered by the pre-1.0 stability story** and tracks the generated
crate's version; regeneration replaces `ably-chat-rs-openapi` wholesale.

## Consequences

- Clean regeneration: the generated crate is isolated, so no
  `.openapi-generator-ignore` gymnastics are required.
- Power users can drop to `ably_chat::raw` when the ergonomic layer lacks
  something.
- `raw`'s surface is explicitly unstable; revisit at 1.0 (keep, hide, or drop).

## Alternatives considered

- **In-crate `pub mod rest` vendoring** — superseded by the workspace decision.
- **Do not re-export the generated crate at all** — rejected: loses the escape
  hatch that keeps early users unblocked.
