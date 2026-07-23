//! The reactions handle and reaction operations (ADR-0010).

use std::future::{Future, IntoFuture};
use std::pin::Pin;

use reqwest::Method;
use serde_json::{Map, Value};

use crate::client::Client;
use crate::dispatch::{decode_json, message_path};
use crate::error::Result;
use crate::types::{ReactionSummary, ReactionType, RoomName, Serial};

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

    /// Adds a reaction to a message.
    ///
    /// `POST /chat/v4/rooms/{roomName}/messages/{serial}/reactions`. The reaction
    /// [`kind`](SendReaction::kind) defaults to [`ReactionType::Distinct`] (the JS
    /// SDK default). **Never retried** (ADR-0006): a send carries no idempotency
    /// key, and a `multiple` reaction increments a counter, so a blind retry could
    /// double-count.
    pub fn send(&self, serial: impl Into<Serial>, name: impl Into<String>) -> SendReaction {
        SendReaction {
            client: self.client.clone(),
            room: self.room.clone(),
            serial: serial.into(),
            name: name.into(),
            kind: ReactionType::Distinct,
            count: None,
        }
    }

    /// Fetches the reaction summary for a single message, optionally filtered to
    /// one client via [`ClientReactions::client_id`].
    ///
    /// `GET /chat/v4/rooms/{roomName}/messages/{serial}/client-reactions`.
    /// Retry-safe. Useful when a message's summary is clipped and you need to
    /// determine whether a specific client has reacted.
    pub fn for_client(&self, serial: impl Into<Serial>) -> ClientReactions {
        ClientReactions {
            client: self.client.clone(),
            room: self.room.clone(),
            serial: serial.into(),
            client_id: None,
        }
    }
}

/// Builder for [`Reactions::send`]; `.await` it to add the reaction. Resolves to
/// `()` on success (the endpoint returns `201` with no body).
#[derive(Clone, Debug)]
pub struct SendReaction {
    client: Client,
    room: RoomName,
    serial: Serial,
    name: String,
    kind: ReactionType,
    count: Option<u64>,
}

impl SendReaction {
    /// Sets the reaction aggregation model. Defaults to [`ReactionType::Distinct`].
    pub fn kind(mut self, kind: ReactionType) -> Self {
        self.kind = kind;
        self
    }

    /// Sets the count for a `multiple`-type reaction (defaults to `1` server-side;
    /// ignored by the server for other types).
    pub fn count(mut self, count: u64) -> Self {
        self.count = Some(count);
        self
    }
}

impl IntoFuture for SendReaction {
    type Output = Result<()>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut obj = Map::new();
            obj.insert("type".to_owned(), Value::String(self.kind.into()));
            obj.insert("name".to_owned(), Value::String(self.name));
            if let Some(count) = self.count {
                obj.insert("count".to_owned(), Value::from(count));
            }
            self.client
                .inner
                .send(
                    Method::POST,
                    &message_path(self.room.as_str(), self.serial.as_str(), "/reactions"),
                    &[],
                    Some(Value::Object(obj)),
                    // Never retried (ADR-0006): no idempotency key and `multiple`
                    // reactions count each call.
                    false,
                )
                .await?;
            Ok(())
        })
    }
}

/// Builder for [`Reactions::for_client`]; `.await` it to fetch a
/// [`ReactionSummary`]. Without [`client_id`](Self::client_id), the server
/// defaults to the authenticated caller's client ID.
#[derive(Clone, Debug)]
pub struct ClientReactions {
    client: Client,
    room: RoomName,
    serial: Serial,
    client_id: Option<String>,
}

impl ClientReactions {
    /// Filters the summary to a specific client ID (`forClientId`).
    pub fn client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = Some(client_id.into());
        self
    }
}

impl IntoFuture for ClientReactions {
    type Output = Result<ReactionSummary>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut query: Vec<(&str, String)> = Vec::new();
            if let Some(cid) = self.client_id {
                query.push(("forClientId", cid));
            }
            let resp = self
                .client
                .inner
                .send(
                    Method::GET,
                    &message_path(self.room.as_str(), self.serial.as_str(), "/client-reactions"),
                    &query,
                    None,
                    false,
                )
                .await?;
            decode_json(&resp.body)
        })
    }
}
