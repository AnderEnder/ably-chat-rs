//! The messages handle and message read operations (ADR-0010).

use std::collections::BTreeMap;
use std::future::{Future, IntoFuture};
use std::pin::Pin;

use futures::Stream;
use reqwest::Method;
use serde_json::{Map, Value};

use crate::client::Client;
use crate::dispatch::{decode_json, message_path, room_path};
use crate::error::Result;
use crate::pagination::{Fetch, Page, run_stream};
use crate::reactions::Reactions;
use crate::types::{Direction, Message, Metadata, RoomName, Serial, Timestamp};

/// Converts a string→string map into a JSON object value (infallible).
fn string_map(map: &BTreeMap<String, String>) -> Value {
    Value::Object(
        map.iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect(),
    )
}

/// Builds the idempotency-key query and the retry-eligibility flag (ADR-0006):
/// a write is only retry-safe when the caller supplied an idempotency key.
fn idempotency(key: &Option<String>) -> (Vec<(&'static str, String)>, bool) {
    match key {
        Some(k) => (vec![("idempotencyKey", k.clone())], true),
        None => (Vec::new(), false),
    }
}

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

    /// Sends a new message to this room.
    ///
    /// `POST /chat/v4/rooms/{roomName}/messages`. Only retry-safe when an
    /// [`idempotency_key`](SendMessage::idempotency_key) is supplied (ADR-0006).
    pub fn send(&self, text: impl Into<String>) -> SendMessage {
        SendMessage {
            client: self.client.clone(),
            room: self.room.clone(),
            text: text.into(),
            metadata: None,
            headers: None,
            idempotency_key: None,
        }
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

    /// Updates (edits) a message, **fully replacing** its content.
    ///
    /// `PUT /chat/v4/rooms/{roomName}/messages/{serial}`. This is a
    /// **full replace**: the supplied `text` and any
    /// [`metadata`](UpdateMessage::metadata) /
    /// [`headers`](UpdateMessage::headers) become the message's entire new
    /// content. Omitted fields are **reset to empty**, not left unchanged — to
    /// preserve existing metadata/headers you must resend them. Produces a new
    /// version with action `message.update`. Only retry-safe when an
    /// [`idempotency_key`](UpdateMessage::idempotency_key) is supplied (ADR-0006).
    pub fn update(&self, serial: impl Into<Serial>, text: impl Into<String>) -> UpdateMessage {
        UpdateMessage {
            client: self.client.clone(),
            room: self.room.clone(),
            serial: serial.into(),
            text: text.into(),
            metadata: None,
            headers: None,
            description: None,
            idempotency_key: None,
        }
    }

    /// Soft-deletes a message.
    ///
    /// `POST /chat/v4/rooms/{roomName}/messages/{serial}/delete` — a `POST` to a
    /// `/delete` sub-resource, **not** an HTTP `DELETE`. Produces a new version
    /// with action `message.delete`; the message remains retrievable with its
    /// delete action applied. Only retry-safe when an
    /// [`idempotency_key`](DeleteMessage::idempotency_key) is supplied (ADR-0006).
    pub fn delete(&self, serial: impl Into<Serial>) -> DeleteMessage {
        DeleteMessage {
            client: self.client.clone(),
            room: self.room.clone(),
            serial: serial.into(),
            description: None,
            metadata: None,
            idempotency_key: None,
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

/// Builder for [`Messages::send`]; `.await` it to publish the message and
/// receive the created [`Message`].
#[derive(Clone, Debug)]
pub struct SendMessage {
    client: Client,
    room: RoomName,
    text: String,
    metadata: Option<Metadata>,
    headers: Option<BTreeMap<String, String>>,
    idempotency_key: Option<String>,
}

impl SendMessage {
    /// Attaches opaque user-defined metadata to the message.
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Attaches user-defined string headers to the message.
    pub fn headers(mut self, headers: BTreeMap<String, String>) -> Self {
        self.headers = Some(headers);
        self
    }

    /// Supplies an idempotency key, making the send safe to retry (ADR-0006).
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

impl IntoFuture for SendMessage {
    type Output = Result<Message>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut obj = Map::new();
            obj.insert("text".to_owned(), Value::String(self.text));
            if let Some(metadata) = self.metadata {
                obj.insert("metadata".to_owned(), Value::Object(metadata));
            }
            if let Some(headers) = &self.headers {
                obj.insert("headers".to_owned(), string_map(headers));
            }
            let (query, has_idem) = idempotency(&self.idempotency_key);
            let resp = self
                .client
                .inner
                .send(
                    Method::POST,
                    &room_path(self.room.as_str(), "/messages"),
                    &query,
                    Some(Value::Object(obj)),
                    has_idem,
                )
                .await?;
            decode_json(&resp.body)
        })
    }
}

/// Builder for [`Messages::update`]; `.await` it to apply the edit and receive
/// the updated [`Message`].
///
/// Update semantics are **full-replace**: see [`Messages::update`].
#[derive(Clone, Debug)]
pub struct UpdateMessage {
    client: Client,
    room: RoomName,
    serial: Serial,
    text: String,
    metadata: Option<Metadata>,
    headers: Option<BTreeMap<String, String>>,
    description: Option<String>,
    idempotency_key: Option<String>,
}

impl UpdateMessage {
    /// Sets the message's new metadata. Omitting this resets metadata to empty
    /// (full-replace; see [`Messages::update`]).
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Sets the message's new headers. Omitting this resets headers to empty
    /// (full-replace; see [`Messages::update`]).
    pub fn headers(mut self, headers: BTreeMap<String, String>) -> Self {
        self.headers = Some(headers);
        self
    }

    /// Attaches an optional description of the update operation.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Supplies an idempotency key, making the update safe to retry (ADR-0006).
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

impl IntoFuture for UpdateMessage {
    type Output = Result<Message>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut message = Map::new();
            message.insert("text".to_owned(), Value::String(self.text));
            if let Some(metadata) = self.metadata {
                message.insert("metadata".to_owned(), Value::Object(metadata));
            }
            if let Some(headers) = &self.headers {
                message.insert("headers".to_owned(), string_map(headers));
            }
            let mut obj = Map::new();
            obj.insert("message".to_owned(), Value::Object(message));
            if let Some(description) = self.description {
                obj.insert("description".to_owned(), Value::String(description));
            }
            let (query, has_idem) = idempotency(&self.idempotency_key);
            let resp = self
                .client
                .inner
                .send(
                    Method::PUT,
                    &message_path(self.room.as_str(), self.serial.as_str(), ""),
                    &query,
                    Some(Value::Object(obj)),
                    has_idem,
                )
                .await?;
            decode_json(&resp.body)
        })
    }
}

/// Builder for [`Messages::delete`]; `.await` it to soft-delete the message and
/// receive the resulting [`Message`] (action `message.delete`).
#[derive(Clone, Debug)]
pub struct DeleteMessage {
    client: Client,
    room: RoomName,
    serial: Serial,
    description: Option<String>,
    metadata: Option<BTreeMap<String, String>>,
    idempotency_key: Option<String>,
}

impl DeleteMessage {
    /// Attaches an optional description of the delete operation.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Attaches optional string metadata describing the delete operation.
    pub fn metadata(mut self, metadata: BTreeMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Supplies an idempotency key, making the delete safe to retry (ADR-0006).
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

impl IntoFuture for DeleteMessage {
    type Output = Result<Message>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut obj = Map::new();
            if let Some(description) = self.description {
                obj.insert("description".to_owned(), Value::String(description));
            }
            if let Some(metadata) = &self.metadata {
                obj.insert("metadata".to_owned(), string_map(metadata));
            }
            // The request body is optional; omit it entirely when unset.
            let body = if obj.is_empty() {
                None
            } else {
                Some(Value::Object(obj))
            };
            let (query, has_idem) = idempotency(&self.idempotency_key);
            let resp = self
                .client
                .inner
                .send(
                    Method::POST,
                    &message_path(self.room.as_str(), self.serial.as_str(), "/delete"),
                    &query,
                    body,
                    has_idem,
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
