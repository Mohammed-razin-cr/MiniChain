use std::{collections::BTreeMap, time::Instant};

use minichain::{
    Block, Operation, Transaction, ValidatorIdentity,
    storage::{RedbStorage, Storage},
};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

#[test]
#[ignore = "manual release-mode storage measurement"]
fn measure_storage_pipeline() {
    for block_count in [100u64, 500, 1_000, 5_000] {
        let directory = TempDir::new().unwrap();
        let storage = RedbStorage::open(
            directory.path().join("performance.redb"),
            Uuid::new_v4(),
            "performance",
        )
        .unwrap();
        let identity = ValidatorIdentity::from_secret_bytes("validator-perf", [51; 32]);
        let mut previous = storage.latest_block().unwrap();
        let mut last_transaction = None;

        let started = Instant::now();
        for height in 1..=block_count {
            let transaction = Transaction::new(
                Operation::UpdateRecord,
                "PERF-RECORD",
                json!({"height": height}),
                BTreeMap::new(),
                &identity,
            );
            let block = Block::new(
                height,
                previous.hash.clone(),
                vec![transaction.clone()],
                identity.validator_id(),
            )
            .unwrap();
            storage.commit_block(block.clone()).unwrap();
            previous = block;
            last_transaction = Some(transaction.id);
        }
        let writes = started.elapsed();

        let started = Instant::now();
        for height in (1..=block_count).rev().take(100) {
            storage.get_block(height).unwrap();
        }
        let block_reads = started.elapsed();
        let started = Instant::now();
        storage.get_transaction(last_transaction.unwrap()).unwrap();
        let transaction_lookup = started.elapsed();
        let started = Instant::now();
        storage.get_record("PERF-RECORD").unwrap();
        let record_lookup = started.elapsed();
        let started = Instant::now();
        storage.latest_block().unwrap();
        let latest_lookup = started.elapsed();
        let started = Instant::now();
        storage.recover().unwrap();
        let recovery = started.elapsed();
        let started = Instant::now();
        let snapshot = storage.create_snapshot().unwrap();
        let snapshot_creation = started.elapsed();
        let started = Instant::now();
        storage.restore_snapshot(snapshot.id, true).unwrap();
        let snapshot_restoration = started.elapsed();

        println!(
            "blocks={block_count} writes={writes:?} reads_100={block_reads:?} transaction_lookup={transaction_lookup:?} record_lookup={record_lookup:?} latest_lookup={latest_lookup:?} recovery={recovery:?} snapshot_create={snapshot_creation:?} snapshot_restore={snapshot_restoration:?}"
        );
    }
}
