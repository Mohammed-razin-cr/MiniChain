use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{MiniChainError, Result};

use super::{Block, ChainValidation, ValidationIssue, create_genesis_block};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blockchain {
    blocks: Vec<Block>,
}

impl Blockchain {
    pub fn new() -> Result<Self> {
        Self::from_blocks(vec![create_genesis_block()])
    }

    pub fn from_blocks(blocks: Vec<Block>) -> Result<Self> {
        let chain = Self { blocks };
        chain.validate_strict()?;
        Ok(chain)
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn height(&self) -> u64 {
        self.blocks
            .last()
            .map(|block| block.header.index)
            .unwrap_or(0)
    }

    pub fn tip(&self) -> &Block {
        self.blocks
            .last()
            .expect("a Blockchain always contains its genesis block")
    }

    pub fn block_at(&self, index: u64) -> Option<&Block> {
        self.blocks.get(index as usize)
    }

    pub fn append(&mut self, block: Block) -> Result<()> {
        validate_next_block(self.tip(), &block)?;
        let existing_ids = self.transaction_ids();
        validate_unique_transactions(&block, &existing_ids)?;
        self.blocks.push(block);
        Ok(())
    }

    pub fn validate_successor(previous: &Block, block: &Block) -> Result<()> {
        validate_next_block(previous, block)?;
        validate_unique_transactions(block, &HashSet::new())
    }

    pub fn validate(&self) -> ChainValidation {
        if self.blocks.is_empty() {
            return invalid_report(0, 0, MiniChainError::EmptyChain);
        }

        let mut issues = Vec::new();
        let mut transaction_ids = HashSet::new();
        if self.blocks[0] != create_genesis_block() {
            issues.push(issue(0, MiniChainError::InvalidGenesis));
        }

        for (position, block) in self.blocks.iter().enumerate() {
            if let Err(error) = validate_block(block) {
                issues.push(issue(block.header.index, error));
            }
            if let Err(error) = validate_unique_transactions(block, &transaction_ids) {
                issues.push(issue(block.header.index, error));
            }
            transaction_ids.extend(block.transactions.iter().map(|transaction| transaction.id));

            if position > 0
                && let Err(error) = validate_link(&self.blocks[position - 1], block)
            {
                issues.push(issue(block.header.index, error));
            }
        }

        if issues.is_empty() {
            ChainValidation::valid(self.blocks.len())
        } else {
            ChainValidation::invalid(self.blocks.len(), issues)
        }
    }

    fn validate_strict(&self) -> Result<()> {
        if self.blocks.is_empty() {
            return Err(MiniChainError::EmptyChain);
        }
        if self.blocks[0] != create_genesis_block() {
            return Err(MiniChainError::InvalidGenesis);
        }

        let mut transaction_ids = HashSet::new();
        for (position, block) in self.blocks.iter().enumerate() {
            validate_block(block)?;
            validate_unique_transactions(block, &transaction_ids)?;
            transaction_ids.extend(block.transactions.iter().map(|transaction| transaction.id));
            if position > 0 {
                validate_link(&self.blocks[position - 1], block)?;
            }
        }
        Ok(())
    }

    fn transaction_ids(&self) -> HashSet<Uuid> {
        self.blocks
            .iter()
            .flat_map(|block| block.transactions.iter())
            .map(|transaction| transaction.id)
            .collect()
    }
}

fn validate_next_block(previous: &Block, block: &Block) -> Result<()> {
    validate_block(block)?;
    validate_link(previous, block)
}

fn validate_link(previous: &Block, block: &Block) -> Result<()> {
    let expected_index = previous.header.index + 1;
    if block.header.index != expected_index {
        return Err(MiniChainError::InvalidBlockIndex {
            index: block.header.index,
            expected: expected_index,
            actual: block.header.index,
        });
    }
    if block.header.previous_hash != previous.hash {
        return Err(MiniChainError::InvalidPreviousHash {
            index: block.header.index,
        });
    }
    if block.header.timestamp < previous.header.timestamp {
        return Err(MiniChainError::BlockTimestampBeforeParent {
            index: block.header.index,
        });
    }
    Ok(())
}

fn validate_block(block: &Block) -> Result<()> {
    block.validate_contents()?;
    validate_unique_transactions(block, &HashSet::new())?;
    Ok(())
}

fn validate_unique_transactions(block: &Block, known_ids: &HashSet<Uuid>) -> Result<()> {
    let mut ids = known_ids.clone();
    for transaction in &block.transactions {
        if !ids.insert(transaction.id) {
            return Err(MiniChainError::DuplicateTransaction { id: transaction.id });
        }
    }
    Ok(())
}

fn issue(block_index: u64, error: MiniChainError) -> ValidationIssue {
    ValidationIssue {
        block_index,
        reason: error.to_string(),
    }
}

fn invalid_report(
    checked_blocks: usize,
    block_index: u64,
    error: MiniChainError,
) -> ChainValidation {
    ChainValidation::invalid(checked_blocks, vec![issue(block_index, error)])
}
