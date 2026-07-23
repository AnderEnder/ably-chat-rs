//! The messages handle and message read operations (ADR-0010).

use crate::client::Client;
use crate::reactions::Reactions;
use crate::types::RoomName;

/// Message operations for a room.
///
/// Cheap to `Clone` (`Arc`-backed via [`Client`]) and `Send + Sync`.
#[derive(Clone, Debug)]
pub struct Messages {
    pub(crate) client: Client,
    pub(crate) room: RoomName,
}

impl Messages {
    pub(crate) fn new(client: Client, room: RoomName) -> Self {
        Self { client, room }
    }

    /// Reaction operations on messages in this room.
    pub fn reactions(&self) -> Reactions {
        Reactions::new(self.client.clone(), self.room.clone())
    }
}
