//! Typed Ably capability model (ADR-0012, SPEC §13.1). Feature `capabilities`.

/// An Ably capability operation. `#[non_exhaustive]`; unknown wire values map to
/// `Other` (ADR-0007) so parsing never fails.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Operation {
    Subscribe,
    Publish,
    Presence,
    ObjectSubscribe,
    ObjectPublish,
    AnnotationSubscribe,
    AnnotationPublish,
    MessageUpdateOwn,
    MessageUpdateAny,
    MessageDeleteOwn,
    MessageDeleteAny,
    History,
    Stats,
    PushSubscribe,
    PushAdmin,
    ChannelMetadata,
    PrivilegedHeaders,
    /// A forward-compatible/custom operation string.
    Other(String),
}

impl Operation {
    /// The exact wire string Ably uses for this operation.
    pub fn as_str(&self) -> &str {
        match self {
            Operation::Subscribe => "subscribe",
            Operation::Publish => "publish",
            Operation::Presence => "presence",
            Operation::ObjectSubscribe => "object-subscribe",
            Operation::ObjectPublish => "object-publish",
            Operation::AnnotationSubscribe => "annotation-subscribe",
            Operation::AnnotationPublish => "annotation-publish",
            Operation::MessageUpdateOwn => "message-update-own",
            Operation::MessageUpdateAny => "message-update-any",
            Operation::MessageDeleteOwn => "message-delete-own",
            Operation::MessageDeleteAny => "message-delete-any",
            Operation::History => "history",
            Operation::Stats => "stats",
            Operation::PushSubscribe => "push-subscribe",
            Operation::PushAdmin => "push-admin",
            Operation::ChannelMetadata => "channel-metadata",
            Operation::PrivilegedHeaders => "privileged-headers",
            Operation::Other(s) => s.as_str(),
        }
    }
}

/// A capability document: resource pattern → set of allowed operation strings.
/// Operation strings are stored (not the enum) so the `BTreeSet` sorts
/// lexicographically by wire value, matching Ably's canonicalization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capability(std::collections::BTreeMap<String, std::collections::BTreeSet<String>>);

impl Capability {
    /// An empty capability document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant `ops` on `resource` (a channel-name pattern, e.g. `"room"`,
    /// `"dms:*"`, `"*"`). Repeated resources merge.
    pub fn allow(
        mut self,
        resource: impl Into<String>,
        ops: impl IntoIterator<Item = Operation>,
    ) -> Self {
        let entry = self.0.entry(resource.into()).or_default();
        for op in ops {
            entry.insert(op.as_str().to_owned());
        }
        self
    }

    /// The canonical capability string for a TokenRequest or an
    /// `x-ably-capability` JWT claim: sorted resource keys, sorted operations,
    /// no whitespace.
    pub fn to_capability_string(&self) -> String {
        // Infallible: the map is String→[String].
        serde_json::to_string(&self.0).expect("capability map is always serializable")
    }

    /// Grant `ops` on a chat room, scoped by **room name**.
    ///
    /// Uses the bare room name, which Ably's product model expands to authorize
    /// both the `/chat/v4` REST API and the `room::$chat` channel. Do **not** pass
    /// `"{room}::$chat"` here — that authorizes only the realtime channel and would
    /// `40160` on REST. The explicit product qualifier `[chat]{room}` is available
    /// via [`Capability::allow`].
    ///
    /// NOTE (pre-1.0): the exact resource form is confirmed only to moderate-high
    /// confidence; verify against a live app before stabilization (see
    /// `docs/research/2026-07-24-ably-chat-auth-permissions.md` §A2.2).
    pub fn for_room(self, room: &str, ops: impl IntoIterator<Item = Operation>) -> Self {
        self.allow(room.to_owned(), ops)
    }

    /// The capability as a native JSON object (not a stringified value), for the
    /// Ably Control API key `capability` field (ADR-0012 §A2.5).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.0).expect("capability map is always serializable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_as_str_matches_ably_strings() {
        assert_eq!(Operation::Publish.as_str(), "publish");
        assert_eq!(Operation::Subscribe.as_str(), "subscribe");
        assert_eq!(Operation::ObjectSubscribe.as_str(), "object-subscribe");
        assert_eq!(Operation::AnnotationPublish.as_str(), "annotation-publish");
        assert_eq!(Operation::MessageUpdateOwn.as_str(), "message-update-own");
        assert_eq!(Operation::MessageDeleteAny.as_str(), "message-delete-any");
        assert_eq!(Operation::ChannelMetadata.as_str(), "channel-metadata");
        assert_eq!(Operation::PrivilegedHeaders.as_str(), "privileged-headers");
        assert_eq!(Operation::Other("custom".into()).as_str(), "custom");
    }

    #[test]
    fn canonical_string_sorts_keys_and_ops_no_whitespace() {
        let cap = Capability::new()
            .allow("z-room", [Operation::Subscribe, Operation::Publish])
            .allow("a-room", [Operation::History]);
        // Keys sorted (a-room before z-room); ops sorted (publish before subscribe); no spaces.
        assert_eq!(
            cap.to_capability_string(),
            r#"{"a-room":["history"],"z-room":["publish","subscribe"]}"#
        );
    }

    #[test]
    fn allow_merges_repeated_resource() {
        let cap = Capability::new()
            .allow("r", [Operation::Publish])
            .allow("r", [Operation::History]);
        assert_eq!(cap.to_capability_string(), r#"{"r":["history","publish"]}"#);
    }

    #[test]
    fn for_room_scopes_bare_room_name() {
        // Bare room name (Ably's documented form). NOT "sports::$chat".
        let cap = Capability::new().for_room("sports", [Operation::Publish, Operation::History]);
        assert_eq!(
            cap.to_capability_string(),
            r#"{"sports":["history","publish"]}"#
        );
    }

    #[test]
    fn to_json_returns_native_object() {
        let cap = Capability::new().allow("r", [Operation::Publish, Operation::History]);
        let v = cap.to_json();
        assert!(v.is_object());
        assert_eq!(v["r"], serde_json::json!(["history", "publish"]));
    }
}
