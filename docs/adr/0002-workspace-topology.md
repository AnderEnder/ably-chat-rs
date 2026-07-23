# ADR-0002: Two-crate workspace

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk
- Supersedes: the earlier single-crate stance (repo collapse)

## Context

The generated code and the hand-written ergonomic layer have very different
lifecycles: the generated code is replaced wholesale on every regeneration, while
the ergonomic layer is authored and evolved by hand. The multi-expert research
recommended isolating them as two crates in a workspace so regeneration never
touches hand-written code and needs no path rewriting or `.openapi-generator-ignore`
juggling. This direction was chosen over the single-crate vendoring alternative.

## Decision

A Cargo **workspace** at the repository root (repo name `ably-chat-rs`) with two
**published** member crates:

- `crates/ably-chat-rs/` — the ergonomic crate (import path `ably_chat`), the
  primary crate users install. Depends on the generated crate.
- `crates/ably-chat-rs-openapi/` — the generated crate (import path
  `ably_chat_openapi`), refreshed wholesale by the OpenAPI generator.

Both members share the `ably-chat-rs` stem for a consistent crate family; the
primary crate is unsuffixed (cf. `serde`/`serde_derive`, `tokio`/`tokio-macros`).

## Consequences

- Regeneration targets `crates/ably-chat-rs-openapi/` only; the ergonomic crate is
  physically separate and never clobbered — no path rewriting.
- **Two crates are published to crates.io**, released in dependency order
  (`ably-chat-rs-openapi` first, then `ably-chat-rs`). Both use the "Ably" mark, so
  both are marked unofficial and dual-licensed (see [ADR-0011](0011-packaging.md)).
- Reverses the earlier single-crate repo collapse: `src/` at the root moves under
  `crates/`.

## Alternatives considered

- **Single crate, vendoring the generated code as `pub mod rest`** — simpler to
  publish (one crate) but messier to regenerate (must protect hand-written files
  with `.openapi-generator-ignore` and re-address module paths). Superseded by
  this ADR per the research direction.
