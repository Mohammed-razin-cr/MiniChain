use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{Operation, Transaction, ValidatorIdentity};

use super::{
    super::{
        app::RecordCommand, client::ApiClient, context::CliContext, error::CliResult, output::emit,
    },
    transaction::read_json_file,
};

pub async fn run(context: &CliContext, command: RecordCommand) -> CliResult<()> {
    let config = context.config()?;
    let client = ApiClient::from_config(&config)?;
    let (value, title) = match command {
        RecordCommand::Create { id, data } => {
            let payload = read_json_file(&data)?;
            let transaction = signed_transaction(&config, Operation::CreateRecord, id, payload)?;
            (
                client.post("/records", &transaction).await?,
                "Record submitted",
            )
        }
        RecordCommand::Get { id } => (client.get(&format!("/records/{id}")).await?, "Record"),
        RecordCommand::Verify { id } => (
            client.post_empty(&format!("/records/{id}/verify")).await?,
            "Record verification",
        ),
        RecordCommand::History { id, limit } => (
            client
                .get(&format!("/records/{id}/history?page=1&limit={limit}"))
                .await?,
            "Record history",
        ),
        RecordCommand::Revoke { id, reason } => {
            client.get(&format!("/records/{id}")).await?;
            let payload = json!({"reason": reason.unwrap_or_else(|| "unspecified".to_owned())});
            let transaction = signed_transaction(&config, Operation::RevokeRecord, id, payload)?;
            (
                client.post("/transactions", &transaction).await?,
                "Revocation submitted",
            )
        }
    };
    emit(&value, context.json, title)
}

fn signed_transaction(
    config: &crate::network::NodeConfig,
    operation: Operation,
    id: String,
    payload: Value,
) -> CliResult<Transaction> {
    let identity = ValidatorIdentity::load_or_create(&config.node_id, &config.identity_path)?;
    Ok(Transaction::new(
        operation,
        id,
        payload,
        BTreeMap::new(),
        &identity,
    ))
}
