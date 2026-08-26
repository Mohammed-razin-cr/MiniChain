use std::fmt::{Display, Formatter};

use crate::MiniChainError;

#[derive(Debug)]
pub struct CliError {
    pub exit_code: i32,
    pub message: String,
    pub reason: Option<String>,
}

impl CliError {
    pub fn new(exit_code: i32, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
            reason: None,
        }
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(3, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(4, message)
    }

    pub fn synchronization(message: impl Into<String>) -> Self {
        Self::new(5, message)
    }
}

impl Display for CliError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Error: {}", self.message)?;
        if let Some(reason) = &self.reason {
            write!(formatter, "\n\nReason:\n{reason}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CliError {}

impl From<MiniChainError> for CliError {
    fn from(error: MiniChainError) -> Self {
        use MiniChainError::*;
        let exit_code = match error {
            InvalidBlockHash { .. }
            | InvalidBlockIndex { .. }
            | InvalidPreviousHash { .. }
            | InvalidGenesis
            | InvalidTransactionHash { .. }
            | InvalidMerkleRoot { .. }
            | InvalidSignature
            | InvalidConfiguration
            | SnapshotInvalid { .. }
            | SnapshotIncompatible => 3,
            StorageUnavailable | PeerUnavailable { .. } | NetworkTimeout => 4,
            SyncFailed | DivergenceDetected { .. } => 5,
            _ => 1,
        };
        Self::new(exit_code, error.to_string())
    }
}

pub type CliResult<T> = std::result::Result<T, CliError>;
