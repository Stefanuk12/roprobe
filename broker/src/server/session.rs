use std::{net::SocketAddr, sync::Arc};

use futures_util::SinkExt;
use tokio::sync::Notify;
use tracing::{debug, info};

use crate::{
    Result,
    protocol::{ClientMessage, DomId, ServerMessage},
    server::{SessionDom, WsWrite},
    upstream::Controls,
};

/// The broker's side of one accepted client connection.
pub struct Session {
    peer: SocketAddr,
    write: WsWrite,
    shutdown: Arc<Notify>,
    controls: Arc<Controls>,
    dom: SessionDom,
    enum_families: Vec<String>,
}

impl Session {
    /// Create a new session.
    pub fn new(peer: SocketAddr, write: WsWrite, shutdown: Arc<Notify>, controls: Arc<Controls>) -> Self {
        Self { peer, write, shutdown, controls, dom: SessionDom::new(), enum_families: Vec::new() }
    }

    /// The client's lazily mirrored DOM.
    pub fn dom(&self) -> &SessionDom {
        &self.dom
    }

    /// Resolve an `Enum` family index to its Roblox family name via the client's `GetEnums()` table.
    #[allow(dead_code, reason = "consumed by enum-family resolution once a DOM viewer needs names")]
    pub fn enum_family(&self, index: u16) -> Option<&str> {
        self.enum_families.get(index as usize).map(String::as_str)
    }

    /// Send a message.
    pub async fn send(&mut self, message: ServerMessage) -> Result<()> {
        self.write.send(message.try_into()?).await?;
        Ok(())
    }

    /// Ask the client to mirror a node's immediate children - `None` for the watch root's top level.
    #[allow(dead_code, reason = "caller lands with the Verde lazy-load integration")]
    pub async fn request_children(&mut self, id: Option<DomId>) -> Result<()> {
        self.send(ServerMessage::RequestChildren(id)).await
    }

    /// Ask the client to mirror a single node by id, without its children.
    #[allow(dead_code, reason = "caller lands with the Verde lazy-load integration")]
    pub async fn request_node(&mut self, id: DomId) -> Result<()> {
        self.send(ServerMessage::RequestNode(id)).await
    }

    /// Ask the client to snapshot a subtree by id - `None` for the whole scope.
    #[allow(dead_code, reason = "caller lands with the Verde lazy-load integration")]
    pub async fn request_snapshot(&mut self, id: Option<DomId>) -> Result<()> {
        self.send(ServerMessage::RequestSnapshot(id)).await
    }

    /// Ask the client to search `from`'s descendants for `query`.
    #[allow(dead_code, reason = "caller lands with the Verde lazy-load integration")]
    pub async fn search(&mut self, from: DomId, query: String) -> Result<()> {
        self.send(ServerMessage::Search { from, query }).await
    }

    /// Ask the client to mirror several nodes by id in one patch.
    #[allow(dead_code, reason = "caller lands with the Verde lazy-load integration")]
    pub async fn request_nodes(&mut self, ids: Vec<DomId>) -> Result<()> {
        self.send(ServerMessage::RequestNodes(ids)).await
    }

    /// Processes a single decoded client message.
    pub async fn handle(&mut self, message: ClientMessage) -> Result<()> {
        debug!(peer = %self.peer, kind = message.kind(), "client message");
        match message {
            ClientMessage::Shutdown => {
                info!(peer = %self.peer, "client requested shutdown");
                self.shutdown.notify_one();
            }
            ClientMessage::SetUpstream { upstream, enabled } => {
                info!(peer = %self.peer, ?upstream, enabled, "client set upstream");
                self.controls.set(upstream, enabled);
                self.send(ServerMessage::UpstreamChanged { upstream, enabled }).await?;
            }
            ClientMessage::UpdateDom(patch) => {
                debug!(
                    peer = %self.peer,
                    upserts = patch.upserts.len(),
                    removals = patch.removals.len(),
                    "client dom patch"
                );
                self.dom.apply(patch);
            }
            ClientMessage::EnumFamilies(families) => {
                info!(peer = %self.peer, count = families.len(), "client sent enum families");
                self.enum_families = families;
            }
        }
        Ok(())
    }
}