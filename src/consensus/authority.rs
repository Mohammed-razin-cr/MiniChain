use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    Block,
    crypto::ValidatorIdentity,
    error::{MiniChainError, Result},
};

use super::{Permission, ValidatorRegistry, required_quorum};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub block: Block,
    pub proposer_id: String,
    pub proposer_signature: Vec<u8>,
}

impl Proposal {
    pub fn new(block: Block, identity: &ValidatorIdentity) -> Self {
        let proposer_signature = identity.sign(block.hash.as_bytes()).to_vec();
        Self {
            block,
            proposer_id: identity.validator_id().to_owned(),
            proposer_signature,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub proposal_hash: String,
    pub validator_id: String,
    pub signature: Vec<u8>,
}

impl Approval {
    pub fn sign(proposal_hash: impl Into<String>, identity: &ValidatorIdentity) -> Self {
        let proposal_hash = proposal_hash.into();
        Self {
            signature: identity.sign(proposal_hash.as_bytes()).to_vec(),
            proposal_hash,
            validator_id: identity.validator_id().to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityConsensus {
    expected_height: u64,
    expected_previous_hash: String,
    proposal_by_height: HashMap<u64, String>,
    approvals: HashMap<String, HashSet<String>>,
    eligible_approvers: HashMap<String, HashSet<String>>,
    required_approvals: HashMap<String, usize>,
}

impl AuthorityConsensus {
    pub fn new(expected_height: u64, expected_previous_hash: impl Into<String>) -> Self {
        Self {
            expected_height,
            expected_previous_hash: expected_previous_hash.into(),
            proposal_by_height: HashMap::new(),
            approvals: HashMap::new(),
            eligible_approvers: HashMap::new(),
            required_approvals: HashMap::new(),
        }
    }

    pub fn register_proposal(
        &mut self,
        proposal: &Proposal,
        registry: &ValidatorRegistry,
    ) -> Result<()> {
        if proposal.block.header.index != self.expected_height {
            return Err(MiniChainError::InvalidBlockIndex {
                index: proposal.block.header.index,
                expected: self.expected_height,
                actual: proposal.block.header.index,
            });
        }
        if proposal.block.header.previous_hash != self.expected_previous_hash {
            return Err(MiniChainError::InvalidPreviousHash {
                index: proposal.block.header.index,
            });
        }
        if proposal.block.header.validator_id != proposal.proposer_id {
            return Err(MiniChainError::BlockValidatorMismatch {
                block_validator: proposal.block.header.validator_id.clone(),
                proposer: proposal.proposer_id.clone(),
            });
        }
        registry.verify(
            &proposal.proposer_id,
            Permission::ProposeBlocks,
            proposal.block.hash.as_bytes(),
            &proposal.proposer_signature,
        )?;
        proposal.block.validate_contents()?;

        let eligible_approvers = registry.active_approver_ids();
        if eligible_approvers.is_empty() {
            return Err(MiniChainError::NoActiveApprovers);
        }
        let required = required_quorum(eligible_approvers.len());

        let height = proposal.block.header.index;
        if let Some(existing_hash) = self.proposal_by_height.get(&height) {
            if existing_hash != &proposal.block.hash {
                return Err(MiniChainError::ConflictingProposal { height });
            }
            return Err(MiniChainError::DuplicateProposal {
                hash: proposal.block.hash.clone(),
            });
        }
        self.proposal_by_height
            .insert(height, proposal.block.hash.clone());
        self.approvals
            .entry(proposal.block.hash.clone())
            .or_default();
        self.eligible_approvers
            .insert(proposal.block.hash.clone(), eligible_approvers);
        self.required_approvals
            .insert(proposal.block.hash.clone(), required);
        Ok(())
    }

    pub fn approve(&mut self, approval: Approval, registry: &ValidatorRegistry) -> Result<()> {
        let approvals = self
            .approvals
            .get_mut(&approval.proposal_hash)
            .ok_or(MiniChainError::ApprovalForWrongProposal)?;
        let eligible = self
            .eligible_approvers
            .get(&approval.proposal_hash)
            .ok_or(MiniChainError::ApprovalForWrongProposal)?;
        if registry.get(&approval.validator_id).is_none() {
            return Err(MiniChainError::UnknownValidator {
                id: approval.validator_id,
            });
        }
        if !eligible.contains(&approval.validator_id) {
            return Err(MiniChainError::ValidatorNotEligibleForProposal {
                id: approval.validator_id,
            });
        }
        registry.verify(
            &approval.validator_id,
            Permission::ApproveBlocks,
            approval.proposal_hash.as_bytes(),
            &approval.signature,
        )?;
        if !approvals.insert(approval.validator_id.clone()) {
            return Err(MiniChainError::DuplicateApproval {
                id: approval.validator_id,
            });
        }
        Ok(())
    }

    pub fn ensure_quorum(&self, proposal_hash: &str, _registry: &ValidatorRegistry) -> Result<()> {
        let received = self.approvals.get(proposal_hash).map_or(0, HashSet::len);
        let required = self
            .required_approvals
            .get(proposal_hash)
            .copied()
            .ok_or(MiniChainError::ApprovalForWrongProposal)?;
        if received < required {
            return Err(MiniChainError::InsufficientQuorum { required, received });
        }
        Ok(())
    }
}
