use std::collections::HashMap;

use rbx_dom_weak::{
    Instance, InstanceBuilder, WeakDom,
    types::{Attributes, Ref, Variant},
    ustr,
};
use tracing::debug;

use crate::protocol::{DomId, DomPatch, DomValue, TagChange};

type ValueChanges = std::collections::HashMap<String, Option<DomValue>>;

/// A lazily populated mirror of the client's DataModel.
pub struct SessionDom {
    dom: WeakDom,
    refs: HashMap<DomId, Ref>,
    ids: HashMap<Ref, DomId>,
    pending: HashMap<Ref, Ref>,
}

impl SessionDom {
    /// Create an empty mirror.
    pub fn new() -> Self {
        Self {
            dom: WeakDom::new(InstanceBuilder::new("DataModel")),
            refs: HashMap::new(),
            ids: HashMap::new(),
            pending: HashMap::new(),
        }
    }

    /// The mirrored DOM.
    pub fn dom(&self) -> &WeakDom {
        &self.dom
    }

    /// Look up a mirrored instance by its client id.
    pub fn get(&self, id: &str) -> Option<&Instance> {
        self.refs.get(id).and_then(|referent| self.dom.get_by_ref(*referent))
    }

    /// The client id a referent belongs to (the root has none).
    pub fn id_of(&self, referent: Ref) -> Option<&DomId> {
        self.ids.get(&referent)
    }

    /// How many instances are mirrored, excluding the synthetic root.
    pub fn len(&self) -> usize {
        self.refs.len()
    }

    /// Whether nothing has been mirrored yet.
    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }

    /// Merge a patch into the mirror.
    pub fn apply(&mut self, patch: DomPatch) {
        let root = self.dom.root_ref();
        let mut moves = Vec::new();
        let mut props = Vec::new();

        // Add or refresh any instances.
        for instance in patch.upserts {
            match self.refs.get(&instance.id) {
                // We have already seen this instance
                Some(&referent) => {
                    // Update the WeakDom about the instance
                    let existing = self.dom.get_by_ref_mut(referent).expect("mapped referent");
                    existing.name = instance.name;
                    existing.class = ustr(&instance.class);

                    // Store the instance data for later
                    props.push((referent, instance.properties, instance.attributes, instance.tags));
                    moves.push((referent, instance.parent));
                }
                // A new instance
                None => {
                    // Create the instance within the WeakDom to get the ref
                    let builder = InstanceBuilder::new(ustr(&instance.class)).with_name(instance.name);
                    let parent = instance.parent.as_ref().and_then(|id| self.refs.get(id)).copied();
                    let referent = self.dom.insert(parent.unwrap_or(root), builder);

                    // Store the mapping
                    self.refs.insert(instance.id.clone(), referent);
                    self.ids.insert(referent, instance.id);

                    // Store the instance data for later
                    props.push((referent, instance.properties, instance.attributes, instance.tags));
                    if parent.is_none() && instance.parent.is_some() {
                        moves.push((referent, instance.parent));
                    }
                }
            }
        }

        // All refs in the props should exist due to above.
        for (referent, properties, attributes, tags) in props {
            // Resolve all of the instance data
            let properties = properties.into_iter().map(|(key, value)| (key, Some(value))).collect();
            let attributes = attributes.into_iter().map(|(key, value)| (key, Some(value))).collect();
            let tags = match tags {
                Some(tags) => TagChange::Replace(tags),
                None => TagChange::None,
            };

            // Merge the instance data with any existing data about the instance within our dom
            Self::merge_values(&mut self.dom, &self.refs, referent, properties, attributes, tags);
        }

        // Resovle all deltas
        for update in patch.updates {
            let Some(&referent) = self.refs.get(&update.id) else {
                debug!(id = %update.id, "update targets an unmirrored instance, dropping");
                continue;
            };
            Self::merge_values(&mut self.dom, &self.refs, referent, update.properties, update.attributes, update.tags);
        }

        // Queue any reparenting
        for (referent, parent) in moves {
            // Resolve the new parent within the WeakDom
            let parent = match parent {
                Some(id) => match self.refs.get(&id) {
                    Some(&parent) => parent,
                    None => {
                        debug!(%id, "upsert parent not mirrored, attaching to root");
                        root
                    }
                },
                None => root,
            };

            // Queue for a reparent
            self.pending.insert(referent, parent);
        }
        self.reparent();

        // Handle any instance removals
        for id in &patch.removals {
            // Make sure it's not already removed
            let Some(&referent) = self.refs.get(id) else {
                continue;
            };

            // Remove the ref and all descendants
            let Self { dom, refs, ids, .. } = &mut *self;
            for instance in dom.descendants_of(referent) {
                if let Some(gone_id) = ids.remove(&instance.referent()) {
                    refs.remove(&gone_id);
                }
            }
            dom.destroy(referent);
        }
    }

    /// Merge property, attribute and tag changes into one mirrored instance.
    fn merge_values(
        dom: &mut WeakDom,
        refs: &HashMap<DomId, Ref>,
        referent: Ref,
        properties: ValueChanges,
        attributes: ValueChanges,
        tags: TagChange,
    ) {
        let existing = dom.get_by_ref_mut(referent).expect("mirrored referent");

        // Go through each property and update it by removing the current value and adding the new value back
        for (key, value) in properties {
            let Some(value) = value else {
                existing.properties.remove(&ustr(&key));
                continue;
            };

            if let DomValue::Ref(id) = &value {
                if !refs.contains_key(id) {
                    debug!(%id, "property references an unmirrored instance, storing a null ref");
                }
            }

            let variant = value.into_variant(|id| refs.get(id).copied());
            existing.properties.insert(ustr(&key), variant);
        }

        if !attributes.is_empty() {
            // Make sure the attributes store exists
            let mut merged = match existing.properties.get(&ustr("Attributes")) {
                Some(Variant::Attributes(attrs)) => attrs.clone(),
                _ => Attributes::new(),
            };

            // Go through each attribute and add/remove it inside
            // NOTE: inserting overwrites any existing
            for (key, value) in attributes {
                match value {
                    Some(value) if value.is_attribute_safe() => {
                        merged.insert(key, value.into_variant(|_| None));
                    }
                    Some(_) => debug!(%key, "not a legal Roblox attribute type, skipping"),
                    None => {
                        merged.remove(key.as_str());
                    }
                }
            }

            // Update the attribute store
            existing.properties.insert(ustr("Attributes"), Variant::Attributes(merged));
        }

        match tags {
            TagChange::None => {}
            TagChange::Replace(tags) => {
                existing.properties.insert(ustr("Tags"), Variant::Tags(tags.into()));
            }
            TagChange::Delta { add, remove } => {
                // Make sure the tags store exists
                let mut merged: Vec<String> = match existing.properties.get(&ustr("Tags")) {
                    Some(Variant::Tags(tags)) => tags.iter().map(str::to_string).collect(),
                    _ => Vec::new(),
                };

                merged.retain(|tag| !remove.contains(tag));
                
                for tag in add {
                    if !merged.contains(&tag) {
                        merged.push(tag);
                    }
                }

                // Update the tags store
                existing.properties.insert(ustr("Tags"), Variant::Tags(merged.into()));
            }
        }
    }

    /// Apply any queued reparent repeatedly until no changes are made.
    fn reparent(&mut self) {
        loop {
            let mut progressed = false;
            let dom = &mut self.dom;

            self.pending.retain(|&referent, &mut parent| {
                // Either side may have been removed since the request arrived.
                if dom.get_by_ref(parent).is_none() {
                    return false;
                }

                let Some(instance) = dom.get_by_ref(referent) else {
                    return false;
                };

                if instance.parent() == parent {
                    progressed = true;
                    return false;
                }

                if parent == referent {
                    debug!(?referent, "upsert requested itself as parent, ignoring");
                    return false;
                }

                if dom.ancestors_of(parent).any(|ancestor| ancestor.referent() == referent) {
                    debug!(?referent, "upsert parent is inside its own subtree, deferring");
                    return true;
                }
                
                dom.transfer_within(referent, parent);
                progressed = true;
                false
            });

            if !progressed {
                break;
            }
        }
    }
}

impl Default for SessionDom {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rbx_dom_weak::types::Variant;

    use super::*;
    use crate::protocol::{DomInstance, DomUpdate, DomValue, TagChange};

    fn node(id: &str, parent: Option<&str>, class: &str, name: &str) -> DomInstance {
        DomInstance {
            id: id.into(),
            parent: parent.map(Into::into),
            class: class.into(),
            name: name.into(),
            properties: HashMap::new(),
            attributes: HashMap::new(),
            tags: None,
        }
    }

    #[test]
    fn upserts_build_a_tree_regardless_of_order_within_a_patch() {
        let mut dom = SessionDom::new();
        dom.apply(DomPatch {
            // Child listed before its parent on purpose.
            upserts: vec![
                node("child", Some("parent"), "Part", "Child"),
                node("parent", None, "Folder", "Parent"),
            ],
            removals: vec![],
            updates: vec![],
        });

        let parent = dom.get("parent").unwrap();
        let child = dom.get("child").unwrap();
        assert_eq!(parent.parent(), dom.dom().root_ref());
        assert_eq!(child.parent(), parent.referent());
        assert_eq!(child.class, ustr("Part"));
        assert_eq!(dom.len(), 2);
    }

    #[test]
    fn upserting_an_existing_id_refreshes_it_in_place() {
        let mut dom = SessionDom::new();
        let mut with_props = node("a", None, "Part", "Before");
        with_props.properties.insert("Anchored".into(), DomValue::Bool(false));
        dom.apply(DomPatch { upserts: vec![with_props], removals: vec![], updates: vec![] });

        let mut updated = node("a", None, "Part", "After");
        updated.properties.insert("Anchored".into(), DomValue::Bool(true));
        dom.apply(DomPatch { upserts: vec![updated], removals: vec![], updates: vec![] });

        let instance = dom.get("a").unwrap();
        assert_eq!(instance.name, "After");
        assert_eq!(instance.properties.get(&ustr("Anchored")), Some(&Variant::Bool(true)));
        assert_eq!(dom.len(), 1);
    }

    #[test]
    fn later_patches_can_reparent_and_remove_subtrees() {
        let mut dom = SessionDom::new();
        dom.apply(DomPatch {
            upserts: vec![
                node("a", None, "Folder", "A"),
                node("b", None, "Folder", "B"),
                node("leaf", Some("a"), "Part", "Leaf"),
            ],
            removals: vec![],
            updates: vec![],
        });

        // Move the leaf from A to B.
        dom.apply(DomPatch {
            upserts: vec![node("leaf", Some("b"), "Part", "Leaf")],
            removals: vec![],
            updates: vec![],
        });
        let b_ref = dom.get("b").unwrap().referent();
        assert_eq!(dom.get("leaf").unwrap().parent(), b_ref);

        // Removing B takes the leaf's mapping with it.
        dom.apply(DomPatch { upserts: vec![], removals: vec!["b".into()], updates: vec![] });
        assert!(dom.get("b").is_none());
        assert!(dom.get("leaf").is_none());
        assert_eq!(dom.len(), 1);
    }

    #[test]
    fn ancestry_swaps_settle_regardless_of_order_within_a_patch() {
        let mut dom = SessionDom::new();
        dom.apply(DomPatch {
            upserts: vec![node("a", None, "Folder", "A"), node("b", Some("a"), "Folder", "B")],
            removals: vec![],
            updates: vec![],
        });

        // Invert the pair: B to the root, A under B — with A listed first,
        // whose move is impossible until B's has been applied.
        dom.apply(DomPatch {
            upserts: vec![node("a", Some("b"), "Folder", "A"), node("b", None, "Folder", "B")],
            removals: vec![],
            updates: vec![],
        });
        let b_ref = dom.get("b").unwrap().referent();
        assert_eq!(dom.get("b").unwrap().parent(), dom.dom().root_ref());
        assert_eq!(dom.get("a").unwrap().parent(), b_ref);
    }

    #[test]
    fn ancestry_swaps_settle_across_patches() {
        let mut dom = SessionDom::new();
        dom.apply(DomPatch {
            upserts: vec![node("a", None, "Folder", "A"), node("b", Some("a"), "Folder", "B")],
            removals: vec![],
            updates: vec![],
        });

        // The swap arrives one half at a time: A's move stays pending while
        // B is still inside A's subtree...
        dom.apply(DomPatch { upserts: vec![node("a", Some("b"), "Folder", "A")], removals: vec![], updates: vec![] });
        assert_eq!(dom.get("a").unwrap().parent(), dom.dom().root_ref());

        // ...and settles once B's half lands.
        dom.apply(DomPatch { upserts: vec![node("b", None, "Folder", "B")], removals: vec![], updates: vec![] });
        let b_ref = dom.get("b").unwrap().referent();
        assert_eq!(dom.get("a").unwrap().parent(), b_ref);
    }

    #[test]
    fn object_properties_resolve_to_mirrored_referents() {
        let mut dom = SessionDom::new();

        // Forward reference: the holder lists its target before the target's
        // own upsert appears in the patch.
        let mut holder = node("holder", None, "ObjectValue", "Holder");
        holder.properties.insert("Value".into(), DomValue::Ref("target".into()));
        dom.apply(DomPatch {
            upserts: vec![holder, node("target", None, "Part", "Target")],
            removals: vec![],
            updates: vec![],
        });
        let target_ref = dom.get("target").unwrap().referent();
        assert_eq!(
            dom.get("holder").unwrap().properties.get(&ustr("Value")),
            Some(&Variant::Ref(target_ref))
        );

        // A target the mirror has never seen stores as the null referent.
        let mut dangling = node("dangling", None, "ObjectValue", "Dangling");
        dangling.properties.insert("Value".into(), DomValue::Ref("never-sent".into()));
        dom.apply(DomPatch { upserts: vec![dangling], removals: vec![], updates: vec![] });
        assert_eq!(
            dom.get("dangling").unwrap().properties.get(&ustr("Value")),
            Some(&Variant::Ref(rbx_dom_weak::types::Ref::none()))
        );
    }

    #[test]
    fn attributes_merge_and_tags_replace() {
        let mut dom = SessionDom::new();
        let mut enemy = node("e", None, "Model", "Enemy");
        enemy.attributes.insert("Health".into(), DomValue::Float(100.0));
        enemy.attributes.insert("Boss".into(), DomValue::Bool(false));
        enemy.tags = Some(vec!["Enemy".into(), "Spawned".into()]);
        dom.apply(DomPatch { upserts: vec![enemy], removals: vec![], updates: vec![] });

        let stored = |dom: &SessionDom, key: &str| dom.get("e").unwrap().properties.get(&ustr(key)).cloned();
        assert_eq!(
            stored(&dom, "Attributes"),
            Some(Variant::Attributes(
                Attributes::new().with("Health", 100.0f64).with("Boss", false)
            ))
        );
        assert_eq!(
            stored(&dom, "Tags"),
            Some(Variant::Tags(vec!["Enemy".to_string(), "Spawned".to_string()].into()))
        );

        // Attributes merge key-by-key; `tags: None` leaves tags untouched.
        let mut update = node("e", None, "Model", "Enemy");
        update.attributes.insert("Health".into(), DomValue::Float(25.0));
        // Object references are invalid as attribute values and are skipped.
        update.attributes.insert("Owner".into(), DomValue::Ref("e".into()));
        dom.apply(DomPatch { upserts: vec![update], removals: vec![], updates: vec![] });
        assert_eq!(
            stored(&dom, "Attributes"),
            Some(Variant::Attributes(
                Attributes::new().with("Health", 25.0f64).with("Boss", false)
            ))
        );
        assert_eq!(
            stored(&dom, "Tags"),
            Some(Variant::Tags(vec!["Enemy".to_string(), "Spawned".to_string()].into()))
        );

        // `Some(tags)` replaces wholesale — including clearing with an empty list.
        let mut retag = node("e", None, "Model", "Enemy");
        retag.tags = Some(vec![]);
        dom.apply(DomPatch { upserts: vec![retag], removals: vec![], updates: vec![] });
        assert_eq!(stored(&dom, "Tags"), Some(Variant::Tags(Vec::<String>::new().into())));
    }

    #[test]
    fn updates_change_values_without_touching_identity() {
        let mut dom = SessionDom::new();
        let mut enemy = node("e", None, "Model", "Enemy");
        enemy.properties.insert("Anchored".into(), DomValue::Bool(false));
        enemy.attributes.insert("Health".into(), DomValue::Float(100.0));
        enemy.attributes.insert("Boss".into(), DomValue::Bool(true));
        enemy.tags = Some(vec!["Enemy".into()]);
        dom.apply(DomPatch { upserts: vec![enemy], removals: vec![], updates: vec![] });

        // Change a property, overwrite one attribute, *remove* another and
        // replace the tags — without re-sending name/class/parent.
        dom.apply(DomPatch {
            upserts: vec![],
            removals: vec![],
            updates: vec![DomUpdate {
                id: "e".into(),
                properties: HashMap::from([("Anchored".to_string(), Some(DomValue::Bool(true)))]),
                attributes: HashMap::from([
                    ("Health".to_string(), Some(DomValue::Float(25.0))),
                    ("Boss".to_string(), None),
                ]),
                tags: TagChange::Replace(vec!["Dead".into(), "Enemy".into()]),
            }],
        });

        let instance = dom.get("e").unwrap();
        assert_eq!(instance.name, "Enemy");
        assert_eq!(instance.class, ustr("Model"));
        assert_eq!(instance.properties.get(&ustr("Anchored")), Some(&Variant::Bool(true)));
        assert_eq!(
            instance.properties.get(&ustr("Attributes")),
            Some(&Variant::Attributes(Attributes::new().with("Health", 25.0f64)))
        );

        // A `None` property forgets the stored value; a tag delta touches
        // only the listed tags (removing absent / adding present are no-ops).
        dom.apply(DomPatch {
            upserts: vec![],
            removals: vec![],
            updates: vec![DomUpdate {
                id: "e".into(),
                properties: HashMap::from([("Anchored".to_string(), None)]),
                attributes: HashMap::new(),
                tags: TagChange::Delta {
                    add: vec!["Boss".into(), "Enemy".into()],
                    remove: vec!["Dead".into(), "never-tagged".into()],
                },
            }],
        });
        let instance = dom.get("e").unwrap();
        assert_eq!(instance.properties.get(&ustr("Anchored")), None);
        assert_eq!(
            instance.properties.get(&ustr("Tags")),
            Some(&Variant::Tags(vec!["Enemy".to_string(), "Boss".to_string()].into()))
        );

        // An update in the same patch as the upsert it targets applies, and
        // one aimed at an unmirrored id is dropped.
        let mut ally = node("a", None, "Model", "Ally");
        ally.properties.insert("Anchored".into(), DomValue::Bool(false));
        dom.apply(DomPatch {
            upserts: vec![ally],
            removals: vec![],
            updates: vec![
                DomUpdate {
                    id: "a".into(),
                    properties: HashMap::from([("Anchored".to_string(), Some(DomValue::Bool(true)))]),
                    ..Default::default()
                },
                DomUpdate { id: "never-sent".into(), ..Default::default() },
            ],
        });
        assert_eq!(dom.get("a").unwrap().properties.get(&ustr("Anchored")), Some(&Variant::Bool(true)));
        assert_eq!(dom.len(), 2);
    }

    #[test]
    fn unknown_parents_and_cycles_fall_back_safely() {
        let mut dom = SessionDom::new();
        dom.apply(DomPatch {
            upserts: vec![node("orphan", Some("never-sent"), "Part", "Orphan")],
            removals: vec![],
            updates: vec![],
        });
        assert_eq!(dom.get("orphan").unwrap().parent(), dom.dom().root_ref());

        dom.apply(DomPatch {
            upserts: vec![
                node("outer", None, "Folder", "Outer"),
                node("inner", Some("outer"), "Folder", "Inner"),
            ],
            removals: vec![],
            updates: vec![],
        });
        // Parenting `outer` under its own child must not detach the subtree.
        dom.apply(DomPatch {
            upserts: vec![node("outer", Some("inner"), "Folder", "Outer")],
            removals: vec![],
            updates: vec![],
        });
        assert_eq!(dom.get("outer").unwrap().parent(), dom.dom().root_ref());

        // Removing an id that was never mirrored is a no-op.
        dom.apply(DomPatch { upserts: vec![], removals: vec!["never-sent".into()], updates: vec![] });
        assert_eq!(dom.len(), 3);
    }
}
