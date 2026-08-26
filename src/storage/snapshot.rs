use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crypto::{encode_hex, sha256};

use super::models::SnapshotPayload;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub id: Uuid,
    pub integrity_hash: String,
    pub(crate) payload: SnapshotPayload,
}

impl Snapshot {
    pub(crate) fn new(payload: SnapshotPayload) -> Result<Self, serde_json::Error> {
        let integrity_hash = payload_hash(&payload)?;
        Ok(Self {
            id: Uuid::new_v4(),
            integrity_hash,
            payload,
        })
    }

    pub fn height(&self) -> u64 {
        self.payload.height
    }

    pub fn chain_id(&self) -> Uuid {
        self.payload.chain_id
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.payload.created_at
    }

    pub fn latest_block_hash(&self) -> &str {
        &self.payload.latest_block_hash
    }

    pub fn verify_integrity(&self) -> bool {
        payload_hash(&self.payload).is_ok_and(|hash| hash == self.integrity_hash)
    }
}

fn payload_hash(payload: &SnapshotPayload) -> Result<String, serde_json::Error> {
    serde_json::to_vec(payload).map(|bytes| encode_hex(&sha256(&bytes)))
}
