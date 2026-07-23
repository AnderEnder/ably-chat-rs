//! Unofficial, ergonomic Rust client for the Ably Chat REST API (v4).
//!
//! Not affiliated with or endorsed by Ably.

/// Low-level generated bindings. Escape hatch; NOT covered by the pre-1.0
/// stability guarantee and may change on regeneration.
pub mod raw {
    pub use ably_chat_openapi::*;
}

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
pub use messages::Messages;

mod reactions;
pub use reactions::Reactions;

mod occupancy;
pub use occupancy::OccupancyHandle;
