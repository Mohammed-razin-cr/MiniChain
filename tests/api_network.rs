use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use minichain::{
    Block, Operation, Transaction, ValidatorIdentity,
    api::{ApiState, serve},
    crypto::{encode_hex, sha256},
    mempool::Mempool,
    network::{ApiConfig, ApiRole, ApiTokenConfig, NetworkNode, NodeConfig, TrustedPeer},
    storage::{RedbStorage, Storage},
};
use reqwest::Client;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};
use uuid::Uuid;

const VIEWER: &str = "network-viewer-token";
const OPERATOR: &str = "network-operator-token";

fn identity(index: u8) -> ValidatorIdentity {
    ValidatorIdentity::from_secret_bytes(format!("node-{index:02}"), [130 + index; 32])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn record_submitted_through_node_one_api_is_queryable_on_all_real_nodes() {
    exercise_record_flow(1).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "manual release-mode full-system load measurement"]
async fn one_hundred_twenty_records_flow_through_three_real_nodes() {
    exercise_record_flow(120).await;
}

async fn exercise_record_flow(record_count: usize) {
    let directories = (0..3).map(|_| TempDir::new().unwrap()).collect::<Vec<_>>();
    let p2p_addresses = (0..3).map(|_| available_address()).collect::<Vec<_>>();
    let chain_id = Uuid::new_v4();
    let storages = directories
        .iter()
        .map(|directory| {
            Arc::new(
                RedbStorage::open(directory.path().join("node.redb"), chain_id, "api-network")
                    .unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let configs = (0..3)
        .map(|index| NodeConfig {
            node_id: format!("node-{:02}", index + 1),
            listen_address: p2p_addresses[index].clone(),
            trusted_peers: (0..3)
                .filter(|peer| *peer != index)
                .map(|peer| {
                    (
                        format!("node-{:02}", peer + 1),
                        TrustedPeer {
                            address: p2p_addresses[peer].clone(),
                            public_key: identity((peer + 1) as u8).public_key().to_vec(),
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
            max_peers: 8,
            chain_id,
            network_id: "api-network".to_owned(),
            storage_path: PathBuf::from(format!("node-{index}.redb")),
            identity_path: PathBuf::from(format!("node-{index}.key")),
            heartbeat_interval_ms: 60_000,
            api: None,
        })
        .collect::<Vec<_>>();
    let mut nodes = Vec::new();
    for index in 0..3 {
        nodes.push(
            NetworkNode::new(
                configs[index].clone(),
                identity((index + 1) as u8),
                Arc::clone(&storages[index]),
                Mempool::new(1_000, Duration::from_secs(300)),
            )
            .unwrap()
            .start()
            .await
            .unwrap(),
        );
    }
    nodes[0].node.connect("node-02").await.unwrap();
    nodes[0].node.connect("node-03").await.unwrap();

    let api_config = api_config();
    let mut api_urls = Vec::new();
    let mut api_tasks: Vec<JoinHandle<minichain::Result<()>>> = Vec::new();
    for index in 0..3 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        api_urls.push(format!("http://{}", listener.local_addr().unwrap()));
        let state = ApiState::new(
            nodes[index].node.clone(),
            Arc::clone(&storages[index]),
            &api_config,
        )
        .unwrap();
        let config = api_config.clone();
        api_tasks.push(tokio::spawn(async move {
            serve(listener, state, &config).await
        }));
    }

    let client = Client::builder().no_proxy().build().unwrap();
    let signer = ValidatorIdentity::from_secret_bytes("api-record-owner", [140; 32]);
    let transactions = (0..record_count)
        .map(|index| {
            Transaction::new(
                Operation::CreateRecord,
                format!("multi-node-api-record-{index}"),
                json!({"type": "degree", "subject": format!("student-{index}")}),
                Default::default(),
                &signer,
            )
        })
        .collect::<Vec<_>>();
    let started = Instant::now();
    for transaction in &transactions {
        let submitted = client
            .post(format!("{}/api/v1/records", api_urls[0]))
            .bearer_auth(OPERATOR)
            .json(transaction)
            .send()
            .await
            .unwrap();
        assert_eq!(submitted.status(), reqwest::StatusCode::ACCEPTED);
    }
    let submission_latency = started.elapsed();
    assert_eq!(nodes[1].node.mempool_len().await, record_count);
    assert_eq!(nodes[2].node.mempool_len().await, record_count);

    let genesis = storages[0].get_block(0).unwrap();
    let block = Block::new(1, genesis.hash, transactions.clone(), "node-01").unwrap();
    nodes[0]
        .node
        .commit_and_broadcast_block(block.clone())
        .await
        .unwrap();
    assert_eq!(nodes[0].node.mempool_len().await, 0);
    assert_eq!(nodes[1].node.mempool_len().await, 0);
    assert_eq!(nodes[2].node.mempool_len().await, 0);

    let last_transaction = transactions.last().unwrap();
    let mut observed = Vec::new();
    for url in &api_urls {
        let record: Value = client
            .get(format!(
                "{url}/api/v1/records/{}",
                last_transaction.record_id
            ))
            .bearer_auth(VIEWER)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let stored_transaction: Value = client
            .get(format!("{url}/api/v1/transactions/{}", last_transaction.id))
            .bearer_auth(VIEWER)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let block_response: Value = client
            .get(format!("{url}/api/v1/blocks/1"))
            .bearer_auth(VIEWER)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let network: Value = client
            .get(format!("{url}/api/v1/network/status"))
            .bearer_auth(VIEWER)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let stats: Value = client
            .get(format!("{url}/api/v1/storage/stats"))
            .bearer_auth(VIEWER)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(record["cryptographically_verified"], true);
        assert_eq!(
            stored_transaction["transaction_id"],
            last_transaction.id.to_string()
        );
        assert_eq!(block_response["hash"], block.hash);
        assert_eq!(stats["blocks"], 2);
        assert_eq!(stats["transactions"], record_count);
        assert_eq!(stats["records"], record_count);
        observed.push((
            network["height"].clone(),
            network["latest_hash"].clone(),
            stats["blocks"].clone(),
            stats["transactions"].clone(),
            stats["records"].clone(),
        ));
    }
    assert!(observed.iter().all(|status| status == &observed[0]));
    println!(
        "records={record_count} three_node_api_submission={submission_latency:?} average_submission={:?}",
        submission_latency / record_count as u32
    );

    for task in api_tasks {
        task.abort();
    }
}

fn api_config() -> ApiConfig {
    ApiConfig {
        listen_address: "127.0.0.1:0".to_owned(),
        allowed_origins: vec!["http://localhost:5173".to_owned()],
        max_body_bytes: 128 * 1024,
        rate_window_ms: 1_000,
        read_requests_per_window: 100,
        write_requests_per_window: 10_000,
        admin_requests_per_window: 5,
        tokens: vec![
            token("network-viewer", ApiRole::Viewer, VIEWER),
            token("network-operator", ApiRole::Operator, OPERATOR),
        ],
    }
}

fn token(identity: &str, role: ApiRole, plaintext: &str) -> ApiTokenConfig {
    ApiTokenConfig {
        identity: identity.to_owned(),
        role,
        token_sha256: encode_hex(&sha256(plaintext.as_bytes())),
    }
}

fn available_address() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}
