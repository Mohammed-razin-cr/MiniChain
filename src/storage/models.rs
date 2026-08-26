use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::Transaction;

pub const STORAGE_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainMetadata {
    pub schema_version: u16,
    pub chain_id: Uuid,
    pub network_id: String,
    pub current_height: u64,
    pub latest_block_hash: String,
    pub genesis_hash: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordStatus {
    Active,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordIndex {
    pub record_id: String,
    pub data: Value,
    pub status: RecordStatus,
    pub latest_transaction_id: Uuid,
    pub block_height: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredTransaction {
    pub transaction: Transaction,
    pub block_height: u64,
    pub block_hash: String,
    pub transaction_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageStats {
    pub blocks: u64,
    pub transactions: u64,
    pub records: u64,
    pub current_height: u64,
    pub latest_block_hash: String,
    pub database_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecordVerification {
    pub record: RecordIndex,
    pub latest_transaction: StoredTransaction,
    pub cryptographically_verified: bool,
    pub merkle_proof_valid: bool,
    pub chain_valid: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SnapshotPayload {
    pub version: u16,
    pub chain_id: Uuid,
    pub height: u64,
    pub latest_block_hash: String,
    pub genesis_hash: String,
    pub created_at: DateTime<Utc>,
    pub blocks: Vec<crate::Block>,
}
