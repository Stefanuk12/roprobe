use std::io::Write as _;

use tracing::info;

use crate::{HandshakeFormat, RunArgs, commands::CommandResult, lockfile::Lockfile, server};

pub async fn run(args: RunArgs, handshake_format: Option<HandshakeFormat>) -> CommandResult {
    // Attempt to start the server
    let listener = server::bind(args.port).await?;
    let port = listener.local_addr()?.port();

    // Initialise the lockfile
    let lockfile = Lockfile::new(port);
    let path = lockfile.write()?;
    info!("lockfile written to {}", path.display());

    if matches!(handshake_format, Some(HandshakeFormat::Stdout)) {
        println!("{}", lockfile.handshake.to_line());
        let _ = std::io::stdout().flush();
    }

    // Start the server
    info!("listening on ws://127.0.0.1:{port}");
    server::run(listener, lockfile.handshake.token.clone()).await;

    // Shutdown the server once closed
    lockfile.remove();
    info!("shut down");
    Ok(())
}
