use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use minichain::{Block, Blockchain, Operation, Transaction, ValidatorIdentity, mempool::Mempool};
use serde_json::json;

#[tokio::test]
#[ignore = "manual release-mode measurement"]
async fn measure_transaction_and_block_pipeline() {
    let identity = ValidatorIdentity::from_secret_bytes("benchmark-validator", [21; 32]);

    for count in [100usize, 500, 1_000, 5_000] {
        let started = Instant::now();
        let transactions = (0..count)
            .map(|index| {
                Transaction::new(
                    Operation::AuditEvent,
                    format!("BENCH-{index}"),
                    json!({"sequence": index, "status": "verified"}),
                    BTreeMap::new(),
                    &identity,
                )
            })
            .collect::<Vec<_>>();
        let creation = started.elapsed();

        let pool = Mempool::new(count, Duration::from_secs(60));
        let started = Instant::now();
        for transaction in transactions.iter().cloned() {
            pool.insert(transaction).await.unwrap();
        }
        let submission = started.elapsed();

        let started = Instant::now();
        let block = Block::new(1, "0".repeat(64), transactions, identity.validator_id()).unwrap();
        let block_creation = started.elapsed();

        let started = Instant::now();
        block.validate_contents().unwrap();
        let validation = started.elapsed();

        println!(
            "count={count} transaction_creation={creation:?} mempool_submission={submission:?} block_creation={block_creation:?} block_validation={validation:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "manual release-mode concurrency measurement"]
async fn measure_concurrent_mempool_submission() {
    let identity = ValidatorIdentity::from_secret_bytes("benchmark-validator", [22; 32]);

    for clients in [10usize, 50, 100] {
        let transaction_count = 5_000;
        let transactions = (0..transaction_count)
            .map(|index| {
                Transaction::new(
                    Operation::AuditEvent,
                    format!("CONCURRENT-{clients}-{index}"),
                    json!({"sequence": index}),
                    BTreeMap::new(),
                    &identity,
                )
            })
            .collect::<Vec<_>>();
        let pool = Mempool::new(transaction_count, Duration::from_secs(60));
        let chunk_size = transaction_count.div_ceil(clients);
        let started = Instant::now();
        let mut tasks = Vec::new();
        for chunk in transactions.chunks(chunk_size) {
            let pool = pool.clone();
            let chunk = chunk.to_vec();
            tasks.push(tokio::spawn(async move {
                for transaction in chunk {
                    pool.insert(transaction).await.unwrap();
                }
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        let elapsed = started.elapsed();
        assert_eq!(pool.len().await, transaction_count);
        println!(
            "clients={clients} transactions={transaction_count} elapsed={elapsed:?} throughput_tps={:.2}",
            transaction_count as f64 / elapsed.as_secs_f64()
        );
    }
}

#[test]
#[ignore = "manual release-mode block-size and chain-validation measurement"]
fn measure_block_sizes_and_chain_validation() {
    let identity = ValidatorIdentity::from_secret_bytes("benchmark-validator", [23; 32]);
    let genesis = Blockchain::new().unwrap().tip().clone();
    for transaction_count in [1usize, 10, 50, 100] {
        let transactions = transactions(&identity, transaction_count, "BLOCK");
        let started = Instant::now();
        let block = Block::new(
            1,
            genesis.hash.clone(),
            transactions,
            identity.validator_id(),
        )
        .unwrap();
        let creation = started.elapsed();
        let serialized_bytes = serde_json::to_vec(&block).unwrap().len();
        let started = Instant::now();
        block.validate_contents().unwrap();
        let validation = started.elapsed();
        println!(
            "block_transactions={transaction_count} serialized_bytes={serialized_bytes} creation={creation:?} validation={validation:?}"
        );
    }

    for block_count in [100usize, 500, 1_000, 5_000] {
        let mut blocks = vec![genesis.clone()];
        for height in 1..=block_count {
            blocks.push(
                Block::new(
                    height as u64,
                    blocks.last().unwrap().hash.clone(),
                    transactions(&identity, 1, &format!("CHAIN-{height}")),
                    identity.validator_id(),
                )
                .unwrap(),
            );
        }
        let chain = Blockchain::from_blocks(blocks).unwrap();
        let started = Instant::now();
        let report = chain.validate();
        let elapsed = started.elapsed();
        assert!(report.valid);
        println!("chain_blocks={block_count} validation={elapsed:?}");
    }
}

fn transactions(identity: &ValidatorIdentity, count: usize, prefix: &str) -> Vec<Transaction> {
    (0..count)
        .map(|index| {
            Transaction::new(
                Operation::AuditEvent,
                format!("{prefix}-{index}"),
                json!({"sequence": index, "status": "verified"}),
                BTreeMap::new(),
                identity,
            )
        })
        .collect()
}
