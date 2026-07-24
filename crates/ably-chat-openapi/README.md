# ably-chat-openapi

[![crates.io](https://img.shields.io/crates/v/ably-chat-openapi.svg)](https://crates.io/crates/ably-chat-openapi)
[![docs.rs](https://img.shields.io/docsrs/ably-chat-openapi)](https://docs.rs/ably-chat-openapi)

**Unofficial**, machine-generated OpenAPI bindings for the
[Ably Chat](https://ably.com/docs/chat) REST API (v4). Not affiliated with or
endorsed by Ably.

The crate publishes as `ably-chat-openapi` and imports as
`ably_chat_openapi`.

## Prefer `ably-chat-rs`

Most users should depend on the ergonomic crate
[`ably-chat-rs`](https://crates.io/crates/ably-chat-rs) instead, which layers a
hand-written, forward-compatible client over these bindings and re-exports them
as `ably_chat::raw` for escape-hatch use.

Reach for `ably-chat-openapi` directly only when the ergonomic layer has a
gap you need to work around.

## Generated code

The contents of `src/` are produced by
[openapi-generator](https://openapi-generator.tech/) from
[`openapi/ably-chat-rest.yaml`](../../openapi/ably-chat-rest.yaml) and **must not
be hand-edited** — a CI codegen gate regenerates the source and diffs it against
the committed tree. This surface is unstable and tracks regeneration; it is not
covered by any stability guarantee.

## Cargo features

All features are additive.

| Feature      | Default | Effect                                          |
| ------------ | ------- | ----------------------------------------------- |
| `native-tls` | yes     | TLS via the system's native library.            |
| `rustls`     | no      | TLS via `rustls` (pure Rust).                   |

## Minimum supported Rust version

MSRV is **1.88** (edition 2024).

## License

Dual-licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
