import!(bytes, instance, patch, update, variant);

/// The client's identity for an instance (e.g. `Instance:GetDebugId()`).
/// Opaque to the broker; it only has to be stable for a session's lifetime.
pub type DomId = String;
