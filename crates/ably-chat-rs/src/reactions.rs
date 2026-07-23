//! The reactions handle and reaction operations (ADR-0010).

use crate::client::Client;
use crate::types::RoomName;

/// Reaction operations on messages in a room.
///
/// Cheap to `Clone` (`Arc`-backed via [`Client`]) and `Send + Sync`.
#[derive(Clone, Debug)]
pub struct Reactions {
    pub(crate) client: Client,
    pub(crate) room: RoomName,
}

impl Reactions {
    pub(crate) fn new(client: Client, room: RoomName) -> Self {
        Self { client, room }
    }
}
