mod run;
mod security;
mod sessions;
mod status;
mod stop;
mod swap;

use std::time::Duration;

use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::{Cli, Command, Error, Result, RunArgs, lockfile::Lockfile, server::SessionId};

pub type CommandResult = Result<()>;

/// How long a lockfile's broker gets to answer before it counts as dead.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Run the correct command based upon the args given to the executable.
pub async fn dispatch(cli: Cli) -> CommandResult {
    match cli.command {
        Some(Command::Run(args)) => run::run(args, cli.handshake).await,
        None => run::run(RunArgs::default(), cli.handshake).await,
        Some(Command::Status) => status::status(),
        Some(Command::Stop(args)) => stop::stop(args).await,
        Some(Command::Sessions) => sessions::sessions().await,
        Some(Command::Swap(args)) => swap::swap(SessionId(args.id)).await,
        Some(Command::Security(args)) => security::security(SessionId(args.id), args.level).await,
    }
}

/// Open a control connection to the broker the `lockfile` advertises.
async fn connect_control(
    lockfile: &Lockfile,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    let port = lockfile.handshake.port;
    let url = format!(
        "ws://127.0.0.1:{port}/?token={}&control=1",
        lockfile.handshake.token
    );
    let (ws, _) = connect_async(url.as_str()).await.map_err(|e| {
        tokio::io::Error::other(format!("could not reach broker on port {port} ({e})"))
    })?;

    Ok(ws)
}

/// Connect to the broker, if it's up.
pub async fn connect_broker() -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    let lockfile = Lockfile::read().ok_or(Error::LockfileNotFound)?;
    connect_control(&lockfile).await
}

/// The lockfile of a broker that is up and answering, clearing one a dead broker left behind.
pub async fn live_broker() -> Option<Lockfile> {
    let lockfile = Lockfile::read()?;

    match tokio::time::timeout(PROBE_TIMEOUT, connect_control(&lockfile)).await {
        Ok(Ok(mut ws)) => {
            let _ = ws.close(None).await;
            Some(lockfile)
        }
        _ => {
            lockfile.remove();
            None
        }
    }
}
