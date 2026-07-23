# ADR-0003: Own the HTTP call layer and domain types

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk

## Context

The ergonomic layer could either **wrap** the generated API functions or **own**
an HTTP dispatch layer talking to `reqwest` directly. Two facts were verified
against the generated code:

1. The generated API functions return only the deserialized **body** and discard
   the HTTP response headers. Ably paginates history/versions **entirely through
   RFC 5988 `Link` headers**, so pagination is impossible through the generated
   functions.
2. Generated `MessageAction` (`src/models/message_action.rs`) is a **closed enum
   with no `#[serde(other)]`**. An unknown action value fails to deserialize the
   *entire* `Message`, before any conversion could run — a forward-compatibility
   hazard.

## Decision

The ergonomic layer **owns a single async HTTP dispatch function** (`reqwest`
directly) and **owns its own domain types** (see
[ADR-0007](0007-domain-modeling.md)). It does not call the generated API
functions and does not depend on the generated model structs at runtime.

## Consequences

- Pagination can read `Link` headers; retries and host handling live in one place
  ([ADR-0006](0006-retry-policy.md)).
- Forward-compatible types survive unknown enum values.
- More hand-written code; the generated models become reference material, not
  load-bearing.

## Alternatives considered

- **Wrap generated functions** — rejected: cannot paginate (headers discarded)
  and inherits the brittle closed enum.
- **Own HTTP but reuse generated model structs** — rejected: still inherits the
  closed-enum deserialization failure on `Message`.
