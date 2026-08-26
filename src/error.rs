use thiserror::Error;

pub type Result<T> = std::result::Result<T, MiniChainError>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MiniChainError {
    #[error("block {index} has an invalid hash: expected {expected}, calculated {calculated}")]
    InvalidBlockHash {
        index: u64,
        expected: String,
        calculated: String,
    },

    #[error("block {index} has index {actual}, but the next index is {expected}")]
    InvalidBlockIndex {
        index: u64,
        expected: u64,
        actual: u64,
    },

    #[error("block {index} does not link to the current chain tip")]
    InvalidPreviousHash { index: u64 },

    #[error("block {index} uses unsupported version {version}")]
    UnsupportedBlockVersion { index: u64, version: u16 },

    #[error("the chain cannot be empty")]
    EmptyChain,

    #[error("the first block is not the MiniChain genesis block")]
    InvalidGenesis,

    #[error("transaction {id} has an invalid hash")]
    InvalidTransactionHash { id: uuid::Uuid },

    #[error("transaction {id} has no signature")]
    MissingTransactionSignature { id: uuid::Uuid },

    #[error("the Ed25519 signature is invalid")]
    InvalidSignature,

    #[error("the Ed25519 public key is invalid")]
    InvalidPublicKey,

    #[error("the value is not valid hexadecimal data")]
    InvalidHex,

    #[error("block {index} has an invalid Merkle root")]
    InvalidMerkleRoot { index: u64 },

    #[error("transaction ID {id} appears more than once in the chain")]
    DuplicateTransaction { id: uuid::Uuid },

    #[error("a Merkle tree requires at least one leaf")]
    EmptyMerkleTree,

    #[error("transaction hash was not found in the Merkle tree")]
    TransactionNotInMerkleTree,

    #[error("block {index} has a timestamp earlier than its parent")]
    BlockTimestampBeforeParent { index: u64 },

    #[error("validator ID {id} is already registered")]
    DuplicateValidator { id: String },

    #[error("validator {id} is not registered")]
    UnknownValidator { id: String },

    #[error("validator {id} is inactive")]
    InactiveValidator { id: String },

    #[error("validator {id} does not have {permission} permission")]
    ValidatorPermissionDenied { id: String, permission: String },

    #[error("validator {id} already approved this proposal")]
    DuplicateApproval { id: String },

    #[error("proposal has {received} approvals but requires {required}")]
    InsufficientQuorum { required: usize, received: usize },

    #[error("a different block has already been proposed at height {height}")]
    ConflictingProposal { height: u64 },

    #[error("approval refers to a different proposal")]
    ApprovalForWrongProposal,

    #[error("transaction {id} is already in the mempool")]
    DuplicateMempoolTransaction { id: uuid::Uuid },

    #[error("the mempool has reached its capacity of {capacity} transactions")]
    MempoolFull { capacity: usize },

    #[error("transaction {id} has expired")]
    ExpiredTransaction { id: uuid::Uuid },

    #[error("non-genesis block {index} contains no transactions")]
    EmptyBlock { index: u64 },

    #[error("the block validator {block_validator} does not match proposer {proposer}")]
    BlockValidatorMismatch {
        block_validator: String,
        proposer: String,
    },

    #[error("proposal {hash} has already been registered")]
    DuplicateProposal { hash: String },

    #[error("transaction payload exceeds the {limit}-byte limit")]
    TransactionPayloadTooLarge { limit: usize },

    #[error("transaction metadata exceeds the configured limit")]
    TransactionMetadataTooLarge,

    #[error("transaction field {field} is empty or too large")]
    InvalidTransactionField { field: &'static str },

    #[error("no active validators are eligible to approve blocks")]
    NoActiveApprovers,

    #[error("validator {id} was not eligible when the proposal was created")]
    ValidatorNotEligibleForProposal { id: String },

    #[error("persistent storage is unavailable")]
    StorageUnavailable,

    #[error("stored data is corrupted: {reason}")]
    StorageCorruption { reason: String },

    #[error("block {height} was not found")]
    BlockNotFound { height: u64 },

    #[error("block hash {hash} was not found")]
    BlockHashNotFound { hash: String },

    #[error("transaction {id} was not found")]
    TransactionNotFound { id: uuid::Uuid },

    #[error("record {id} was not found")]
    RecordNotFound { id: String },

    #[error("stored blockchain metadata does not match chain data")]
    MetadataMismatch,

    #[error("snapshot {id} is invalid")]
    SnapshotInvalid { id: uuid::Uuid },

    #[error("snapshot is incompatible with the active chain")]
    SnapshotIncompatible,

    #[error("restoration would overwrite active state; force must be explicitly enabled")]
    RestoreWouldOverwrite,

    #[error("derived storage index is corrupted: {index}")]
    IndexCorrupted { index: &'static str },

    #[error("peer {peer} is unavailable")]
    PeerUnavailable { peer: String },

    #[error("network handshake failed")]
    HandshakeFailed,

    #[error("peer identity is not trusted: {peer}")]
    InvalidPeerIdentity { peer: String },

    #[error("unsupported network protocol version {version}")]
    ProtocolMismatch { version: u16 },

    #[error("network message is malformed or invalid")]
    InvalidMessage,

    #[error("network message exceeds the {limit}-byte limit")]
    MessageTooLarge { limit: usize },

    #[error("network synchronization failed")]
    SyncFailed,

    #[error("chain divergence detected at height {height}")]
    DivergenceDetected { height: u64 },

    #[error("network operation timed out")]
    NetworkTimeout,

    #[error("configuration is invalid")]
    InvalidConfiguration,

    #[error("node identity key is invalid or unavailable")]
    IdentityKeyUnavailable,
}
