# ADR-0009: Pagination as `Page<T>` + `Stream` over Link headers

- Status: Accepted
- Date: 2026-07-23
- Deciders: Andrii Radyk

## Context

`getMessages` (history) and `getMessageVersions` return a JSON **array** body and
paginate **entirely through RFC 5988 `Link` response headers** (`rel="next"`,
`first`, `current`). The dispatch layer ([ADR-0003](0003-own-the-call-layer.md))
already reads response headers, which the generated functions cannot.

## Decision

Model a page as `Page<T>` (the items plus the parsed link cursors). Offer:

- a **manual** path — `page.next().await? -> Option<Page<T>>`; and
- a **streaming** path — an `into_stream()` returning a `futures::Stream<Item = Result<T>>`
  that transparently follows `next` until exhausted.

## Consequences

- Idiomatic streaming with back-pressure; callers can also page manually.
- Adds a `futures` dependency.
- The two paginated endpoints share one paginator implementation.

## Alternatives considered

- **Return `Vec<T>` (first page only)** — rejected: silently drops history.
- **Callback/visitor pagination** — rejected: non-idiomatic in async Rust.
