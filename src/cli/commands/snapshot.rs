use std::io::{self, Write};

use serde_json::json;

use super::super::{
    app::SnapshotCommand,
    client::ApiClient,
    context::CliContext,
    error::{CliError, CliResult},
    output::emit,
};

pub async fn run(context: &CliContext, command: SnapshotCommand) -> CliResult<()> {
    let client = ApiClient::from_config(&context.config()?)?;
    let (value, title) = match command {
        SnapshotCommand::List { limit } => (
            client
                .get(&format!("/snapshots?page=1&limit={limit}"))
                .await?,
            "Snapshots",
        ),
        SnapshotCommand::Create => (
            client.post_empty("/snapshots/create").await?,
            "Snapshot created",
        ),
        SnapshotCommand::Verify { id } => (
            client.get(&format!("/snapshots/{id}/verify")).await?,
            "Snapshot verification",
        ),
        SnapshotCommand::Info { id } => (
            client.get(&format!("/snapshots/{id}/verify")).await?,
            "Snapshot info",
        ),
        SnapshotCommand::Restore { id, force, yes } => {
            if !yes && !confirm_restore()? {
                return Err(CliError::new(1, "Snapshot restoration was cancelled"));
            }
            (
                client
                    .post("/snapshots/restore", &json!({"id": id, "force": force}))
                    .await?,
                "Snapshot restored",
            )
        }
    };
    emit(&value, context.json, title)
}

fn confirm_restore() -> CliResult<bool> {
    eprint!("WARNING: this operation will replace local blockchain state. Continue? [y/N] ");
    io::stderr().flush().map_err(|error| {
        CliError::new(1, "Could not display confirmation").reason(error.to_string())
    })?;
    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|error| {
        CliError::new(1, "Could not read confirmation").reason(error.to_string())
    })?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
