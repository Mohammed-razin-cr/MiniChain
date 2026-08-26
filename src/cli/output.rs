use serde::Serialize;
use serde_json::Value;

use super::error::{CliError, CliResult};

pub fn emit<T: Serialize>(value: &T, json: bool, title: &str) -> CliResult<()> {
    let value = serde_json::to_value(value)
        .map_err(|error| CliError::new(1, "Could not format output").reason(error.to_string()))?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|error| {
                CliError::new(1, "Could not format JSON output").reason(error.to_string())
            })?
        );
    } else {
        println!("{}\n", title.to_uppercase());
        render(&value, 0);
    }
    Ok(())
}

fn render(value: &Value, indent: usize) {
    match value {
        Value::Object(entries) => {
            for (key, value) in entries {
                let label = key.replace('_', " ");
                match value {
                    Value::Array(_) | Value::Object(_) => {
                        println!("{}{}:", " ".repeat(indent), title_case(&label));
                        render(value, indent + 2);
                    }
                    _ => println!(
                        "{}{:20} {}",
                        " ".repeat(indent),
                        format!("{}:", title_case(&label)),
                        scalar(value)
                    ),
                }
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                println!("{}(none)", " ".repeat(indent));
            }
            for item in items {
                print!("{}- ", " ".repeat(indent));
                match item {
                    Value::Object(_) | Value::Array(_) => {
                        println!();
                        render(item, indent + 2);
                    }
                    _ => println!("{}", scalar(item)),
                }
            }
        }
        _ => println!("{}{}", " ".repeat(indent), scalar(value)),
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => "-".to_owned(),
        other => other.to_string(),
    }
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
        .unwrap_or_default()
}
