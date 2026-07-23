//! Forward-compatible domain types owned by the ergonomic crate (ADR-0007).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Opaque, user-defined JSON metadata. Not interpreted by Ably; treat as
/// untrusted input when reading.
pub type Metadata = serde_json::Map<String, serde_json::Value>;

/// Defines a transparent `String` newtype with the shared identifier ergonomics.
///
/// The `ord` marker documents intent: identifiers that are region-scoped (e.g.
/// [`Serial`]) deliberately omit `Ord`; if a keyed id later needs ordering, add
/// a separate derive rather than flipping this marker.
macro_rules! string_newtype {
    ($(#[$m:meta])* $name:ident, ord = $ord:tt) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            /// Borrows the underlying string.
            pub fn as_str(&self) -> &str { &self.0 }
            /// Consumes the newtype, returning the owned string.
            pub fn into_string(self) -> String { self.0 }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl From<String> for $name {
            fn from(s: String) -> Self { Self(s) }
        }
        impl From<&str> for $name {
            fn from(s: &str) -> Self { Self(s.to_owned()) }
        }
        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str { &self.0 }
        }
    };
}

string_newtype!(
    /// A message's unique, region-scoped identifier.
    ///
    /// Intentionally does **not** implement `Ord`: serials are region-scoped and
    /// not globally ordered (ADR-0007).
    Serial,
    ord = false
);

string_newtype!(
    /// The name of a chat room.
    RoomName,
    ord = false
);

/// Milliseconds since the Unix epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Returns the raw milliseconds-since-epoch value.
    pub fn as_millis(&self) -> i64 {
        self.0
    }

    /// Converts to a `chrono` UTC datetime, if representable.
    #[cfg(feature = "chrono")]
    pub fn to_chrono(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::from_timestamp_millis(self.0)
    }
}

impl From<i64> for Timestamp {
    fn from(v: i64) -> Self {
        Self(v)
    }
}

/// The action that produced a message version.
///
/// Forward-compatible: an unknown wire value is preserved in [`Other`] rather
/// than failing deserialization (ADR-0007).
///
/// [`Other`]: MessageAction::Other
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum MessageAction {
    /// `message.create`
    Create,
    /// `message.update`
    Update,
    /// `message.delete`
    Delete,
    /// An unrecognised action value, preserved verbatim.
    Other(String),
}

impl From<String> for MessageAction {
    fn from(s: String) -> Self {
        match s.as_str() {
            "message.create" => Self::Create,
            "message.update" => Self::Update,
            "message.delete" => Self::Delete,
            _ => Self::Other(s),
        }
    }
}

impl From<MessageAction> for String {
    fn from(a: MessageAction) -> String {
        match a {
            MessageAction::Create => "message.create".into(),
            MessageAction::Update => "message.update".into(),
            MessageAction::Delete => "message.delete".into(),
            MessageAction::Other(s) => s,
        }
    }
}

/// History/versions ordering. Query-only; serialized as a lowercase string by
/// the dispatch layer, never via serde.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Oldest first.
    Forwards,
    /// Newest first (the default).
    Backwards,
}

/// The reaction aggregation model.
///
/// Forward-compatible: an unknown wire value is preserved in [`Other`] rather
/// than failing deserialization (ADR-0007).
///
/// [`Other`]: ReactionType::Other
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ReactionType {
    /// At most one reaction per client.
    Unique,
    /// At most one of each named reaction per client.
    Distinct,
    /// Repeatable and counted.
    Multiple,
    /// An unrecognised reaction type, preserved verbatim.
    Other(String),
}

impl From<String> for ReactionType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "unique" => Self::Unique,
            "distinct" => Self::Distinct,
            "multiple" => Self::Multiple,
            _ => Self::Other(s),
        }
    }
}

impl From<ReactionType> for String {
    fn from(t: ReactionType) -> String {
        match t {
            ReactionType::Unique => "unique".into(),
            ReactionType::Distinct => "distinct".into(),
            ReactionType::Multiple => "multiple".into(),
            ReactionType::Other(s) => s,
        }
    }
}

/// A chat message in the V4 REST representation.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// The message's unique, region-scoped identifier.
    pub serial: Serial,
    /// Details of the latest create/update/delete version of this message.
    pub version: MessageVersion,
    /// The text content of the message.
    pub text: String,
    /// The client ID of the user who created the message.
    pub client_id: String,
    /// The action that produced this message version.
    pub action: MessageAction,
    /// Arbitrary user-defined metadata.
    #[serde(default)]
    pub metadata: Metadata,
    /// Arbitrary user-defined string headers.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// User claim attached by the server, present only when the publishing
    /// token carried a matching `ably.room.<roomName>` claim.
    #[serde(default)]
    pub user_claim: Option<String>,
    /// Milliseconds since the Unix epoch at which the message was created.
    pub timestamp: Timestamp,
    /// Summary of reactions on this message. Absent groups default to empty.
    #[serde(default)]
    pub reactions: ReactionSummary,
}

/// Details of the latest create/update/delete version of a message.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageVersion {
    /// Unique identifier of this message version.
    pub serial: Serial,
    /// Milliseconds since the Unix epoch at which this version was created.
    pub timestamp: Timestamp,
    /// Client ID of the user who performed the update or deletion.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Optional description supplied with an update or deletion.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional metadata supplied with an update or deletion.
    #[serde(default)]
    pub metadata: Option<BTreeMap<String, String>>,
}

/// Occupancy metrics for a room.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Occupancy {
    /// The number of connections to the room.
    pub connections: u64,
    /// The number of members currently present in the room.
    pub presence_members: u64,
}

/// Summary of reactions on a message, grouped by reaction type. Each map is
/// keyed by the reaction name (e.g. an emoji). Absent groups default to empty.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReactionSummary {
    /// `unique` reactions, keyed by reaction name.
    #[serde(default)]
    pub unique: BTreeMap<String, ClientIdList>,
    /// `distinct` reactions, keyed by reaction name.
    #[serde(default)]
    pub distinct: BTreeMap<String, ClientIdList>,
    /// `multiple` reactions, keyed by reaction name.
    #[serde(default)]
    pub multiple: BTreeMap<String, ClientIdCounts>,
}

/// Aggregated set of client IDs for a `unique`/`distinct` reaction.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientIdList {
    /// Total number of clients that applied this reaction.
    pub total: u64,
    /// The client IDs that applied this reaction.
    #[serde(default)]
    pub client_ids: Vec<String>,
    /// Whether the `client_ids` list was truncated.
    #[serde(default)]
    pub clipped: bool,
}

/// Aggregated per-client counts for a `multiple` reaction.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientIdCounts {
    /// Total count across all clients (sum of per-client counts).
    pub total: u64,
    /// Map of client ID to that client's reaction count.
    #[serde(default)]
    pub client_ids: BTreeMap<String, u64>,
    /// Total count contributed by unidentified clients.
    #[serde(default)]
    pub total_unidentified: u64,
    /// Whether the `client_ids` map was truncated.
    #[serde(default)]
    pub clipped: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_roundtrips_through_json() {
        let s: Serial = serde_json::from_str("\"01abc@def:001\"").unwrap();
        assert_eq!(s.as_str(), "01abc@def:001");
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"01abc@def:001\"");
    }

    #[test]
    fn timestamp_is_epoch_millis() {
        let t: Timestamp = serde_json::from_str("1700000000000").unwrap();
        assert_eq!(t.as_millis(), 1_700_000_000_000);
    }

    #[test]
    fn unknown_action_is_captured_not_rejected() {
        let a: MessageAction = serde_json::from_str("\"message.future\"").unwrap();
        assert_eq!(a, MessageAction::Other("message.future".into()));
        let c: MessageAction = serde_json::from_str("\"message.create\"").unwrap();
        assert_eq!(c, MessageAction::Create);
        // Known variants round-trip to their wire string.
        assert_eq!(
            serde_json::to_string(&MessageAction::Delete).unwrap(),
            "\"message.delete\""
        );
    }

    #[test]
    fn unknown_reaction_type_is_captured_not_rejected() {
        let r: ReactionType = serde_json::from_str("\"future\"").unwrap();
        assert_eq!(r, ReactionType::Other("future".into()));
        let d: ReactionType = serde_json::from_str("\"distinct\"").unwrap();
        assert_eq!(d, ReactionType::Distinct);
        assert_eq!(
            serde_json::to_string(&ReactionType::Multiple).unwrap(),
            "\"multiple\""
        );
    }

    #[test]
    fn message_deserializes_from_wire_camelcase() {
        // Byte-faithful to openapi/ably-chat-rest.yaml (camelCase, nested version,
        // reactions omitted).
        let wire = r#"{
            "serial": "01726585978590-001@abcdefghij:001",
            "version": {
                "serial": "01726585978590-001@abcdefghij:001",
                "timestamp": 1700000000000,
                "clientId": "alice",
                "description": "edited"
            },
            "text": "hello",
            "clientId": "alice",
            "action": "message.create",
            "metadata": {"priority": "high"},
            "headers": {"topic": "announcements"},
            "timestamp": 1700000000000,
            "userClaim": "room-claim"
        }"#;
        let msg: Message = serde_json::from_str(wire).unwrap();
        assert_eq!(msg.serial.as_str(), "01726585978590-001@abcdefghij:001");
        assert_eq!(msg.text, "hello");
        assert_eq!(msg.client_id, "alice");
        assert_eq!(msg.action, MessageAction::Create);
        assert_eq!(msg.timestamp.as_millis(), 1_700_000_000_000);
        assert_eq!(msg.version.client_id.as_deref(), Some("alice"));
        assert_eq!(msg.version.description.as_deref(), Some("edited"));
        assert_eq!(msg.metadata["priority"], serde_json::json!("high"));
        assert_eq!(msg.headers["topic"], "announcements");
        assert_eq!(msg.user_claim.as_deref(), Some("room-claim"));
        // Absent reactions default to empty groups.
        assert!(msg.reactions.unique.is_empty());
        assert!(msg.reactions.distinct.is_empty());
        assert!(msg.reactions.multiple.is_empty());
    }

    #[test]
    fn reaction_summary_deserializes_camelcase_fields() {
        let wire = r#"{
            "unique": {"👍": {"total": 2, "clientIds": ["alice", "bob"], "clipped": false}},
            "multiple": {"🎉": {"total": 5, "clientIds": {"alice": 3, "bob": 2}, "totalUnidentified": 1}}
        }"#;
        let summary: ReactionSummary = serde_json::from_str(wire).unwrap();
        let thumbs = &summary.unique["\u{1f44d}"];
        assert_eq!(thumbs.total, 2);
        assert_eq!(thumbs.client_ids, vec!["alice", "bob"]);
        assert!(!thumbs.clipped);
        let party = &summary.multiple["\u{1f389}"];
        assert_eq!(party.total, 5);
        assert_eq!(party.client_ids["alice"], 3);
        assert_eq!(party.total_unidentified, 1);
        assert!(summary.distinct.is_empty());
    }

    #[test]
    fn occupancy_deserializes() {
        let occ: Occupancy =
            serde_json::from_str(r#"{"connections": 3, "presenceMembers": 2}"#).unwrap();
        assert_eq!(occ.connections, 3);
        assert_eq!(occ.presence_members, 2);
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn timestamp_converts_to_chrono() {
        let t = Timestamp::from(1_700_000_000_000);
        assert_eq!(t.to_chrono().unwrap().timestamp_millis(), 1_700_000_000_000);
    }
}
