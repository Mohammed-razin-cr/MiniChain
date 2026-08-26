use std::collections::BTreeMap;

use chrono::{Duration, Utc};
use minichain::{Block, Blockchain, MiniChainError, Operation, Transaction, ValidatorIdentity};
use serde_json::json;
use uuid::Uuid;

fn identity() -> ValidatorIdentity {
    ValidatorIdentity::from_secret_bytes("issuer", [11; 32])
}

fn transaction(id: Uuid, payload: serde_json::Value) -> Transaction {
    Transaction::with_identity(
        id,
        Utc::now(),
        Operation::CreateRecord,
        "CERT-SECURITY",
        payload,
        BTreeMap::new(),
        &identity(),
    )
}

#[test]
fn replayed_id_is_rejected_even_with_a_different_valid_payload() {
    let id = Uuid::new_v4();
    let first = transaction(id, json!({"version": 1}));
    let replay = transaction(id, json!({"version": 2}));
    let mut chain = Blockchain::new().unwrap();
    chain
        .append(Block::new(1, chain.tip().hash.clone(), vec![first], "issuer").unwrap())
        .unwrap();

    let error = chain
        .append(Block::new(2, chain.tip().hash.clone(), vec![replay], "issuer").unwrap())
        .unwrap_err();
    assert_eq!(error, MiniChainError::DuplicateTransaction { id });
}

#[test]
fn identical_payload_with_a_new_id_is_not_treated_as_replay() {
    let first = transaction(Uuid::new_v4(), json!({"same": true}));
    let second = transaction(Uuid::new_v4(), json!({"same": true}));
    let mut chain = Blockchain::new().unwrap();
    chain
        .append(Block::new(1, chain.tip().hash.clone(), vec![first], "issuer").unwrap())
        .unwrap();
    chain
        .append(Block::new(2, chain.tip().hash.clone(), vec![second], "issuer").unwrap())
        .unwrap();
    assert_eq!(chain.height(), 2);
}

#[test]
fn modified_timestamp_and_actor_invalidate_the_signature() {
    let mut changed_timestamp = transaction(Uuid::new_v4(), json!({"valid": true}));
    changed_timestamp.timestamp += Duration::seconds(1);
    changed_timestamp.hash = changed_timestamp.calculate_hash();
    assert_eq!(
        changed_timestamp.validate(),
        Err(MiniChainError::InvalidSignature)
    );

    let mut changed_actor = transaction(Uuid::new_v4(), json!({"valid": true}));
    changed_actor.actor_id = "another-actor".to_owned();
    changed_actor.hash = changed_actor.calculate_hash();
    assert_eq!(
        changed_actor.validate(),
        Err(MiniChainError::InvalidSignature)
    );
}

#[test]
fn empty_truncated_and_malformed_signatures_are_rejected() {
    let mut empty = transaction(Uuid::new_v4(), json!({}));
    empty.signature.clear();
    empty.hash = empty.calculate_hash();
    assert!(matches!(
        empty.validate(),
        Err(MiniChainError::MissingTransactionSignature { .. })
    ));

    let mut truncated = transaction(Uuid::new_v4(), json!({}));
    truncated.signature.truncate(32);
    truncated.hash = truncated.calculate_hash();
    assert_eq!(truncated.validate(), Err(MiniChainError::InvalidSignature));

    let mut malformed = transaction(Uuid::new_v4(), json!({}));
    malformed.signature = vec![0xff; 64];
    malformed.hash = malformed.calculate_hash();
    assert_eq!(malformed.validate(), Err(MiniChainError::InvalidSignature));
}

#[test]
fn oversized_payload_and_metadata_are_rejected() {
    let oversized = transaction(Uuid::new_v4(), json!("x".repeat(65 * 1024)));
    assert_eq!(
        oversized.validate(),
        Err(MiniChainError::TransactionPayloadTooLarge { limit: 64 * 1024 })
    );

    let mut metadata = BTreeMap::new();
    for index in 0..65 {
        metadata.insert(format!("key-{index}"), json!(index));
    }
    let too_many_entries = Transaction::new(
        Operation::AuditEvent,
        "AUDIT-1",
        json!({}),
        metadata,
        &identity(),
    );
    assert_eq!(
        too_many_entries.validate(),
        Err(MiniChainError::TransactionMetadataTooLarge)
    );
}

#[test]
fn unknown_serialized_transaction_fields_are_rejected() {
    let transaction = transaction(Uuid::new_v4(), json!({}));
    let mut value = serde_json::to_value(transaction).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), json!(true));
    assert!(serde_json::from_value::<Transaction>(value).is_err());
}
