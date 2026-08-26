use super::super::{
    app::NetworkCommand, client::ApiClient, context::CliContext, error::CliResult, output::emit,
};

pub async fn run(context: &CliContext, command: NetworkCommand) -> CliResult<()> {
    let client = ApiClient::from_config(&context.config()?)?;
    let (value, title) = match command {
        NetworkCommand::Status => (client.get("/network/status").await?, "Network status"),
        NetworkCommand::Peers { limit } => (
            client
                .get(&format!("/network/peers?page=1&limit={limit}"))
                .await?,
            "Network peers",
        ),
        NetworkCommand::Consistency => (
            client.get("/network/consistency").await?,
            "Network consistency",
        ),
        NetworkCommand::Sync { peer } => (
            client.post_empty(&format!("/network/sync/{peer}")).await?,
            "Network synchronization",
        ),
        NetworkCommand::Ping { peer } => (
            client.post_empty(&format!("/network/ping/{peer}")).await?,
            "Peer ping",
        ),
        NetworkCommand::Connect { peer } => (
            client
                .post_empty(&format!("/network/connect/{peer}"))
                .await?,
            "Peer connection",
        ),
    };
    emit(&value, context.json, title)
}
