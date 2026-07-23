//! Forward-compatible domain types owned by the ergonomic crate (ADR-0007).

use serde::{Deserialize, Serialize};

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
}
