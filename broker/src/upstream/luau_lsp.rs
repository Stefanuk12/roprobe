use std::time::Duration;

use tokio::{net::TcpStream, sync::watch};
use tracing::{info, warn};

pub const DEFAULT_PORT: u16 = 3667;
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

/// Run the probe loop while the switch is on.
pub async fn maintain(port: u16, mut enabled: watch::Receiver<bool>) {
    loop {
        if !*enabled.borrow_and_update() {
            info!("luau-lsp probing paused");
            if enabled.wait_for(|on| *on).await.is_err() {
                return;
            }
            info!("luau-lsp probing resumed");
        }

        tokio::select! {
            _ = probe_loop(port) => {}
            result = enabled.wait_for(|on| !*on) => {
                if result.is_err() {
                    return;
                }
            }
        }
    }
}

/// Watch for the luau-lsp studio plugin server.
async fn probe_loop(port: u16) {
    let mut reachable = false;
    let mut warned = false;

    loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(_) => {
                if !reachable {
                    info!("luau-lsp reachable on :{port}");
                }
                reachable = true;
                warned = false;
            }
            Err(e) => {
                if reachable {
                    warn!("luau-lsp connection lost on :{port}, retrying");
                } else if !warned {
                    warn!(
                        "luau-lsp not reachable on :{port} ({e}), retrying every {}s",
                        PROBE_INTERVAL.as_secs()
                    );
                    warned = true;
                }
                reachable = false;
            }
        }

        tokio::time::sleep(PROBE_INTERVAL).await;
    }
}
