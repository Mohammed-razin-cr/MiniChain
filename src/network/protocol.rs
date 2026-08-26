use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

use crate::{
    Block, Transaction,
    crypto::{ValidatorIdentity, verify_signature},
    error::{MiniChainError, Result},
};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_SYNC_BLOCKS: u64 = 16;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkStatus {
    pub node_id: String,
    pub height: u64,
    pub latest_hash: String,
    pub protocol_version: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MessagePayload {
    Hello {
        listen_address: String,
        status: NetworkStatus,
    },
    HelloAck {
        status: NetworkStatus,
    },
    Ping {
        nonce: Uuid,
    },
    Pong {
        nonce: Uuid,
    },
    GetStatus,
    Status(NetworkStatus),
    Transaction(Transaction),
    Block(Block),
    SyncRequest {
        from_height: u64,
        to_height: u64,
    },
    SyncResponse {
        blocks: Vec<Block>,
    },
    Reject {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub version: u16,
    pub message_id: Uuid,
    pub sender: String,
    pub timestamp: DateTime<Utc>,
    pub payload: MessagePayload,
    pub signature: Vec<u8>,
}

impl Envelope {
    pub fn signed(payload: MessagePayload, identity: &ValidatorIdentity) -> Result<Self> {
        let mut envelope = Self {
            version: PROTOCOL_VERSION,
            message_id: Uuid::new_v4(),
            sender: identity.validator_id().to_owned(),
            timestamp: Utc::now(),
            payload,
            signature: Vec::new(),
        };
        envelope.signature = identity.sign(&envelope.signing_bytes()?).to_vec();
        Ok(envelope)
    }

    pub fn verify(&self, expected_sender: &str, public_key: &[u8]) -> Result<()> {
        if self.version != PROTOCOL_VERSION {
            return Err(MiniChainError::ProtocolMismatch {
                version: self.version,
            });
        }
        if self.sender != expected_sender {
            return Err(MiniChainError::InvalidPeerIdentity {
                peer: self.sender.clone(),
            });
        }
        if (Utc::now() - self.timestamp).abs() > Duration::minutes(5) {
            return Err(MiniChainError::InvalidMessage);
        }
        verify_signature(public_key, &self.signing_bytes()?, &self.signature)
    }

    fn signing_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(&(
            self.version,
            self.message_id,
            &self.sender,
            self.timestamp,
            &self.payload,
        ))
        .map_err(|_| MiniChainError::InvalidMessage)
    }
}

pub(crate) async fn write_envelope<W: AsyncWrite + Unpin>(
    writer: &mut W,
    envelope: &Envelope,
) -> Result<()> {
    let bytes = serde_json::to_vec(envelope).map_err(|_| MiniChainError::InvalidMessage)?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(MiniChainError::MessageTooLarge {
            limit: MAX_MESSAGE_BYTES,
        });
    }
    writer
        .write_u32(bytes.len() as u32)
        .await
        .map_err(|_| MiniChainError::PeerUnavailable {
            peer: envelope.sender.clone(),
        })?;
    writer
        .write_all(&bytes)
        .await
        .map_err(|_| MiniChainError::PeerUnavailable {
            peer: envelope.sender.clone(),
        })?;
    writer
        .flush()
        .await
        .map_err(|_| MiniChainError::PeerUnavailable {
            peer: envelope.sender.clone(),
        })
}

pub(crate) async fn read_envelope<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Envelope> {
    let length = reader
        .read_u32()
        .await
        .map_err(|_| MiniChainError::InvalidMessage)? as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(MiniChainError::MessageTooLarge {
            limit: MAX_MESSAGE_BYTES,
        });
    }
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|_| MiniChainError::InvalidMessage)?;
    serde_json::from_slice(&bytes).map_err(|_| MiniChainError::InvalidMessage)
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncWriteExt, duplex};

    use super::*;

    #[test]
    fn signed_envelope_rejects_payload_changes_and_wrong_identity() {
        let identity = ValidatorIdentity::from_secret_bytes("node-01", [81; 32]);
        let mut envelope = Envelope::signed(MessagePayload::GetStatus, &identity).unwrap();
        envelope.verify("node-01", &identity.public_key()).unwrap();
        envelope.payload = MessagePayload::Reject {
            reason: "changed".to_owned(),
        };
        assert_eq!(
            envelope
                .verify("node-01", &identity.public_key())
                .unwrap_err(),
            MiniChainError::InvalidSignature
        );
        assert!(matches!(
            envelope.verify("node-02", &identity.public_key()),
            Err(MiniChainError::InvalidPeerIdentity { .. })
        ));
    }

    #[tokio::test]
    async fn oversized_frame_is_rejected_before_allocation() {
        let (mut writer, mut reader) = duplex(16);
        let task = tokio::spawn(async move {
            writer
                .write_u32((MAX_MESSAGE_BYTES + 1) as u32)
                .await
                .unwrap();
        });
        assert_eq!(
            read_envelope(&mut reader).await.unwrap_err(),
            MiniChainError::MessageTooLarge {
                limit: MAX_MESSAGE_BYTES
            }
        );
        task.await.unwrap();
    }

    #[test]
    fn unknown_envelope_fields_are_rejected() {
        let identity = ValidatorIdentity::from_secret_bytes("node-01", [81; 32]);
        let envelope = Envelope::signed(MessagePayload::GetStatus, &identity).unwrap();
        let mut value = serde_json::to_value(envelope).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), serde_json::json!(true));
        assert!(serde_json::from_value::<Envelope>(value).is_err());
    }
}
