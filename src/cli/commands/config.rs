use serde_json::{Value, json};

use super::super::{app::ConfigCommand, context::CliContext, error::CliResult, output::emit};

pub async fn run(context: &CliContext, command: ConfigCommand) -> CliResult<()> {
    let config = context.config()?;
    match command {
        ConfigCommand::Validate => emit(
            &json!({"valid": true, "path": context.config_path}),
            context.json,
            "Configuration validation",
        ),
        ConfigCommand::Show => {
            let mut value = serde_json::to_value(config).expect("node configuration serializes");
            redact(&mut value);
            emit(&value, context.json, "Effective configuration")
        }
    }
}

fn redact(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        if object.contains_key("identity_path") {
            object.insert("identity_path".to_owned(), json!("********"));
        }
        if let Some(tokens) = object
            .get_mut("api")
            .and_then(Value::as_object_mut)
            .and_then(|api| api.get_mut("tokens"))
            .and_then(Value::as_array_mut)
        {
            for token in tokens {
                if let Some(token) = token.as_object_mut() {
                    token.insert("token_sha256".to_owned(), json!("********"));
                }
            }
        }
    }
}
