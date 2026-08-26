use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    Block, Operation, Transaction,
    network::{ApiRole, Peer, SyncState},
    storage::{RecordStatus, Snapshot, StoredTransaction},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthIdentityResponse {
    pub identity: String,
    pub role: ApiRole,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub node_id: String,
    pub height: u64,
    pub sync: SyncState,
    pub peers: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadyResponse {
    pub ready: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaginationQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockListQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub from: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionListQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub operation: Option<Operation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u64,
    pub limit: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockResponse {
    pub index: u64,
    pub timestamp: DateTime<Utc>,
    pub hash: String,
    pub previous_hash: String,
    pub merkle_root: String,
    pub validator: String,
    pub validator_signature_status: SignatureStatus,
    pub transaction_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions: Option<Vec<Transaction>>,
}

impl BlockResponse {
    pub fn from_block(block: Block, include_transactions: bool) -> Self {
        Self {
            index: block.header.index,
            timestamp: block.header.timestamp,
            hash: block.hash,
            previous_hash: block.header.previous_hash,
            merkle_root: block.header.merkle_root,
            validator: block.header.validator_id,
            validator_signature_status: if block.header.validator_signature.is_some() {
                SignatureStatus::PresentUnverified
            } else {
                SignatureStatus::Missing
            },
            transaction_count: block.transactions.len(),
            transactions: include_transactions.then_some(block.transactions),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureStatus {
    Missing,
    PresentUnverified,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionRequest {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub operation: Operation,
    pub record_id: String,
    pub actor_id: String,
    pub payload: Value,
    pub metadata: BTreeMap<String, Value>,
    pub signer_public_key: Vec<u8>,
    pub signature: Vec<u8>,
    pub hash: String,
}

impl From<TransactionRequest> for Transaction {
    fn from(request: TransactionRequest) -> Self {
        Self {
            id: request.id,
            timestamp: request.timestamp,
            operation: request.operation,
            record_id: request.record_id,
            actor_id: request.actor_id,
            payload: request.payload,
            metadata: request.metadata,
            signer_public_key: request.signer_public_key,
            signature: request.signature,
            hash: request.hash,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub transaction_id: Uuid,
    pub operation: Operation,
    pub record_id: String,
    pub actor_id: String,
    pub timestamp: DateTime<Utc>,
    pub hash: String,
    pub signature_valid: bool,
    pub block_height: u64,
    pub block_hash: String,
    pub payload: Value,
}

impl From<StoredTransaction> for TransactionResponse {
    fn from(stored: StoredTransaction) -> Self {
        let signature_valid = stored.transaction.validate().is_ok();
        Self {
            transaction_id: stored.transaction.id,
            operation: stored.transaction.operation,
            record_id: stored.transaction.record_id,
            actor_id: stored.transaction.actor_id,
            timestamp: stored.transaction.timestamp,
            hash: stored.transaction.hash,
            signature_valid,
            block_height: stored.block_height,
            block_hash: stored.block_hash,
            payload: stored.transaction.payload,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmissionResponse {
    pub accepted: bool,
    pub transaction_id: Uuid,
    pub mempool_size: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionStatusResponse {
    pub transaction_id: Uuid,
    pub status: &'static str,
    pub block_height: Option<u64>,
    pub block_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordResponse {
    pub record_id: String,
    pub status: RecordStatus,
    pub data: Value,
    pub latest_transaction: TransactionResponse,
    pub block_height: u64,
    pub block_hash: String,
    pub cryptographically_verified: bool,
    pub merkle_proof_valid: bool,
    pub chain_valid: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatorQuery {
    pub active: Option<bool>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatorResponse {
    pub validator_id: String,
    pub public_key: Vec<u8>,
    pub active: bool,
    pub current_height: u64,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub network_address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkStatusResponse {
    pub node_id: String,
    pub height: u64,
    pub latest_hash: String,
    pub sync_state: SyncState,
    pub peer_count: usize,
    pub healthy_peers: usize,
    pub protocol_version: u16,
    pub mempool_size: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerResponse {
    pub id: String,
    pub address: String,
    pub state: crate::network::PeerState,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub height: u64,
    pub latest_hash: String,
    pub protocol_version: u16,
    pub latency_ms: Option<u64>,
    pub failure_count: u32,
}

impl From<Peer> for PeerResponse {
    fn from(peer: Peer) -> Self {
        Self {
            id: peer.id,
            address: peer.address,
            state: peer.state,
            last_heartbeat: peer.last_heartbeat,
            height: peer.height,
            latest_hash: peer.latest_hash,
            protocol_version: peer.protocol_version,
            latency_ms: peer.latency_ms,
            failure_count: peer.failure_count,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsistencyResponse {
    pub consistent: bool,
    pub checked_peers: usize,
    pub local_height: u64,
    pub local_hash: String,
    pub inconsistent_peers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkActionResponse {
    pub peer_id: String,
    pub height: u64,
    pub latest_hash: String,
    pub latency_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainValidationResponse {
    pub valid: bool,
    pub checked_height: u64,
    pub failure_block: Option<u64>,
    pub failure_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotResponse {
    pub id: Uuid,
    pub height: u64,
    pub chain_id: Uuid,
    pub latest_block_hash: String,
    pub created_at: DateTime<Utc>,
    pub integrity_hash: String,
    pub valid: Option<bool>,
}

impl SnapshotResponse {
    pub fn from_snapshot(snapshot: Snapshot, valid: Option<bool>) -> Self {
        Self {
            id: snapshot.id,
            height: snapshot.height(),
            chain_id: snapshot.chain_id(),
            latest_block_hash: snapshot.latest_block_hash().to_owned(),
            created_at: snapshot.created_at(),
            integrity_hash: snapshot.integrity_hash,
            valid,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreSnapshotRequest {
    pub id: Uuid,
    #[serde(default)]
    pub force: bool,
}
