use std::{collections::HashSet, sync::Arc};

use crate::{
    protocol::DomId,
    server::{DomChange, Mirror, SessionDom},
    upstream::verde::{
        protocol::{self, DeltaOp},
        serialize,
    },
};

/// Tracks the Verde currently mirrors.
///
/// - `known`: the nodes Verde has
/// - `loaded`: the nodes that have had their children already sent
#[derive(Default)]
pub struct TreeState {
    pub known: HashSet<DomId>,
    pub loaded: HashSet<DomId>,
    pub delta_buf: Vec<DeltaOp>,
    pub added_roots: Vec<String>,
}

impl TreeState {
    /// Populate `delta_buf` based upon a singular [`DomChange`], filtering out what Verde already has/expanded.
    pub fn translate(&mut self, mirror: &Arc<Mirror>, change: &DomChange) {
        mirror.with_dom(|dom| {
            for id in &change.added {
                // Instance already within the mirror
                let Some(instance) = dom.get(id) else {
                    continue;
                };

                // Make sure the parent is known by Verde and us
                match dom.id_of(instance.parent()).cloned() {
                    // A new root (service): verde always wants these.
                    None => {
                        if let Some(node) = serialize::shallow_node(dom, id) {
                            self.delta_buf
                                .push(DeltaOp::add_subtree(None, id.clone(), vec![node]));
                            self.added_roots.push(id.clone());
                            self.known.insert(id.clone());
                        }
                    }

                    // The parent is known by Verde, add it in.
                    Some(parent) if self.known.contains(&parent) => {
                        if let Some(node) = serialize::shallow_node(dom, id) {
                            self.delta_buf.push(DeltaOp::add_subtree(
                                Some(parent.clone()),
                                id.clone(),
                                vec![node],
                            ));
                            self.known.insert(id.clone());
                            self.loaded.insert(parent);
                        }
                    }

                    // Parent unknown to verde: it has no place for this yet.
                    Some(_) => {}
                }
            }

            for id in &change.removed {
                if self.known.remove(id) {
                    self.delta_buf.push(DeltaOp::remove_node(id.clone()));
                }
            }

            for id in &change.moved {
                if !self.known.contains(id) {
                    continue;
                }

                let new_parent = dom
                    .get(id)
                    .and_then(|instance| dom.id_of(instance.parent()).cloned());
                self.delta_buf
                    .push(DeltaOp::move_node(id.clone(), new_parent));
            }

            for id in &change.renamed {
                if !self.known.contains(id) {
                    continue;
                }

                if let Some(instance) = dom.get(id) {
                    self.delta_buf
                        .push(DeltaOp::rename(id.clone(), instance.name.clone()));
                }
            }

            for id in &change.children_changed {
                if !self.known.contains(id) {
                    continue;
                }

                if let Some(instance) = dom.get(id) {
                    self.delta_buf.push(DeltaOp::has_children(
                        id.clone(),
                        dom.has_children(instance.referent()),
                    ));
                }
            }
        });
    }

    /// Make sure that every ancestor associated with `ref_id` is known to Verde,
    /// so when the instance is clicked on, it properly resolves the path.
    pub fn reveal_path(&mut self, dom: &SessionDom, ref_id: &DomId) {
        // Walk up from the ref, collecting the not-yet-known chain.
        let mut chain = Vec::new();
        let mut current = ref_id.clone();
        loop {
            // Everything above this is known
            if self.known.contains(&current) {
                break;
            }

            // Make sure the current node is actually mirrored, to start
            let Some(instance) = dom.get(&current) else {
                return;
            };

            // Add to the chain, and set the new current parent node
            chain.push(current.clone());
            match dom.id_of(instance.parent()).cloned() {
                Some(parent) => current = parent,
                None => break, // reached a root (Parent is `nil`)
            }
        }

        // Add ancestors before descendants so each node's parent is already placed.
        for id in chain.iter().rev() {
            let Some(node) = serialize::shallow_node(dom, id) else {
                continue;
            };

            // Schedule the delta for Verde
            let parent = dom
                .get(id)
                .and_then(|instance| dom.id_of(instance.parent()).cloned());
            self.delta_buf
                .push(DeltaOp::add_subtree(parent.clone(), id.clone(), vec![node]));
            self.known.insert(id.clone());

            match parent {
                Some(parent) => {
                    self.loaded.insert(parent);
                }
                None => self.added_roots.push(id.clone()),
            }
        }
    }

    /// Re-initialise the current state based upon a snapshot.
    ///
    /// Sets `loaded` if the snapshot is a full one.
    pub fn seed(&mut self, snapshot: &protocol::Snapshot, full: bool) {
        self.delta_buf.clear();
        self.added_roots.clear();

        self.known = snapshot.nodes.iter().map(|node| node.id.clone()).collect();
        if full {
            self.loaded = self.known.clone();
        } else {
            self.loaded.clear();
        }
    }
}
