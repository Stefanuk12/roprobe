use std::{sync::Arc, time::Duration};

use futures_util::{SinkExt as _, StreamExt as _, stream::FuturesUnordered};
use tokio::{
    net::TcpStream,
    sync::{broadcast, watch},
    time::{Instant, timeout},
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};
use tracing::{debug, info, warn};

use crate::{
    server::{DomChange, DomRequest, Mirror},
    upstream::verde::{
        OperationTask,
        protocol::{self, DeltaOp, Inbound, Outbound},
        run_operation, serialize, translate,
        tree_state::TreeState,
    },
};

type VerdeStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type VerdeSink = futures_util::stream::SplitSink<VerdeStream, Message>;
type VerdeSplitRead = futures_util::stream::SplitStream<VerdeStream>;
type WsResult = Result<(), tokio_tungstenite::tungstenite::Error>;

/// Verde acks every second and drops a plugin after 5s of silence.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Coalesce a burst of DOM changes into one `explorer_delta`.
const FLUSH_DELAY: Duration = Duration::from_millis(100);

/// Why [`Connection::serve`] stopped serving a session.
pub enum Served {
    /// The active session changed; rebuild for the new one.
    Switched,
    /// The socket closed or fell idle; drop the connection.
    Closed,
}

/// A singular Verde connection.
pub struct Connection {
    write: VerdeSink,
    tree: TreeState,
    flush_at: Option<Instant>,
    ops: FuturesUnordered<OperationTask>,
    security_level: u8,
}

impl Connection {
    pub fn new(write: VerdeSink, security_level: u8) -> Self {
        Self {
            write,
            tree: TreeState::default(),
            flush_at: None,
            ops: FuturesUnordered::new(),
            security_level,
        }
    }

    /// Initialise the sesion, wiping any previous session, then sending a roots snapshot to Verde.
    pub async fn greet(&mut self, snapshot: protocol::Snapshot) -> WsResult {
        self.tree = TreeState::default();
        self.flush_at = None;
        self.ops = FuturesUnordered::new();

        self.tree.seed(&snapshot, false);
        self.send(Outbound::ExplorerSnapshot {
            payload: snapshot,
            is_full: false,
        })
        .await
    }

    /// The main verde/DOM event loop for one session.
    ///
    /// - serve requests
    /// - stream deltas
    /// - cleanup when the session switches or the socket dies
    pub async fn serve<S>(
        &mut self,
        mirror: &Arc<Mirror>,
        changed: &mut broadcast::Receiver<Arc<DomChange>>,
        current: &mut watch::Receiver<S>,
        read: &mut VerdeSplitRead,
    ) -> Result<Served, tokio_tungstenite::tungstenite::Error> {
        loop {
            // Arm the debounce whenever there's buffered work and nothing pending.
            if self.flush_at.is_none() && !self.tree.delta_buf.is_empty() {
                self.flush_at = Some(Instant::now() + FLUSH_DELAY);
            }

            tokio::select! {
                // A dead server never speaks; time the read out to notice.
                inbound = timeout(READ_TIMEOUT, read.next()) => {
                    let message = match inbound {
                        Err(_) => {
                            debug!("verde idle beyond {}s, dropping", READ_TIMEOUT.as_secs());
                            return Ok(Served::Closed);
                        }
                        Ok(None) => return Ok(Served::Closed),
                        Ok(Some(Err(e))) => return Err(e),
                        Ok(Some(Ok(message))) => message,
                    };

                    match message {
                        Message::Close(_) => return Ok(Served::Closed),
                        Message::Ping(data) => self.write.send(Message::Pong(data)).await?,
                        Message::Text(text) => match serde_json::from_str::<Inbound>(&text) {
                            Ok(inbound) => self.handle_inbound(mirror, inbound).await?,
                            Err(e) => debug!("verde: undecodable frame: {e}"),
                        },
                        _ => {}
                    }
                }

                // The active session changed: stop, so the caller rebuilds.
                res = current.changed() => {
                    // Session store dropped, application shutting down?
                    return Ok(if res.is_err() { Served::Closed } else { Served::Switched });
                }

                // The mirror changed: fold it into buffered delta ops.
                event = changed.recv() => match event {
                    Ok(change) => self.tree.translate(mirror, &change),
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        warn!("verde fell behind by {dropped} changes, resyncing with a full snapshot");
                        self.full_resync(mirror).await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(Served::Closed),
                },

                // The debounce elapsed: flush one delta.
                _ = tokio::time::sleep_until(self.flush_at.unwrap_or_else(Instant::now)), if self.flush_at.is_some() => {
                    self.flush().await?;
                    self.flush_at = None;
                }

                // A relayed operation finished: prime verde's tree with each
                // instance its properties reference so those links reveal on click,
                // then send the result.
                Some((result, refs)) = self.ops.next() => {
                    if !refs.is_empty() {
                        mirror.with_dom(|dom| {
                            for ref_id in &refs {
                                self.tree.reveal_path(dom, ref_id);
                            }
                        });
                        if !self.tree.delta_buf.is_empty() {
                            self.flush().await?;
                            self.flush_at = None;
                        }
                    }
                    self.send(result).await?;
                }
            }
        }
    }

    /// Dispatch one decoded inbound verde message.
    async fn handle_inbound(&mut self, mirror: &Arc<Mirror>, inbound: Inbound) -> WsResult {
        debug!("verde inbound {inbound:?}");
        match inbound {
            // Heartbeat: keep verde's 5s timer alive.
            Inbound::Ack { request_id } => self.send(Outbound::Ack { request_id }).await?,

            Inbound::RequestSnapshot { full } => {
                let payload = if full {
                    mirror.with_dom(serialize::full)
                } else {
                    mirror.with_dom(serialize::roots)
                };

                self.tree.seed(&payload, full);
                self.send(Outbound::ExplorerSnapshot {
                    payload,
                    is_full: full,
                })
                .await?;
            }

            Inbound::RequestChildren { parent_ids } => {
                for parent in parent_ids {
                    // Verde already knows about these children, ignore
                    if self.tree.loaded.contains(&parent) || !self.tree.known.contains(&parent) {
                        continue;
                    }

                    // Read the mirror's DOM for the requested children, checking if they have children too
                    // for the indicator.
                    let (children, has_children) = mirror.with_dom(|dom| {
                        let children = serialize::children_of(dom, &parent);
                        let has_children = dom
                            .get(&parent)
                            .is_some_and(|instance| dom.has_children(instance.referent()));
                        (children, has_children)
                    });
                    self.tree.loaded.insert(parent.clone());

                    if !children.is_empty() {
                        // Mark the nodes as known by Verde.
                        let root_id = children[0].id.clone();
                        for child in &children {
                            self.tree.known.insert(child.id.clone());
                        }

                        self.tree.delta_buf.push(DeltaOp::add_subtree(
                            Some(parent),
                            root_id,
                            children,
                        ));
                    } else if has_children {
                        // Lazy loading: we know there are children, but we have to ask the client for the children.
                        if let Some(sink) = mirror.dom_sink() {
                            let _ = sink.send(DomRequest::Children(Some(parent))).await;
                        }
                    } else {
                        // No children found.
                        self.tree
                            .delta_buf
                            .push(DeltaOp::has_children(parent, false));
                    }
                }
            }

            Inbound::RequestSearch { query } => {
                // We need to ask for a full snapshot for search to properly work
                if let Some(sink) = mirror.dom_sink() {
                    let before = mirror.with_dom(|d| d.len());
                    let _ = sink.send(DomRequest::Snapshot(None)).await;
                    match mirror.wait_population(Duration::from_secs(10), usize::MAX).await {
                        Some(x) => info!("loaded {} nodes", x.abs_diff(before)),
                        None => info!("no current session"),
                    };
                }

                // Search through our mirror
                let (nodes, truncated) = mirror.with_dom(|dom| serialize::search(dom, &query));
                self.send(Outbound::SearchResult {
                    query,
                    nodes,
                    truncated,
                    partial: false,
                })
                .await?;
            }

            Inbound::ReleaseSubtree {
                parent_ids,
                node_ids,
            } => {
                for parent in &parent_ids {
                    self.tree.loaded.remove(parent);
                }
                for node in &node_ids {
                    self.tree.known.remove(node);
                    self.tree.loaded.remove(node);
                }
            }

            Inbound::Operation {
                operation_id,
                operation,
            } => {
                self.ops.push(run_operation(
                    mirror.op_sink(),
                    Arc::clone(mirror),
                    operation_id,
                    translate::to_operation(&operation),
                    self.security_level,
                ));
            }

            Inbound::Error { message } => warn!("verde reported an error: {message}"),
            Inbound::Unknown => {}
        }

        Ok(())
    }

    /// Send the entire mirror over, dropping buffered deltas.
    ///
    /// Used when Verde lags behind, to refresh its view.
    async fn full_resync(&mut self, mirror: &Arc<Mirror>) -> WsResult {
        self.flush_at = None;

        let snapshot = mirror.with_dom(serialize::full);
        self.tree.seed(&snapshot, true);
        self.send(Outbound::ExplorerSnapshot {
            payload: snapshot,
            is_full: true,
        })
        .await
    }

    /// Flush the buffered delta as one `explorer_delta`, if there's anything to send.
    async fn flush(&mut self) -> WsResult {
        if self.tree.delta_buf.is_empty() {
            return Ok(());
        }

        let ops = std::mem::take(&mut self.tree.delta_buf);
        let added_root_ids = std::mem::take(&mut self.tree.added_roots);
        self.send(Outbound::ExplorerDelta {
            ops,
            added_root_ids,
        })
        .await
    }

    /// Serialize and send one outbound verde message as a JSON text frame.
    async fn send(&mut self, message: Outbound) -> WsResult {
        let json = serde_json::to_string(&message).expect("serialize verde message");
        self.write.send(Message::Text(json.into())).await
    }
}
