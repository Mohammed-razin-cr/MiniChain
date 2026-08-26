use std::process::Command;

use serde_json::json;

use super::super::{
    app::DevCommand,
    context::CliContext,
    error::{CliError, CliResult},
    output::emit,
};

pub async fn run(context: &CliContext, command: DevCommand) -> CliResult<()> {
    match command {
        DevCommand::Test { release } => {
            let mut process = Command::new("cargo");
            process.arg("test");
            if release {
                process.arg("--release");
            }
            let status = process.status().map_err(|error| {
                CliError::unavailable("Could not start cargo test").reason(error.to_string())
            })?;
            if !status.success() {
                return Err(CliError::validation("Development test suite failed"));
            }
            emit(
                &json!({"tests": "passed", "release": release}),
                context.json,
                "Developer tests",
            )
        }
    }
}
