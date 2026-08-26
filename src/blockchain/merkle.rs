use serde::{Deserialize, Serialize};

use crate::{
    crypto::{decode_hex, encode_hex, sha256},
    error::{MiniChainError, Result},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleTree {
    sorted_hashes: Vec<String>,
    levels: Vec<Vec<[u8; 32]>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf_hash: String,
    pub steps: Vec<MerkleStep>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleStep {
    pub sibling: String,
    pub sibling_on_left: bool,
}

impl MerkleTree {
    pub fn from_hashes(hashes: &[String]) -> Result<Self> {
        if hashes.is_empty() {
            return Err(MiniChainError::EmptyMerkleTree);
        }

        let mut sorted_hashes = hashes.to_vec();
        sorted_hashes.sort_unstable();
        let leaves = sorted_hashes
            .iter()
            .map(|hash| leaf_node(hash))
            .collect::<Result<Vec<_>>>()?;
        let mut levels = vec![leaves];

        while levels.last().map_or(0, Vec::len) > 1 {
            let current = levels.last().expect("the leaf level is present");
            let mut parents = Vec::with_capacity(current.len().div_ceil(2));
            for pair in current.chunks(2) {
                let right = pair.get(1).unwrap_or(&pair[0]);
                parents.push(parent_node(&pair[0], right));
            }
            levels.push(parents);
        }

        Ok(Self {
            sorted_hashes,
            levels,
        })
    }

    pub fn root(&self) -> String {
        encode_hex(&self.levels.last().expect("tree has a root")[0])
    }

    pub fn proof(&self, transaction_hash: &str) -> Result<MerkleProof> {
        let mut index = self
            .sorted_hashes
            .binary_search_by(|candidate| candidate.as_str().cmp(transaction_hash))
            .map_err(|_| MiniChainError::TransactionNotInMerkleTree)?;
        let mut steps = Vec::with_capacity(self.levels.len().saturating_sub(1));

        for level in &self.levels[..self.levels.len() - 1] {
            let (sibling_index, sibling_on_left) = if index % 2 == 0 {
                ((index + 1).min(level.len() - 1), false)
            } else {
                (index - 1, true)
            };
            steps.push(MerkleStep {
                sibling: encode_hex(&level[sibling_index]),
                sibling_on_left,
            });
            index /= 2;
        }

        Ok(MerkleProof {
            leaf_hash: transaction_hash.to_owned(),
            steps,
        })
    }
}

impl MerkleProof {
    pub fn verify(&self, expected_root: &str) -> bool {
        let Ok(mut current) = leaf_node(&self.leaf_hash) else {
            return false;
        };

        for step in &self.steps {
            let Ok(sibling) = decode_digest(&step.sibling) else {
                return false;
            };
            current = if step.sibling_on_left {
                parent_node(&sibling, &current)
            } else {
                parent_node(&current, &sibling)
            };
        }
        encode_hex(&current) == expected_root
    }
}

pub fn empty_merkle_root() -> String {
    encode_hex(&sha256(&[]))
}

fn leaf_node(hash: &str) -> Result<[u8; 32]> {
    let hash = decode_digest(hash)?;
    let mut bytes = Vec::with_capacity(33);
    bytes.push(0);
    bytes.extend_from_slice(&hash);
    Ok(sha256(&bytes))
}

fn parent_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(65);
    bytes.push(1);
    bytes.extend_from_slice(left);
    bytes.extend_from_slice(right);
    sha256(&bytes)
}

fn decode_digest(hash: &str) -> Result<[u8; 32]> {
    decode_hex(hash)?
        .try_into()
        .map_err(|_| MiniChainError::InvalidHex)
}
