use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc, oneshot};

use crate::{
    protocol::{DomId, DomPatch, EnumFamily, OpResult, Operation},
    server::{DomChange, SessionDom},
};

/// How many [`DomChange`]s a slow upstream subscriber may fall behind before being told to resync from a full snapshot.
const CHANGE_CAPACITY: usize = 1024;

/// One operation relayed from an upstream (verde) to the client session, with a channel to carry the result back.
#[derive(Debug)]
pub struct OpRequest {
    /// The typed operation for the client to apply.
    pub operation: Operation,
    /// Where the client's result lands.
    pub reply: oneshot::Sender<OpResult>,
}

/// A fire-and-forget lazy-population request from an upstream (verde) for the client to mirror more of the tree, whose nodes arrive asynchronously as a [`DomPatch`] on the change feed rather than a direct reply.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[allow(dead_code, reason = "node/snapshot/search/nodes land as verde wires each lazy path; children is live")]
pub enum DomRequest {
    /// Mirror a node's immediate children — `None` for the watch root's top level.
    Children(Option<DomId>),
    /// Mirror a single node by id, without its children.
    Node(DomId),
    /// Snapshot a subtree by id — `None` for the whole scope.
    Snapshot(Option<DomId>),
    /// Search `from`'s descendants for `query`, mirroring the matches.
    Search { from: DomId, query: String },
    /// Mirror several nodes by id in one patch.
    Nodes(Vec<DomId>),
}

/// A mirror of the client's Dom.
#[derive(Debug)]
pub struct Mirror {
    dom: Mutex<SessionDom>,
    changes: broadcast::Sender<Arc<DomChange>>,
    op_sink: Mutex<Option<(u32, mpsc::Sender<OpRequest>)>>,
    dom_sink: Mutex<Option<(u32, mpsc::Sender<DomRequest>)>>,
    enum_catalog: Mutex<Arc<Vec<EnumFamily>>>,
}

impl Mirror {
    /// Create an empty mirror.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            dom: Mutex::new(SessionDom::new()),
            changes: broadcast::channel(CHANGE_CAPACITY).0,
            op_sink: Mutex::new(None),
            dom_sink: Mutex::new(None),
            enum_catalog: Mutex::new(Arc::new(Vec::new())),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<DomChange>> {
        self.changes.subscribe()
    }

    /// Merge a patch and broadcast what changed.
    pub fn apply(&self, patch: DomPatch) {
        let mut dom = self.dom.lock().expect("mirror dom lock");
        let change = dom.apply(patch);
        if !change.is_empty() {
            let _ = self.changes.send(Arc::new(change));
        }
    }

    /// Atomically subscribe to future changes and read the current DOM (one lock; see [`Mirror::apply`]).
    pub fn subscribe_with<R>(&self, read: impl FnOnce(&SessionDom) -> R) -> (broadcast::Receiver<Arc<DomChange>>, R) {
        let dom = self.dom.lock().expect("mirror dom lock");
        let receiver = self.changes.subscribe();
        let value = read(&dom);
        (receiver, value)
    }

    /// Read the current DOM (no change-stream ordering guarantees).
    pub fn with_dom<R>(&self, read: impl FnOnce(&SessionDom) -> R) -> R {
        let dom = self.dom.lock().expect("mirror dom lock");
        read(&dom)
    }

    /// Forget everything mirrored so far (a new client session starts fresh).
    pub fn reset(&self) {
        *self.dom.lock().expect("mirror dom lock") = SessionDom::new();
    }

    /// Point operation relaying at the now-active client session `owner`.
    pub fn install_op_sink(&self, owner: u32, sink: mpsc::Sender<OpRequest>) {
        *self.op_sink.lock().expect("mirror op sink lock") = Some((owner, sink));
    }

    /// Drop the operation sink if `owner` still holds it (a no-op once a successor installed its own), failing pending relays fast.
    pub fn clear_op_sink(&self, owner: u32) {
        let mut sink = self.op_sink.lock().expect("mirror op sink lock");
        if sink.as_ref().is_some_and(|(id, _)| *id == owner) {
            *sink = None;
        }
    }

    /// A clone of the current session's operation sink, if one is active.
    pub fn op_sink(&self) -> Option<mpsc::Sender<OpRequest>> {
        self.op_sink.lock().expect("mirror op sink lock").as_ref().map(|(_, sink)| sink.clone())
    }

    /// Point lazy dom-population requests at the now-active client session `owner`.
    pub fn install_dom_sink(&self, owner: u32, sink: mpsc::Sender<DomRequest>) {
        *self.dom_sink.lock().expect("mirror dom sink lock") = Some((owner, sink));
    }

    /// Drop the dom-request sink if `owner` still holds it, a no-op once a successor installed its own.
    pub fn clear_dom_sink(&self, owner: u32) {
        let mut sink = self.dom_sink.lock().expect("mirror dom sink lock");
        if sink.as_ref().is_some_and(|(id, _)| *id == owner) {
            *sink = None;
        }
    }

    /// A clone of the current session's dom-request sink, if one is active.
    pub fn dom_sink(&self) -> Option<mpsc::Sender<DomRequest>> {
        self.dom_sink.lock().expect("mirror dom sink lock").as_ref().map(|(_, sink)| sink.clone())
    }

    /// Store the client's enum catalog (sent once on connect).
    pub fn set_enum_catalog(&self, families: Vec<EnumFamily>) {
        *self.enum_catalog.lock().expect("mirror enum catalog lock") = Arc::new(families);
    }

    /// A cheap snapshot of the enum catalog for resolving `DomValue::Enum`s.
    pub fn enum_catalog(&self) -> Arc<Vec<EnumFamily>> {
        Arc::clone(&self.enum_catalog.lock().expect("mirror enum catalog lock"))
    }

    /// The `(name, class)` of a mirrored instance for resolving a `DomValue::Ref` to a display name; `None` if unmirrored.
    pub fn resolve_ref(&self, id: &str) -> Option<(String, String)> {
        self.with_dom(|dom| dom.get(id).map(|instance| (instance.name.clone(), instance.class.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dom_requests_reach_the_installed_sink_until_cleared() {
        let mirror = Mirror::new();
        assert!(mirror.dom_sink().is_none(), "no sink before a session installs one");

        let (tx, mut rx) = mpsc::channel::<DomRequest>(4);
        mirror.install_dom_sink(1, tx);

        mirror.dom_sink().expect("sink installed").send(DomRequest::Children(None)).await.unwrap();
        assert!(matches!(rx.recv().await, Some(DomRequest::Children(None))));

        // A different owner's clear is ignored; only the holder's takes effect.
        mirror.clear_dom_sink(2);
        assert!(mirror.dom_sink().is_some(), "another owner cannot clear the sink");
        mirror.clear_dom_sink(1);
        assert!(mirror.dom_sink().is_none(), "the holder clears its own sink");
    }
}
