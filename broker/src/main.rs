use std::process::ExitCode;

use broker::{Cli, commands, logging};
use clap::Parser;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(cli.verbose);

    if let Err(e) = commands::dispatch(cli).await {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
