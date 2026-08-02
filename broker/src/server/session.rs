use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, atomic::AtomicU8},
};

use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::{
    Context, Result,
    protocol::{ClientMessage, DomId, OpResult, ServerMessage},
    server::{DomRequest, Mirror, OpRequest, WsWrite},
};

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct SessionId(pub u32);
impl core::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SessionId({})", self.0)
    }
}

/// Follow-up work a [`Session::handle`] call defers to its caller, for anything
/// that re-enters the sessions lock the caller is already holding.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[must_use = "the caller must run this once it has released the sessions lock"]
pub enum PostHandle {
    #[default]
    None,
    /// Make this session the active (forwarded) one.
    Activate(SessionId),
}

/// The broker's side of one accepted client connection.
#[derive(Debug)]
pub struct Session {
    ctx: Context,

    pub id: SessionId,
    pub peer: SocketAddr,
    write: WsWrite,

    pub mirror: Arc<Mirror>,
    pub security_level: Arc<AtomicU8>,
    pending: HashMap<u32, oneshot::Sender<OpResult>>,
    next_op_id: u32,
}

impl Session {
    /// Create a new session.
    pub fn new(
        ctx: Context,
        peer: SocketAddr,
        write: WsWrite,
        mirror: Arc<Mirror>,
        security_level: u8,
        id: SessionId,
    ) -> Self {
        Self {
            ctx,
            mirror,
            security_level: Arc::new(security_level.into()),
            peer,
            write,
            id,
            pending: HashMap::new(),
            next_op_id: 0,
        }
    }

    /// Relay an upstream operation to the client.
    pub async fn dispatch_operation(&mut self, request: OpRequest) -> Result<()> {
        let id = self.next_op_id;
        self.next_op_id = self.next_op_id.wrapping_add(1);
        self.pending.insert(id, request.reply);
        self.send(ServerMessage::Operation {
            id,
            op: request.operation,
        })
        .await
    }

    /// Send a message.
    pub async fn send(&mut self, message: ServerMessage) -> Result<()> {
        self.write.send(message.try_into()?).await?;
        Ok(())
    }

    /// Forward a raw WebSocket frame, e.g. answering a keepalive ping with a pong
    /// so the client's socket doesn't consider us dead and hang up.
    pub async fn send_frame(&mut self, frame: Message) -> Result<()> {
        self.write.send(frame).await?;
        Ok(())
    }

    /// Relay a lazy dom-population request from an upstream to the client.
    pub async fn handle_dom_request(&mut self, request: DomRequest) -> Result<()> {
        match request {
            DomRequest::Children(id) => self.request_children(id).await,
            DomRequest::Node(id) => self.request_node(id).await,
            DomRequest::Snapshot(id) => self.request_snapshot(id).await,
            DomRequest::Search { from, query } => self.search(from, query).await,
            DomRequest::Nodes(ids) => self.request_nodes(ids).await,
        }
    }

    /// Ask the client to mirror a node's immediate children - `None` for the watch root's top level.
    pub async fn request_children(&mut self, id: Option<DomId>) -> Result<()> {
        self.send(ServerMessage::RequestChildren(id)).await
    }

    /// Ask the client to mirror a single node by id, without its children.
    pub async fn request_node(&mut self, id: DomId) -> Result<()> {
        self.send(ServerMessage::RequestNode(id)).await
    }

    /// Ask the client to snapshot a subtree by id - `None` for the whole scope.
    pub async fn request_snapshot(&mut self, id: Option<DomId>) -> Result<()> {
        self.send(ServerMessage::RequestSnapshot(id)).await
    }

    /// Ask the client to search `from`'s descendants for `query`.
    pub async fn search(&mut self, from: DomId, query: String) -> Result<()> {
        self.send(ServerMessage::Search { from, query }).await
    }

    /// Ask the client to mirror several nodes by id in one patch.
    pub async fn request_nodes(&mut self, ids: Vec<DomId>) -> Result<()> {
        self.send(ServerMessage::RequestNodes(ids)).await
    }

    /// Processes a single decoded client message, returning any [`PostHandle`] the caller must run once it has released the sessions lock.
    pub async fn handle(&mut self, message: ClientMessage) -> Result<PostHandle> {
        debug!(peer = %self.peer, kind = message.kind(), "client message");
        match message {
            ClientMessage::Shutdown => {
                info!(peer = %self.peer, "client requested shutdown");
                self.ctx.shutdown.notify_one();
            }
            ClientMessage::SetUpstream { upstream, enabled } => {
                info!(peer = %self.peer, ?upstream, enabled, "client set upstream");
                self.ctx.controls.set(upstream, enabled);
                self.send(ServerMessage::UpstreamChanged { upstream, enabled })
                    .await?;
            }
            ClientMessage::UpdateDom(patch) => {
                debug!(
                    peer = %self.peer,
                    upserts = patch.upserts.len(),
                    removals = patch.removals.len(),
                    "client dom patch"
                );
                self.mirror.apply(patch);
            }
            ClientMessage::EnumFamilies(families) => {
                info!(peer = %self.peer, count = families.len(), "client sent enum catalog");
                self.mirror.set_enum_catalog(families);
            }
            ClientMessage::OperationResult { id, result } => match self.pending.remove(&id) {
                Some(reply) => {
                    let _ = reply.send(result);
                }
                None => {
                    warn!(peer = %self.peer, id, "operation result for an unknown id, dropping")
                }
            },
            // `set_current` takes the sessions lock our caller already holds, so it is deferred rather than awaited here.
            ClientMessage::RequestActive => {
                info!(peer = %self.peer, id = self.id.0, "client requested the active slot");
                return Ok(PostHandle::Activate(self.id));
            }
            // Control-only messages have no meaning on a syncing connection.
            ClientMessage::SwapActive(..)
            | ClientMessage::ListSessions
            | ClientMessage::SetSecurity { .. } => {
                warn!(peer = %self.peer, "control message on a syncing connection, ignoring");
            }
        }
        Ok(PostHandle::None)
    }
}
