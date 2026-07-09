use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use squash::ReverseDeserialize;

use super::{DomId, DomValue};

/// How a [`DomUpdate`] touches the stored tag set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum TagChange {
    /// Leave the stored tags alone.
    #[default]
    None,
    /// Replace the stored set wholesale (an empty list clears it).
    Replace(Vec<String>),
    /// Add and remove individual tags, leaving the rest untouched.
    Delta { add: Vec<String>, remove: Vec<String> },
}

/// A lightweight change to an *already-mirrored* instance.
#[derive(Debug, Clone, Default, Serialize, ReverseDeserialize)]
pub struct DomUpdate {
    pub id: DomId,
    pub properties: HashMap<String, Option<DomValue>>,
    pub attributes: HashMap<String, Option<DomValue>>,
    pub tags: TagChange,
}
