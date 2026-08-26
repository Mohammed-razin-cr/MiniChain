use serde::Serialize;

use super::super::{client::ApiClient, context::CliContext, error::CliResult, output::emit};

#[derive(Serialize)]
struct Diagnostics {
    overall: &'static str,
    checks: Vec<Check>,
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    status: &'static str,
    detail: String,
}

pub async fn run(context: &CliContext) -> CliResult<()> {
    let config = context.config()?;
    let client = ApiClient::from_config(&config)?;
    let mut checks = vec![Check {
        name: "configuration",
        status: "pass",
        detail: "strict TOML validation succeeded".to_owned(),
    }];
    check(
        &mut checks,
        "api",
        client.public_get("/ready").await,
        |value| value["ready"] == true,
    );
    check(
        &mut checks,
        "storage",
        client.get("/storage/stats").await,
        |_| true,
    );
    check(
        &mut checks,
        "blockchain_integrity",
        client.post_empty("/blockchain/validate").await,
        |value| value["valid"] == true,
    );
    check(
        &mut checks,
        "latest_block",
        client.get("/network/status").await,
        |value| {
            value["latest_hash"]
                .as_str()
                .is_some_and(|hash| hash.len() == 64)
        },
    );
    check(
        &mut checks,
        "genesis",
        client.post_empty("/blocks/0/verify").await,
        |value| value["valid"] == true,
    );
    check(
        &mut checks,
        "mempool",
        client.get("/network/status").await,
        |value| value["mempool_size"].is_u64(),
    );
    check(
        &mut checks,
        "validator_registry",
        client.get("/validators?limit=100").await,
        |value| value["total"].as_u64().is_some_and(|total| total > 0),
    );
    check(
        &mut checks,
        "peer_connectivity",
        client.get("/network/peers?limit=100").await,
        |value| value["total"].as_u64().is_some_and(|total| total > 0),
    );
    check(
        &mut checks,
        "network_consistency",
        client.get("/network/consistency").await,
        |value| value["consistent"] == true,
    );
    check(
        &mut checks,
        "snapshot_subsystem",
        client.get("/snapshots?limit=1").await,
        |_| true,
    );
    let overall = if checks.iter().any(|check| check.status == "fail") {
        "fail"
    } else if checks.iter().any(|check| check.status == "warn") {
        "warning"
    } else {
        "pass"
    };
    emit(
        &Diagnostics { overall, checks },
        context.json,
        "MiniChain diagnostics",
    )
}

fn check<F>(
    checks: &mut Vec<Check>,
    name: &'static str,
    result: CliResult<serde_json::Value>,
    passes: F,
) where
    F: FnOnce(&serde_json::Value) -> bool,
{
    match result {
        Ok(value) if passes(&value) => checks.push(Check {
            name,
            status: "pass",
            detail: "check succeeded".to_owned(),
        }),
        Ok(_) => checks.push(Check {
            name,
            status: "warn",
            detail: "the service responded but reported a degraded state".to_owned(),
        }),
        Err(error) => checks.push(Check {
            name,
            status: "fail",
            detail: error.message,
        }),
    }
}
