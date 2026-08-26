use serde_json::json;

use super::super::{
    app::ChainCommand, client::ApiClient, context::CliContext, error::CliResult, output::emit,
};

pub async fn run(context: &CliContext, command: ChainCommand) -> CliResult<()> {
    let client = ApiClient::from_config(&context.config()?)?;
    let (value, title) = match command {
        ChainCommand::Status => {
            let health = client.public_get("/health").await?;
            let stats = client.get("/storage/stats").await?;
            let validation = client.post_empty("/blockchain/validate").await?;
            (
                json!({
                    "height": health["height"],
                    "head": stats["latest_block_hash"],
                    "integrity": if validation["valid"] == true { "valid" } else { "invalid" },
                    "blocks": stats["blocks"],
                    "transactions": stats["transactions"],
                    "records": stats["records"],
                }),
                "Chain status",
            )
        }
        ChainCommand::Validate => (
            client.post_empty("/blockchain/validate").await?,
            "Chain validation",
        ),
        ChainCommand::Height => {
            let health = client.public_get("/health").await?;
            (json!({"height": health["height"]}), "Chain height")
        }
        ChainCommand::Head => {
            let status = client.get("/network/status").await?;
            (
                json!({"height": status["height"], "hash": status["latest_hash"]}),
                "Chain head",
            )
        }
        ChainCommand::Consistency => (
            client.get("/network/consistency").await?,
            "Chain consistency",
        ),
    };
    emit(&value, context.json, title)
}
