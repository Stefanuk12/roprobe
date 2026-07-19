mod api;
mod connection;
pub mod connector;
mod protocol;
mod serialize;
mod translate;
pub(crate) mod tree_state;
pub use connector::*;

use std::{sync::Arc, time::Duration};

use tokio::{sync::oneshot, time::timeout};

use self::protocol::Outbound;
use crate::{
    protocol::{DomId, DomValue, OpResult, Operation},
    server::{Mirror, OpRequest},
};

const OP_TIMEOUT: Duration = Duration::from_secs(25);

/// An operation's result plus every instance id its property reads reference —
/// so verde's tree can be primed to reveal those targets on click.
type OperationTask =
    std::pin::Pin<Box<dyn std::future::Future<Output = (Outbound, Vec<DomId>)> + Send>>;

/// Relay one operation to the client session and shape its outcome for verde,
/// resolving references/enums against `mirror`, with `operation` `None` when the
/// verde JSON didn't parse into a known op.
fn run_operation(
    sink: Option<tokio::sync::mpsc::Sender<OpRequest>>,
    mirror: Arc<Mirror>,
    operation_id: String,
    mut operation: Option<Operation>,
    security_level: u8,
) -> OperationTask {
    // Resolve the GetProperties target's class, then tell the client exactly which
    // properties to read (from the API dump) and attach that same class's metadata.
    let class = match &operation {
        Some(Operation::GetProperties { node, .. }) => {
            mirror.with_dom(|dom| dom.get(node).map(|instance| instance.class.to_string()))
        }
        _ => None,
    };
    if let (Some(Operation::GetProperties { properties, .. }), Some(class)) =
        (&mut operation, &class)
    {
        *properties = api::class_properties(class, security_level);
    }
    Box::pin(async move {
        let result = dispatch(sink, operation).await;
        // The ids of any instance-reference properties, to reveal in verde's tree.
        let refs: Vec<DomId> = match &result {
            OpResult::Reads(reads) => reads
                .properties
                .iter()
                .filter_map(|read| match &read.value {
                    DomValue::Ref(id) => Some(id.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let outcome = translate::to_outcome(result, &mirror, class.as_deref());
        (
            Outbound::OperationResult {
                operation_id,
                result: outcome,
            },
            refs,
        )
    })
}

/// Hand an operation to the active session and await the client's result.
async fn dispatch(
    sink: Option<tokio::sync::mpsc::Sender<OpRequest>>,
    operation: Option<Operation>,
) -> OpResult {
    let Some(operation) = operation else {
        return OpResult::Err("unsupported_operation".to_string());
    };
    let Some(sink) = sink else {
        return OpResult::Err("no_session".to_string());
    };

    let (reply, result) = oneshot::channel();
    let request = OpRequest { operation, reply };
    if sink.send(request).await.is_err() {
        return OpResult::Err("session_ended".to_string());
    }

    match timeout(OP_TIMEOUT, result).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => OpResult::Err("session_ended".to_string()),
        Err(_) => OpResult::Err("timeout".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        protocol::Reads,
        upstream::verde::{protocol::DeltaOp, tree_state::TreeState},
    };

    fn get_properties() -> Operation {
        Operation::GetProperties {
            node: "n".into(),
            properties: vec![],
        }
    }

    #[tokio::test]
    async fn translate_forwards_a_whole_added_subtree() {
        use crate::protocol::{DomInstance, DomPatch};

        fn node(id: &str, parent: Option<&str>) -> DomInstance {
            DomInstance {
                id: id.into(),
                parent: parent.map(Into::into),
                class: "Folder".into(),
                name: id.into(),
                has_children: false,
                properties: Default::default(),
                attributes: Default::default(),
                tags: None,
            }
        }

        let mirror = Mirror::new();
        let (mut changes, _) = mirror.subscribe_with(|_| ());
        // A root and two children, all arriving in one patch.
        mirror.apply(DomPatch {
            upserts: vec![
                node("root", None),
                node("a", Some("root")),
                node("b", Some("root")),
            ],
            removals: vec![],
            updates: vec![],
        });
        let change = changes.recv().await.expect("a change");

        let mut tree = TreeState::default();
        tree.translate(&mirror, &change);

        // The whole subtree reaches verde — the root plus both children — rather
        // than the root alone with its descendants dropped.
        let subtrees = tree
            .delta_buf
            .iter()
            .filter(|op| matches!(op, DeltaOp::AddSubtree { .. }))
            .count();
        assert_eq!(subtrees, 3, "root and both children are all sent");
        assert_eq!(tree.added_roots, vec!["root".to_string()]);
        assert!(
            tree.known.contains("a") && tree.known.contains("b"),
            "children are now tracked"
        );
        assert!(tree.loaded.contains("root"), "the parent is marked loaded");
    }

    #[test]
    fn reveal_path_adds_the_unknown_ancestor_chain() {
        use crate::protocol::{DomInstance, DomPatch};

        fn node(id: &str, parent: Option<&str>) -> DomInstance {
            DomInstance {
                id: id.into(),
                parent: parent.map(Into::into),
                class: "Folder".into(),
                name: id.into(),
                has_children: false,
                properties: Default::default(),
                attributes: Default::default(),
                tags: None,
            }
        }

        let mirror = Mirror::new();
        mirror.apply(DomPatch {
            upserts: vec![
                node("root", None),
                node("mid", Some("root")),
                node("leaf", Some("mid")),
            ],
            removals: vec![],
            updates: vec![],
        });

        // Verde knows only the root; reveal a deep ref.
        let mut tree = TreeState {
            known: HashSet::from(["root".to_string()]),
            ..Default::default()
        };
        mirror.with_dom(|dom| tree.reveal_path(dom, &"leaf".into()));

        // `mid` then `leaf` are attached under their now-present parents.
        let subtrees = tree
            .delta_buf
            .iter()
            .filter(|op| matches!(op, DeltaOp::AddSubtree { .. }))
            .count();
        assert_eq!(subtrees, 2, "mid and leaf are added");
        assert!(tree.known.contains("mid") && tree.known.contains("leaf"));
        assert!(
            tree.loaded.contains("root") && tree.loaded.contains("mid"),
            "parents on the path are loaded"
        );

        // Revealing an already-known node is a no-op.
        tree.delta_buf.clear();
        mirror.with_dom(|dom| tree.reveal_path(dom, &"leaf".into()));
        assert!(
            tree.delta_buf.is_empty(),
            "no duplicate adds for a path already in the tree"
        );
    }

    #[tokio::test]
    async fn dispatch_relays_to_the_session_and_returns_the_result() {
        let (tx, mut rx) = mpsc::channel::<OpRequest>(1);
        tokio::spawn(async move {
            let request = rx.recv().await.expect("relayed operation");
            assert!(matches!(request.operation, Operation::GetProperties { .. }));
            // The client echoes back an (empty) reads result.
            let _ = request.reply.send(OpResult::Reads(Reads {
                properties: vec![],
                tags: vec![],
                attributes: vec![],
            }));
        });

        let result = dispatch(Some(tx), Some(get_properties())).await;
        assert!(matches!(result, OpResult::Reads(_)));
    }

    #[tokio::test]
    async fn dispatch_without_an_operation_reports_unsupported() {
        let result = dispatch(None, None).await;
        assert_eq!(result, OpResult::Err("unsupported_operation".into()));
    }

    #[tokio::test]
    async fn dispatch_without_a_session_fails_fast() {
        let result = dispatch(None, Some(get_properties())).await;
        assert_eq!(result, OpResult::Err("no_session".into()));
    }

    #[tokio::test]
    async fn dispatch_reports_a_dropped_session() {
        let (tx, rx) = mpsc::channel::<OpRequest>(1);
        // The session goes away before answering.
        drop(rx);
        let result = dispatch(Some(tx), Some(get_properties())).await;
        assert_eq!(result, OpResult::Err("session_ended".into()));
    }
}
