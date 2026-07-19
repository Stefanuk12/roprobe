use std::{sync::Arc, time::Duration};

use futures_util::StreamExt as _;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use tracing::{debug, info, warn};

use crate::{Context, upstream::verde::{connection::{Connection, Served}, serialize}};

type VerdeStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsResult = Result<(), tokio_tungstenite::tungstenite::Error>;

pub const DEFAULT_PORT: u16 = 9000;
const RETRY_DELAY: Duration = Duration::from_secs(2);

/// Manages the persistent connection to Verde.
pub struct VerdeConnector {
    ctx: Context,
    port: u16,
}

impl VerdeConnector {
    pub fn new(ctx: Context, verde_port: u16) -> Self {
        Self {
            ctx,
            port: verde_port,
        }
    }

    /// Keep the connection alive while enabled.
    pub async fn start(mut self) {
        let mut enabled = self.ctx.controls.verde.subscribe();
        loop {
            if !*enabled.borrow_and_update() {
                info!("verde connection paused");
                if enabled.wait_for(|on| *on).await.is_err() {
                    return;
                }
                info!("verde connection resumed");
            }

            tokio::select! {
                _ = self.connect_loop() => {},
                result = enabled.wait_for(|on| !*on) => {
                    if result.is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// Keep a WebSocket connection to verde open, serving the mirror over it.
    async fn connect_loop(&mut self) {
        let port = self.port;
        let url = format!("ws://127.0.0.1:{port}");
        let mut warned = false;

        loop {
            match connect_async(url.as_str()).await {
                Ok((ws, _)) => {
                    info!("connected to verde on :{port}");
                    warned = false;

                    if let Err(e) = self.run_connection(ws).await {
                        debug!("verde connection error: {e}");
                    }
                    warn!("verde connection lost, reconnecting");
                }
                Err(e) if !warned => {
                    warn!(
                        "verde not reachable on :{port} ({e}), retrying every {}s",
                        RETRY_DELAY.as_secs()
                    );
                    warned = true;
                }
                _ => {}
            }

            tokio::time::sleep(RETRY_DELAY).await;
        }
    }

    /// Listens to the active session, handling context switches and DOM deltas.
    async fn run_connection(&self, ws: VerdeStream) -> WsResult {
        let (write, mut read) = ws.split();
        let sessions = &self.ctx.sessions;
        let mut conn = Connection::new(write, 8);

        // Rebuilt whenever the active session changes.
        let mut current = sessions.subscribe_current().await;

        loop {
            // Mark the current session as seen before subscribing to it.
            current.borrow_and_update();

            // Give the connection some time to populate.
            match sessions.wait_current_dom_populated(Duration::from_secs(10)).await {
                Some(c) => info!("client initialised verde with {c} nodes"),
                None => info!("client could not be initialised?")
            };

            // The current session's roots snapshot and change feed.
            let Some((_id, mut changed, snapshot)) =
                sessions.subscribe_current_dom(serialize::roots).await
            else {
                // Session store dropped, application shutting down?
                if current.changed().await.is_err() {
                    return Ok(());
                }
                continue;
            };
            
            // Grab the current session's mirror.
            let Some(mirror) = ({
                sessions
                    .read()
                    .await
                    .current()
                    .map(|session| Arc::clone(&session.mirror))
            }) else {
                if current.changed().await.is_err() {
                    return Ok(());
                }
                continue;
            };
            
            // Serve this connection until it switches or the socket dies.
            conn.greet(snapshot).await?;
            match conn
                .serve(&mirror, &mut changed, &mut current, &mut read)
                .await?
            {
                Served::Switched => continue,
                Served::Closed => return Ok(()),
            }
        }
    }
}
