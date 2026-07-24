# ADR-0011: Packaging — names, licence, edition, MSRV

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk
- Amended 2026-07-24 by [ADR-0012](0012-token-issuance-permissions.md): **MSRV raised to
  1.88** (`jsonwebtoken` → `simple_asn1` → `time 0.3.54` requires rustc 1.88), and a third
  published member crate, `ably-auth-openapi`, was added. The decision text below is
  historical (records the original `1.85` / two-crate state).

## Context

The workspace ([ADR-0002](0002-workspace-topology.md)) publishes two crates, both
of which use the "Ably" trademark and are unofficial.

## Decision

- **Members (both published):** `ably-chat-rs` (ergonomic; import path
  `ably_chat`) and `ably-chat-openapi` (generated; import path
  `ably_chat_openapi`). Both share the `ably-chat` stem for a consistent
  family. Released in dependency order: `ably-chat-openapi` first, then
  `ably-chat-rs`.
- **Unofficial:** both crate descriptions and READMEs state they are not
  affiliated with or endorsed by Ably. The bare name `ably-chat` is avoided
  (trademark/impersonation).
- **Licence:** dual `MIT OR Apache-2.0`, with `LICENSE-MIT` + `LICENSE-APACHE` in
  each crate.
- **Edition/MSRV:** `edition = "2024"`, `rust-version = "1.85"`.
- **Version:** both start at `0.x`; `repository`/`homepage` must be set before the
  first publish.

## Consequences

- Two release artifacts to keep in lockstep.
- Consistent, honest metadata; docs.rs builds for both.

## Alternatives considered

- **One published crate** — tied to the single-crate topology, superseded by
  ADR-0002.
- **Publish as `ably-chat`** — rejected on trademark grounds.
