use serde_json::json;

use crate::storage::Storage;

use super::super::{app::StorageCommand, context::CliContext, error::CliResult, output::emit};

pub async fn run(context: &CliContext, command: StorageCommand) -> CliResult<()> {
    let storage = context.offline_storage()?;
    let (value, title) = match command {
        StorageCommand::Status | StorageCommand::Stats => {
            let stats = storage.stats()?;
            (
                serde_json::to_value(stats).expect("storage stats serialize"),
                "Storage",
            )
        }
        StorageCommand::Verify => {
            let chain = storage.recover()?;
            let validation = chain.validate();
            (
                json!({
                    "database": "healthy",
                    "indexes": "valid",
                    "height": chain.height(),
                    "chain_valid": validation.valid,
                    "checked_blocks": validation.checked_blocks,
                }),
                "Storage verification",
            )
        }
        StorageCommand::RebuildIndexes => {
            storage.rebuild_indexes()?;
            storage.recover()?;
            (
                json!({"rebuilt": true, "verified": true}),
                "Storage indexes",
            )
        }
    };
    emit(&value, context.json, title)
}
