# ably-chat-rs

[![crates.io](https://img.shields.io/crates/v/ably-chat-rs.svg)](https://crates.io/crates/ably-chat-rs)
[![docs.rs](https://img.shields.io/docsrs/ably-chat-rs)](https://docs.rs/ably-chat-rs)

An **unofficial**, ergonomic Rust client for the [Ably Chat](https://ably.com/docs/chat)
REST API (v4). Not affiliated with or endorsed by Ably.

The crate publishes as `ably-chat-rs` and imports as `ably_chat`.

## Install

```bash
cargo add ably-chat-rs
```

By default the pure-Rust `rustls` TLS backend is used (no OpenSSL). To use the
system `native-tls` backend instead:

```bash
cargo add ably-chat-rs --no-default-features --features native-tls
```

## Usage

Build a `Client` with an `Auth` credential, scope it to a room with
`client.room(name)`, then chain into messages, reactions, or occupancy. Every
operation is a builder that terminates in a bare `.await`; every fallible call
returns `ably_chat::Result<T>`. Handles are cheap to `Clone` (`Arc`-backed) and
`Send + Sync`.

```rust
use ably_chat::prelude::*;
use futures::StreamExt;

async fn run() -> ably_chat::Result<()> {
    // Rooms are implicit — scoping to one creates nothing server-side.
    let client = Client::builder(Auth::api_key("appId.keyId:keySecret")).build();
    let room = client.room("my-room");

    // Send a message.
    let sent = room.messages().send("hello, world").await?;
    println!("sent message {}", sent.serial);

    // Stream history (newest first by default), following pagination. The
    // stream is `!Unpin`, so pin it before polling with `.next()`.
    let mut history = std::pin::pin!(room.messages().history().into_stream());
    while let Some(message) = history.next().await {
        let message = message?;
        println!("{}: {}", message.client_id, message.text);
    }
    Ok(())
}
```

## What this crate covers

The ten REST operations of the Ably Chat REST API:

- **Messages:** send, get, update (full-replace), soft-delete, history, versions.
- **Reactions:** send, delete, and a client's reactions on a message.
- **Occupancy:** room occupancy metrics.

### Not covered

- **No realtime.** Live message/presence/typing subscriptions, room reactions,
  and live reaction summaries are realtime-transport features with no REST
  endpoint; they are out of scope. Use Ably's realtime SDKs for those.
- **No room/channel CRUD.** Chat rooms are channel-backed and implicit — a room
  exists the first time a client uses it. There is no create-room / delete-room
  operation and this crate does not expose one.

## Cargo features

All features are additive.

| Feature      | Default | Effect                                                   |
| ------------ | ------- | -------------------------------------------------------- |
| `rustls`     | yes     | TLS via `rustls` (Rust; aws-lc-rs + platform verifier).  |
| `native-tls` | no      | TLS via the system's native library (OpenSSL/SChannel).  |
| `chrono`     | no      | `Timestamp::to_chrono()` conversion to `chrono::DateTime`.|

## Low-level escape hatch: `ably_chat::raw`

The generated OpenAPI bindings (`ably-chat-openapi`) are re-exported as
`ably_chat::raw`. Drop down to them when the ergonomic layer has a gap. This
module is **not** covered by the pre-1.0 stability guarantee and may change on
regeneration.

## Caveat: singleton response bodies

Single-resource responses (`getMessage`, `sendMessage`, `updateMessage`,
`deleteMessage`, `getOccupancy`, `getClientReactions`) are modelled as **bare
JSON objects**, following Ably's documented REST convention. This is
corroborated but not yet wire-captured; it should be confirmed against a live
endpoint before 1.0:

```bash
curl -sS "https://rest.ably.io/chat/v4/rooms/my-room/occupancy" \
  -H "X-Ably-Version: 4" -u "{keyName}:{keySecret}" | head -c1
# '{'  → bare object (matches this crate's model)
# '['  → 1-element array (the model would need adjusting)
```

## Minimum supported Rust version

MSRV is **1.85** (edition 2024).

## License

Dual-licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
