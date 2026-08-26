use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};
use zeroize::Zeroizing;

pub struct ValidatorIdentity {
    validator_id: String,
    signing_key: SigningKey,
}

impl ValidatorIdentity {
    pub fn generate(validator_id: impl Into<String>) -> Self {
        Self {
            validator_id: validator_id.into(),
            signing_key: SigningKey::generate(&mut OsRng),
        }
    }

    pub fn from_secret_bytes(validator_id: impl Into<String>, secret: [u8; 32]) -> Self {
        Self {
            validator_id: validator_id.into(),
            signing_key: SigningKey::from_bytes(&secret),
        }
    }

    pub fn load_or_create(
        validator_id: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> crate::Result<Self> {
        let validator_id = validator_id.into();
        let path = path.as_ref();
        if path.exists() {
            let bytes = Zeroizing::new(
                fs::read(path).map_err(|_| crate::MiniChainError::IdentityKeyUnavailable)?,
            );
            let secret: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| crate::MiniChainError::IdentityKeyUnavailable)?;
            return Ok(Self::from_secret_bytes(validator_id, secret));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| crate::MiniChainError::IdentityKeyUnavailable)?;
        }
        let identity = Self::generate(validator_id);
        let secret = Zeroizing::new(identity.signing_key.to_bytes());
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| crate::MiniChainError::IdentityKeyUnavailable)?;
        file.write_all(secret.as_slice())
            .and_then(|()| file.sync_all())
            .map_err(|_| crate::MiniChainError::IdentityKeyUnavailable)?;
        restrict_key_permissions(path)?;
        Ok(identity)
    }

    pub fn validator_id(&self) -> &str {
        &self.validator_id
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.signing_key.sign(message).to_bytes()
    }
}

#[cfg(unix)]
fn restrict_key_permissions(path: &Path) -> crate::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| crate::MiniChainError::IdentityKeyUnavailable)
}

#[cfg(not(unix))]
fn restrict_key_permissions(_path: &Path) -> crate::Result<()> {
    Ok(())
}
