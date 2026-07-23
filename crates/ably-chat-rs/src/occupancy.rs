//! The occupancy handle and occupancy read operation (ADR-0010).

use std::future::{Future, IntoFuture};
use std::pin::Pin;

use reqwest::Method;

use crate::client::Client;
use crate::dispatch::{decode_json, room_path};
use crate::error::Result;
use crate::types::{Occupancy, RoomName};

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

    /// Fetches the current occupancy metrics for the room.
    ///
    /// `GET /chat/v4/rooms/{roomName}/occupancy`. Retry-safe.
    pub fn get(&self) -> GetOccupancy {
        GetOccupancy {
            client: self.client.clone(),
            room: self.room.clone(),
        }
    }
}

/// Builder for [`OccupancyHandle::get`]; `.await` it to fetch [`Occupancy`].
#[derive(Clone, Debug)]
pub struct GetOccupancy {
    client: Client,
    room: RoomName,
}

impl IntoFuture for GetOccupancy {
    type Output = Result<Occupancy>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let resp = self
                .client
                .inner
                .send(
                    Method::GET,
                    &room_path(self.room.as_str(), "/occupancy"),
                    &[],
                    None,
                    false,
                )
                .await?;
            decode_json(&resp.body)
        })
    }
}
