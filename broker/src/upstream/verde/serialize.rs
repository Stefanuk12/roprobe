use std::collections::{HashSet, VecDeque};

use rbx_dom_weak::{Instance, types::Ref};

use super::protocol::{Node, Snapshot};
use crate::{protocol::DomId, server::SessionDom};

/// Verde caps a search at this many nodes before flagging the result truncated.
const MAX_SEARCH_RESULTS: usize = 500;

/// Build a node for `instance`, with the given (possibly empty) `children`.
fn make_node(
    dom: &SessionDom,
    id: &DomId,
    instance: &Instance,
    children: Vec<String>,
    has_children: bool,
) -> Node {
    Node {
        id: id.clone(),
        name: instance.name.clone(),
        class_name: instance.class.to_string(),
        parent_id: dom.id_of(instance.parent()).cloned(),
        children,
        has_children: Some(has_children),
        // The eager mirror carries no properties yet, so a script's Enabled /
        // RunContext are unknown; leave them unset.
        disabled: None,
        run_context: None,
    }
}

/// A shallow node: identity + parent + a `hasChildren` flag, no children listed.
pub fn shallow_node(dom: &SessionDom, id: &DomId) -> Option<Node> {
    let instance = dom.get(id)?;
    let has_children = dom.has_children(instance.referent());
    Some(make_node(dom, id, instance, Vec::new(), has_children))
}

/// The roots-only snapshot: the mirrored services, shallow.
pub fn roots(dom: &SessionDom) -> Snapshot {
    let weak = dom.dom();
    let root = weak.root_ref();
    let mut root_ids = Vec::new();
    let mut nodes = Vec::new();

    for &child in weak.get_by_ref(root).expect("mirror root").children() {
        if let Some(id) = dom.id_of(child).cloned() {
            if let Some(node) = shallow_node(dom, &id) {
                root_ids.push(id);
                nodes.push(node);
            }
        }
    }

    Snapshot { root_ids, nodes }
}

/// The whole mirrored tree, deep (every node lists its child ids).
pub fn full(dom: &SessionDom) -> Snapshot {
    let weak = dom.dom();
    let root = weak.root_ref();
    let root_children: Vec<Ref> = weak
        .get_by_ref(root)
        .expect("mirror root")
        .children()
        .to_vec();
    let root_ids: Vec<String> = root_children
        .iter()
        .filter_map(|&r| dom.id_of(r).cloned())
        .collect();

    let mut nodes = Vec::new();
    let mut queue: VecDeque<Ref> = root_children.into_iter().collect();
    while let Some(referent) = queue.pop_front() {
        let Some(instance) = weak.get_by_ref(referent) else {
            continue;
        };
        let Some(id) = dom.id_of(referent).cloned() else {
            continue;
        };

        let child_ids: Vec<String> = instance
            .children()
            .iter()
            .filter_map(|&r| dom.id_of(r).cloned())
            .collect();
        let has_children = !child_ids.is_empty();
        nodes.push(make_node(dom, &id, instance, child_ids, has_children));

        for &child in instance.children() {
            queue.push_back(child);
        }
    }

    Snapshot { root_ids, nodes }
}

/// The shallow nodes of `parent`'s immediate children.
pub fn children_of(dom: &SessionDom, parent: &DomId) -> Vec<Node> {
    let Some(instance) = dom.get(parent) else {
        return Vec::new();
    };

    instance
        .children()
        .iter()
        .filter_map(|&r| dom.id_of(r).cloned())
        .filter_map(|id| shallow_node(dom, &id))
        .collect()
}

/// The lowercased `a.b.c` path of `referent`, services first.
fn lower_path(dom: &SessionDom, referent: Ref) -> String {
    let weak = dom.dom();
    let root = weak.root_ref();
    let mut names = Vec::new();
    let mut current = referent;

    while current != root {
        let Some(instance) = weak.get_by_ref(current) else {
            break;
        };
        names.push(instance.name.to_lowercase());
        current = instance.parent();
    }
    names.reverse();
    names.join(".")
}

/// Whether every token appears in `referent`'s path or its class name.
fn matches(dom: &SessionDom, referent: Ref, tokens: &[String]) -> bool {
    let Some(instance) = dom.dom().get_by_ref(referent) else {
        return false;
    };
    let path = lower_path(dom, referent);
    let class = instance.class.to_string().to_lowercase();

    tokens
        .iter()
        .all(|token| path.contains(token) || class.contains(token))
}

/// Add `referent` and its ancestors (up to the root) as shallow nodes, deduped,
/// returning `false` once the result would exceed [`MAX_SEARCH_RESULTS`].
fn include_with_ancestors(
    dom: &SessionDom,
    referent: Ref,
    nodes: &mut Vec<Node>,
    included: &mut HashSet<DomId>,
    truncated: &mut bool,
) -> bool {
    let weak = dom.dom();
    let root = weak.root_ref();

    // Collect the chain from `referent` up to (excluding) the root.
    let mut chain = Vec::new();
    let mut current = referent;
    while current != root {
        chain.push(current);
        let Some(instance) = weak.get_by_ref(current) else {
            break;
        };
        current = instance.parent();
    }

    // Add ancestors before descendants.
    for &node_ref in chain.iter().rev() {
        let Some(id) = dom.id_of(node_ref).cloned() else {
            continue;
        };
        if included.contains(&id) {
            continue;
        }
        if nodes.len() >= MAX_SEARCH_RESULTS {
            *truncated = true;
            return false;
        }
        if let Some(node) = shallow_node(dom, &id) {
            included.insert(id);
            nodes.push(node);
        }
    }

    true
}

/// Search the mirror for nodes matching every whitespace/`.`-separated token in
/// `query`, returning the matches plus their ancestors and whether it truncated.
pub fn search(dom: &SessionDom, query: &str) -> (Vec<Node>, bool) {
    let tokens: Vec<String> = query
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '.')
        .filter(|token| !token.is_empty())
        .map(String::from)
        .collect();
    if tokens.is_empty() {
        return (Vec::new(), false);
    }

    let weak = dom.dom();
    let root = weak.root_ref();
    let mut nodes = Vec::new();
    let mut included = HashSet::new();
    let mut truncated = false;

    for descendant in weak.descendants_of(root) {
        let referent = descendant.referent();
        if referent == root {
            continue;
        }
        if !matches(dom, referent, &tokens) {
            continue;
        }
        if !include_with_ancestors(dom, referent, &mut nodes, &mut included, &mut truncated) {
            break;
        }
    }

    (nodes, truncated)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::protocol::{DomInstance, DomPatch};

    fn instance(id: &str, parent: Option<&str>, class: &str, name: &str) -> DomInstance {
        DomInstance {
            id: id.into(),
            parent: parent.map(Into::into),
            class: class.into(),
            name: name.into(),
            has_children: false,
            properties: HashMap::new(),
            attributes: HashMap::new(),
            tags: None,
        }
    }

    fn seeded() -> SessionDom {
        let mut dom = SessionDom::new();
        dom.apply(DomPatch {
            upserts: vec![
                instance("ws", None, "Workspace", "Workspace"),
                instance("part", Some("ws"), "Part", "Brick"),
                instance("weld", Some("part"), "Weld", "Weld"),
            ],
            removals: vec![],
            updates: vec![],
        });
        dom
    }

    #[test]
    fn roots_are_shallow_services_with_a_null_parent() {
        let snapshot = roots(&seeded());
        assert_eq!(snapshot.root_ids, vec!["ws".to_string()]);
        assert_eq!(snapshot.nodes.len(), 1);

        let node = &snapshot.nodes[0];
        assert_eq!(node.class_name, "Workspace");
        assert_eq!(node.parent_id, None);
        assert_eq!(node.has_children, Some(true));
        assert!(node.children.is_empty(), "roots are shallow");
    }

    #[test]
    fn children_of_lists_shallow_children_with_their_parent() {
        let children = children_of(&seeded(), &"ws".to_string());
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, "part");
        assert_eq!(children[0].parent_id, Some("ws".to_string()));
        assert_eq!(children[0].has_children, Some(true));
        assert!(children[0].children.is_empty());
    }

    #[test]
    fn full_lists_every_node_with_its_child_ids() {
        let snapshot = full(&seeded());
        assert_eq!(snapshot.root_ids, vec!["ws".to_string()]);
        assert_eq!(snapshot.nodes.len(), 3);

        let ws = snapshot.nodes.iter().find(|node| node.id == "ws").unwrap();
        assert_eq!(ws.children, vec!["part".to_string()]);
        let leaf = snapshot
            .nodes
            .iter()
            .find(|node| node.id == "weld")
            .unwrap();
        assert!(leaf.children.is_empty());
        assert_eq!(leaf.has_children, Some(false));
    }

    #[test]
    fn search_matches_by_name_and_grafts_ancestors() {
        let mut dom = SessionDom::new();
        dom.apply(DomPatch {
            upserts: vec![
                instance("ws", None, "Workspace", "Workspace"),
                instance("brick", Some("ws"), "Part", "Brick"),
                instance("cam", Some("ws"), "Camera", "Camera"),
            ],
            removals: vec![],
            updates: vec![],
        });

        let (nodes, truncated) = search(&dom, "brick");
        assert!(!truncated);
        let ids: Vec<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
        assert!(ids.contains(&"brick"), "the match is included");
        assert!(ids.contains(&"ws"), "its ancestor is grafted");
        assert!(!ids.contains(&"cam"), "an unrelated sibling is not matched");
    }

    #[test]
    fn a_lazily_mirrored_root_advertises_children_via_the_flag() {
        // Only the service is mirrored (lazy) — its children aren't — but the
        // client flagged it as having some.
        let mut dom = SessionDom::new();
        let mut workspace = instance("ws", None, "Workspace", "Workspace");
        workspace.has_children = true;
        dom.apply(DomPatch {
            upserts: vec![workspace],
            removals: vec![],
            updates: vec![],
        });

        let snapshot = roots(&dom);
        assert_eq!(snapshot.nodes.len(), 1);
        assert_eq!(
            snapshot.nodes[0].has_children,
            Some(true),
            "expandable despite no mirrored children"
        );
        assert!(snapshot.nodes[0].children.is_empty());
    }

    #[test]
    fn search_matches_by_class_name_too() {
        let (nodes, _) = search(&seeded(), "weld");
        let ids: Vec<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
        // "weld" matches both the Weld instance's name and class.
        assert!(ids.contains(&"weld"));
        assert!(
            ids.contains(&"part") && ids.contains(&"ws"),
            "ancestors grafted"
        );
    }
}
