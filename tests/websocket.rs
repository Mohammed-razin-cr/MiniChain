use std::{collections::HashMap, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use minichain::{
    Operation, Transaction, ValidatorIdentity,
    api::{ApiState, serve},
    crypto::{encode_hex, sha256},
    mempool::Mempool,
    network::{ApiConfig, ApiRole, ApiTokenConfig, NetworkNode, NodeConfig},
    storage::RedbStorage,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use uuid::Uuid;

const TOKEN: &str = "websocket-test-token";
type Client = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn authenticated_clients_receive_events_and_can_reconnect() {
    let fixture = fixture().await;
    let mut clients = connect_clients(&fixture.url, 10).await;
    let first = transaction(1);
    fixture
        .node
        .broadcast_transaction(first.clone())
        .await
        .unwrap();
    receive_transaction_event(&mut clients, first.id).await;

    clients.clear();
    let mut reconnected = connect_clients(&fixture.url, 1).await;
    let second = transaction(2);
    fixture
        .node
        .broadcast_transaction(second.clone())
        .await
        .unwrap();
    receive_transaction_event(&mut reconnected, second.id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "manual release-mode WebSocket fan-out measurement"]
async fn measure_websocket_fanout() {
    let fixture = fixture().await;
    for count in [10usize, 50, 100] {
        let connected = std::time::Instant::now();
        let mut clients = connect_clients(&fixture.url, count).await;
        let authentication = connected.elapsed();
        let transaction = transaction(count as u64);
        let started = std::time::Instant::now();
        fixture
            .node
            .broadcast_transaction(transaction.clone())
            .await
            .unwrap();
        receive_transaction_event(&mut clients, transaction.id).await;
        let fanout = started.elapsed();
        println!("websocket_clients={count} connect_and_auth={authentication:?} fanout={fanout:?}");
    }
}

struct Fixture {
    node: NetworkNode,
    url: String,
    _node: minichain::network::RunningNode,
    _server: JoinHandle<()>,
    _directory: TempDir,
}

async fn fixture() -> Fixture {
    let directory = TempDir::new().unwrap();
    let identity = ValidatorIdentity::from_secret_bytes("node-websocket", [124; 32]);
    let token = ApiTokenConfig {
        identity: "viewer".to_owned(),
        role: ApiRole::Viewer,
        token_sha256: encode_hex(&sha256(TOKEN.as_bytes())),
    };
    let api_config = ApiConfig {
        listen_address: "127.0.0.1:0".to_owned(),
        allowed_origins: vec!["http://localhost:5173".to_owned()],
        max_body_bytes: 128 * 1024,
        rate_window_ms: 1_000,
        read_requests_per_window: 10_000,
        write_requests_per_window: 10_000,
        admin_requests_per_window: 10_000,
        tokens: vec![token],
    };
    let chain_id = Uuid::new_v4();
    let storage = Arc::new(
        RedbStorage::open(
            directory.path().join("websocket.redb"),
            chain_id,
            "websocket-test",
        )
        .unwrap(),
    );
    let config = NodeConfig {
        node_id: "node-websocket".to_owned(),
        listen_address: available_address(),
        trusted_peers: HashMap::new(),
        max_peers: 8,
        chain_id,
        network_id: "websocket-test".to_owned(),
        storage_path: directory.path().join("websocket.redb"),
        identity_path: directory.path().join("websocket.key"),
        heartbeat_interval_ms: 60_000,
        api: Some(api_config.clone()),
    };
    let node = NetworkNode::new(
        config,
        identity,
        Arc::clone(&storage),
        Mempool::new(1_000, Duration::from_secs(300)),
    )
    .unwrap()
    .start()
    .await
    .unwrap();
    let state = ApiState::new(node.node.clone(), storage, &api_config).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve(listener, state, &api_config).await.unwrap();
    });
    Fixture {
        node: node.node.clone(),
        url: format!("ws://{address}/api/v1/events"),
        _node: node,
        _server: server,
        _directory: directory,
    }
}

async fn connect_clients(url: &str, count: usize) -> Vec<Client> {
    let mut tasks = Vec::with_capacity(count);
    for _ in 0..count {
        let url = url.to_owned();
        tasks.push(tokio::spawn(async move {
            let (mut client, _) = connect_async(url).await.unwrap();
            client
                .send(Message::Text(json!({"token": TOKEN}).to_string().into()))
                .await
                .unwrap();
            let authenticated = timeout(Duration::from_secs(5), client.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let value: Value = serde_json::from_str(authenticated.to_text().unwrap()).unwrap();
            assert_eq!(value["type"], "authenticated");
            client
        }));
    }
    let mut clients = Vec::with_capacity(count);
    for task in tasks {
        clients.push(task.await.unwrap());
    }
    clients
}

async fn receive_transaction_event(clients: &mut [Client], id: Uuid) {
    for client in clients {
        let message = timeout(Duration::from_secs(5), client.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
        assert_eq!(value["event"]["type"], "transaction_broadcast");
        assert_eq!(value["event"]["data"], id.to_string());
    }
}

fn transaction(sequence: u64) -> Transaction {
    Transaction::new(
        Operation::AuditEvent,
        format!("WEBSOCKET-{sequence}"),
        json!({"sequence": sequence}),
        Default::default(),
        &ValidatorIdentity::from_secret_bytes("websocket-client", [125; 32]),
    )
}

fn available_address() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}
