//! The local WebSocket listener that brokers connections between the Roblox
//! client and editor-side clients.

use std::{net::SocketAddr, sync::Arc};

use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Notify,
};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{ErrorResponse, Request, Response},
        http::{Response as HttpResponse, StatusCode},
    },
};
use tracing::{debug, error, info, warn};

use crate::protocol::ClientMessage;

/// Bind on loopback only. Port 0 lets the OS assign a free port.
pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port)).await
}

/// Constantly accept connections until we're forced to exit.
pub async fn run(listener: TcpListener, token: String) {
    let token = Arc::new(token);
    let shutdown = Arc::new(Notify::new());

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) => {
                        let token = Arc::clone(&token);
                        let shutdown = Arc::clone(&shutdown);

                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, peer, token, shutdown).await {
                                warn!(%peer, "connection ended: {e}");
                            }
                        });
                    }
                    Err(e) => error!("accept error: {e}"),
                }
            }
            _ = shutdown_signal() => {
                info!("shutdown signal received");
                break;
            }
            _ = shutdown.notified() => {
                info!("shutdown requested by client");
                break;
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    token: Arc<String>,
    shutdown: Arc<Notify>,
) -> crate::Result<()> {
    // Verify the auth token, otherwise reject the request.
    let callback = move |req: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let authorized = req
            .uri()
            .query()
            .map(|query| query_has_token(query, token.as_str()))
            .unwrap_or(false);

        if authorized {
            Ok(response)
        } else {
            let denied = HttpResponse::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Some("invalid or missing token".to_string()))
                .expect("build unauthorized response");
            Err(denied)
        }
    };

    // Accept the connection
    let ws = accept_hdr_async(stream, callback).await?;
    info!(%peer, "client connected");

    let (mut write, mut read) = ws.split();
    write
        .send(Message::Text(
            "{\"type\":\"hello\",\"role\":\"broker\"}".into(),
        ))
        .await?;

    // Minimal loop for now: log frames so connectivity is observable. Real
    // routing (game <-> editor clients) lands when the protocol is wired up.
    while let Some(msg) = read.next().await {
        match msg? {
            Message::Text(text) => match ClientMessage::from_json(&text) {
                Ok(ClientMessage::Shutdown) => {
                    info!(%peer, "client requested shutdown");
                    shutdown.notify_one();
                    break;
                }
                // Ok(message) => debug!(%peer, kind = message.kind(), "client message"),
                Err(_) => debug!(%peer, "text frame: {text}"),
            },
            Message::Binary(bytes) => debug!(%peer, bytes = bytes.len(), "binary frame"),
            Message::Close(_) => break,
            _ => {}
        }
    }

    info!(%peer, "client disconnected");
    Ok(())
}

/// Make sure the request is authenticated / token matches.
fn query_has_token(query: &str, token: &str) -> bool {
    query.split('&').any(|pair| {
        let mut parts = pair.splitn(2, '=');
        matches!((parts.next(), parts.next()), (Some("token"), Some(value)) if value == token)
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
