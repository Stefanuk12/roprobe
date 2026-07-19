use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::upstream;

/// roprobe broker - brokers traffic between Roblox, and the extensions / other clients.
#[derive(Debug, Parser)]
#[command(name = "broker", version, about, long_about = None)]
pub struct Cli {
    #[arg(long, global = true, value_name = "FORMAT")]
    pub handshake: Option<HandshakeFormat>,

    /// Increase log verbosity (`-v` debug, `-vv` trace), overridden by `RUST_LOG`.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the broker daemon (the default when no subcommand is given).
    Run(RunArgs),
    /// Print details of the currently running broker, if there is one.
    Status,
    /// Stop a running broker.
    Stop(StopArgs),
    /// List the connected client sessions and which one is syncing.
    Sessions,
    /// Switch which connected client is syncing (find ids with `sessions`).
    Swap(SwapArgs),
}

#[derive(Debug, Args)]
pub struct SwapArgs {
    /// Id of the session to make active (from `sessions`).
    pub id: u32,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Port to bind on loopback; `0` (the default) lets the OS pick a free port.
    #[arg(long, default_value_t = 0)]
    pub port: u16,

    /// Port of verde's WebSocket server to (re)connect to.
    #[arg(long, default_value_t = upstream::verde::DEFAULT_PORT)]
    pub verde_port: u16,

    /// Port of the luau-lsp studio plugin server to (re)connect to.
    #[arg(long, default_value_t = upstream::luau_lsp::DEFAULT_PORT)]
    pub luau_lsp_port: u16,

    /// Fixed auth token (a fresh 256-bit one is minted when omitted); pin it for testing so restarts don't invalidate a connected client's token.
    #[arg(long)]
    pub token: Option<String>,

    /// Highest property write-security tier the inspector shows; raise it to expose Roblox-internal properties (e.g. `Player.User` at `roblox`).
    #[arg(long, value_enum, default_value_t = SecurityLevel::LocalUser)]
    pub security_level: SecurityLevel,

    /// Start with the verde connection disabled (clients can still enable it at runtime).
    #[arg(long)]
    pub no_verde: bool,

    /// Start with luau-lsp studio plugin server probing disabled (clients can still enable it at runtime).
    #[arg(long)]
    pub no_luau_lsp: bool,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            port: 0,
            verde_port: upstream::verde::DEFAULT_PORT,
            luau_lsp_port: upstream::luau_lsp::DEFAULT_PORT,
            token: None,
            security_level: SecurityLevel::LocalUser,
            no_verde: false,
            no_luau_lsp: false,
        }
    }
}

#[derive(Debug, Default, Args)]
pub struct StopArgs {
    /// Kill the broker process by PID instead of asking it to shut down gracefully over its socket, for when the broker is unresponsive.
    #[arg(long)]
    pub force: bool,
}

/// How (and whether) to surface the handshake to the spawning process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HandshakeFormat {
    /// Print the handshake JSON line to stdout.
    Stdout,
}

/// The highest property write-security tier the inspector may show; higher tiers expose more Roblox-internal properties (e.g. `Player.User` at `roblox`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum SecurityLevel {
    /// Only freely-writable properties.
    None,
    /// ...plus plugin-writable ones.
    Plugin,
    /// ...plus local-user-writable ones (the default).
    #[default]
    LocalUser,
    /// ...plus Roblox-script-writable ones.
    RobloxScript,
    /// ...plus Roblox-only-writable internals (e.g. `Player.User`).
    Roblox,
}

impl SecurityLevel {
    /// The write-security ordinal this tier admits, matching the dump's encoding.
    pub fn ordinal(self) -> u8 {
        match self {
            SecurityLevel::None => 0,
            SecurityLevel::Plugin => 1,
            SecurityLevel::LocalUser => 2,
            SecurityLevel::RobloxScript => 3,
            SecurityLevel::Roblox => 4,
        }
    }
}
