use minichain::{MerkleProof, MerkleTree};
use sha2::{Digest, Sha256};

fn hashes(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("{:x}", Sha256::digest(index.to_be_bytes())))
        .collect()
}

#[test]
fn proofs_work_for_one_two_three_and_four_transactions() {
    for count in 1..=4 {
        let hashes = hashes(count);
        let tree = MerkleTree::from_hashes(&hashes).unwrap();
        for hash in &hashes {
            assert!(tree.proof(hash).unwrap().verify(&tree.root()));
        }
    }
}

#[test]
fn odd_leaf_is_duplicated_deterministically() {
    let hashes = hashes(3);
    let first = MerkleTree::from_hashes(&hashes).unwrap();
    let mut reversed = hashes.clone();
    reversed.reverse();
    let second = MerkleTree::from_hashes(&reversed).unwrap();
    assert_eq!(first.root(), second.root());
}

#[test]
fn a_large_tree_produces_valid_proofs() {
    let hashes = hashes(1_001);
    let tree = MerkleTree::from_hashes(&hashes).unwrap();
    for index in [0, 1, 499, 1_000] {
        assert!(tree.proof(&hashes[index]).unwrap().verify(&tree.root()));
    }
}

#[test]
fn proof_rejects_wrong_root_wrong_transaction_and_changed_path() {
    let hashes = hashes(4);
    let tree = MerkleTree::from_hashes(&hashes).unwrap();
    let proof = tree.proof(&hashes[0]).unwrap();
    assert!(!proof.verify(&"0".repeat(64)));

    let wrong_transaction = MerkleProof {
        leaf_hash: hashes[1].clone(),
        steps: proof.steps.clone(),
    };
    assert!(!wrong_transaction.verify(&tree.root()));

    let mut changed_path = proof;
    changed_path.steps[0].sibling = "f".repeat(64);
    assert!(!changed_path.verify(&tree.root()));
}

#[test]
fn proof_rejects_short_extra_and_wrong_position_paths() {
    let hashes = hashes(8);
    let tree = MerkleTree::from_hashes(&hashes).unwrap();
    let proof = tree.proof(&hashes[3]).unwrap();

    let mut short = proof.clone();
    short.steps.pop();
    assert!(!short.verify(&tree.root()));

    let mut extra = proof.clone();
    extra.steps.push(proof.steps[0].clone());
    assert!(!extra.verify(&tree.root()));

    let mut wrong_position = proof;
    wrong_position.steps[0].sibling_on_left = !wrong_position.steps[0].sibling_on_left;
    assert!(!wrong_position.verify(&tree.root()));
}
