# ADR-0006: Retry only idempotent-safe requests

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk

## Context

Transient failures (network, 429, 5xx, host rotation) warrant retries. But
blindly retrying non-idempotent writes is dangerous: `sendMessageReaction` with
`type = multiple` **counts each call**, so a retried POST double-counts. The REST
API offers an `idempotencyKey` query parameter on the three message writes
(send/update/delete).

## Decision

Retries are governed by a **safety predicate**: a request is retry-eligible only
if it is `GET`/`DELETE` **or** it carries an `idempotencyKey`. Respect
`Retry-After` where present. The dispatch layer ([ADR-0003](0003-own-the-call-layer.md))
owns this logic.

## Consequences

- No accidental double-counting of `multiple` reactions or duplicate sends.
- `sendMessage`/`updateMessage`/`deleteMessage` become retry-safe when an
  idempotency key is attached.
- `sendMessageReaction` is not retried (it has no idempotency key).

## Alternatives considered

- **Retry all transient failures** — rejected: unsafe for counted reactions.
- **No retries** — rejected: poor resilience for a network client.
