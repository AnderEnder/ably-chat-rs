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
}
