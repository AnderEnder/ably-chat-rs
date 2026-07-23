//! The room-scoped handle (ADR-0010).

use crate::client::Client;
use crate::messages::Messages;
use crate::occupancy::OccupancyHandle;
use crate::types::RoomName;

/// A handle to a single chat room, scoped by [`RoomName`].
///
/// Rooms are implicit: obtaining a `Room` neither creates nor deletes anything
/// server-side. Cheap to `Clone` (`Arc`-backed via [`Client`]) and `Send + Sync`.
#[derive(Clone, Debug)]
pub struct Room {
    client: Client,
    room: RoomName,
}

impl Room {
    pub(crate) fn new(client: Client, room: RoomName) -> Self {
        Self { client, room }
    }

    /// The name of this room.
    pub fn name(&self) -> &RoomName {
        &self.room
    }

    /// Message operations (send, get, update, delete, history, versions).
    pub fn messages(&self) -> Messages {
        Messages::new(self.client.clone(), self.room.clone())
    }

    /// Room occupancy metrics.
    pub fn occupancy(&self) -> OccupancyHandle {
        OccupancyHandle::new(self.client.clone(), self.room.clone())
    }
}
