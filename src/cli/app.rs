use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "minichain",
    version,
    about = "Operate and inspect a MiniChain permissioned blockchain node",
    long_about = None
)]
pub struct Cli {
    #[arg(short, long, global = true, default_value = "config/node-01.toml")]
    pub config: PathBuf,
    #[arg(long, global = true, help = "Emit machine-readable JSON")]
    pub json: bool,
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start or inspect a node process.
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    /// Inspect and validate the blockchain.
    Chain {
        #[command(subcommand)]
        command: ChainCommand,
    },
    /// Inspect blocks by height or hash.
    Block {
        #[command(subcommand)]
        command: BlockCommand,
    },
    /// Submit and inspect signed transactions.
    Transaction {
        #[command(subcommand)]
        command: TransactionCommand,
    },
    /// Create, revoke, inspect, and verify records.
    Record {
        #[command(subcommand)]
        command: RecordCommand,
    },
    /// Inspect the validator registry.
    Validator {
        #[command(subcommand)]
        command: ValidatorCommand,
    },
    /// Inspect peers and operate trusted-peer synchronization.
    Network {
        #[command(subcommand)]
        command: NetworkCommand,
    },
    /// Inspect or repair offline local storage.
    Storage {
        #[command(subcommand)]
        command: StorageCommand,
    },
    /// Create, inspect, verify, and restore snapshots.
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    /// Run a multi-subsystem node health check.
    Diagnostics,
    /// Display or validate redacted configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Run safe demonstrations in temporary state.
    Demo {
        #[command(subcommand)]
        command: DemoCommand,
    },
    /// Run developer tooling.
    Dev {
        #[command(subcommand)]
        command: DevCommand,
    },
    /// Create or inspect a node identity without printing its secret.
    Identity {
        #[command(subcommand)]
        command: IdentityCommand,
    },
}

#[derive(Subcommand)]
pub enum NodeCommand {
    /// Start P2P and HTTP listeners and run until Ctrl-C.
    Start,
    /// Show live node health and network status.
    Status,
    /// Show live node and configured endpoint information.
    Info,
}

#[derive(Subcommand)]
pub enum ChainCommand {
    Status,
    Validate,
    Height,
    Head,
    Consistency,
}

#[derive(Subcommand)]
pub enum BlockCommand {
    List {
        #[arg(long, default_value_t = 0)]
        from: u64,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(1..=100))]
        limit: u64,
    },
    Show {
        height: u64,
    },
    Hash {
        hash: String,
    },
    Latest,
    Verify {
        height: u64,
    },
}

#[derive(Subcommand)]
pub enum TransactionCommand {
    /// Submit a complete signed transaction JSON file.
    Submit {
        file: PathBuf,
    },
    Show {
        id: String,
    },
    Verify {
        id: String,
    },
    Status {
        id: String,
    },
    List {
        #[arg(long, default_value_t = 1)]
        page: u64,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(1..=100))]
        limit: u64,
        #[arg(long)]
        operation: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum RecordCommand {
    /// Sign and submit a CREATE_RECORD transaction from a JSON payload file.
    Create {
        id: String,
        data: PathBuf,
    },
    Get {
        id: String,
    },
    Verify {
        id: String,
    },
    History {
        id: String,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(1..=100))]
        limit: u64,
    },
    /// Sign and submit a REVOKE_RECORD transaction.
    Revoke {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ValidatorCommand {
    List {
        #[arg(long)]
        active: Option<bool>,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(1..=100))]
        limit: u64,
    },
    Show {
        id: String,
    },
    Status,
    Verify {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum NetworkCommand {
    Status,
    Peers {
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(1..=100))]
        limit: u64,
    },
    Consistency,
    Sync {
        peer: String,
    },
    Ping {
        peer: String,
    },
    /// Authenticate a configured trusted peer by ID.
    Connect {
        peer: String,
    },
}

#[derive(Subcommand)]
pub enum StorageCommand {
    Status,
    Verify,
    RebuildIndexes,
    Stats,
}

#[derive(Subcommand)]
pub enum SnapshotCommand {
    List {
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(1..=100))]
        limit: u64,
    },
    Create,
    Verify {
        id: String,
    },
    Restore {
        id: String,
        #[arg(long)]
        force: bool,
        #[arg(long, help = "Skip the destructive-operation prompt")]
        yes: bool,
    },
    Info {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    Show,
    Validate,
}

#[derive(Subcommand)]
pub enum DemoCommand {
    /// Generate and commit clearly labelled institutional demo records in temporary storage.
    Seed,
    /// Run the complete deterministic release demonstration in isolated temporary storage.
    Run,
    Blockchain,
    Tamper,
    Network,
    Recovery,
    Snapshot,
}

#[derive(Subcommand)]
pub enum DevCommand {
    Test {
        #[arg(long)]
        release: bool,
    },
}

#[derive(Subcommand)]
pub enum IdentityCommand {
    Init {
        #[arg(long)]
        node_id: String,
        #[arg(short, long)]
        output: PathBuf,
    },
}
