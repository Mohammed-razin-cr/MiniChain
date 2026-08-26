use serde_json::json;

use crate::ValidatorIdentity;

use super::super::{app::IdentityCommand, context::CliContext, error::CliResult, output::emit};

pub async fn run(context: &CliContext, command: IdentityCommand) -> CliResult<()> {
    match command {
        IdentityCommand::Init { node_id, output } => {
            let identity = ValidatorIdentity::load_or_create(&node_id, &output)?;
            emit(
                &json!({
                    "node_id": node_id,
                    "public_key": identity.public_key(),
                    "private_key": "********",
                    "path": output,
                }),
                context.json,
                "Node identity",
            )
        }
    }
}
