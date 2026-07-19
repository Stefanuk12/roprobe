use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_tungstenite::{
    WebSocketStream, accept_hdr_async, tungstenite::{
        Bytes, Message, handshake::server::{ErrorResponse, Request, Response}, http::{Response as HttpResponse, StatusCode},
    },
};
use tracing::{error, info, warn};

use crate::{
    Context, Result,
    protocol::{ClientMessage, ServerMessage, text_decode},
};

pub mod manager;
import!(dom, mirror, session);

pub type WsWrite = SplitSink<WebSocketStream<TcpStream>, Message>;

enum ReadNext {
    ClientMessage(ClientMessage),
    Ping(Bytes),
    Close,
    Noop,
}

async fn read_next(
    peer: SocketAddr,
    read: &mut SplitStream<WebSocketStream<TcpStream>>,
) -> Result<ReadNext> {
    let Some(frame) = read.next().await else {
        info!(%peer, "client stream ended (tcp eof)");
        return Ok(ReadNext::Close);
    };
    let frame = frame.inspect_err(|e| warn!(%peer, "read error: {e}"))?;
    match frame {
        Message::Binary(bytes) => match ClientMessage::from_bytes(bytes) {
            Ok(message) => return Ok(ReadNext::ClientMessage(message)),
            Err(e) => {
                warn!(%peer, "undecodable binary frame: {e}");
                Err(e)?
            }
        },
        Message::Text(text) => match text_decode(text.as_str()) {
            Some(bytes) => match ClientMessage::from_bytes(bytes) {
                Ok(message) => return Ok(ReadNext::ClientMessage(message)),
                Err(e) => {
                    warn!(%peer, "undecodable client frame: {e}");
                    Ok(ReadNext::Noop)
                }
            },
            None => {
                warn!(%peer, "client sent a non-base64 text frame");
                Ok(ReadNext::Noop)
            }
        },
        Message::Close(reason) => {
            info!(%peer, ?reason, "client sent close frame");
            Ok(ReadNext::Close)
        }
        Message::Ping(data) => {
            info!(%peer, "ping received, sending pong");
            Ok(ReadNext::Ping(data))
        }
        Message::Pong(_) => {
            info!(%peer, "pong received");
            Ok(ReadNext::Noop)
        }
        other => {
            info!(%peer, kind = ?std::mem::discriminant(&other), "other frame received");
            Ok(ReadNext::Noop)
        }
    }
}

/// The main server that drives everything.
pub struct Server {
    ctx: Context,
    default_security_level: u8,
}

impl Server {
    pub fn new(ctx: Context, default_security_level: u8) -> Self {
        Self {
            ctx,
            default_security_level,
        }
    }

    /// Bind on loopback only, port `0` letting the OS assign a free port.
    pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
        TcpListener::bind(("0.0.0.0", port)).await
    }

    /// Constantly accept connections until we're forced to exit.
    pub async fn run(self, listener: TcpListener) {
        let server = Arc::new(self);

        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer)) => {
                            let server = Arc::clone(&server);
                            tokio::spawn(async move {
                                if let Err(e) = server.handle_connection(stream, peer).await {
                                    warn!(%peer, "connection ended: {e}");
                                }
                            });
                        }
                        Err(e) => error!("accept error: {e}"),
                    }
                }
                _ = Self::shutdown_signal() => {
                    info!("shutdown signal received");
                    break;
                }
                _ = server.ctx.shutdown.notified() => {
                    info!("shutdown requested by client");
                    break;
                }
            }
        }
    }

    async fn handle_connection(&self, stream: TcpStream, peer: SocketAddr) -> crate::Result<()> {
        // A control connection (the `swap`/`sessions`/`stop` commands) authenticates
        // like any other but doesn't sync a DataModel, so it stays out of the gate.
        let control = Arc::new(AtomicBool::new(false));

        // Verify the auth token, otherwise reject the request.
        let callback = {
            let token = Arc::new(self.ctx.lockfile.handshake.token.as_str());
            let control = Arc::clone(&control);
            move |req: &Request, response: Response| -> Result<Response, ErrorResponse> {
                let query = req.uri().query().unwrap_or_default();

                if !Self::query_has_token(query, &token) {
                    let denied = HttpResponse::builder()
                        .status(StatusCode::UNAUTHORIZED)
                        .body(Some("invalid or missing token".to_string()))
                        .expect("build unauthorized response");
                    return Err(denied);
                }

                if Self::query_is_control(query) {
                    control.store(true, Ordering::Relaxed);
                }

                Ok(response)
            }
        };

        // Accept the connection
        let ws = accept_hdr_async(stream, callback).await?;

        // Specialised message handler for control connections
        if control.load(Ordering::Relaxed) {
            info!(%peer, "control connection");
            return self.handle_control_connection(ws, peer).await;
        }

        // Non-control after this
        info!(%peer, "client connected");

        // Initialise the mirror
        let (op_tx, mut op_rx) = mpsc::channel::<OpRequest>(32);
        let (dom_tx, mut dom_rx) = mpsc::channel::<DomRequest>(32);

        let mirror = Mirror::new();
        mirror.install_op_sink(1, op_tx.clone());
        mirror.install_dom_sink(1, dom_tx.clone());

        // Create and add the session
        let (write, mut read) = ws.split();
        let id = SessionId(rand::random());
        let mut session = Session::new(
            self.ctx.clone(),
            peer,
            write,
            mirror,
            self.default_security_level,
            id
        );
        session.send(ServerMessage::Hello).await?;
        self.ctx.sessions.write().await.insert(session);
        let session_guard = self.ctx.controls.track_session();

        // Serve client frames and upstream operation relays until the socket closes.
        let outcome = loop {
            tokio::select! {
                frame = read_next(peer, &mut read) => {
                    match frame {
                        Err(e) => break Err(e),
                        Ok(ReadNext::Noop) => continue,
                        Ok(ReadNext::Close) => break Ok(()),
                        Ok(ReadNext::Ping(data)) => {
                            let mut guard = self.ctx.sessions.write().await;
                            let Some(session) = guard.find_mut(id) else {
                                break Ok(())
                            };

                            let _ = session.send_frame(Message::Pong(data)).await;
                        }
                        Ok(ReadNext::ClientMessage(message)) => {
                            let mut guard = self.ctx.sessions.write().await;
                            let Some(session) = guard.find_mut(id) else {
                                break Ok(())
                            };

                            if let Err(e) = session.handle(message).await {
                                break Err(e);
                            }
                        }
                    }
                }

                Some(request) = op_rx.recv() => {
                    let mut guard = self.ctx.sessions.write().await;
                    let Some(session) = guard.find_mut(id) else {
                        break Ok(())
                    };

                    if let Err(e) = session.dispatch_operation(request).await {
                        break Err(e);
                    }
                }
                Some(request) = dom_rx.recv() => {
                    let mut guard = self.ctx.sessions.write().await;
                    let Some(session) = guard.find_mut(id) else {
                        break Ok(())
                    };

                    if let Err(e) = session.handle_dom_request(request).await {
                        break Err(e);
                    }
                }
            }
        };

        // Remove the client
        self.ctx.sessions.write().await.remove(id);
        drop(session_guard);
        info!(%peer, id = id.0, "client disconnected");
        outcome
    }

    /// Serve a control connection (`swap`/`sessions`/`stop`): it never syncs a
    /// DataModel, so it stays out of the gate and just switches or lists sessions.
    async fn handle_control_connection(
        &self,
        ws: WebSocketStream<TcpStream>,
        peer: SocketAddr,
    ) -> crate::Result<()> {
        let (mut write, mut read) = ws.split();
        while let Ok(message) = read_next(peer, &mut read).await {
            match message {
                ReadNext::Noop => continue,
                ReadNext::Close => break,
                ReadNext::ClientMessage(ClientMessage::Shutdown) => {
                    info!(%peer, "control requested shutdown");
                    self.ctx.shutdown.notify_one();
                    break;
                }
                ReadNext::ClientMessage(ClientMessage::SwapActive(id)) => {
                    let outcome = match self.ctx.sessions.set_current(Some(id)).await {
                        Ok(Some(id)) => Ok(Some(id.0)),
                        Ok(None) => Ok(None::<u32>),
                        Err(e) => Err(e)
                    };
                    let id = id.0;
                    info!(%peer, id, ?outcome, "control swap");
                }
                ReadNext::ClientMessage(ClientMessage::ListSessions) => {
                    let sessions = self.ctx.sessions.holder.read().await.list();
                    write
                        .send(ServerMessage::Sessions(sessions).try_into()?)
                        .await?;
                }
                ReadNext::Ping(data) => {
                    write.send(Message::Pong(data)).await?;
                }
                ReadNext::ClientMessage(other) => {
                    warn!(%peer, kind = other.kind(), "unexpected message on a control connection")
                }
            }
        }

        info!(%peer, "control connection closed");
        Ok(())
    }

    /// Make sure the request is authenticated / token matches.
    fn query_has_token(query: &str, token: &str) -> bool {
        query.split('&').any(|pair| {
            let mut parts = pair.splitn(2, '=');
            matches!((parts.next(), parts.next()), (Some("token"), Some(value)) if value == token)
        })
    }

    /// Whether the request carries `control=1`, marking a control connection.
    fn query_is_control(query: &str) -> bool {
        query.split('&').any(|pair| {
            let mut parts = pair.splitn(2, '=');
            matches!((parts.next(), parts.next()), (Some("control"), Some("1")))
        })
    }

    /// Listen for when the user wants to shut down.
    async fn shutdown_signal() {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            if let Ok(mut term) = signal(SignalKind::terminate()) {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
                return;
            }
        }
        let _ = tokio::signal::ctrl_c().await;
    }
}
