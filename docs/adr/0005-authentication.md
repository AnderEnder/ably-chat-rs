# ADR-0005: Static credentials only (Basic key / Bearer token)

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk

## Context

Ably accepts HTTP **Basic** auth with an API key (`keyName:keySecret`, a
long-lived secret used server-side) or **Bearer** auth with an Ably Token/JWT
(short-lived, expires). Ably's own SDKs support an `authCallback` that fetches a
fresh token on demand — powerful, but it means storing an async closure/trait in
the client and refreshing on 401.

## Decision

The client builder accepts **either** an API key (Basic) **or** a token (Bearer),
modeled as an enum. **Static credentials only** in 0.x — the caller supplies a
currently-valid credential. No automatic token refresh. Reserve a
`TokenProvider`-style extension point for a later ADR.

## Consequences

- Covers the dominant server-side API-key use case with minimal complexity.
- Callers using expiring tokens must supply a fresh token (or rebuild the client)
  until refresh support lands.
- The auth input is an enum/trait that can gain a refresh variant without
  breaking callers.

## Alternatives considered

- **Static + auto-refresh (`authCallback`)** — deferred: largest complexity sink,
  unnecessary for the primary use case.
