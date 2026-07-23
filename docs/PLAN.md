# ably-chat-rs — implementation plan

Phased plan to build the ergonomic wrapper per the [SPEC](SPEC.md) and
[ADRs](adr/). Each phase has an exit gate; a phase is "done" only when its gate
is green. No phase starts implementation before this plan and the SPEC are
approved.

Current state: the repo root is a single generated crate (`src/apis`,
`src/models`, generated `lib.rs`) that compiles. Phase 0 restructures it into the
workspace.

---

## Phase 0 — Restructure into the workspace

Implements [ADR-0002](adr/0002-workspace-topology.md), [ADR-0011](adr/0011-packaging.md).

- Create the workspace root `Cargo.toml` (`[workspace] members = ["crates/*"]`).
- Move the generated crate to `crates/ably-chat-openapi/` (its `Cargo.toml`,
  `src/apis`, `src/models`, generated `lib.rs`); keep its name `ably-chat-openapi`
  (import `ably_chat_openapi`), unofficial description, dual licence.
- Create `crates/ably-chat-rs/` skeleton: `Cargo.toml` (name `ably-chat-rs`,
  `[lib] name = "ably_chat"`, edition 2024, MSRV 1.85, dual licence), depending on
  `ably-chat-openapi`; `src/lib.rs` with `pub mod raw` re-exporting the
  generated crate and empty module declarations.
- Move `LICENSE-*` into each crate; keep root copies.
- Set up `.openapi-generator-ignore` / regeneration command targeting
  `crates/ably-chat-openapi` (`-o crates/ably-chat-openapi`).

**Gate:** `cargo check --workspace` passes; `ably_chat::raw` resolves.

## Phase 1 — Core (client, config, auth, dispatch, error, types)

Implements ADR-0003, 0005, 0006, 0007, 0008.

- Domain types: `Serial`, `RoomName`, `Timestamp`, forward-compat enums,
  `Metadata`/`Headers`, `Message`, `MessageVersion`, `Occupancy`, reaction
  summaries + `serde` (de)serialization.
- `Error` + `Result`; parse the Ably error envelope; `is_retryable()` / `status()`.
- `Client` + builder: credentials enum (Basic/Bearer), host, timeout, injectable
  `reqwest::Client`, redacting `Debug`, TLS features.
- `dispatch`: build request, apply auth + `X-Ably-Version: 4`, run the
  retry-safety predicate, map responses/errors, expose response headers.

**Gate:** `cargo check`; unit tests for auth header encoding, error-envelope
mapping, and the retry predicate.

## Phase 2 — Read operations + pagination

Implements SPEC §6.1 (get, history, versions), §6.3 (occupancy), §6.2
(client-reactions); [ADR-0009](adr/0009-pagination-streaming.md).

- Handle chain: `Client::room`, `Room::messages`/`occupancy`,
  `Messages::reactions`.
- `get`, `occupancy().get`, `reactions().for_client`.
- `Page<T>` + `next()` + `into_stream()`; `history()` and `versions()`.

**Gate:** `cargo test` incl. `wiremock` tests for a single get, an error mapping,
and a two-page history that follows a `Link: rel="next"` header via the stream.

## Phase 3 — Write operations

Implements SPEC §6.1 (send, update, delete); idempotency + retry (ADR-0006).

- `send`, `update` (document full-replace), `delete` (soft, `POST …/delete`),
  each as an `IntoFuture` builder with `idempotency_key`.

**Gate:** `cargo test` incl. `wiremock` tests for send (201 → `Message`), update
full-replace, delete action, and idempotency-key passthrough.

## Phase 4 — Reactions write

Implements SPEC §6.2 (send, delete).

- `reactions().send` (not retried), `reactions().delete` (name-required rule for
  non-`unique`).

**Gate:** `cargo test` incl. `wiremock` tests for reaction send and a
name-required validation error.

## Phase 5 — Ergonomic polish

Implements [ADR-0010](adr/0010-ergonomic-surface.md).

- Finalize `IntoFuture` builders, a `prelude`, re-exports, and doc examples.
- Ensure handles are `Send + Sync` and cheap `Clone`.

**Gate:** `cargo test`; doctests compile (`cargo test --doc`); `cargo clippy`
clean.

## Phase 6 — Docs, features, publish prep

Implements ADR-0011.

- Reframe the root/README(s) crate-first; crate-level docs and `docs.rs`
  metadata (`all-features`).
- Confirm all feature combinations build (`--no-default-features`,
  `--features rustls,chrono,time`).
- Confirm the SPEC §3 singleton-body assumption against a live endpoint.
- `cargo publish --dry-run` for `ably-chat-openapi` then `ably-chat-rs`; set
  `repository`/`homepage`.

**Gate:** both dry-runs succeed; docs build; live singleton-body check recorded.

---

## Dependency order

Phase 0 → 1 → {2, 3} → 4 → 5 → 6. Phases 2 and 3 may proceed in parallel once
Phase 1 lands, since they share only the dispatch layer.

## Traceability

Every phase cites the ADRs and SPEC sections it implements; every gate is a
`cargo` command that must pass. No silent scope cuts — anything deferred is noted
in the phase's exit report.
