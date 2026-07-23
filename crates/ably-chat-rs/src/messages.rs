//! The messages handle and message read operations (ADR-0010).

use std::future::{Future, IntoFuture};
use std::pin::Pin;

use futures::Stream;
use reqwest::Method;

use crate::client::Client;
use crate::dispatch::{decode_json, message_path, room_path};
use crate::error::Result;
use crate::pagination::{Fetch, Page, run_stream};
use crate::reactions::Reactions;
use crate::types::{Direction, Message, RoomName, Serial, Timestamp};

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

    /// Queries message history.
    ///
    /// `GET /chat/v4/rooms/{roomName}/messages`, paginated. Retry-safe. Defaults
    /// to **newest first** (`direction = backwards`) and `limit = 100`, matching
    /// the JS SDK. `.await` for the first [`Page<Message>`], or
    /// [`into_stream`](History::into_stream) to follow all pages.
    pub fn history(&self) -> History {
        History {
            client: self.client.clone(),
            room: self.room.clone(),
            start: None,
            end: None,
            direction: Direction::Backwards,
            limit: 100,
            from_serial: None,
        }
    }

    /// Queries all versions (create, updates, deletes) of a message.
    ///
    /// `GET /chat/v4/rooms/{roomName}/messages/{serial}/versions`, paginated.
    /// Retry-safe. `.await` for the first [`Page<Message>`], or
    /// [`into_stream`](Versions::into_stream) to follow all pages.
    pub fn versions(&self, serial: impl Into<Serial>) -> Versions {
        Versions {
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

/// Serializes a [`Direction`] to its wire query value.
fn direction_str(d: Direction) -> &'static str {
    match d {
        Direction::Forwards => "forwards",
        Direction::Backwards => "backwards",
    }
}

/// Builder for [`Messages::history`]. `.await` yields the first
/// [`Page<Message>`]; [`into_stream`](Self::into_stream) follows all pages.
#[derive(Clone, Debug)]
pub struct History {
    client: Client,
    room: RoomName,
    start: Option<i64>,
    end: Option<i64>,
    direction: Direction,
    limit: u32,
    from_serial: Option<Serial>,
}

impl History {
    /// Earliest timestamp to include (epoch millis; inclusive).
    pub fn start(mut self, start: impl Into<Timestamp>) -> Self {
        self.start = Some(start.into().as_millis());
        self
    }

    /// Latest timestamp to include (epoch millis; exclusive).
    pub fn end(mut self, end: impl Into<Timestamp>) -> Self {
        self.end = Some(end.into().as_millis());
        self
    }

    /// Ordering. Defaults to [`Direction::Backwards`] (newest first).
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Maximum messages per page (1..=1000). Defaults to `100`.
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Region-scoped serial to page from.
    pub fn from_serial(mut self, serial: impl Into<Serial>) -> Self {
        self.from_serial = Some(serial.into());
        self
    }

    /// Builds the query for the first request. `direction` and `limit` are
    /// always sent so the effective defaults are explicit on the wire.
    fn query(&self) -> Vec<(&'static str, String)> {
        let mut query: Vec<(&'static str, String)> = Vec::new();
        if let Some(start) = self.start {
            query.push(("start", start.to_string()));
        }
        if let Some(end) = self.end {
            query.push(("end", end.to_string()));
        }
        query.push(("direction", direction_str(self.direction).to_owned()));
        query.push(("limit", self.limit.to_string()));
        if let Some(from_serial) = &self.from_serial {
            query.push(("fromSerial", from_serial.as_str().to_owned()));
        }
        query
    }

    /// Streams every message across all history pages, following `next` links
    /// until exhausted.
    pub fn into_stream(self) -> impl Stream<Item = Result<Message>> + Send {
        let path = room_path(self.room.as_str(), "/messages");
        let query = self.query();
        run_stream(self.client, Vec::new(), Fetch::First { path, query })
    }
}

impl IntoFuture for History {
    type Output = Result<Page<Message>>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let path = room_path(self.room.as_str(), "/messages");
            let query = self.query();
            Page::fetch_first(self.client, path, query).await
        })
    }
}

/// Builder for [`Messages::versions`]. `.await` yields the first
/// [`Page<Message>`]; [`into_stream`](Self::into_stream) follows all pages.
#[derive(Clone, Debug)]
pub struct Versions {
    client: Client,
    room: RoomName,
    serial: Serial,
}

impl Versions {
    fn path(&self) -> String {
        message_path(self.room.as_str(), self.serial.as_str(), "/versions")
    }

    /// Streams every version across all pages, following `next` links until
    /// exhausted.
    pub fn into_stream(self) -> impl Stream<Item = Result<Message>> + Send {
        let path = self.path();
        run_stream(
            self.client,
            Vec::new(),
            Fetch::First {
                path,
                query: Vec::new(),
            },
        )
    }
}

impl IntoFuture for Versions {
    type Output = Result<Page<Message>>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let path = self.path();
            Page::fetch_first(self.client, path, Vec::new()).await
        })
    }
}
