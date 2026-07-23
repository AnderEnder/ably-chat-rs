//! Unofficial, ergonomic Rust client for the Ably Chat REST API (v4).
//!
//! Not affiliated with or endorsed by Ably.
//!
//! # Overview
//!
//! Build a [`Client`] with an [`Auth`] credential, scope it to a room with
//! [`Client::room`], then chain into [`Messages`], [`Reactions`], or
//! [`OccupancyHandle`]. Each operation is a builder that terminates in a bare
//! `.await` (via [`IntoFuture`]); every fallible call
//! returns [`Result<T>`]. Handles are cheap to [`Clone`] (`Arc`-backed) and
//! `Send + Sync`.
//!
//! History and versions are paginated: `.await` a query for the first
//! [`Page`], or call `.into_stream()` to follow the `next` links to exhaustion.
//!
//! # Example
//!
//! ```no_run
//! use ably_chat::prelude::*;
//! use futures::StreamExt;
//!
//! # async fn run() -> ably_chat::Result<()> {
//! // Build a client and scope it to a room (rooms are implicit — this creates
//! // nothing server-side).
//! let client = Client::builder(Auth::api_key("appId.keyId:keySecret")).build();
//! let room = client.room("my-room");
//!
//! // Send a message.
//! let sent = room.messages().send("hello, world").await?;
//! println!("sent message {}", sent.serial);
//!
//! // Stream history (newest first by default), following pagination. The
//! // stream is `!Unpin`, so pin it before polling with `.next()`.
//! let mut history = std::pin::pin!(room.messages().history().into_stream());
//! while let Some(message) = history.next().await {
//!     let message = message?;
//!     println!("{}: {}", message.client_id, message.text);
//! }
//! # Ok(())
//! # }
//! ```

/// Low-level generated bindings. Escape hatch; NOT covered by the pre-1.0
/// stability guarantee and may change on regeneration.
pub mod raw {
    pub use ably_chat_openapi::*;
}

pub mod prelude;

// Ergonomic layer (filled in by later phases).
mod error;
pub use error::{Error, ErrorInfo, Result};

mod types;
pub use types::*;

mod config;
pub use config::*;

mod client;
pub use client::{Client, ClientBuilder};

mod dispatch;

mod room;
pub use room::Room;

mod messages;
pub use messages::{
    DeleteMessage, GetMessage, History, Messages, SendMessage, UpdateMessage, Versions,
};

mod reactions;
pub use reactions::{ClientReactions, DeleteReaction, Reactions, SendReaction};

mod occupancy;
pub use occupancy::{GetOccupancy, OccupancyHandle};

mod pagination;
pub use pagination::Page;
