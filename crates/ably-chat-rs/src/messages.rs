//! The messages handle and message read operations (ADR-0010).

use std::future::{Future, IntoFuture};
use std::pin::Pin;

use reqwest::Method;

use crate::client::Client;
use crate::dispatch::{decode_json, message_path};
use crate::error::Result;
use crate::reactions::Reactions;
use crate::types::{Message, RoomName, Serial};

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

    /// Fetches a single message by its serial (latest version).
    ///
    /// `GET /chat/v4/rooms/{roomName}/messages/{serial}`. Retry-safe.
    pub fn get(&self, serial: impl Into<Serial>) -> GetMessage {
        GetMessage {
            client: self.client.clone(),
            room: self.room.clone(),
            serial: serial.into(),
        }
    }
}

/// Builder for [`Messages::get`]; `.await` it to fetch a [`Message`].
#[derive(Clone, Debug)]
pub struct GetMessage {
    client: Client,
    room: RoomName,
    serial: Serial,
}

impl IntoFuture for GetMessage {
    type Output = Result<Message>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let resp = self
                .client
                .inner
                .send(
                    Method::GET,
                    &message_path(self.room.as_str(), self.serial.as_str(), ""),
                    &[],
                    None,
                    false,
                )
                .await?;
            decode_json(&resp.body)
        })
    }
}
