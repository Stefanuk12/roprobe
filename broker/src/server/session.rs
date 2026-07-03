use std::{net::SocketAddr, sync::Arc};

use futures_util::SinkExt;
use tokio::sync::Notify;
use tracing::{debug, info};

use crate::{Result, protocol::{ClientMessage, ServerMessage}, server::WsWrite, upstream::Controls};

/// The broker's side of one accepted client connection.
pub struct Session {
    peer: SocketAddr,
    write: WsWrite,
    shutdown: Arc<Notify>,
    controls: Arc<Controls>,
}

impl Session {
    /// Create a new session.
    pub fn new(peer: SocketAddr, write: WsWrite, shutdown: Arc<Notify>, controls: Arc<Controls>) -> Self {
        Self { peer, write, shutdown, controls }
    }

    /// Send a message.
    pub async fn send(&mut self, message: ServerMessage) -> Result<()> {
        self.write.send(message.try_into()?).await?;
        Ok(())
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
        }
        Ok(())
    }
}