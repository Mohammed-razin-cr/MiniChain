use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::MiniChainError;

#[derive(Clone, Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Clone, Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", message)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "A valid bearer token is required",
        )
    }

    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "The authenticated role cannot perform this operation",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

impl From<MiniChainError> for ApiError {
    fn from(error: MiniChainError) -> Self {
        use MiniChainError::*;
        let (status, code) = match &error {
            BlockNotFound { .. }
            | BlockHashNotFound { .. }
            | TransactionNotFound { .. }
            | RecordNotFound { .. }
            | UnknownValidator { .. } => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            SnapshotInvalid { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "SNAPSHOT_INVALID"),
            DuplicateTransaction { .. }
            | DuplicateMempoolTransaction { .. }
            | RestoreWouldOverwrite
            | ConflictingProposal { .. } => (StatusCode::CONFLICT, "CONFLICT"),
            TransactionPayloadTooLarge { .. }
            | TransactionMetadataTooLarge
            | MessageTooLarge { .. } => (StatusCode::PAYLOAD_TOO_LARGE, "VALIDATION_ERROR"),
            InvalidBlockHash { .. }
            | InvalidBlockIndex { .. }
            | InvalidPreviousHash { .. }
            | UnsupportedBlockVersion { .. }
            | EmptyChain
            | InvalidGenesis
            | InvalidTransactionHash { .. }
            | MissingTransactionSignature { .. }
            | InvalidSignature
            | InvalidPublicKey
            | InvalidHex
            | InvalidMerkleRoot { .. }
            | EmptyMerkleTree
            | TransactionNotInMerkleTree
            | BlockTimestampBeforeParent { .. }
            | ExpiredTransaction { .. }
            | EmptyBlock { .. }
            | InvalidTransactionField { .. } => {
                (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
            }
            StorageCorruption { .. } | MetadataMismatch | IndexCorrupted { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "CHAIN_INVALID")
            }
            PeerUnavailable { .. } | NetworkTimeout => {
                (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE")
            }
            SyncFailed | DivergenceDetected { .. } => (StatusCode::CONFLICT, "CHAIN_INVALID"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            "The node could not complete the request".to_owned()
        } else {
            error.to_string()
        };
        Self::new(status, code, message)
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;
