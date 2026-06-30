mod run;
mod status;
mod stop;

use crate::{Cli, Command, Result, RunArgs};

pub type CommandResult = Result<()>;

/// Run the correct command based upon the args given to the executable.
pub async fn dispatch(cli: Cli) -> CommandResult {
    match cli.command {
        Some(Command::Run(args)) => run::run(args, cli.handshake).await,
        None => run::run(RunArgs::default(), cli.handshake).await,
        Some(Command::Status) => status::status(),
        Some(Command::Stop(args)) => stop::stop(args).await,
    }
}
