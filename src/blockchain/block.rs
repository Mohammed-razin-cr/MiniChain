use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    crypto::{encode_hex, sha256},
    error::{MiniChainError, Result},
};

use super::{MerkleTree, Transaction, empty_merkle_root};

pub const CURRENT_BLOCK_VERSION: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockHeader {
    pub index: u64,
    pub block_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub previous_hash: String,
    pub merkle_root: String,
    pub validator_id: String,
    pub validator_signature: Option<String>,
    pub version: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub hash: String,
}

impl Block {
    pub fn new(
        index: u64,
        previous_hash: impl Into<String>,
        transactions: Vec<Transaction>,
        validator_id: impl Into<String>,
    ) -> Result<Self> {
        let transaction_hashes = transactions
            .iter()
            .map(|transaction| transaction.hash.clone())
            .collect::<Vec<_>>();
        let merkle_root = if transaction_hashes.is_empty() {
            empty_merkle_root()
        } else {
            MerkleTree::from_hashes(&transaction_hashes)?.root()
        };
        let header = BlockHeader {
            index,
            block_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            previous_hash: previous_hash.into(),
            merkle_root,
            validator_id: validator_id.into(),
            validator_signature: None,
            version: CURRENT_BLOCK_VERSION,
        };
        Ok(Self::from_header(header, transactions))
    }

    pub fn from_header(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        let mut block = Self {
            header,
            transactions,
            hash: String::new(),
        };
        block.hash = block.calculate_hash();
        block
    }

    pub fn calculate_hash(&self) -> String {
        encode_hex(&sha256(&self.canonical_bytes()))
    }

    pub fn has_valid_hash(&self) -> bool {
        self.hash == self.calculate_hash()
    }

    pub fn calculated_merkle_root(&self) -> Result<String> {
        if self.transactions.is_empty() {
            return Ok(empty_merkle_root());
        }
        let hashes = self
            .transactions
            .iter()
            .map(|transaction| transaction.hash.clone())
            .collect::<Vec<_>>();
        Ok(MerkleTree::from_hashes(&hashes)?.root())
    }

    pub fn validate_contents(&self) -> Result<()> {
        if self.header.version != CURRENT_BLOCK_VERSION {
            return Err(MiniChainError::UnsupportedBlockVersion {
                index: self.header.index,
                version: self.header.version,
            });
        }
        if self.header.index > 0 && self.transactions.is_empty() {
            return Err(MiniChainError::EmptyBlock {
                index: self.header.index,
            });
        }
        for transaction in &self.transactions {
            transaction.validate()?;
        }
        if self.header.merkle_root != self.calculated_merkle_root()? {
            return Err(MiniChainError::InvalidMerkleRoot {
                index: self.header.index,
            });
        }

        let calculated = self.calculate_hash();
        if self.hash != calculated {
            return Err(MiniChainError::InvalidBlockHash {
                index: self.header.index,
                expected: self.hash.clone(),
                calculated,
            });
        }
        Ok(())
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        append_u16(&mut bytes, self.header.version);
        append_u64(&mut bytes, self.header.index);
        bytes.extend_from_slice(self.header.block_id.as_bytes());
        append_i64(&mut bytes, self.header.timestamp.timestamp());
        append_u32(&mut bytes, self.header.timestamp.timestamp_subsec_nanos());
        append_string(&mut bytes, &self.header.previous_hash);
        append_string(&mut bytes, &self.header.merkle_root);
        append_string(&mut bytes, &self.header.validator_id);

        match &self.header.validator_signature {
            Some(signature) => {
                bytes.push(1);
                append_string(&mut bytes, signature);
            }
            None => bytes.push(0),
        }

        append_u64(&mut bytes, self.transactions.len() as u64);
        for transaction in &self.transactions {
            append_string(&mut bytes, &transaction.hash);
        }
        bytes
    }
}

fn append_string(target: &mut Vec<u8>, value: &str) {
    append_u64(target, value.len() as u64);
    target.extend_from_slice(value.as_bytes());
}

fn append_u16(target: &mut Vec<u8>, value: u16) {
    target.extend_from_slice(&value.to_be_bytes());
}

fn append_u32(target: &mut Vec<u8>, value: u32) {
    target.extend_from_slice(&value.to_be_bytes());
}

fn append_u64(target: &mut Vec<u8>, value: u64) {
    target.extend_from_slice(&value.to_be_bytes());
}

fn append_i64(target: &mut Vec<u8>, value: i64) {
    target.extend_from_slice(&value.to_be_bytes());
}
