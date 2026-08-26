use std::collections::BTreeMap;

use chrono::{Duration, TimeZone, Utc};
use minichain::{
    Block, Blockchain, MiniChainError, Operation, Transaction, ValidatorIdentity,
    blockchain::{BlockHeader, CURRENT_BLOCK_VERSION},
};
use serde_json::json;
use uuid::Uuid;

fn identity() -> ValidatorIdentity {
    ValidatorIdentity::from_secret_bytes("validator-01", [7; 32])
}

fn transaction(id: u128) -> Transaction {
    Transaction::with_identity(
        Uuid::from_u128(id),
        Utc.timestamp_opt(1_735_689_700 + id as i64, 0)
            .single()
            .unwrap(),
        Operation::CreateRecord,
        format!("CERT-{id}"),
        serde_json::json!({"course": "MCA", "institution": "Example Institute"}),
        BTreeMap::new(),
        &identity(),
    )
}

fn fixed_block(index: u64, previous_hash: String, transactions: Vec<Transaction>) -> Block {
    let provisional = Block::new(
        index,
        previous_hash.clone(),
        transactions.clone(),
        "validator-01",
    )
    .unwrap();
    let header = BlockHeader {
        index,
        block_id: Uuid::from_u128(index as u128 + 100),
        timestamp: Utc
            .timestamp_opt(1_735_689_600 + index as i64 * 200, 0)
            .single()
            .unwrap(),
        previous_hash,
        merkle_root: provisional.header.merkle_root,
        validator_id: "validator-01".to_owned(),
        validator_signature: None,
        version: CURRENT_BLOCK_VERSION,
    };
    Block::from_header(header, transactions)
}

#[test]
fn a_new_chain_contains_a_stable_genesis_block() {
    let first = Blockchain::new().unwrap();
    let second = Blockchain::new().unwrap();
    assert_eq!(first.blocks(), second.blocks());
    assert_eq!(first.height(), 0);
    assert!(first.validate().valid);
}

#[test]
fn block_hashing_is_deterministic_after_serialization() {
    let block = fixed_block(1, "a".repeat(64), vec![transaction(1)]);
    let serialized = serde_json::to_string(&block).unwrap();
    let restored: Block = serde_json::from_str(&serialized).unwrap();
    assert_eq!(block.hash, restored.calculate_hash());
    assert_eq!(block, restored);
}

#[test]
fn appending_a_linked_block_advances_the_height() {
    let mut chain = Blockchain::new().unwrap();
    let block = fixed_block(1, chain.tip().hash.clone(), vec![transaction(1)]);
    chain.append(block).unwrap();
    assert_eq!(chain.height(), 1);
    assert!(chain.validate().valid);
}

#[test]
fn a_block_with_the_wrong_parent_is_rejected() {
    let mut chain = Blockchain::new().unwrap();
    let block = fixed_block(1, "f".repeat(64), vec![transaction(1)]);
    assert_eq!(
        chain.append(block).unwrap_err(),
        MiniChainError::InvalidPreviousHash { index: 1 }
    );
}

#[test]
fn modified_transaction_data_is_rejected() {
    let mut chain = Blockchain::new().unwrap();
    let block = fixed_block(1, chain.tip().hash.clone(), vec![transaction(1)]);
    chain.append(block).unwrap();
    let mut stored_blocks = chain.blocks().to_vec();
    stored_blocks[1].transactions[0].payload = json!({"course": "changed"});
    assert!(matches!(
        Blockchain::from_blocks(stored_blocks).unwrap_err(),
        MiniChainError::InvalidTransactionHash { .. }
    ));
}

#[test]
fn wrong_merkle_root_is_rejected_even_with_a_recalculated_block_hash() {
    let mut chain = Blockchain::new().unwrap();
    let mut block = fixed_block(1, chain.tip().hash.clone(), vec![transaction(1)]);
    block.header.merkle_root = "0".repeat(64);
    block.hash = block.calculate_hash();
    assert_eq!(
        chain.append(block).unwrap_err(),
        MiniChainError::InvalidMerkleRoot { index: 1 }
    );
}

#[test]
fn duplicate_transaction_across_blocks_is_rejected() {
    let mut chain = Blockchain::new().unwrap();
    let repeated = transaction(1);
    let first = fixed_block(1, chain.tip().hash.clone(), vec![repeated.clone()]);
    chain.append(first).unwrap();
    let second = fixed_block(2, chain.tip().hash.clone(), vec![repeated.clone()]);
    assert_eq!(
        chain.append(second).unwrap_err(),
        MiniChainError::DuplicateTransaction { id: repeated.id }
    );
}

#[test]
fn a_timestamp_before_the_parent_is_rejected() {
    let mut chain = Blockchain::new().unwrap();
    let mut block = fixed_block(1, chain.tip().hash.clone(), vec![transaction(1)]);
    block.header.timestamp = chain.tip().header.timestamp - Duration::seconds(1);
    block.hash = block.calculate_hash();
    assert_eq!(
        chain.append(block).unwrap_err(),
        MiniChainError::BlockTimestampBeforeParent { index: 1 }
    );
}
