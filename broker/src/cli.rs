use clap::{Args, Parser, Subcommand, ValueEnum};

/// roprobe broker - brokers traffic between Roblox, and the extensions / other clients.
#[derive(Debug, Parser)]
#[command(name = "broker", version, about, long_about = None)]
pub struct Cli {
    #[arg(long, global = true, value_name = "FORMAT")]
    pub handshake: Option<HandshakeFormat>,

    /// Increase log verbosity: `-v` for debug, `-vv` for trace.
    /// `RUST_LOG`, when set, overrides this.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the broker daemon.
    /// This is the default when no subcommand is given.
    Run(RunArgs),
    /// Print details of the currently running broker, if there is one.
    Status,
    /// Stop a running broker.
    Stop(StopArgs),
}

#[derive(Debug, Default, Args)]
pub struct RunArgs {
    /// Port to bind on loopback.
    /// `0`` (the default) lets the OS pick a free port.
    #[arg(long, default_value_t = 0)]
    pub port: u16,
}

#[derive(Debug, Default, Args)]
pub struct StopArgs {
    /// Kill the broker process by PID instead of asking it to shut down
    /// gracefully over its socket. Use this if the broker is unresponsive.
    #[arg(long)]
    pub force: bool,
}

/// How (and whether) to surface the handshake to the spawning process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HandshakeFormat {
    /// Print the handshake JSON line to stdout.
    Stdout,
}
