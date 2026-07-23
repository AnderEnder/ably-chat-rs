//! Convenience re-exports for common usage.
//!
//! ```
//! use ably_chat::prelude::*;
//! ```
//!
//! Brings the client entry points, credentials, the room-scoped handles, the
//! error/result types, [`Page`], and the domain types into scope. Operation
//! builders (e.g. the value returned by `messages().send(..)`) are intentionally
//! *not* re-exported: they are awaited immediately and rarely named.

pub use crate::client::{Client, ClientBuilder};
pub use crate::config::Auth;
pub use crate::error::{Error, ErrorInfo, Result};
pub use crate::pagination::Page;

// Room-scoped handles (ADR-0010).
pub use crate::messages::Messages;
pub use crate::occupancy::OccupancyHandle;
pub use crate::reactions::Reactions;
pub use crate::room::Room;

// Domain types (ADR-0007).
pub use crate::types::{
    ClientIdCounts, ClientIdList, Direction, Message, MessageAction, MessageVersion, Metadata,
    Occupancy, ReactionSummary, ReactionType, RoomName, Serial, Timestamp,
};
