use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use minichain::{MiniChainError, Operation, Transaction, ValidatorIdentity};
use serde_json::json;
use uuid::Uuid;

fn signed_transaction(identity: &ValidatorIdentity) -> Transaction {
    Transaction::with_identity(
        Uuid::from_u128(42),
        Utc.timestamp_opt(1_735_689_700, 0).single().unwrap(),
        Operation::CreateRecord,
        "CERT-42",
        json!({"student": "Ada", "course": "MCA"}),
        BTreeMap::new(),
        identity,
    )
}

#[test]
fn valid_ed25519_signature_is_accepted() {
    let transaction = signed_transaction(&ValidatorIdentity::from_secret_bytes("issuer", [3; 32]));
    assert!(transaction.validate().is_ok());
}

#[test]
fn modified_message_is_rejected() {
    let mut transaction =
        signed_transaction(&ValidatorIdentity::from_secret_bytes("issuer", [3; 32]));
    transaction.payload = json!({"student": "Mallory"});
    transaction.hash = transaction.calculate_hash();
    assert_eq!(
        transaction.validate(),
        Err(MiniChainError::InvalidSignature)
    );
}

#[test]
fn modified_signature_is_rejected() {
    let mut transaction =
        signed_transaction(&ValidatorIdentity::from_secret_bytes("issuer", [3; 32]));
    transaction.signature[0] ^= 0x01;
    transaction.hash = transaction.calculate_hash();
    assert_eq!(
        transaction.validate(),
        Err(MiniChainError::InvalidSignature)
    );
}

#[test]
fn wrong_public_key_is_rejected() {
    let mut transaction =
        signed_transaction(&ValidatorIdentity::from_secret_bytes("issuer", [3; 32]));
    transaction.signer_public_key = ValidatorIdentity::from_secret_bytes("other", [4; 32])
        .public_key()
        .to_vec();
    transaction.hash = transaction.calculate_hash();
    assert_eq!(
        transaction.validate(),
        Err(MiniChainError::InvalidSignature)
    );
}

#[test]
fn malformed_public_key_is_rejected_without_panicking() {
    let mut transaction =
        signed_transaction(&ValidatorIdentity::from_secret_bytes("issuer", [3; 32]));
    transaction.signer_public_key.truncate(10);
    transaction.hash = transaction.calculate_hash();
    assert_eq!(
        transaction.validate(),
        Err(MiniChainError::InvalidPublicKey)
    );
}

#[test]
fn object_key_order_does_not_change_the_transaction_hash() {
    let identity = ValidatorIdentity::from_secret_bytes("issuer", [3; 32]);
    let create = |payload| {
        Transaction::with_identity(
            Uuid::from_u128(42),
            Utc.timestamp_opt(1_735_689_700, 0).single().unwrap(),
            Operation::CreateRecord,
            "CERT-42",
            payload,
            BTreeMap::new(),
            &identity,
        )
    };
    assert_eq!(
        create(json!({"a": 1, "b": 2})).hash,
        create(json!({"b": 2, "a": 1})).hash
    );
}
