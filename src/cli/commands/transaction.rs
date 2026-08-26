use std::fs;

use serde_json::{Value, json};

use crate::Transaction;

use super::super::{
    app::TransactionCommand,
    client::ApiClient,
    context::CliContext,
    error::{CliError, CliResult},
    output::emit,
};

pub async fn run(context: &CliContext, command: TransactionCommand) -> CliResult<()> {
    let client = ApiClient::from_config(&context.config()?)?;
    let (value, title) = match command {
        TransactionCommand::Submit { file } => {
            let transaction: Transaction =
                serde_json::from_slice(&fs::read(&file).map_err(|error| {
                    CliError::validation(format!("Could not read {}", file.display()))
                        .reason(error.to_string())
                })?)
                .map_err(|error| {
                    CliError::validation("Transaction JSON is invalid").reason(error.to_string())
                })?;
            transaction.validate().map_err(CliError::from)?;
            (
                client.post("/transactions", &transaction).await?,
                "Transaction submitted",
            )
        }
        TransactionCommand::Show { id } => (
            client.get(&format!("/transactions/{id}")).await?,
            "Transaction",
        ),
        TransactionCommand::Verify { id } => {
            let transaction = client.get(&format!("/transactions/{id}")).await?;
            (
                json!({
                    "transaction_id": id,
                    "valid": transaction["signature_valid"],
                    "hash": transaction["hash"],
                    "block_height": transaction["block_height"],
                    "block_hash": transaction["block_hash"],
                }),
                "Transaction verification",
            )
        }
        TransactionCommand::Status { id } => (
            client.get(&format!("/transactions/{id}/status")).await?,
            "Transaction status",
        ),
        TransactionCommand::List {
            page,
            limit,
            operation,
        } => {
            let operation = operation
                .map(|value| format!("&operation={}", value.to_uppercase()))
                .unwrap_or_default();
            (
                client
                    .get(&format!(
                        "/transactions?page={page}&limit={limit}{operation}"
                    ))
                    .await?,
                "Transactions",
            )
        }
    };
    emit(&value, context.json, title)
}

pub fn read_json_file(path: &std::path::Path) -> CliResult<Value> {
    serde_json::from_slice(&fs::read(path).map_err(|error| {
        CliError::validation(format!("Could not read {}", path.display())).reason(error.to_string())
    })?)
    .map_err(|error| CliError::validation("JSON input is invalid").reason(error.to_string()))
}
