use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::{Block, BlockHeader, CURRENT_BLOCK_VERSION, empty_merkle_root};

const GENESIS_ID: Uuid = Uuid::from_u128(0x6d696e69636861696e00000000000001);
const GENESIS_VALIDATOR: &str = "minichain-genesis";

pub fn create_genesis_block() -> Block {
    let header = BlockHeader {
        index: 0,
        block_id: GENESIS_ID,
        timestamp: Utc
            .timestamp_opt(1_735_689_600, 0)
            .single()
            .expect("the fixed genesis timestamp is valid"),
        previous_hash: "0".repeat(64),
        merkle_root: empty_merkle_root(),
        validator_id: GENESIS_VALIDATOR.to_owned(),
        validator_signature: None,
        version: CURRENT_BLOCK_VERSION,
    };
    Block::from_header(header, Vec::new())
}
