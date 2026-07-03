use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

pub const DEFAULT_PORT: u16 = 9000;
const RETRY_DELAY: Duration = Duration::from_secs(2);

/// Run the connection loop while the switch is on.
pub async fn maintain(port: u16, mut enabled: watch::Receiver<bool>) {
    loop {
        if !*enabled.borrow_and_update() {
            info!("verde connection paused");
            if enabled.wait_for(|on| *on).await.is_err() {
                return;
            }
            info!("verde connection resumed");
        }

        tokio::select! {
            _ = connect_loop(port) => {}
            result = enabled.wait_for(|on| !*on) => {
                if result.is_err() {
                    return;
                }
            }
        }
    }
}

/// Keep a WebSocket connection to verde open.
async fn connect_loop(port: u16) {
    let url = format!("ws://127.0.0.1:{port}");
    let mut warned = false;

    loop {
        match connect_async(url.as_str()).await {
            Ok((mut ws, _)) => {
                info!("connected to verde on :{port}");
                warned = false;

                while let Some(msg) = ws.next().await {
                    match msg {
                        Ok(Message::Ping(data)) => {
                            if ws.send(Message::Pong(data)).await.is_err() {
                                break;
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(msg) => debug!("verde message: {msg:?}"),
                        Err(e) => {
                            debug!("verde read error: {e}");
                            break;
                        }
                    }
                }

                warn!("verde connection lost, reconnecting");
            }
            Err(e) => {
                if !warned {
                    warn!(
                        "verde not reachable on :{port} ({e}), retrying every {}s",
                        RETRY_DELAY.as_secs()
                    );
                    warned = true;
                }
            }
        }

        tokio::time::sleep(RETRY_DELAY).await;
    }
}
