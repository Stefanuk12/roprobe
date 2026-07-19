use std::io::IsTerminal as _;

use tracing_subscriber::{EnvFilter, fmt};

/// Initialises the console logger.
pub fn init(verbose: u8) {
    let default_level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("broker={default_level},warn")));

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_target(false)
        .init();
}
