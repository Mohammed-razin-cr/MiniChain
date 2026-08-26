use serde_json::json;

use super::super::{
    app::BlockCommand, client::ApiClient, context::CliContext, error::CliResult, output::emit,
};

pub async fn run(context: &CliContext, command: BlockCommand) -> CliResult<()> {
    let client = ApiClient::from_config(&context.config()?)?;
    let (value, title) = match command {
        BlockCommand::List { from, limit } => (
            client
                .get(&format!("/blocks?from={from}&limit={limit}"))
                .await?,
            "Blocks",
        ),
        BlockCommand::Show { height } => (
            client.get(&format!("/blocks/{height}")).await?,
            "Block detail",
        ),
        BlockCommand::Hash { hash } => (
            client.get(&format!("/blocks/hash/{hash}")).await?,
            "Block detail",
        ),
        BlockCommand::Latest => {
            let status = client.get("/network/status").await?;
            let height = status["height"].as_u64().unwrap_or(0);
            (
                client.get(&format!("/blocks/{height}")).await?,
                "Latest block",
            )
        }
        BlockCommand::Verify { height } => {
            let verification = client
                .post_empty(&format!("/blocks/{height}/verify"))
                .await?;
            (
                json!({"height": height, "verification": verification}),
                "Block verification",
            )
        }
    };
    emit(&value, context.json, title)
}
