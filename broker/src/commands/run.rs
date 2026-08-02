use std::{io::Write as _, sync::Arc};

use tokio::sync::Notify;
use tracing::info;

use crate::{
    Context, Error, HandshakeFormat, RunArgs,
    commands::{CommandResult, live_broker},
    lockfile::Lockfile,
    protocol::Handshake,
    server::Server,
    upstream::{self, verde::VerdeConnector},
};

pub async fn run(args: RunArgs, handshake_format: Option<HandshakeFormat>) -> CommandResult {
    // A broker is already up
    if let Some(live) = live_broker().await {
        let handshake = &live.handshake;
        let is_pinned_elsewhere = (args.port != 0 && args.port != handshake.port)
            || args.token.is_some_and(|token| token != handshake.token);

        if is_pinned_elsewhere {
            return Err(Error::BrokerAlreadyRunning {
                port: handshake.port,
                pid: live.pid,
            });
        }

        if matches!(handshake_format, Some(HandshakeFormat::Stdout)) {
            println!("{}", handshake.to_line());
            let _ = std::io::stdout().flush();
        }

        info!(pid = live.pid, port = handshake.port, "broker already running, using it");
        return Ok(());
    }

    // Attempt to start the server
    let listener = Server::bind(args.port).await?;
    let port = listener.local_addr()?.port();

    // Initialise the lockfile, pinning a fixed token when one was supplied.
    let lockfile = match args.token.clone() {
        Some(token) => Lockfile::from(Handshake { port, token }),
        None => Lockfile::new(port),
    };
    let path = lockfile.write()?;
    info!("lockfile written to {}", path.display());

    if matches!(handshake_format, Some(HandshakeFormat::Stdout)) {
        println!("{}", lockfile.handshake.to_line());
        let _ = std::io::stdout().flush();
    }

    // Initialise the main context
    let controls = upstream::Controls::new(!args.no_verde, !args.no_luau_lsp);
    let shutdown = Arc::new(Notify::new());
    let lockfile = Arc::new(lockfile);
    let ctx = Context::new(
        Default::default(),
        Arc::clone(&lockfile),
        controls.clone(),
        shutdown,
    );

    // Pin the property write-security threshold verde's inspector will apply.
    // upstream::verde::set_security_level(args.security_level.ordinal());

    // Maintain the connections to the extensions.
    let verde = tokio::spawn(VerdeConnector::new(ctx.clone(), args.verde_port).start());
    // let luau_lsp = tokio::spawn(upstream::luau_lsp::maintain(
    //     args.luau_lsp_port,
    //      ctx.controls.subscribe(upstream::Upstream::LuauLsp),
    //     Arc::clone(&mirror),
    // ));

    // Start the server
    info!("listening on ws://127.0.0.1:{port}");
    let server = Server::new(ctx, args.security_level.ordinal());
    server.run(listener).await;

    // Shutdown the server once closed
    verde.abort();
    // luau_lsp.abort();
    lockfile.remove(); // Stop advertising a broker that is going away
    info!("shut down");
    Ok(())
}
