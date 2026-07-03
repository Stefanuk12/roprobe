use std::io::Write as _;

use tracing::info;

use crate::{
    HandshakeFormat, RunArgs, commands::CommandResult, lockfile::Lockfile, server, upstream,
};

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

    // Keep (re)connecting to the local tools we broker for, in the background,
    // while at least one client session is active.
    let controls = upstream::Controls::new(!args.no_verde, !args.no_luau_lsp);
    let verde = tokio::spawn(upstream::verde::maintain(
        args.verde_port,
        controls.subscribe(upstream::Upstream::Verde),
    ));
    let luau_lsp = tokio::spawn(upstream::luau_lsp::maintain(
        args.luau_lsp_port,
        controls.subscribe(upstream::Upstream::LuauLsp),
    ));

    // Start the server
    info!("listening on ws://127.0.0.1:{port}");
    server::run(listener, lockfile.handshake.token.clone(), controls).await;

    // Shutdown the server once closed
    verde.abort();
    luau_lsp.abort();
    lockfile.remove();
    info!("shut down");
    Ok(())
}
