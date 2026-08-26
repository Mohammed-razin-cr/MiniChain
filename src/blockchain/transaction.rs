use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    crypto::{ValidatorIdentity, encode_hex, sha256, verify_signature},
    error::{MiniChainError, Result},
};

pub const MAX_RECORD_ID_BYTES: usize = 256;
pub const MAX_ACTOR_ID_BYTES: usize = 128;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_METADATA_ENTRIES: usize = 64;
pub const MAX_METADATA_BYTES: usize = 16 * 1024;
const MAX_JSON_DEPTH: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Operation {
    CreateRecord,
    UpdateRecord,
    RevokeRecord,
    VerifyRecord,
    AuditEvent,
}

impl Operation {
    fn code(self) -> u8 {
        match self {
            Self::CreateRecord => 1,
            Self::UpdateRecord => 2,
            Self::RevokeRecord => 3,
            Self::VerifyRecord => 4,
            Self::AuditEvent => 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub operation: Operation,
    pub record_id: String,
    pub actor_id: String,
    pub payload: Value,
    pub metadata: BTreeMap<String, Value>,
    pub signer_public_key: Vec<u8>,
    pub signature: Vec<u8>,
    pub hash: String,
}

impl Transaction {
    pub fn new(
        operation: Operation,
        record_id: impl Into<String>,
        payload: Value,
        metadata: BTreeMap<String, Value>,
        identity: &ValidatorIdentity,
    ) -> Self {
        Self::with_identity(
            Uuid::new_v4(),
            Utc::now(),
            operation,
            record_id,
            payload,
            metadata,
            identity,
        )
    }

    pub fn with_identity(
        id: Uuid,
        timestamp: DateTime<Utc>,
        operation: Operation,
        record_id: impl Into<String>,
        payload: Value,
        metadata: BTreeMap<String, Value>,
        identity: &ValidatorIdentity,
    ) -> Self {
        let mut transaction = Self {
            id,
            timestamp,
            operation,
            record_id: record_id.into(),
            actor_id: identity.validator_id().to_owned(),
            payload,
            metadata,
            signer_public_key: identity.public_key().to_vec(),
            signature: Vec::new(),
            hash: String::new(),
        };
        transaction.signature = identity.sign(&transaction.signing_bytes()).to_vec();
        transaction.hash = transaction.calculate_hash();
        transaction
    }

    pub fn calculate_hash(&self) -> String {
        encode_hex(&sha256(&self.canonical_bytes()))
    }

    pub fn validate(&self) -> Result<()> {
        validate_text(&self.record_id, MAX_RECORD_ID_BYTES, "record_id")?;
        validate_text(&self.actor_id, MAX_ACTOR_ID_BYTES, "actor_id")?;
        if json_size_within(&self.payload, MAX_PAYLOAD_BYTES).is_none() {
            return Err(MiniChainError::TransactionPayloadTooLarge {
                limit: MAX_PAYLOAD_BYTES,
            });
        }
        if self.metadata.len() > MAX_METADATA_ENTRIES
            || metadata_size_within(&self.metadata, MAX_METADATA_BYTES).is_none()
        {
            return Err(MiniChainError::TransactionMetadataTooLarge);
        }
        if self.hash != self.calculate_hash() {
            return Err(MiniChainError::InvalidTransactionHash { id: self.id });
        }
        if self.signature.is_empty() {
            return Err(MiniChainError::MissingTransactionSignature { id: self.id });
        }
        verify_signature(
            &self.signer_public_key,
            &self.signing_bytes(),
            &self.signature,
        )
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(1);
        bytes.extend_from_slice(self.id.as_bytes());
        append_i64(&mut bytes, self.timestamp.timestamp());
        append_u32(&mut bytes, self.timestamp.timestamp_subsec_nanos());
        bytes.push(self.operation.code());
        append_string(&mut bytes, &self.record_id);
        append_string(&mut bytes, &self.actor_id);
        append_json(&mut bytes, &self.payload);
        append_u64(&mut bytes, self.metadata.len() as u64);
        for (key, value) in &self.metadata {
            append_string(&mut bytes, key);
            append_json(&mut bytes, value);
        }
        append_bytes(&mut bytes, &self.signer_public_key);
        bytes
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = self.signing_bytes();
        append_bytes(&mut bytes, &self.signature);
        bytes
    }
}

fn validate_text(value: &str, limit: usize, field: &'static str) -> Result<()> {
    if value.is_empty() || value.len() > limit {
        return Err(MiniChainError::InvalidTransactionField { field });
    }
    Ok(())
}

fn metadata_size_within(metadata: &BTreeMap<String, Value>, limit: usize) -> Option<usize> {
    let mut size = 0usize;
    for (key, value) in metadata {
        size = size.checked_add(key.len())?;
        size = size.checked_add(json_size_within(value, limit.saturating_sub(size))?)?;
        if size > limit {
            return None;
        }
    }
    Some(size)
}

fn json_size_within(value: &Value, limit: usize) -> Option<usize> {
    let mut size = 0usize;
    let mut stack = vec![(value, 0usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > MAX_JSON_DEPTH {
            return None;
        }
        size = size.checked_add(1)?;
        match current {
            Value::Null | Value::Bool(_) => {}
            Value::Number(number) => size = size.checked_add(number.to_string().len())?,
            Value::String(text) => size = size.checked_add(text.len())?,
            Value::Array(values) => {
                stack.extend(values.iter().map(|value| (value, depth + 1)));
            }
            Value::Object(values) => {
                for (key, value) in values {
                    size = size.checked_add(key.len())?;
                    stack.push((value, depth + 1));
                }
            }
        }
        if size > limit {
            return None;
        }
    }
    Some(size)
}

fn append_json(target: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Null => target.push(0),
        Value::Bool(false) => target.push(1),
        Value::Bool(true) => target.push(2),
        Value::Number(number) => {
            target.push(3);
            append_string(target, &number.to_string());
        }
        Value::String(value) => {
            target.push(4);
            append_string(target, value);
        }
        Value::Array(values) => {
            target.push(5);
            append_u64(target, values.len() as u64);
            for value in values {
                append_json(target, value);
            }
        }
        Value::Object(values) => {
            target.push(6);
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
            append_u64(target, entries.len() as u64);
            for (key, value) in entries {
                append_string(target, key);
                append_json(target, value);
            }
        }
    }
}

fn append_string(target: &mut Vec<u8>, value: &str) {
    append_bytes(target, value.as_bytes());
}

fn append_bytes(target: &mut Vec<u8>, value: &[u8]) {
    append_u64(target, value.len() as u64);
    target.extend_from_slice(value);
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
