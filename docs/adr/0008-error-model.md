# ADR-0008: One public error type with the Ably envelope

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk

## Context

Failures span three kinds: transport (`reqwest`), (de)serialization, and API
errors carrying Ably's error envelope (`code`, `message`, `statusCode`, `href`).
Callers need to branch on retryability ([ADR-0006](0006-retry-policy.md)) and on
specific Chat error codes.

## Decision

A single public `Error` enum (via `thiserror`), `#[non_exhaustive]`, with variants
for transport, decode, and api errors. The api variant carries the parsed
`ErrorInfo`. Provide accessors: `status() -> Option<u16>`, `is_retryable() -> bool`,
and helpers for the notable Chat codes — 40400 (message not found), 42211
(rejected by a room rule), 42213 (rejected by moderation). Expose
`type Result<T> = std::result::Result<T, Error>;`.

## Consequences

- One type to import and match on across the whole API.
- The retry layer consumes `is_retryable()`.
- `#[non_exhaustive]` keeps adding variants non-breaking.

## Alternatives considered

- **Per-operation error enums** (as the generated crate has) — rejected: poor
  ergonomics; callers would juggle many error types.
