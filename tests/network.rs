use std::{
    collections::{BTreeMap, HashMap},
    net::TcpListener as StdListener,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use minichain::{
    Block, Operation, Transaction, ValidatorIdentity,
    mempool::Mempool,
    network::{NetworkNode, NodeConfig, TrustedPeer},
    storage::{RedbStorage, Storage},
};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

fn available_address() -> String {
    let listener = StdListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

fn identity(index: u8) -> ValidatorIdentity {
    ValidatorIdentity::from_secret_bytes(format!("node-{index:02}"), [70 + index; 32])
}

fn configurations(addresses: &[String]) -> Vec<NodeConfig> {
    (1..=3)
        .map(|node_index| {
            let trusted_peers = (1..=3)
                .filter(|peer_index| *peer_index != node_index)
                .map(|peer_index| {
                    let peer_identity = identity(peer_index as u8);
                    (
                        peer_identity.validator_id().to_owned(),
                        TrustedPeer {
                            address: addresses[peer_index - 1].clone(),
                            public_key: peer_identity.public_key().to_vec(),
                        },
                    )
                })
                .collect::<HashMap<_, _>>();
            NodeConfig {
                node_id: format!("node-{node_index:02}"),
                listen_address: addresses[node_index - 1].clone(),
                trusted_peers,
                max_peers: 8,
                chain_id: Uuid::nil(),
                network_id: "network-test".to_owned(),
                storage_path: PathBuf::from(format!("node-{node_index}.redb")),
                identity_path: PathBuf::from(format!("node-{node_index}.key")),
                heartbeat_interval_ms: 60_000,
                api: None,
            }
        })
        .collect()
}

fn transaction(sequence: u64) -> Transaction {
    Transaction::new(
        Operation::UpdateRecord,
        "NETWORK-RECORD",
        json!({"sequence": sequence}),
        BTreeMap::new(),
        &identity(1),
    )
}

fn commit_next(storage: &RedbStorage, transaction: Transaction) -> Block {
    let chain = storage.recover().unwrap();
    let block = Block::new(
        chain.height() + 1,
        chain.tip().hash.clone(),
        vec![transaction],
        "node-01",
    )
    .unwrap();
    storage.commit_block(block.clone()).unwrap();
    block
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn three_real_tcp_nodes_propagate_and_recover_after_downtime() {
    let addresses = (0..3).map(|_| available_address()).collect::<Vec<_>>();
    let configs = configurations(&addresses);
    let directories = (0..3).map(|_| TempDir::new().unwrap()).collect::<Vec<_>>();
    let chain_id = Uuid::new_v4();
    let storages = directories
        .iter()
        .map(|directory| {
            Arc::new(
                RedbStorage::open(directory.path().join("node.redb"), chain_id, "network-test")
                    .unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let make_node = |index: usize| {
        NetworkNode::new(
            configs[index].clone(),
            identity((index + 1) as u8),
            Arc::clone(&storages[index]),
            Mempool::new(1_000, Duration::from_secs(300)),
        )
        .unwrap()
    };

    let node1 = make_node(0).start().await.unwrap();
    let node2 = make_node(1).start().await.unwrap();
    let node3 = make_node(2).start().await.unwrap();
    let started = Instant::now();
    node1.node.connect("node-02").await.unwrap();
    let handshake = started.elapsed();
    node1.node.connect("node-03").await.unwrap();
    assert!(node1.node.heartbeat_once("node-02").await.unwrap() < Duration::from_secs(1));

    let first = transaction(1);
    let started = Instant::now();
    node1
        .node
        .broadcast_transaction(first.clone())
        .await
        .unwrap();
    let transaction_propagation = started.elapsed();
    assert_eq!(node1.node.mempool_len().await, 1);
    assert_eq!(node2.node.mempool_len().await, 1);
    assert_eq!(node3.node.mempool_len().await, 1);
    let first_block = commit_next(&storages[0], first);
    let started = Instant::now();
    node1.node.broadcast_block(first_block).await.unwrap();
    let block_propagation = started.elapsed();
    assert_eq!(node2.node.local_status().await.unwrap().height, 1);
    assert_eq!(node3.node.local_status().await.unwrap().height, 1);
    assert_eq!(node2.node.mempool_len().await, 0);

    drop(node3);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut latest = None;
    let mut first_missed_transaction = None;
    for sequence in 2..=4 {
        let missed_transaction = transaction(sequence);
        if first_missed_transaction.is_none() {
            first_missed_transaction = Some(missed_transaction.clone());
        }
        let block = commit_next(&storages[0], missed_transaction);
        let _ = node1.node.broadcast_block(block.clone()).await;
        latest = Some(block);
    }
    assert_eq!(node2.node.local_status().await.unwrap().height, 4);
    assert_eq!(storages[2].recover().unwrap().height(), 1);

    let restarted_pool = Mempool::new(1_000, Duration::from_secs(300));
    restarted_pool
        .insert(first_missed_transaction.unwrap())
        .await
        .unwrap();
    let restarted3 = NetworkNode::new(
        configs[2].clone(),
        identity(3),
        Arc::clone(&storages[2]),
        restarted_pool,
    )
    .unwrap()
    .start()
    .await
    .unwrap();
    let out_of_order = node1
        .node
        .broadcast_block(latest.expect("a latest block was produced"))
        .await;
    assert!(out_of_order.is_err());
    assert_eq!(restarted3.node.local_status().await.unwrap().height, 1);
    let started = Instant::now();
    restarted3.node.sync_from("node-01").await.unwrap();
    let synchronization = started.elapsed();
    assert_eq!(restarted3.node.mempool_len().await, 0);

    let status1 = node1.node.local_status().await.unwrap();
    let status2 = node2.node.local_status().await.unwrap();
    let status3 = restarted3.node.local_status().await.unwrap();
    assert_eq!(status1.height, 4);
    assert_eq!(status1.latest_hash, status2.latest_hash);
    assert_eq!(status1.latest_hash, status3.latest_hash);
    println!(
        "handshake={handshake:?} transaction_to_two_peers={transaction_propagation:?} block_to_two_peers={block_propagation:?} sync_three_blocks={synchronization:?}"
    );
}
