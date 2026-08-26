use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};

use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

use crate::{
    Block, Blockchain, Operation, Transaction, ValidatorIdentity,
    consensus::{Approval, AuthorityConsensus, Permission, Proposal, Validator, ValidatorRegistry},
    mempool::Mempool,
    network::{NetworkNode, NodeConfig, TrustedPeer},
    storage::{RedbStorage, Storage},
};

use super::super::{app::DemoCommand, context::CliContext, error::CliResult, output::emit};

pub async fn run(context: &CliContext, command: DemoCommand) -> CliResult<()> {
    let value = match command {
        DemoCommand::Seed => seed_demo()?,
        DemoCommand::Run => release_demo().await?,
        DemoCommand::Blockchain => blockchain_demo()?,
        DemoCommand::Tamper => tamper_demo()?,
        DemoCommand::Recovery => recovery_demo()?,
        DemoCommand::Snapshot => snapshot_demo()?,
        DemoCommand::Network => network_demo().await?,
    };
    emit(&value, context.json, "MiniChain demonstration")
}

fn seed_demo() -> CliResult<serde_json::Value> {
    let directory = TempDir::new().map_err(|error| crate::MiniChainError::StorageCorruption {
        reason: error.to_string(),
    })?;
    let storage = RedbStorage::open(
        directory.path().join("seed.redb"),
        Uuid::new_v4(),
        "seed-demo",
    )?;
    let identity = demo_identity(0);
    let transactions = demo_transactions(&identity);
    let chain = storage.recover()?;
    storage.commit_block(Block::new(
        1,
        chain.tip().hash.clone(),
        transactions.clone(),
        identity.validator_id(),
    )?)?;
    for transaction in transactions
        .iter()
        .filter(|transaction| transaction.operation == Operation::CreateRecord)
    {
        storage.verify_record(&transaction.record_id)?;
    }
    Ok(json!({
        "demo": "seed",
        "records": transactions.iter().map(|transaction| json!({
            "id": transaction.record_id,
            "operation": transaction.operation,
            "demo_data": transaction.metadata.get("demo_data") == Some(&json!(true)),
        })).collect::<Vec<_>>(),
        "height": storage.recover()?.height(),
        "chain_valid": storage.recover()?.validate().valid,
        "temporary_storage": true,
    }))
}

async fn release_demo() -> CliResult<serde_json::Value> {
    let directories = (0..3)
        .map(|_| TempDir::new())
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| crate::MiniChainError::StorageCorruption {
            reason: error.to_string(),
        })?;
    let addresses = (0..3).map(|_| available_address()).collect::<Vec<_>>();
    let chain_id = Uuid::new_v4();
    let storages = directories
        .iter()
        .map(|directory| {
            RedbStorage::open(directory.path().join("node.redb"), chain_id, "release-demo")
                .map(Arc::new)
        })
        .collect::<crate::Result<Vec<_>>>()?;
    let mut node_handles = Vec::new();
    for index in 0..3 {
        let trusted_peers = (0..3)
            .filter(|peer| *peer != index)
            .map(|peer| {
                let identity = demo_identity(peer);
                (
                    identity.validator_id().to_owned(),
                    TrustedPeer {
                        address: addresses[peer].clone(),
                        public_key: identity.public_key().to_vec(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        node_handles.push(NetworkNode::new(
            NodeConfig {
                node_id: format!("demo-node-{}", index + 1),
                listen_address: addresses[index].clone(),
                trusted_peers,
                max_peers: 8,
                chain_id,
                network_id: "release-demo".to_owned(),
                storage_path: directories[index].path().join("node.redb"),
                identity_path: directories[index].path().join("node.key"),
                heartbeat_interval_ms: 60_000,
                api: None,
            },
            demo_identity(index),
            Arc::clone(&storages[index]),
            Mempool::new(100, Duration::from_secs(60)),
        )?);
    }
    let mut running = Vec::new();
    for node in &node_handles {
        running.push(Some(node.clone().start().await?));
    }
    node_handles[0].connect("demo-node-2").await?;
    node_handles[0].connect("demo-node-3").await?;

    let transactions = demo_transactions(&demo_identity(0));
    for transaction in &transactions {
        node_handles[0]
            .broadcast_transaction(transaction.clone())
            .await?;
    }
    let first_block = approved_block(&storages[0], transactions, &node_handles)?;
    node_handles[0]
        .commit_and_broadcast_block(first_block)
        .await?;
    let initial = status_for_nodes(&node_handles).await?;
    ensure_consistent(&initial)?;
    for storage in &storages {
        storage.verify_record("DEMO-CERT-001")?;
    }

    let snapshot = storages[0].create_snapshot()?;
    storages[0].verify_snapshot(snapshot.id)?;

    drop(running[2].take());
    tokio::time::sleep(Duration::from_millis(50)).await;
    let update = Transaction::new(
        Operation::UpdateRecord,
        "DEMO-CERT-001",
        json!({
            "demo_data": true,
            "status": "verified",
            "note": "Updated while demo-node-3 was offline"
        }),
        demo_metadata(),
        &demo_identity(0),
    );
    let second_block = approved_block(&storages[0], vec![update], &node_handles)?;
    let offline_broadcast_detected = node_handles[0]
        .commit_and_broadcast_block(second_block)
        .await
        .is_err();
    if storages[0].recover()?.height() != 2 || storages[1].recover()?.height() != 2 {
        return Err(crate::MiniChainError::SyncFailed.into());
    }

    running[2] = Some(node_handles[2].clone().start().await?);
    node_handles[2].sync_from("demo-node-1").await?;
    let recovered = status_for_nodes(&node_handles).await?;
    ensure_consistent(&recovered)?;
    let record = storages[2].verify_record("DEMO-CERT-001")?;

    let valid_chain = storages[0].recover()?;
    let mut altered_blocks = valid_chain.blocks().to_vec();
    altered_blocks[1].transactions[0].payload = json!({"demo_data": true, "tampered": true});
    let tampering_detected = Blockchain::from_blocks(altered_blocks).is_err();

    let restored = node_handles[0].restore_snapshot(snapshot.id, true).await?;
    let snapshot_height = restored.height();
    node_handles[0].sync_from("demo-node-2").await?;
    let final_statuses = status_for_nodes(&node_handles).await?;
    ensure_consistent(&final_statuses)?;
    let final_valid = storages.iter().all(|storage| {
        storage
            .recover()
            .is_ok_and(|chain| chain.validate().valid && chain.height() == 2)
    });
    if !tampering_detected || !final_valid {
        return Err(crate::MiniChainError::SyncFailed.into());
    }

    Ok(json!({
        "demo": "release",
        "steps": [
            {"step": 1, "name": "Start nodes", "status": "PASS"},
            {"step": 2, "name": "Authenticate peers", "status": "PASS"},
            {"step": 3, "name": "Seed demo records", "status": "PASS"},
            {"step": 4, "name": "Authority quorum", "status": "PASS"},
            {"step": 5, "name": "Replicate first block", "status": "PASS"},
            {"step": 6, "name": "Verify records", "status": "PASS"},
            {"step": 7, "name": "Create and verify snapshot", "status": "PASS"},
            {"step": 8, "name": "Detect offline node", "status": if offline_broadcast_detected {"PASS"} else {"FAIL"}},
            {"step": 9, "name": "Restart and synchronize node", "status": "PASS"},
            {"step": 10, "name": "Detect controlled tampering", "status": "PASS"},
            {"step": 11, "name": "Restore snapshot in demo state", "status": "PASS"},
            {"step": 12, "name": "Resynchronize and validate", "status": "PASS"}
        ],
        "network": "CONSISTENT",
        "chain": "VALID",
        "height": final_statuses[0].height,
        "latest_hash": final_statuses[0].latest_hash,
        "snapshot_height": snapshot_height,
        "record_status": record.record.status,
        "tampering_detected": tampering_detected,
        "consensus_scope": "signed in-memory authority quorum; distributed consensus transport is not implemented",
        "temporary_storage": true,
    }))
}

fn demo_transactions(identity: &ValidatorIdentity) -> Vec<Transaction> {
    [
        (
            "DEMO-CERT-001",
            "certificate",
            "Certificate of systems study",
        ),
        (
            "DEMO-COURSE-001",
            "course_completion",
            "Distributed systems laboratory",
        ),
        ("DEMO-ACHIEVEMENT-001", "achievement", "Project milestone"),
        (
            "DEMO-DOCUMENT-001",
            "document_verification",
            "Institutional document verification",
        ),
    ]
    .into_iter()
    .map(|(id, kind, description)| {
        Transaction::new(
            Operation::CreateRecord,
            id,
            json!({
                "demo_data": true,
                "record_type": kind,
                "description": description,
                "subject": "Synthetic learner DEMO-001"
            }),
            demo_metadata(),
            identity,
        )
    })
    .chain(std::iter::once(Transaction::new(
        Operation::AuditEvent,
        "DEMO-AUDIT-001",
        json!({
            "demo_data": true,
            "event": "release demonstration seeded",
            "outcome": "success"
        }),
        demo_metadata(),
        identity,
    )))
    .collect()
}

fn demo_metadata() -> BTreeMap<String, serde_json::Value> {
    BTreeMap::from([
        ("demo_data".to_owned(), json!(true)),
        ("environment".to_owned(), json!("isolated-temporary")),
    ])
}

fn approved_block(
    storage: &RedbStorage,
    transactions: Vec<Transaction>,
    _nodes: &[NetworkNode],
) -> CliResult<Block> {
    let chain = storage.recover()?;
    let identities = (0..3).map(demo_identity).collect::<Vec<_>>();
    let mut registry = ValidatorRegistry::default();
    for identity in &identities {
        registry.register(Validator::new(
            identity.validator_id(),
            identity.public_key().to_vec(),
            "release-demo",
            [
                Permission::ProposeBlocks,
                Permission::ApproveBlocks,
                Permission::SubmitTransactions,
            ],
        )?)?;
    }
    let block = Block::new(
        chain.height() + 1,
        chain.tip().hash.clone(),
        transactions,
        identities[0].validator_id(),
    )?;
    let proposal = Proposal::new(block.clone(), &identities[0]);
    let mut consensus = AuthorityConsensus::new(block.header.index, chain.tip().hash.clone());
    consensus.register_proposal(&proposal, &registry)?;
    for identity in &identities {
        consensus.approve(Approval::sign(&block.hash, identity), &registry)?;
    }
    consensus.ensure_quorum(&block.hash, &registry)?;
    Ok(block)
}

async fn status_for_nodes(nodes: &[NetworkNode]) -> CliResult<Vec<crate::network::NetworkStatus>> {
    let mut statuses = Vec::new();
    for node in nodes {
        statuses.push(node.local_status().await?);
    }
    Ok(statuses)
}

fn ensure_consistent(statuses: &[crate::network::NetworkStatus]) -> CliResult<()> {
    if statuses.is_empty()
        || !statuses.iter().all(|status| {
            status.height == statuses[0].height && status.latest_hash == statuses[0].latest_hash
        })
    {
        return Err(crate::MiniChainError::SyncFailed.into());
    }
    Ok(())
}

fn blockchain_demo() -> CliResult<serde_json::Value> {
    let identity = ValidatorIdentity::from_secret_bytes("demo-validator", [201; 32]);
    let mut chain = Blockchain::new()?;
    let transaction = Transaction::new(
        Operation::CreateRecord,
        "DEMO-RECORD",
        json!({"type": "demonstration", "valid": true}),
        Default::default(),
        &identity,
    );
    let block = Block::new(
        1,
        chain.tip().hash.clone(),
        vec![transaction],
        "demo-validator",
    )?;
    chain.append(block)?;
    let validation = chain.validate();
    Ok(json!({
        "demo": "blockchain",
        "height": chain.height(),
        "head": chain.tip().hash,
        "valid": validation.valid,
        "checked_blocks": validation.checked_blocks,
    }))
}

fn tamper_demo() -> CliResult<serde_json::Value> {
    let identity = ValidatorIdentity::from_secret_bytes("demo-validator", [202; 32]);
    let mut chain = Blockchain::new()?;
    let transaction = Transaction::new(
        Operation::CreateRecord,
        "TAMPER-DEMO",
        json!({"original": true}),
        Default::default(),
        &identity,
    );
    chain.append(Block::new(
        1,
        chain.tip().hash.clone(),
        vec![transaction],
        "demo-validator",
    )?)?;
    let mut blocks = chain.blocks().to_vec();
    blocks[1].transactions[0].payload = json!({"original": false});
    let detected = Blockchain::from_blocks(blocks).is_err();
    Ok(json!({
        "demo": "tamper",
        "original_chain_valid": chain.validate().valid,
        "tampering_detected": detected,
        "user_storage_modified": false,
    }))
}

fn recovery_demo() -> CliResult<serde_json::Value> {
    let directory = TempDir::new().map_err(|error| crate::MiniChainError::StorageCorruption {
        reason: error.to_string(),
    })?;
    let chain_id = Uuid::new_v4();
    let path = directory.path().join("recovery.redb");
    let storage = RedbStorage::open(&path, chain_id, "recovery-demo")?;
    let before = storage.recover()?.tip().hash.clone();
    drop(storage);
    let recovered = RedbStorage::open(&path, chain_id, "recovery-demo")?.recover()?;
    Ok(json!({
        "demo": "recovery",
        "height": recovered.height(),
        "same_head": recovered.tip().hash == before,
        "temporary_storage": true,
    }))
}

fn snapshot_demo() -> CliResult<serde_json::Value> {
    let directory = TempDir::new().map_err(|error| crate::MiniChainError::StorageCorruption {
        reason: error.to_string(),
    })?;
    let storage = RedbStorage::open(
        directory.path().join("snapshot.redb"),
        Uuid::new_v4(),
        "snapshot-demo",
    )?;
    let snapshot = storage.create_snapshot()?;
    let verified = storage.verify_snapshot(snapshot.id)?;
    Ok(json!({
        "demo": "snapshot",
        "snapshot_id": snapshot.id,
        "height": verified.height(),
        "integrity_valid": verified.verify_integrity(),
        "temporary_storage": true,
    }))
}

async fn network_demo() -> CliResult<serde_json::Value> {
    let directories = (0..3)
        .map(|_| TempDir::new())
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| crate::MiniChainError::StorageCorruption {
            reason: error.to_string(),
        })?;
    let addresses = (0..3).map(|_| available_address()).collect::<Vec<_>>();
    let chain_id = Uuid::new_v4();
    let storages = directories
        .iter()
        .map(|directory| {
            RedbStorage::open(directory.path().join("node.redb"), chain_id, "network-demo")
                .map(Arc::new)
        })
        .collect::<crate::Result<Vec<_>>>()?;
    let mut nodes = Vec::new();
    for index in 0..3 {
        let trusted_peers = (0..3)
            .filter(|peer| *peer != index)
            .map(|peer| {
                let identity = demo_identity(peer);
                (
                    identity.validator_id().to_owned(),
                    TrustedPeer {
                        address: addresses[peer].clone(),
                        public_key: identity.public_key().to_vec(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let node = NetworkNode::new(
            NodeConfig {
                node_id: format!("demo-node-{}", index + 1),
                listen_address: addresses[index].clone(),
                trusted_peers,
                max_peers: 8,
                chain_id,
                network_id: "network-demo".to_owned(),
                storage_path: directories[index].path().join("node.redb"),
                identity_path: directories[index].path().join("node.key"),
                heartbeat_interval_ms: 60_000,
                api: None,
            },
            demo_identity(index),
            Arc::clone(&storages[index]),
            Mempool::new(100, Duration::from_secs(60)),
        )?;
        nodes.push(node.start().await?);
    }
    nodes[0].node.connect("demo-node-2").await?;
    nodes[0].node.connect("demo-node-3").await?;
    let transaction = Transaction::new(
        Operation::CreateRecord,
        "NETWORK-DEMO",
        json!({"propagated": true}),
        Default::default(),
        &demo_identity(0),
    );
    nodes[0]
        .node
        .broadcast_transaction(transaction.clone())
        .await?;
    let mut transaction_propagated = true;
    for node in &nodes {
        transaction_propagated &= node.node.mempool_len().await == 1;
    }
    let genesis = storages[0].get_block(0)?;
    let block = Block::new(1, genesis.hash, vec![transaction], "demo-node-1")?;
    nodes[0].node.commit_and_broadcast_block(block).await?;
    let statuses = futures_statuses(&nodes).await?;
    let consistent = statuses.iter().all(|status| {
        status.height == statuses[0].height && status.latest_hash == statuses[0].latest_hash
    });
    Ok(json!({
        "demo": "network",
        "nodes": statuses,
        "transaction_propagated": transaction_propagated,
        "consistent": consistent,
        "temporary_storage": true,
    }))
}

async fn futures_statuses(
    nodes: &[crate::network::RunningNode],
) -> CliResult<Vec<crate::network::NetworkStatus>> {
    let mut statuses = Vec::new();
    for node in nodes {
        statuses.push(node.node.local_status().await?);
    }
    Ok(statuses)
}

fn demo_identity(index: usize) -> ValidatorIdentity {
    ValidatorIdentity::from_secret_bytes(
        format!("demo-node-{}", index + 1),
        [210 + index as u8; 32],
    )
}

fn available_address() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback is available");
    listener
        .local_addr()
        .expect("listener has an address")
        .to_string()
}
