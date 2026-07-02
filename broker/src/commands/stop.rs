use std::{io, process::Command, time::Duration};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tracing::{info, warn};

use crate::{
    Error, StopArgs, commands::CommandResult, lockfile::Lockfile, protocol::ClientMessage,
};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn stop(args: StopArgs) -> CommandResult {
    let lockfile = Lockfile::read().ok_or(Error::LockfileNotFound)?;

    // Terminate the process, if forced
    if args.force {
        terminate(lockfile.pid)?;
        lockfile.remove();
        info!(pid = lockfile.pid, "killed broker by pid");
        return Ok(());
    }

    // Try to connect to the server
    let port = lockfile.handshake.port;
    let url = format!("ws://127.0.0.1:{port}/?token={}", lockfile.handshake.token);
    let (mut ws, _) = connect_async(url.as_str()).await.map_err(|e| {
        io::Error::other(format!(
            "could not reach broker on port {port} ({e}); use `--force` to kill pid {}",
            lockfile.pid
        ))
    })?;

    // ... and request a shutdown
    ws.send(ClientMessage::Shutdown.try_into()?)
        .await
        .map_err(io::Error::other)?;

    // Wait for confirmation
    let drained = tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
        while ws.next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        warn!("broker did not close the connection in time; it may still be shutting down");
    }

    lockfile.remove();
    info!(pid = lockfile.pid, "asked broker to shut down");
    Ok(())
}

#[cfg(unix)]
fn terminate(pid: u32) -> io::Result<()> {
    check(Command::new("kill").arg(pid.to_string()).status()?)
}

#[cfg(windows)]
fn terminate(pid: u32) -> io::Result<()> {
    check(
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()?,
    )
}

fn check(status: std::process::ExitStatus) -> io::Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("terminate exited with {status}")))
    }
}
