mod app;
mod client;
mod commands;
mod context;
mod error;
mod output;

use clap::Parser;

pub use error::{CliError, CliResult};

use app::{Cli, Command};
use context::CliContext;

pub async fn run() -> CliResult<()> {
    let cli = Cli::parse();
    initialize_tracing(cli.verbose);
    let context = CliContext {
        config_path: cli.config,
        json: cli.json,
    };
    match cli.command {
        Command::Node { command } => commands::node::run(&context, command).await,
        Command::Chain { command } => commands::chain::run(&context, command).await,
        Command::Block { command } => commands::block::run(&context, command).await,
        Command::Transaction { command } => commands::transaction::run(&context, command).await,
        Command::Record { command } => commands::record::run(&context, command).await,
        Command::Validator { command } => commands::validator::run(&context, command).await,
        Command::Network { command } => commands::network::run(&context, command).await,
        Command::Storage { command } => commands::storage::run(&context, command).await,
        Command::Snapshot { command } => commands::snapshot::run(&context, command).await,
        Command::Diagnostics => commands::diagnostics::run(&context).await,
        Command::Config { command } => commands::config::run(&context, command).await,
        Command::Demo { command } => commands::demo::run(&context, command).await,
        Command::Dev { command } => commands::dev::run(&context, command).await,
        Command::Identity { command } => commands::identity::run(&context, command).await,
    }
}

fn initialize_tracing(verbosity: u8) {
    let default = match verbosity {
        0 => "minichain=warn",
        1 => "minichain=info",
        _ => "minichain=debug",
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default)),
        )
        .with_target(false)
        .try_init();
}
