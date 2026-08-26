use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    crypto::verify_signature,
    error::{MiniChainError, Result},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ProposeBlocks,
    ApproveBlocks,
    SubmitTransactions,
}

impl std::fmt::Display for Permission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::ProposeBlocks => "propose_blocks",
            Self::ApproveBlocks => "approve_blocks",
            Self::SubmitTransactions => "submit_transactions",
        };
        formatter.write_str(name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validator {
    pub id: String,
    pub public_key: Vec<u8>,
    pub active: bool,
    pub permissions: BTreeSet<Permission>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub block_height: u64,
    pub network_address: String,
}

impl Validator {
    pub fn new(
        id: impl Into<String>,
        public_key: Vec<u8>,
        network_address: impl Into<String>,
        permissions: impl IntoIterator<Item = Permission>,
    ) -> Result<Self> {
        let id = id.into();
        verify_public_key(&public_key)?;
        Ok(Self {
            id,
            public_key,
            active: true,
            permissions: permissions.into_iter().collect(),
            last_heartbeat: None,
            block_height: 0,
            network_address: network_address.into(),
        })
    }

    pub fn validate(&self) -> Result<()> {
        verify_public_key(&self.public_key)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ValidatorRegistry {
    validators: HashMap<String, Validator>,
}

impl ValidatorRegistry {
    pub fn register(&mut self, validator: Validator) -> Result<()> {
        if self.validators.contains_key(&validator.id) {
            return Err(MiniChainError::DuplicateValidator {
                id: validator.id.clone(),
            });
        }
        self.validators.insert(validator.id.clone(), validator);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Validator> {
        self.validators.get(id)
    }

    pub fn set_active(&mut self, id: &str, active: bool) -> Result<()> {
        self.get_mut(id)?.active = active;
        Ok(())
    }

    pub fn heartbeat(&mut self, id: &str, height: u64, observed_at: DateTime<Utc>) -> Result<()> {
        let validator = self.get_mut(id)?;
        validator.last_heartbeat = Some(observed_at);
        validator.block_height = height;
        Ok(())
    }

    pub fn authorize(&self, id: &str, permission: Permission) -> Result<&Validator> {
        let validator = self
            .validators
            .get(id)
            .ok_or_else(|| MiniChainError::UnknownValidator { id: id.to_owned() })?;
        if !validator.active {
            return Err(MiniChainError::InactiveValidator { id: id.to_owned() });
        }
        if !validator.permissions.contains(&permission) {
            return Err(MiniChainError::ValidatorPermissionDenied {
                id: id.to_owned(),
                permission: permission.to_string(),
            });
        }
        Ok(validator)
    }

    pub fn verify(
        &self,
        id: &str,
        permission: Permission,
        message: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        let validator = self.authorize(id, permission)?;
        verify_signature(&validator.public_key, message, signature)
    }

    pub fn active_approver_count(&self) -> usize {
        self.validators
            .values()
            .filter(|validator| {
                validator.active && validator.permissions.contains(&Permission::ApproveBlocks)
            })
            .count()
    }

    pub fn active_approver_ids(&self) -> std::collections::HashSet<String> {
        self.validators
            .values()
            .filter(|validator| {
                validator.active && validator.permissions.contains(&Permission::ApproveBlocks)
            })
            .map(|validator| validator.id.clone())
            .collect()
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut Validator> {
        self.validators
            .get_mut(id)
            .ok_or_else(|| MiniChainError::UnknownValidator { id: id.to_owned() })
    }
}

fn verify_public_key(public_key: &[u8]) -> Result<()> {
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| MiniChainError::InvalidPublicKey)?;
    ed25519_dalek::VerifyingKey::from_bytes(&public_key)
        .map(|_| ())
        .map_err(|_| MiniChainError::InvalidPublicKey)
}
