use serde::Serialize;
use squash::ReverseDeserialize;

use super::{DomId, DomInstance, DomUpdate};

/// Represents a lazy, incremental update to a DOM.
#[derive(Debug, Clone, Default, Serialize, ReverseDeserialize)]
pub struct DomPatch {
    /// Add a new instance, or refresh it.
    ///
    /// NOTE: the parent of the instance must exist within the DOM prior to adding it.
    pub upserts: Vec<DomInstance>,
    /// The ref ids of nodes to remove.
    pub removals: Vec<DomId>,
    /// Property/attribute/tag-only changes to already-mirrored instances.
    pub updates: Vec<DomUpdate>,
}
