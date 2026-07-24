# Architecture Decision Records

Each ADR captures one architecturally-significant decision for `ably-chat-rs`:
its context, the decision, consequences, and the alternatives rejected. ADRs are
immutable once **Accepted**; a later ADR may **supersede** an earlier one.

| ADR | Title | Status |
| --- | ----- | ------ |
| [0001](0001-rest-only-scope.md) | REST-only scope, shaped for future realtime | Accepted |
| [0002](0002-workspace-topology.md) | Two-crate workspace | Accepted |
| [0003](0003-own-the-call-layer.md) | Own the HTTP call layer and domain types | Accepted |
| [0004](0004-generated-code-as-dependency.md) | Generated code as a dependency crate, re-exported as `raw` | Accepted |
| [0005](0005-authentication.md) | Static credentials only (Basic key / Bearer token) | Accepted |
| [0006](0006-retry-policy.md) | Retry only idempotent-safe requests | Accepted |
| [0007](0007-domain-modeling.md) | Domain modeling & forward compatibility | Accepted |
| [0008](0008-error-model.md) | One public error type with the Ably envelope | Accepted |
| [0009](0009-pagination-streaming.md) | Pagination as `Page<T>` + `Stream` over Link headers | Accepted |
| [0010](0010-ergonomic-surface.md) | Handle-chain surface, `IntoFuture` builders | Accepted |
| [0011](0011-packaging.md) | Packaging: name, licence, edition, MSRV | Accepted |

Detailed design (code-level) lives in [`../rust-wrapper-design.md`](../rust-wrapper-design.md).
The normative contract lives in [`../SPEC.md`](../SPEC.md). Realtime support is
planned in a separate repository —
[ably-realtime-rs](https://github.com/AnderEnder/ably-realtime-rs) — which holds
its own design, protocol spec, and ADRs.
