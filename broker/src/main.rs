use broker::{Result, Cli, commands, logging};
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    logging::init(cli.verbose);
    commands::dispatch(cli).await
}
