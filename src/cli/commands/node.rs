use std::{sync::Arc, time::Duration};

use serde_json::json;

use crate::{
    ValidatorIdentity,
    api::{ApiState, serve},
    mempool::Mempool,
    network::NetworkNode,
    storage::RedbStorage,
};

use super::super::{
    app::NodeCommand,
    client::ApiClient,
    context::CliContext,
    error::{CliError, CliResult},
    output::emit,
};

pub async fn run(context: &CliContext, command: NodeCommand) -> CliResult<()> {
    match command {
        NodeCommand::Start => start(context).await,
        NodeCommand::Status => status(context, false).await,
        NodeCommand::Info => status(context, true).await,
    }
}

async fn start(context: &CliContext) -> CliResult<()> {
    let config = context.config()?;
    let identity = ValidatorIdentity::load_or_create(&config.node_id, &config.identity_path)?;
    let storage = Arc::new(RedbStorage::open(
        &config.storage_path,
        config.chain_id,
        &config.network_id,
    )?);
    let node = NetworkNode::new(
        config.clone(),
        identity,
        Arc::clone(&storage),
        Mempool::new(10_000, Duration::from_secs(300)),
    )?;
    let running = node.start().await?;
    running.node.maintain_peers_once().await;
    let status = running.node.local_status().await?;
    emit(
        &json!({
            "node_id": status.node_id,
            "status": "running",
            "p2p_address": config.listen_address,
            "height": status.height,
            "latest_hash": status.latest_hash,
            "protocol": status.protocol_version,
            "api_address": config.api.as_ref().map(|api| &api.listen_address),
        }),
        context.json,
        "Node started",
    )?;
    if let Some(api_config) = config.api.clone() {
        let listener = tokio::net::TcpListener::bind(&api_config.listen_address)
            .await
            .map_err(|error| {
                CliError::unavailable("Could not bind the API listener").reason(error.to_string())
            })?;
        let state = ApiState::new(running.node.clone(), Arc::clone(&storage), &api_config)
            .map_err(|error| CliError::validation(error.message))?;
        tokio::select! {
            result = serve(listener, state, &api_config) => result.map_err(CliError::from),
            signal = tokio::signal::ctrl_c() => signal
                .map_err(|error| CliError::unavailable("Could not listen for Ctrl-C").reason(error.to_string())),
        }
    } else {
        tokio::signal::ctrl_c().await.map_err(|error| {
            CliError::unavailable("Could not listen for Ctrl-C").reason(error.to_string())
        })
    }
}

async fn status(context: &CliContext, include_config: bool) -> CliResult<()> {
    let config = context.config()?;
    let client = ApiClient::from_config(&config)?;
    let health = client.public_get("/health").await?;
    let network = client.get("/network/status").await?;
    let mut result = json!({
        "node_id": health["node_id"],
        "status": health["status"],
        "height": network["height"],
        "latest_hash": network["latest_hash"],
        "peers": network["peer_count"],
        "healthy_peers": network["healthy_peers"],
        "sync_state": network["sync_state"],
        "protocol": network["protocol_version"],
    });
    if include_config {
        result["p2p_address"] = json!(config.listen_address);
        result["api_address"] = json!(config.api.map(|api| api.listen_address));
        result["network_id"] = json!(config.network_id);
        result["chain_id"] = json!(config.chain_id);
    }
    emit(
        &result,
        context.json,
        if include_config {
            "Node info"
        } else {
            "Node status"
        },
    )
}
