use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{
        Path, Query,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    MiniChainError, Operation, Transaction,
    network::{ApiRole, PeerState, SyncState},
    storage::{RecordVerification, Storage},
};

use super::{
    auth::AuthContext,
    dto::*,
    errors::{ApiError, ApiResult},
    router::ApiState,
};

const DEFAULT_LIMIT: u64 = 20;
const MAX_LIMIT: u64 = 100;

pub(crate) async fn whoami(
    Extension(context): Extension<AuthContext>,
) -> ApiResult<Json<AuthIdentityResponse>> {
    context.require(ApiRole::Viewer)?;
    Ok(Json(AuthIdentityResponse {
        identity: context.identity,
        role: context.role,
    }))
}

pub(crate) async fn health(
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> ApiResult<Json<HealthResponse>> {
    let local = state.node.local_status().await.map_err(ApiError::from)?;
    let peers = state.node.peers().await;
    Ok(Json(HealthResponse {
        status: "healthy",
        node_id: local.node_id,
        height: local.height,
        sync: sync_state(local.height, &local.latest_hash, &peers),
        peers: peers.len(),
    }))
}

pub(crate) async fn ready(
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> (StatusCode, Json<ReadyResponse>) {
    let ready = state.ready.load(std::sync::atomic::Ordering::Acquire);
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadyResponse { ready }),
    )
}

pub(crate) async fn list_blocks(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    query: Result<Query<BlockListQuery>, QueryRejection>,
) -> ApiResult<Json<Page<BlockResponse>>> {
    context.require(ApiRole::Viewer)?;
    let query = query
        .map_err(|_| ApiError::validation("Block pagination parameters are invalid"))?
        .0;
    let (page, limit) = pagination_values(query.page, query.limit)?;
    let requested_from = query.from;
    let storage = Arc::clone(&state.storage);
    let response = storage_task(move || {
        let total = storage.stats()?.blocks;
        let offset = requested_from.unwrap_or_else(|| (page - 1).saturating_mul(limit));
        let blocks = if offset >= total {
            Vec::new()
        } else {
            storage.get_blocks(offset, (offset + limit - 1).min(total - 1))?
        };
        Ok(Page {
            items: blocks
                .into_iter()
                .map(|block| BlockResponse::from_block(block, false))
                .collect(),
            page,
            limit,
            total,
        })
    })
    .await?;
    Ok(Json(response))
}

pub(crate) async fn get_block(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(height): Path<String>,
) -> ApiResult<Json<BlockResponse>> {
    context.require(ApiRole::Viewer)?;
    let height = height
        .parse::<u64>()
        .map_err(|_| ApiError::validation("Block height must be an unsigned integer"))?;
    let storage = Arc::clone(&state.storage);
    let block = storage_task(move || storage.get_block(height)).await?;
    Ok(Json(BlockResponse::from_block(block, true)))
}

pub(crate) async fn get_block_by_hash(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(hash): Path<String>,
) -> ApiResult<Json<BlockResponse>> {
    context.require(ApiRole::Viewer)?;
    validate_hash(&hash)?;
    let storage = Arc::clone(&state.storage);
    let block = storage_task(move || storage.get_block_by_hash(&hash)).await?;
    Ok(Json(BlockResponse::from_block(block, true)))
}

pub(crate) async fn get_transaction(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<TransactionResponse>> {
    context.require(ApiRole::Viewer)?;
    let id = parse_uuid(&id, "transaction ID")?;
    let storage = Arc::clone(&state.storage);
    let transaction = storage_task(move || storage.get_transaction(id)).await?;
    Ok(Json(transaction.into()))
}

pub(crate) async fn list_transactions(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    query: Result<Query<TransactionListQuery>, QueryRejection>,
) -> ApiResult<Json<Page<TransactionResponse>>> {
    context.require(ApiRole::Viewer)?;
    let query = query
        .map_err(|_| ApiError::validation("Transaction filters are invalid"))?
        .0;
    let (page, limit) = pagination_values(query.page, query.limit)?;
    let operation = query.operation;
    let storage = Arc::clone(&state.storage);
    let transactions = storage_task(move || storage.transactions()).await?;
    let transactions = transactions
        .into_iter()
        .filter(|stored| operation.is_none_or(|value| stored.transaction.operation == value))
        .map(TransactionResponse::from)
        .collect();
    Ok(Json(page_items(transactions, page, limit)))
}

pub(crate) async fn transaction_status(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<TransactionStatusResponse>> {
    context.require(ApiRole::Viewer)?;
    let id = parse_uuid(&id, "transaction ID")?;
    let storage = Arc::clone(&state.storage);
    match storage_task(move || storage.get_transaction(id)).await {
        Ok(stored) => Ok(Json(TransactionStatusResponse {
            transaction_id: id,
            status: "committed",
            block_height: Some(stored.block_height),
            block_hash: Some(stored.block_hash),
        })),
        Err(error) if error.status == StatusCode::NOT_FOUND => {
            if state.node.mempool_transaction(id).await.is_some() {
                Ok(Json(TransactionStatusResponse {
                    transaction_id: id,
                    status: "pending",
                    block_height: None,
                    block_hash: None,
                }))
            } else {
                Err(error)
            }
        }
        Err(error) => Err(error),
    }
}

pub(crate) async fn submit_transaction(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    body: Result<Json<TransactionRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<SubmissionResponse>)> {
    context.require(ApiRole::Operator)?;
    let transaction = Transaction::from(json_body(body)?);
    transaction.validate().map_err(ApiError::from)?;
    let id = transaction.id;
    state
        .node
        .broadcast_transaction(transaction)
        .await
        .map_err(ApiError::from)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(SubmissionResponse {
            accepted: true,
            transaction_id: id,
            mempool_size: state.node.mempool_len().await,
        }),
    ))
}

pub(crate) async fn create_record(
    state: axum::extract::State<ApiState>,
    context: Extension<AuthContext>,
    body: Result<Json<TransactionRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<SubmissionResponse>)> {
    if let Ok(Json(transaction)) = &body
        && transaction.operation != Operation::CreateRecord
    {
        return Err(ApiError::validation(
            "Record creation requires operation CREATE_RECORD",
        ));
    }
    submit_transaction(state, context, body).await
}

pub(crate) async fn get_record(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<RecordResponse>> {
    context.require(ApiRole::Viewer)?;
    validate_record_id(&id)?;
    let storage = Arc::clone(&state.storage);
    let verification = storage_task(move || storage.verify_record(&id)).await?;
    Ok(Json(record_response(verification)))
}

pub(crate) async fn verify_record(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<RecordResponse>> {
    context.require(ApiRole::Viewer)?;
    validate_record_id(&id)?;
    let storage = Arc::clone(&state.storage);
    let verification = storage_task(move || storage.verify_record(&id)).await?;
    Ok(Json(record_response(verification)))
}

pub(crate) async fn record_history(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(id): Path<String>,
    query: Result<Query<PaginationQuery>, QueryRejection>,
) -> ApiResult<Json<Page<TransactionResponse>>> {
    context.require(ApiRole::Viewer)?;
    validate_record_id(&id)?;
    let (page, limit) = pagination(query)?;
    let storage = Arc::clone(&state.storage);
    let history = storage_task(move || storage.record_history(&id)).await?;
    Ok(Json(page_items(
        history.into_iter().map(TransactionResponse::from).collect(),
        page,
        limit,
    )))
}

pub(crate) async fn validators(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    query: Result<Query<ValidatorQuery>, QueryRejection>,
) -> ApiResult<Json<Page<ValidatorResponse>>> {
    context.require(ApiRole::Viewer)?;
    let query = query
        .map_err(|_| ApiError::validation("Invalid validator filter"))?
        .0;
    let (page, limit) = pagination_values(query.page, query.limit)?;
    let filter = query.active;
    let storage = Arc::clone(&state.storage);
    let validators = storage_task(move || storage.validators()).await?;
    let mut validators = validators
        .into_iter()
        .filter(|validator| filter.is_none_or(|active| validator.active == active))
        .map(|validator| ValidatorResponse {
            validator_id: validator.id,
            public_key: validator.public_key,
            active: validator.active,
            current_height: validator.block_height,
            last_heartbeat: validator.last_heartbeat,
            network_address: validator.network_address,
        })
        .collect::<Vec<_>>();
    validators.sort_by(|left, right| left.validator_id.cmp(&right.validator_id));
    Ok(Json(page_items(validators, page, limit)))
}

pub(crate) async fn get_validator(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<ValidatorResponse>> {
    context.require(ApiRole::Viewer)?;
    let storage = Arc::clone(&state.storage);
    let validator = storage_task(move || storage.get_validator(&id)).await?;
    Ok(Json(ValidatorResponse {
        validator_id: validator.id,
        public_key: validator.public_key,
        active: validator.active,
        current_height: validator.block_height,
        last_heartbeat: validator.last_heartbeat,
        network_address: validator.network_address,
    }))
}

pub(crate) async fn verify_block(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(height): Path<String>,
) -> ApiResult<Json<ChainValidationResponse>> {
    context.require(ApiRole::Viewer)?;
    let height = height
        .parse::<u64>()
        .map_err(|_| ApiError::validation("Block height must be an unsigned integer"))?;
    let storage = Arc::clone(&state.storage);
    let block = storage_task(move || storage.get_block(height)).await?;
    match block.validate_contents() {
        Ok(()) => Ok(Json(ChainValidationResponse {
            valid: true,
            checked_height: height,
            failure_block: None,
            failure_reason: None,
        })),
        Err(error) => Ok(Json(ChainValidationResponse {
            valid: false,
            checked_height: height,
            failure_block: Some(height),
            failure_reason: Some(error.to_string()),
        })),
    }
}

pub(crate) async fn storage_stats(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
) -> ApiResult<Json<crate::storage::StorageStats>> {
    context.require(ApiRole::Viewer)?;
    let storage = Arc::clone(&state.storage);
    Ok(Json(storage_task(move || storage.stats()).await?))
}

pub(crate) async fn network_status(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
) -> ApiResult<Json<NetworkStatusResponse>> {
    context.require(ApiRole::Viewer)?;
    let local = state.node.local_status().await.map_err(ApiError::from)?;
    let peers = state.node.peers().await;
    let healthy = peers
        .iter()
        .filter(|peer| peer.state == PeerState::Ready)
        .count();
    Ok(Json(NetworkStatusResponse {
        node_id: local.node_id,
        height: local.height,
        latest_hash: local.latest_hash.clone(),
        sync_state: sync_state(local.height, &local.latest_hash, &peers),
        peer_count: peers.len(),
        healthy_peers: healthy,
        protocol_version: local.protocol_version,
        mempool_size: state.node.mempool_len().await,
    }))
}

pub(crate) async fn network_peers(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    query: Result<Query<PaginationQuery>, QueryRejection>,
) -> ApiResult<Json<Page<PeerResponse>>> {
    context.require(ApiRole::Viewer)?;
    let (page, limit) = pagination(query)?;
    let mut peers = state
        .node
        .peers()
        .await
        .into_iter()
        .map(PeerResponse::from)
        .collect::<Vec<_>>();
    peers.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(Json(page_items(peers, page, limit)))
}

pub(crate) async fn network_consistency(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
) -> ApiResult<Json<ConsistencyResponse>> {
    context.require(ApiRole::Viewer)?;
    let local = state.node.local_status().await.map_err(ApiError::from)?;
    let peers = state.node.peers().await;
    let inconsistent_peers = peers
        .iter()
        .filter(|peer| peer.height != local.height || peer.latest_hash != local.latest_hash)
        .map(|peer| peer.id.clone())
        .collect::<Vec<_>>();
    Ok(Json(ConsistencyResponse {
        consistent: inconsistent_peers.is_empty(),
        checked_peers: peers.len(),
        local_height: local.height,
        local_hash: local.latest_hash,
        inconsistent_peers,
    }))
}

pub(crate) async fn network_sync(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(peer): Path<String>,
) -> ApiResult<Json<NetworkActionResponse>> {
    context.require(ApiRole::Operator)?;
    let status = state.node.sync_from(&peer).await.map_err(ApiError::from)?;
    Ok(Json(NetworkActionResponse {
        peer_id: peer,
        height: status.height,
        latest_hash: status.latest_hash,
        latency_ms: None,
    }))
}

pub(crate) async fn network_ping(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(peer): Path<String>,
) -> ApiResult<Json<NetworkActionResponse>> {
    context.require(ApiRole::Operator)?;
    let latency = state
        .node
        .heartbeat_once(&peer)
        .await
        .map_err(ApiError::from)?;
    let local = state.node.local_status().await.map_err(ApiError::from)?;
    Ok(Json(NetworkActionResponse {
        peer_id: peer,
        height: local.height,
        latest_hash: local.latest_hash,
        latency_ms: Some(latency.as_millis() as u64),
    }))
}

pub(crate) async fn network_connect(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(peer): Path<String>,
) -> ApiResult<Json<NetworkActionResponse>> {
    context.require(ApiRole::Operator)?;
    let status = state.node.connect(&peer).await.map_err(ApiError::from)?;
    Ok(Json(NetworkActionResponse {
        peer_id: peer,
        height: status.height,
        latest_hash: status.latest_hash,
        latency_ms: None,
    }))
}

pub(crate) async fn validate_chain(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
) -> ApiResult<Json<ChainValidationResponse>> {
    context.require(ApiRole::Operator)?;
    let storage = Arc::clone(&state.storage);
    let result = tokio::task::spawn_blocking(move || storage.recover())
        .await
        .map_err(|_| ApiError::from(MiniChainError::StorageUnavailable))?;
    let response = match result {
        Ok(chain) => {
            let validation = chain.validate();
            ChainValidationResponse {
                valid: validation.valid,
                checked_height: chain.height(),
                failure_block: validation.issues.first().map(|issue| issue.block_index),
                failure_reason: validation.issues.first().map(|issue| issue.reason.clone()),
            }
        }
        Err(error) => ChainValidationResponse {
            valid: false,
            checked_height: state
                .storage
                .metadata()
                .map_or(0, |value| value.current_height),
            failure_block: failure_block(&error),
            failure_reason: Some(error.to_string()),
        },
    };
    Ok(Json(response))
}

pub(crate) async fn create_snapshot(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
) -> ApiResult<(StatusCode, Json<SnapshotResponse>)> {
    context.require(ApiRole::Admin)?;
    let storage = Arc::clone(&state.storage);
    let snapshot = storage_task(move || storage.create_snapshot()).await?;
    Ok((
        StatusCode::CREATED,
        Json(SnapshotResponse::from_snapshot(snapshot, Some(true))),
    ))
}

pub(crate) async fn list_snapshots(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    query: Result<Query<PaginationQuery>, QueryRejection>,
) -> ApiResult<Json<Page<SnapshotResponse>>> {
    context.require(ApiRole::Admin)?;
    let (page, limit) = pagination(query)?;
    let storage = Arc::clone(&state.storage);
    let snapshots = storage_task(move || storage.snapshots()).await?;
    let total = snapshots.len() as u64;
    let offset = (page - 1).saturating_mul(limit) as usize;
    let items = snapshots
        .into_iter()
        .skip(offset)
        .take(limit as usize)
        .map(|snapshot| SnapshotResponse::from_snapshot(snapshot, None))
        .collect();
    Ok(Json(Page {
        items,
        page,
        limit,
        total,
    }))
}

pub(crate) async fn verify_snapshot(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<SnapshotResponse>> {
    context.require(ApiRole::Admin)?;
    let id = parse_uuid(&id, "snapshot ID")?;
    let storage = Arc::clone(&state.storage);
    let snapshot = storage_task(move || storage.verify_snapshot(id)).await?;
    Ok(Json(SnapshotResponse::from_snapshot(snapshot, Some(true))))
}

pub(crate) async fn restore_snapshot(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(context): Extension<AuthContext>,
    body: Result<Json<RestoreSnapshotRequest>, JsonRejection>,
) -> ApiResult<Json<ChainValidationResponse>> {
    context.require(ApiRole::Admin)?;
    let request = json_body(body)?;
    let chain = state
        .node
        .restore_snapshot(request.id, request.force)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ChainValidationResponse {
        valid: true,
        checked_height: chain.height(),
        failure_block: None,
        failure_reason: None,
    }))
}

fn record_response(verification: RecordVerification) -> RecordResponse {
    let transaction = TransactionResponse::from(verification.latest_transaction);
    RecordResponse {
        record_id: verification.record.record_id,
        status: verification.record.status,
        data: verification.record.data,
        block_height: transaction.block_height,
        block_hash: transaction.block_hash.clone(),
        latest_transaction: transaction,
        cryptographically_verified: verification.cryptographically_verified,
        merkle_proof_valid: verification.merkle_proof_valid,
        chain_valid: verification.chain_valid,
    }
}

fn pagination(query: Result<Query<PaginationQuery>, QueryRejection>) -> ApiResult<(u64, u64)> {
    let query = query
        .map_err(|_| ApiError::validation("Pagination parameters are invalid"))?
        .0;
    pagination_values(query.page, query.limit)
}

fn pagination_values(page: Option<u64>, limit: Option<u64>) -> ApiResult<(u64, u64)> {
    let page = page.unwrap_or(1);
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if page == 0 || limit == 0 || limit > MAX_LIMIT {
        return Err(ApiError::validation(format!(
            "page must be positive and limit must be between 1 and {MAX_LIMIT}"
        )));
    }
    Ok((page, limit))
}

fn page_items<T>(items: Vec<T>, page: u64, limit: u64) -> Page<T> {
    let total = items.len() as u64;
    let offset = (page - 1).saturating_mul(limit) as usize;
    Page {
        items: items
            .into_iter()
            .skip(offset)
            .take(limit as usize)
            .collect(),
        page,
        limit,
        total,
    }
}

fn json_body<T>(body: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    body.map(|Json(value)| value).map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "VALIDATION_ERROR",
                "The request body is too large",
            )
        } else {
            ApiError::validation("The JSON request body is invalid")
        }
    })
}

fn parse_uuid(value: &str, name: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(value).map_err(|_| ApiError::validation(format!("Invalid {name}")))
}

fn validate_hash(value: &str) -> ApiResult<()> {
    if value.len() != 64 || crate::crypto::decode_hex(value).is_err() {
        return Err(ApiError::validation(
            "Block hash must be 64 hexadecimal characters",
        ));
    }
    Ok(())
}

fn validate_record_id(value: &str) -> ApiResult<()> {
    if value.is_empty() || value.len() > crate::blockchain::MAX_RECORD_ID_BYTES {
        return Err(ApiError::validation("Record ID is empty or too large"));
    }
    Ok(())
}

fn sync_state(height: u64, hash: &str, peers: &[crate::network::Peer]) -> SyncState {
    if peers.iter().any(|peer| peer.state == PeerState::Syncing) {
        SyncState::Syncing
    } else if peers.iter().any(|peer| {
        peer.height == height && !peer.latest_hash.is_empty() && peer.latest_hash != hash
    }) {
        SyncState::Diverged
    } else if peers.iter().any(|peer| peer.height > height) {
        SyncState::Behind
    } else if !peers.is_empty()
        && peers
            .iter()
            .all(|peer| matches!(peer.state, PeerState::Disconnected | PeerState::Failed))
    {
        SyncState::Offline
    } else {
        SyncState::Synced
    }
}

fn failure_block(error: &MiniChainError) -> Option<u64> {
    match error {
        MiniChainError::InvalidBlockHash { index, .. }
        | MiniChainError::InvalidBlockIndex { index, .. }
        | MiniChainError::InvalidPreviousHash { index }
        | MiniChainError::UnsupportedBlockVersion { index, .. }
        | MiniChainError::InvalidMerkleRoot { index }
        | MiniChainError::BlockTimestampBeforeParent { index }
        | MiniChainError::EmptyBlock { index } => Some(*index),
        _ => None,
    }
}

async fn storage_task<T, F>(operation: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> crate::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|_| ApiError::from(MiniChainError::StorageUnavailable))?
        .map_err(ApiError::from)
}
