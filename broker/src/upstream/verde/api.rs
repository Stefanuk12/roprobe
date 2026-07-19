use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use serde::{Deserialize, Deserializer};
use serde_tuple::Deserialize_tuple;

pub fn bool_as_int<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(u8::deserialize(d)? != 0)
}

/// One property's inspector metadata.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize_tuple)]
pub struct PropMeta {
    pub category: String,
    #[serde(deserialize_with = "bool_as_int")]
    pub read_only: bool,
    pub layout_order: u32,
    pub security_level: u8,
}
impl Default for PropMeta {
    fn default() -> Self {
        Self {
            category: String::from("Other"),
            read_only: false,
            layout_order: 0,
            security_level: 2,
        }
    }
}

#[derive(Deserialize)]
struct ClassEntry {
    /// Superclass name, absent for a root class.
    #[serde(default)]
    #[serde(rename = "s")]
    superclass: Option<String>,
    /// This class's own properties.
    #[serde(rename = "p")]
    properties: HashMap<String, PropMeta>,
}

fn dump() -> &'static HashMap<String, ClassEntry> {
    static DUMP: OnceLock<HashMap<String, ClassEntry>> = OnceLock::new();
    DUMP.get_or_init(|| {
        serde_json::from_str(include_str!("api-meta.json")).expect("parse bundled api dump")
    })
}

/// [`class_properties`] against an explicit write-security threshold.
pub fn class_properties(class: &str, threshold: u8) -> Vec<String> {
    let dump = dump();
    let mut names = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut current = Some(class);

    while let Some(name) = current {
        let Some(entry) = dump.get(name) else { break };
        for (property, meta) in &entry.properties {
            if meta.security_level <= threshold && seen.insert(property.as_str()) {
                names.push(property.clone());
            }
        }
        current = entry.superclass.as_deref();
    }

    names
}

/// A property's `(category, is_read_only, layout_order)` resolved up the class
/// hierarchy, `None` for an unknown class/property (e.g. a hidden or removed one).
pub fn property_meta(class: &str, property: &str) -> Option<PropMeta> {
    let dump = dump();
    let mut current = Some(class);

    while let Some(name) = current {
        let entry = dump.get(name)?;
        if let Some(meta) = entry.properties.get(property) {
            return Some(meta.clone());
        }
        current = entry.superclass.as_deref();
    }

    None
}

#[cfg(test)]
mod tests {
    use crate::SecurityLevel;

    use super::*;

    #[test]
    fn resolves_inherited_properties_and_read_only() {
        // Inherited from Instance, writable.
        assert_eq!(
            property_meta("Part", "Name").map(|meta| (meta.category, meta.read_only)),
            Some((String::from("Data"), false))
        );
        // Inherited from Object, read-only.
        assert_eq!(
            property_meta("Part", "ClassName").map(|meta| meta.read_only),
            Some(true)
        );
        // An own, visible property of BasePart.
        assert!(property_meta("Part", "Transparency").is_some());
        // Position is intentionally absent (Hidden in the dump; verde derives it
        // from CFrame), as are unknown properties / classes.
        assert_eq!(property_meta("Part", "Position"), None);
        assert_eq!(property_meta("Part", "NotARealProperty"), None);
        assert_eq!(property_meta("NotARealClass", "Name"), None);
    }

    #[test]
    fn class_properties_collects_the_visible_hierarchy() {
        let props = class_properties("Part", 2);
        // Own + inherited visible properties are present, deduped.
        for name in ["Anchored", "Transparency", "CFrame", "Name"] {
            assert!(
                props.iter().any(|p| p == name),
                "expected {name} in Part's properties"
            );
        }
        // Hidden/derived ones are excluded.
        assert!(!props.iter().any(|p| p == "Position"));
        // No duplicates.
        let mut sorted = props.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), props.len(), "property list has no duplicates");
        // Unknown class yields nothing.
        assert!(class_properties("NotARealClass", 2).is_empty());
    }

    #[test]
    fn class_properties_filters_by_write_security() {
        // `Player.User` is Roblox-only-write (ordinal 4): hidden at the default
        // level, revealed only when the threshold is raised to it.
        assert!(
            !class_properties("Player", SecurityLevel::default().ordinal())
                .iter()
                .any(|p| p == "User")
        );
        assert!(class_properties("Player", 4).iter().any(|p| p == "User"));
        // Ordinary read-only props stay visible at the default level.
        for name in ["AccountAge", "UserId", "MembershipType"] {
            assert!(
                class_properties("Player", SecurityLevel::default().ordinal())
                    .iter()
                    .any(|p| p == name),
                "expected {name} at the default level"
            );
        }
    }
}
