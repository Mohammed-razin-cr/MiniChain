use std::{collections::BTreeMap, sync::Arc};

use minichain::{
    Block, MiniChainError, Operation, Transaction, ValidatorIdentity,
    consensus::{Permission, Validator},
    storage::{RecordStatus, RedbStorage, Storage},
};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

fn open_storage(directory: &TempDir, chain_id: Uuid) -> RedbStorage {
    RedbStorage::open(directory.path().join("chain.redb"), chain_id, "local-test").unwrap()
}

fn append(storage: &RedbStorage, sequence: u64, operation: Operation) -> Transaction {
    let chain = storage.recover().unwrap();
    let transaction = Transaction::new(
        operation,
        "CERT-100",
        json!({"sequence": sequence, "course": "MCA"}),
        BTreeMap::new(),
        &ValidatorIdentity::from_secret_bytes("validator-1", [31; 32]),
    );
    let block = Block::new(
        chain.height() + 1,
        chain.tip().hash.clone(),
        vec![transaction.clone()],
        "validator-1",
    )
    .unwrap();
    storage.commit_block(block).unwrap();
    transaction
}

#[test]
fn fresh_storage_persists_genesis_and_metadata() {
    let directory = TempDir::new().unwrap();
    let chain_id = Uuid::new_v4();
    let storage = open_storage(&directory, chain_id);

    assert_eq!(storage.recover().unwrap().height(), 0);
    assert_eq!(storage.metadata().unwrap().chain_id, chain_id);
    assert_eq!(
        storage.get_block(0).unwrap(),
        storage.latest_block().unwrap()
    );
    assert_eq!(storage.stats().unwrap().blocks, 1);
}

#[test]
fn blocks_transactions_records_and_hash_index_are_queryable() {
    let directory = TempDir::new().unwrap();
    let storage = open_storage(&directory, Uuid::new_v4());
    let transaction = append(&storage, 1, Operation::CreateRecord);
    let block = storage.get_block(1).unwrap();

    assert_eq!(storage.get_block_by_hash(&block.hash).unwrap(), block);
    let stored = storage.get_transaction(transaction.id).unwrap();
    assert_eq!(stored.block_height, 1);
    assert_eq!(stored.transaction, transaction);
    let record = storage.get_record("CERT-100").unwrap();
    assert_eq!(record.status, RecordStatus::Active);
    assert_eq!(record.block_height, 1);
}

#[test]
fn restart_recovers_and_continues_from_the_persisted_height() {
    let directory = TempDir::new().unwrap();
    let chain_id = Uuid::new_v4();
    {
        let storage = open_storage(&directory, chain_id);
        append(&storage, 1, Operation::CreateRecord);
        append(&storage, 2, Operation::UpdateRecord);
        assert_eq!(storage.recover().unwrap().height(), 2);
    }

    let reopened = open_storage(&directory, chain_id);
    assert_eq!(reopened.recover().unwrap().height(), 2);
    let third = append(&reopened, 3, Operation::UpdateRecord);
    assert_eq!(reopened.recover().unwrap().height(), 3);
    assert_eq!(reopened.get_transaction(third.id).unwrap().block_height, 3);
    assert_eq!(reopened.get_record("CERT-100").unwrap().block_height, 3);
}

#[test]
fn snapshot_restore_removes_newer_state_and_chain_can_continue() {
    let directory = TempDir::new().unwrap();
    let storage = open_storage(&directory, Uuid::new_v4());
    for sequence in 1..=5 {
        append(&storage, sequence, Operation::UpdateRecord);
    }
    let snapshot = storage.create_snapshot().unwrap();
    assert_eq!(storage.verify_snapshot(snapshot.id).unwrap().height(), 5);
    append(&storage, 6, Operation::UpdateRecord);
    append(&storage, 7, Operation::UpdateRecord);
    assert_eq!(storage.recover().unwrap().height(), 7);
    assert_eq!(
        storage.restore_snapshot(snapshot.id, false).unwrap_err(),
        MiniChainError::RestoreWouldOverwrite
    );

    let restored = storage.restore_snapshot(snapshot.id, true).unwrap();
    assert_eq!(restored.height(), 5);
    append(&storage, 6, Operation::UpdateRecord);
    assert_eq!(storage.recover().unwrap().height(), 6);
    assert_eq!(storage.get_record("CERT-100").unwrap().block_height, 6);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_reads_return_consistent_results() {
    let directory = TempDir::new().unwrap();
    let storage = Arc::new(open_storage(&directory, Uuid::new_v4()));
    let transaction = append(&storage, 1, Operation::CreateRecord);
    let mut tasks = Vec::new();
    for _ in 0..32 {
        let storage = Arc::clone(&storage);
        tasks.push(tokio::task::spawn_blocking(move || {
            let block = storage.get_block(1)?;
            let indexed = storage.get_transaction(transaction.id)?;
            let record = storage.get_record("CERT-100")?;
            Ok::<_, MiniChainError>((block, indexed, record))
        }));
    }
    for task in tasks {
        let (block, indexed, record) = task.await.unwrap().unwrap();
        assert_eq!(block.header.index, 1);
        assert_eq!(indexed.block_height, 1);
        assert_eq!(record.block_height, 1);
    }
}

#[test]
fn invalid_database_path_fails_clearly() {
    let directory = TempDir::new().unwrap();
    let result = RedbStorage::open(
        directory.path().join("missing-parent").join("chain.redb"),
        Uuid::new_v4(),
        "local-test",
    );
    assert!(matches!(result, Err(MiniChainError::StorageUnavailable)));
}

#[test]
fn validator_state_survives_restart() {
    let directory = TempDir::new().unwrap();
    let chain_id = Uuid::new_v4();
    let identity = ValidatorIdentity::from_secret_bytes("validator-persisted", [61; 32]);
    let mut validator = Validator::new(
        identity.validator_id(),
        identity.public_key().to_vec(),
        "127.0.0.1:8081",
        [Permission::ProposeBlocks, Permission::ApproveBlocks],
    )
    .unwrap();
    validator.block_height = 42;
    {
        let storage = open_storage(&directory, chain_id);
        storage.save_validator(&validator).unwrap();
    }
    let reopened = open_storage(&directory, chain_id);
    assert_eq!(reopened.get_validator(&validator.id).unwrap(), validator);
}
