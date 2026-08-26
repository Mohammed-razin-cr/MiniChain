use ed25519_dalek::{Signature, VerifyingKey};

use crate::error::{MiniChainError, Result};

pub fn verify_signature(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| MiniChainError::InvalidPublicKey)?;
    let verifying_key =
        VerifyingKey::from_bytes(&public_key).map_err(|_| MiniChainError::InvalidPublicKey)?;
    let signature =
        Signature::from_slice(signature).map_err(|_| MiniChainError::InvalidSignature)?;

    verifying_key
        .verify_strict(message, &signature)
        .map_err(|_| MiniChainError::InvalidSignature)
}
