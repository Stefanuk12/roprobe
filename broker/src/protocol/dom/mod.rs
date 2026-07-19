import!(bytes, instance, patch, update, variant);

/// The client's identity for an instance (e.g. `Instance:GetDebugId()`), opaque to the broker and only required to be stable for a session's lifetime.
pub type DomId = String;
