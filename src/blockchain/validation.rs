use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub block_index: u64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainValidation {
    pub valid: bool,
    pub checked_blocks: usize,
    pub issues: Vec<ValidationIssue>,
}

impl ChainValidation {
    pub fn valid(checked_blocks: usize) -> Self {
        Self {
            valid: true,
            checked_blocks,
            issues: Vec::new(),
        }
    }

    pub fn invalid(checked_blocks: usize, issues: Vec<ValidationIssue>) -> Self {
        Self {
            valid: false,
            checked_blocks,
            issues,
        }
    }
}
