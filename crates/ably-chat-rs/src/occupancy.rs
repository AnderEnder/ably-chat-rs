//! The occupancy handle and occupancy read operation (ADR-0010).

use crate::client::Client;
use crate::types::RoomName;

/// Occupancy operations for a room.
///
/// Named `OccupancyHandle` to avoid clashing with the [`Occupancy`] data type.
/// Cheap to `Clone` (`Arc`-backed via [`Client`]) and `Send + Sync`.
///
/// [`Occupancy`]: crate::types::Occupancy
#[derive(Clone, Debug)]
pub struct OccupancyHandle {
    pub(crate) client: Client,
    pub(crate) room: RoomName,
}

impl OccupancyHandle {
    pub(crate) fn new(client: Client, room: RoomName) -> Self {
        Self { client, room }
    }
}
