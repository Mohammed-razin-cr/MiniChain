use std::collections::BTreeMap;

use minichain::consensus::{
    Approval, AuthorityConsensus, Permission, Proposal, Validator, ValidatorRegistry,
    required_quorum,
};
use minichain::{Block, MiniChainError, Operation, Transaction, ValidatorIdentity};
use serde_json::json;

fn identities() -> Vec<ValidatorIdentity> {
    (1..=4)
        .map(|value| {
            ValidatorIdentity::from_secret_bytes(format!("validator-{value}"), [value; 32])
        })
        .collect()
}

fn registry(identities: &[ValidatorIdentity]) -> ValidatorRegistry {
    let mut registry = ValidatorRegistry::default();
    for (index, identity) in identities.iter().enumerate() {
        registry
            .register(
                Validator::new(
                    identity.validator_id(),
                    identity.public_key().to_vec(),
                    format!("127.0.0.1:{}", 8081 + index),
                    [Permission::ProposeBlocks, Permission::ApproveBlocks],
                )
                .unwrap(),
            )
            .unwrap();
    }
    registry
}

fn proposal(identity: &ValidatorIdentity) -> Proposal {
    let transaction = Transaction::new(
        Operation::CreateRecord,
        "CERT-1",
        json!({"course": "MCA"}),
        BTreeMap::new(),
        identity,
    );
    Proposal::new(
        Block::new(
            1,
            "0".repeat(64),
            vec![transaction],
            identity.validator_id(),
        )
        .unwrap(),
        identity,
    )
}

#[test]
fn quorum_rule_requires_three_of_four() {
    assert_eq!(required_quorum(4), 3);
}

#[test]
fn three_valid_approvals_reach_quorum_but_two_do_not() {
    let identities = identities();
    let registry = registry(&identities);
    let proposal = proposal(&identities[0]);
    let mut consensus = AuthorityConsensus::new(1, "0".repeat(64));
    consensus.register_proposal(&proposal, &registry).unwrap();

    for (index, identity) in identities[..2].iter().enumerate() {
        consensus
            .approve(Approval::sign(&proposal.block.hash, identity), &registry)
            .unwrap();
        assert_eq!(
            consensus
                .ensure_quorum(&proposal.block.hash, &registry)
                .unwrap_err(),
            MiniChainError::InsufficientQuorum {
                required: 3,
                received: index + 1,
            }
        );
    }

    consensus
        .approve(
            Approval::sign(&proposal.block.hash, &identities[2]),
            &registry,
        )
        .unwrap();
    consensus
        .ensure_quorum(&proposal.block.hash, &registry)
        .unwrap();
    consensus
        .approve(
            Approval::sign(&proposal.block.hash, &identities[3]),
            &registry,
        )
        .unwrap();
    consensus
        .ensure_quorum(&proposal.block.hash, &registry)
        .unwrap();
}

#[test]
fn unknown_and_duplicate_approvals_are_rejected() {
    let identities = identities();
    let registry = registry(&identities);
    let proposal = proposal(&identities[0]);
    let mut consensus = AuthorityConsensus::new(1, "0".repeat(64));
    consensus.register_proposal(&proposal, &registry).unwrap();
    let approval = Approval::sign(&proposal.block.hash, &identities[0]);
    consensus.approve(approval.clone(), &registry).unwrap();
    assert_eq!(
        consensus.approve(approval, &registry).unwrap_err(),
        MiniChainError::DuplicateApproval {
            id: identities[0].validator_id().to_owned()
        }
    );

    let stranger = ValidatorIdentity::from_secret_bytes("stranger", [9; 32]);
    assert_eq!(
        consensus
            .approve(Approval::sign(&proposal.block.hash, &stranger), &registry)
            .unwrap_err(),
        MiniChainError::UnknownValidator {
            id: "stranger".to_owned()
        }
    );
}

#[test]
fn conflicting_proposal_at_the_same_height_is_rejected() {
    let identities = identities();
    let registry = registry(&identities);
    let first = proposal(&identities[0]);
    let second = proposal(&identities[0]);
    let mut consensus = AuthorityConsensus::new(1, "0".repeat(64));
    consensus.register_proposal(&first, &registry).unwrap();

    assert_eq!(
        consensus.register_proposal(&second, &registry).unwrap_err(),
        MiniChainError::ConflictingProposal { height: 1 }
    );
}

#[test]
fn inactive_proposer_cannot_register_a_block() {
    let identities = identities();
    let mut registry = registry(&identities);
    registry
        .set_active(identities[0].validator_id(), false)
        .unwrap();
    let proposal = proposal(&identities[0]);

    assert_eq!(
        AuthorityConsensus::new(1, "0".repeat(64))
            .register_proposal(&proposal, &registry)
            .unwrap_err(),
        MiniChainError::InactiveValidator {
            id: identities[0].validator_id().to_owned()
        }
    );
}

#[test]
fn proposal_with_a_tampered_block_is_rejected() {
    let identities = identities();
    let registry = registry(&identities);
    let mut proposal = proposal(&identities[0]);
    proposal.block.header.merkle_root = "0".repeat(64);
    proposal.block.hash = proposal.block.calculate_hash();
    proposal.proposer_signature = identities[0].sign(proposal.block.hash.as_bytes()).to_vec();

    assert_eq!(
        AuthorityConsensus::new(1, "0".repeat(64))
            .register_proposal(&proposal, &registry)
            .unwrap_err(),
        MiniChainError::InvalidMerkleRoot { index: 1 }
    );
}

#[test]
fn duplicate_validator_id_is_rejected() {
    let identities = identities();
    let mut registry = ValidatorRegistry::default();
    let make_validator = || {
        Validator::new(
            identities[0].validator_id(),
            identities[0].public_key().to_vec(),
            "127.0.0.1:8081",
            [Permission::ApproveBlocks],
        )
        .unwrap()
    };
    registry.register(make_validator()).unwrap();
    assert!(matches!(
        registry.register(make_validator()).unwrap_err(),
        MiniChainError::DuplicateValidator { .. }
    ));
}

#[test]
fn invalid_approval_signature_never_counts_toward_quorum() {
    let identities = identities();
    let registry = registry(&identities);
    let proposal = proposal(&identities[0]);
    let mut consensus = AuthorityConsensus::new(1, "0".repeat(64));
    consensus.register_proposal(&proposal, &registry).unwrap();
    let mut approval = Approval::sign(&proposal.block.hash, &identities[0]);
    approval.signature[0] ^= 1;

    assert_eq!(
        consensus.approve(approval, &registry).unwrap_err(),
        MiniChainError::InvalidSignature
    );
    assert_eq!(
        consensus
            .ensure_quorum(&proposal.block.hash, &registry)
            .unwrap_err(),
        MiniChainError::InsufficientQuorum {
            required: 3,
            received: 0,
        }
    );
}

#[test]
fn stale_wrong_parent_and_mismatched_validator_proposals_are_rejected() {
    let identities = identities();
    let registry = registry(&identities);

    let mut stale = proposal(&identities[0]);
    stale.block.header.index = 0;
    stale.block.hash = stale.block.calculate_hash();
    stale.proposer_signature = identities[0].sign(stale.block.hash.as_bytes()).to_vec();
    assert!(matches!(
        AuthorityConsensus::new(1, "0".repeat(64)).register_proposal(&stale, &registry),
        Err(MiniChainError::InvalidBlockIndex {
            expected: 1,
            actual: 0,
            ..
        })
    ));

    let mut wrong_parent = proposal(&identities[0]);
    wrong_parent.block.header.previous_hash = "f".repeat(64);
    wrong_parent.block.hash = wrong_parent.block.calculate_hash();
    wrong_parent.proposer_signature = identities[0]
        .sign(wrong_parent.block.hash.as_bytes())
        .to_vec();
    assert_eq!(
        AuthorityConsensus::new(1, "0".repeat(64))
            .register_proposal(&wrong_parent, &registry)
            .unwrap_err(),
        MiniChainError::InvalidPreviousHash { index: 1 }
    );

    let mut mismatch = proposal(&identities[0]);
    mismatch.block.header.validator_id = identities[1].validator_id().to_owned();
    mismatch.block.hash = mismatch.block.calculate_hash();
    mismatch.proposer_signature = identities[0].sign(mismatch.block.hash.as_bytes()).to_vec();
    assert!(matches!(
        AuthorityConsensus::new(1, "0".repeat(64)).register_proposal(&mismatch, &registry),
        Err(MiniChainError::BlockValidatorMismatch { .. })
    ));
}

#[test]
fn duplicate_proposal_is_rejected_explicitly() {
    let identities = identities();
    let registry = registry(&identities);
    let proposal = proposal(&identities[0]);
    let mut consensus = AuthorityConsensus::new(1, "0".repeat(64));
    consensus.register_proposal(&proposal, &registry).unwrap();
    assert_eq!(
        consensus
            .register_proposal(&proposal, &registry)
            .unwrap_err(),
        MiniChainError::DuplicateProposal {
            hash: proposal.block.hash.clone()
        }
    );
}

#[test]
fn inactive_approver_cannot_contribute_to_quorum() {
    let identities = identities();
    let mut registry = registry(&identities);
    let proposal = proposal(&identities[0]);
    let mut consensus = AuthorityConsensus::new(1, "0".repeat(64));
    consensus.register_proposal(&proposal, &registry).unwrap();
    registry
        .set_active(identities[1].validator_id(), false)
        .unwrap();
    assert!(matches!(
        consensus.approve(
            Approval::sign(&proposal.block.hash, &identities[1]),
            &registry
        ),
        Err(MiniChainError::InactiveValidator { .. })
    ));
}

#[test]
fn membership_changes_cannot_lower_a_proposals_quorum() {
    let identities = identities();
    let mut registry = registry(&identities);
    let proposal = proposal(&identities[0]);
    let mut consensus = AuthorityConsensus::new(1, "0".repeat(64));
    consensus.register_proposal(&proposal, &registry).unwrap();
    for identity in &identities[..2] {
        consensus
            .approve(Approval::sign(&proposal.block.hash, identity), &registry)
            .unwrap();
    }

    registry
        .set_active(identities[2].validator_id(), false)
        .unwrap();
    registry
        .set_active(identities[3].validator_id(), false)
        .unwrap();
    assert_eq!(
        consensus
            .ensure_quorum(&proposal.block.hash, &registry)
            .unwrap_err(),
        MiniChainError::InsufficientQuorum {
            required: 3,
            received: 2,
        }
    );
}

#[test]
fn invalid_proposer_signature_is_rejected() {
    let identities = identities();
    let registry = registry(&identities);
    let mut proposal = proposal(&identities[0]);
    proposal.proposer_signature[0] ^= 1;
    assert_eq!(
        AuthorityConsensus::new(1, "0".repeat(64))
            .register_proposal(&proposal, &registry)
            .unwrap_err(),
        MiniChainError::InvalidSignature
    );
}
