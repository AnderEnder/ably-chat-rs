# ably-auth-openapi

**Unofficial**, machine-generated OpenAPI bindings for the subset of the Ably
**platform** REST API used for authentication and token lifecycle — requesting
Ably Tokens (`POST /keys/{keyName}/requestToken`), revoking them
(`POST /keys/{keyName}/revokeTokens`), and reading server time (`GET /time`).
Not affiliated with or endorsed by Ably.

The crate publishes as `ably-auth-openapi` and imports as `ably_auth_openapi`.

## A separate API from Chat

Token issuance and revocation live on the Ably **platform** host under `/keys/…`
— a different API from the Ably Chat REST API (`/chat/v4/*`, bound by
[`ably-chat-openapi`](https://crates.io/crates/ably-chat-openapi)). A token
server uses these endpoints to mint the Bearer credentials that a Chat client
then presents. See [ADR-0012](../../docs/adr/0012-token-issuance-permissions.md)
and [SPEC §13](../../docs/SPEC.md).

## Prefer `ably-chat-rs`

Most users should depend on the ergonomic crate
[`ably-chat-rs`](https://crates.io/crates/ably-chat-rs) instead. Reach for these
bindings directly only for the platform token endpoints, which the ergonomic
layer does not itself call.

## Generated code

The contents of `src/` are produced by
[openapi-generator](https://openapi-generator.tech/) from
[`openapi/ably-auth-rest.yaml`](../../openapi/ably-auth-rest.yaml) and **must not
be hand-edited** — a CI codegen gate regenerates the source and diffs it against
the committed tree. This surface is unstable and tracks regeneration; it is not
covered by any stability guarantee.

## Cargo features

All features are additive.

| Feature      | Default | Effect                               |
| ------------ | ------- | ------------------------------------ |
| `rustls`     | yes     | TLS via `rustls` (pure Rust).        |
| `native-tls` | no      | TLS via the system's native library. |

## Minimum supported Rust version

MSRV is **1.88** (edition 2024).

## License

Dual-licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
