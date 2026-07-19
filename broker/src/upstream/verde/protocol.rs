use std::{sync::OnceLock, time::Instant};

use serde::{Deserialize, Serialize};

/// Monotonic seconds since the first call - verde only needs increasing values.
fn timestamp() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// One instance in verde's flat tree representation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub name: String,
    pub class_name: String,
    /// `None` for the roots (services); serialized as `null`, never omitted.
    pub parent_id: Option<String>,
    /// Always serialized (`[]` for a shallow node).
    pub children: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_children: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_context: Option<String>,
}

/// A whole (or roots-only) tree snapshot.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub root_ids: Vec<String>,
    pub nodes: Vec<Node>,
}

/// An incremental tree edit within an `explorer_delta`, where serde's enum-level
/// `rename_all` only renames variants so each variant camelCases its own fields.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum DeltaOp {
    #[serde(rename = "add_subtree", rename_all = "camelCase")]
    AddSubtree {
        timestamp: f64,
        /// Omitted for a new root (matching the plugin, whose `nil` field drops
        /// from the JSON), present for a child attach.
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        root_id: String,
        /// Set only for a new root (`parent_id` is `None`); verde derives the
        /// delta's `addedRootIds` from these and needs it to register the root.
        #[serde(skip_serializing_if = "Option::is_none")]
        added_root_id: Option<String>,
        nodes: Vec<Node>,
    },
    #[serde(rename = "remove_node")]
    RemoveNode { timestamp: f64, id: String },
    #[serde(rename = "update_node", rename_all = "camelCase")]
    UpdateNode {
        timestamp: f64,
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        has_children: Option<bool>,
    },
    #[serde(rename = "move_node", rename_all = "camelCase")]
    MoveNode {
        timestamp: f64,
        id: String,
        new_parent_id: Option<String>,
    },
}

impl DeltaOp {
    pub fn add_subtree(parent_id: Option<String>, root_id: String, nodes: Vec<Node>) -> Self {
        // A parentless subtree is a new root; verde keys on a per-op `addedRootId`.
        let added_root_id = parent_id.is_none().then(|| root_id.clone());
        Self::AddSubtree {
            timestamp: timestamp(),
            parent_id,
            root_id,
            added_root_id,
            nodes,
        }
    }

    pub fn remove_node(id: String) -> Self {
        Self::RemoveNode {
            timestamp: timestamp(),
            id,
        }
    }

    pub fn rename(id: String, name: String) -> Self {
        Self::UpdateNode {
            timestamp: timestamp(),
            id,
            name: Some(name),
            has_children: None,
        }
    }

    pub fn has_children(id: String, has_children: bool) -> Self {
        Self::UpdateNode {
            timestamp: timestamp(),
            id,
            name: None,
            has_children: Some(has_children),
        }
    }

    pub fn move_node(id: String, new_parent_id: Option<String>) -> Self {
        Self::MoveNode {
            timestamp: timestamp(),
            id,
            new_parent_id,
        }
    }
}

/// A relayed operation's outcome, echoed back to verde inside `operation_result`.
#[derive(Debug, Clone, Serialize)]
pub struct OperationOutcome {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Messages verde's extension sends to a plugin.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Inbound {
    #[serde(rename = "ack", rename_all = "camelCase")]
    Ack {
        #[serde(default)]
        request_id: Option<String>,
    },
    #[serde(rename = "request_snapshot")]
    RequestSnapshot {
        #[serde(default)]
        full: bool,
    },
    #[serde(rename = "request_children", rename_all = "camelCase")]
    RequestChildren {
        #[serde(default)]
        parent_ids: Vec<String>,
    },
    #[serde(rename = "request_search")]
    RequestSearch {
        #[serde(default)]
        query: String,
    },
    #[serde(rename = "release_subtree", rename_all = "camelCase")]
    ReleaseSubtree {
        #[serde(default)]
        parent_ids: Vec<String>,
        #[serde(default)]
        node_ids: Vec<String>,
    },
    #[serde(rename = "operation", rename_all = "camelCase")]
    Operation {
        operation_id: String,
        operation: serde_json::Value,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: String,
    },
    /// Any tag we don't recognise (a newer extension).
    #[serde(other)]
    Unknown,
}

/// Messages a plugin sends to verde's extension.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum Outbound {
    #[serde(rename = "ack", rename_all = "camelCase")]
    Ack {
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "explorer_snapshot", rename_all = "camelCase")]
    ExplorerSnapshot { payload: Snapshot, is_full: bool },
    #[serde(rename = "explorer_delta", rename_all = "camelCase")]
    ExplorerDelta {
        ops: Vec<DeltaOp>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        added_root_ids: Vec<String>,
    },
    #[serde(rename = "search_result")]
    SearchResult {
        query: String,
        nodes: Vec<Node>,
        truncated: bool,
        partial: bool,
    },
    #[serde(rename = "operation_result", rename_all = "camelCase")]
    OperationResult {
        operation_id: String,
        result: OperationOutcome,
    },
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn node() -> Node {
        Node {
            id: "a".into(),
            name: "Workspace".into(),
            class_name: "Workspace".into(),
            parent_id: None,
            children: vec![],
            has_children: Some(true),
            disabled: None,
            run_context: None,
        }
    }

    #[test]
    fn snapshot_uses_verde_field_names_and_null_root_parents() {
        let out = Outbound::ExplorerSnapshot {
            payload: Snapshot {
                root_ids: vec!["a".into()],
                nodes: vec![node()],
            },
            is_full: false,
        };
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["type"], "explorer_snapshot");
        assert_eq!(json["isFull"], false);
        assert_eq!(json["payload"]["rootIds"][0], "a");

        let node = &json["payload"]["nodes"][0];
        assert_eq!(node["className"], "Workspace");
        assert_eq!(
            node["parentId"],
            Value::Null,
            "roots serialize a null parentId"
        );
        assert_eq!(node["hasChildren"], true);
        assert_eq!(node["children"], json!([]), "children is always present");
        assert!(
            node.get("disabled").is_none(),
            "unset optionals are omitted"
        );
    }

    #[test]
    fn delta_ops_carry_verde_field_names() {
        let moved = serde_json::to_value(DeltaOp::move_node("x".into(), Some("y".into()))).unwrap();
        assert_eq!(moved["type"], "move_node");
        assert_eq!(moved["id"], "x");
        assert_eq!(moved["newParentId"], "y");

        let flagged = serde_json::to_value(DeltaOp::has_children("x".into(), false)).unwrap();
        assert_eq!(flagged["type"], "update_node");
        assert_eq!(flagged["hasChildren"], false);
        assert!(flagged.get("name").is_none());

        let detached = serde_json::to_value(DeltaOp::move_node("x".into(), None)).unwrap();
        assert_eq!(detached["newParentId"], Value::Null);
    }

    #[test]
    fn explorer_delta_omits_empty_added_roots() {
        let out = Outbound::ExplorerDelta {
            ops: vec![DeltaOp::remove_node("z".into())],
            added_root_ids: vec![],
        };
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["type"], "explorer_delta");
        assert!(json.get("addedRootIds").is_none());
    }

    #[test]
    fn operation_result_nests_the_result_object() {
        let out = Outbound::OperationResult {
            operation_id: "op1".into(),
            result: OperationOutcome {
                success: true,
                error: None,
                data: Some(json!({ "x": 1 })),
            },
        };
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["type"], "operation_result");
        assert_eq!(json["operationId"], "op1");
        assert_eq!(json["result"]["success"], true);
        assert!(json["result"].get("error").is_none());
        assert_eq!(json["result"]["data"]["x"], 1);
    }

    #[test]
    fn inbound_parses_verde_requests() {
        let ack: Inbound = serde_json::from_str(r#"{"type":"ack","requestId":"7"}"#).unwrap();
        assert!(matches!(ack, Inbound::Ack { request_id: Some(r) } if r == "7"));

        let children: Inbound =
            serde_json::from_str(r#"{"type":"request_children","parentIds":["a","b"]}"#).unwrap();
        assert!(
            matches!(children, Inbound::RequestChildren { parent_ids } if parent_ids == ["a", "b"])
        );

        let operation: Inbound =
            serde_json::from_str(r#"{"type":"operation","operationId":"1","operation":{"type":"get_properties","nodeId":"n"}}"#)
                .unwrap();
        let Inbound::Operation {
            operation_id,
            operation,
        } = operation
        else {
            panic!("expected an operation");
        };
        assert_eq!(operation_id, "1");
        assert_eq!(operation["nodeId"], "n");

        let unknown: Inbound = serde_json::from_str(r#"{"type":"future_thing","x":1}"#).unwrap();
        assert!(matches!(unknown, Inbound::Unknown));
    }
}
