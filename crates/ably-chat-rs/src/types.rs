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
}
