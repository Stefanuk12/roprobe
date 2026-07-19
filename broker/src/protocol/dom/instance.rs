use std::collections::HashMap;

use serde::Serialize;
use squash::ReverseDeserialize;

use super::{DomId, DomValue};

#[derive(Debug, Clone, Serialize, ReverseDeserialize)]
pub struct DomInstance {
    pub id: DomId,
    pub parent: Option<DomId>,
    pub class: String,
    pub name: String,
    /// Whether the *live* instance has any children (independent of whether they are mirrored yet), letting a tree viewer show an expand arrow and trigger a lazy `RequestChildren` for an unloaded node.
    pub has_children: bool,
    pub properties: HashMap<String, DomValue>,
    pub attributes: HashMap<String, DomValue>,
    pub tags: Option<Vec<String>>,
}
