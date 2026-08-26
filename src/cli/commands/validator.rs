use serde_json::json;

use super::super::{
    app::ValidatorCommand, client::ApiClient, context::CliContext, error::CliResult, output::emit,
};

pub async fn run(context: &CliContext, command: ValidatorCommand) -> CliResult<()> {
    let client = ApiClient::from_config(&context.config()?)?;
    let (value, title) = match command {
        ValidatorCommand::List { active, limit } => {
            let filter = active
                .map(|value| format!("&active={value}"))
                .unwrap_or_default();
            (
                client
                    .get(&format!("/validators?page=1&limit={limit}{filter}"))
                    .await?,
                "Validators",
            )
        }
        ValidatorCommand::Show { id } => {
            (client.get(&format!("/validators/{id}")).await?, "Validator")
        }
        ValidatorCommand::Status => (
            client.get("/validators?page=1&limit=100").await?,
            "Validator status",
        ),
        ValidatorCommand::Verify { id } => {
            let validator = client.get(&format!("/validators/{id}")).await?;
            (
                json!({
                    "validator_id": id,
                    "public_identity_valid": validator["public_key"].as_array().is_some_and(|key| key.len() == 32),
                    "active": validator["active"],
                    "current_height": validator["current_height"],
                }),
                "Validator verification",
            )
        }
    };
    emit(&value, context.json, title)
}
