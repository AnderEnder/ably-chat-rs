//! Compile-time guarantees for the public surface (SPEC §10, ADR-0010):
//! every handle, builder, `Page`, and the `Error` type is `Send + Sync` and can
//! be reached through the crate prelude.
//!
//! The `use ably_chat::prelude::*;` glob doubles as a check that the prelude
//! actually re-exports the client, auth, handles, and domain types. The
//! `assert_send_sync` calls fail to *compile* (not merely fail at runtime) if
//! any type stops being `Send + Sync`, so this is a genuine regression lock.

use ably_chat::prelude::*;

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn public_handles_builders_and_types_are_send_sync() {
    // Entry point and its builder.
    assert_send_sync::<Client>();
    assert_send_sync::<ClientBuilder>();
    assert_send_sync::<Auth>();

    // Handle chain.
    assert_send_sync::<Room>();
    assert_send_sync::<Messages>();
    assert_send_sync::<Reactions>();
    assert_send_sync::<OccupancyHandle>();

    // Operation builders (held across `.await` points).
    assert_send_sync::<ably_chat::SendMessage>();
    assert_send_sync::<ably_chat::GetMessage>();
    assert_send_sync::<ably_chat::UpdateMessage>();
    assert_send_sync::<ably_chat::DeleteMessage>();
    assert_send_sync::<ably_chat::History>();
    assert_send_sync::<ably_chat::Versions>();
    assert_send_sync::<ably_chat::SendReaction>();
    assert_send_sync::<ably_chat::DeleteReaction>();
    assert_send_sync::<ably_chat::ClientReactions>();
    assert_send_sync::<ably_chat::GetOccupancy>();

    // Pagination and error.
    assert_send_sync::<Page<Message>>();
    assert_send_sync::<Error>();

    // Domain types reachable through the prelude.
    assert_send_sync::<Message>();
    assert_send_sync::<Occupancy>();
    assert_send_sync::<ReactionSummary>();
    assert_send_sync::<Serial>();
    assert_send_sync::<RoomName>();
    assert_send_sync::<Timestamp>();
}
