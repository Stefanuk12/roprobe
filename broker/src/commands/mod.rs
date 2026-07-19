mod run;
mod sessions;
mod status;
mod stop;
mod swap;

use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::{Cli, Command, Error, Result, RunArgs, lockfile::Lockfile, server::SessionId};

pub type CommandResult = Result<()>;

/// Run the correct command based upon the args given to the executable.
pub async fn dispatch(cli: Cli) -> CommandResult {
    match cli.command {
        Some(Command::Run(args)) => run::run(args, cli.handshake).await,
        None => run::run(RunArgs::default(), cli.handshake).await,
        Some(Command::Status) => status::status(),
        Some(Command::Stop(args)) => stop::stop(args).await,
        Some(Command::Sessions) => sessions::sessions().await,
        Some(Command::Swap(args)) => swap::swap(SessionId(args.id)).await,
    }
}

/// Connect to the broker, if it's up.
pub async fn connect_broker() -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    // Resolve the lockfile which contains the current broker session
    let lockfile = Lockfile::read().ok_or(Error::LockfileNotFound)?;
    let port = lockfile.handshake.port;

    // Connect to the current broker
    let url = format!(
        "ws://127.0.0.1:{port}/?token={}&control=1",
        lockfile.handshake.token
    );
    let (ws, _) = connect_async(url.as_str()).await.map_err(|e| {
        tokio::io::Error::other(format!("could not reach broker on port {port} ({e})"))
    })?;

    Ok(ws)
}
