use std::{collections::BTreeMap, sync::Arc};

use minichain::{Block, Blockchain, MiniChainError, Operation, Transaction, ValidatorIdentity};
use serde_json::json;

fn transaction() -> Transaction {
    Transaction::new(
        Operation::AuditEvent,
        "AUDIT-1",
        json!({"event": "created"}),
        BTreeMap::new(),
        &ValidatorIdentity::from_secret_bytes("validator-1", [12; 32]),
    )
}

#[test]
fn incorrect_hash_index_parent_and_empty_block_return_specific_errors() {
    let mut chain = Blockchain::new().unwrap();
    let mut wrong_hash = Block::new(
        1,
        chain.tip().hash.clone(),
        vec![transaction()],
        "validator-1",
    )
    .unwrap();
    wrong_hash.hash = "0".repeat(64);
    assert!(matches!(
        chain.append(wrong_hash),
        Err(MiniChainError::InvalidBlockHash { index: 1, .. })
    ));

    let wrong_index = Block::new(
        2,
        chain.tip().hash.clone(),
        vec![transaction()],
        "validator-1",
    )
    .unwrap();
    assert!(matches!(
        chain.append(wrong_index),
        Err(MiniChainError::InvalidBlockIndex {
            expected: 1,
            actual: 2,
            ..
        })
    ));

    let wrong_parent = Block::new(1, "f".repeat(64), vec![transaction()], "validator-1").unwrap();
    assert_eq!(
        chain.append(wrong_parent).unwrap_err(),
        MiniChainError::InvalidPreviousHash { index: 1 }
    );

    let empty = Block::new(1, chain.tip().hash.clone(), Vec::new(), "validator-1").unwrap();
    assert_eq!(
        chain.append(empty).unwrap_err(),
        MiniChainError::EmptyBlock { index: 1 }
    );
}

#[test]
fn malformed_and_unknown_serialized_block_data_is_rejected() {
    assert!(serde_json::from_str::<Block>(r#"{"header": "#).is_err());

    let block = Block::new(1, "0".repeat(64), vec![transaction()], "validator-1").unwrap();
    let mut value = serde_json::to_value(block).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unknown_field".to_owned(), json!(42));
    assert!(serde_json::from_value::<Block>(value).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_chain_validation_is_deterministic() {
    let mut chain = Blockchain::new().unwrap();
    let transactions = (0..100)
        .map(|index| {
            Transaction::new(
                Operation::AuditEvent,
                format!("AUDIT-{index}"),
                json!({"sequence": index}),
                BTreeMap::new(),
                &ValidatorIdentity::from_secret_bytes("validator-1", [12; 32]),
            )
        })
        .collect();
    chain
        .append(Block::new(1, chain.tip().hash.clone(), transactions, "validator-1").unwrap())
        .unwrap();
    let chain = Arc::new(chain);

    let mut tasks = Vec::new();
    for _ in 0..16 {
        let chain = Arc::clone(&chain);
        tasks.push(tokio::task::spawn_blocking(move || chain.validate()));
    }
    for task in tasks {
        let result = task.await.unwrap();
        assert!(result.valid);
        assert_eq!(result.checked_blocks, 2);
    }
}
