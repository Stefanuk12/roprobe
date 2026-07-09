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
    pub properties: HashMap<String, DomValue>,
    pub attributes: HashMap<String, DomValue>,
    pub tags: Option<Vec<String>>,
}
