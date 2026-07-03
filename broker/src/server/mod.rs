use std::{net::SocketAddr, sync::Arc};

use futures_util::{StreamExt, stream::SplitSink};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Notify,
};
use tokio_tungstenite::{
    WebSocketStream, accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{ErrorResponse, Request, Response},
        http::{Response as HttpResponse, StatusCode},
    },
};
use tracing::{debug, error, info, warn};

use crate::{
    protocol::{ClientMessage, ServerMessage},
    upstream::Controls,
};

import!(session);
pub type WsWrite = SplitSink<WebSocketStream<TcpStream>, Message>;

/// Constantly accept connections until we're forced to exit.
pub async fn run(listener: TcpListener, token: String, controls: Controls) {
    let token = Arc::new(token);
    let shutdown = Arc::new(Notify::new());
    let controls = Arc::new(controls);

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) => {
                        let token = Arc::clone(&token);
                        let shutdown = Arc::clone(&shutdown);
                        let controls = Arc::clone(&controls);

                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, peer, token, shutdown, controls).await {
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

/// Bind on loopback only. Port 0 lets the OS assign a free port.
pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port)).await
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    token: Arc<String>,
    shutdown: Arc<Notify>,
    controls: Arc<Controls>,
) -> crate::Result<()> {
    // Verify the auth token, otherwise reject the request.
    let callback = move |req: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let authorized = req
            .uri()
            .query()
            .map(|query| query_has_token(query, token.as_str()))
            .unwrap_or(false);

        if !authorized {
            let denied = HttpResponse::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Some("invalid or missing token".to_string()))
                .expect("build unauthorized response");
            return Err(denied)
        }

        Ok(response)
    };

    // Accept the connection
    let ws = accept_hdr_async(stream, callback).await?;
    info!(%peer, "client connected");

    // Upstream connections only run while at least one session is counted.
    let _session = controls.track_session();

    let (write, mut read) = ws.split();
    let mut session = Session::new(peer, write, shutdown, Arc::clone(&controls));
    session.send(ServerMessage::Hello).await?;

    // Constantly listen for new messages and handle them.
    while let Some(msg) = read.next().await {
        match msg? {
            Message::Binary(bytes) => match ClientMessage::from_bytes(bytes) {
                Ok(message) => session.handle(message).await?,
                Err(e) => debug!(%peer, "undecodable binary frame: {e}"),
            },
            Message::Text(text) => debug!(%peer, "text frame: {text}"),
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
