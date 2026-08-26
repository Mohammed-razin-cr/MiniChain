use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method, StatusCode, header},
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

use crate::{
    MiniChainError, Result,
    network::{ApiConfig, NetworkNode},
    storage::RedbStorage,
};

use super::{
    auth::AuthService,
    errors::{ApiError, ErrorBody, ErrorEnvelope},
    events, handlers,
    middleware::{RateLimiter, authenticate_and_limit, limit_public, request_id},
};

#[derive(Clone)]
pub struct ApiState {
    pub node: NetworkNode,
    pub storage: Arc<RedbStorage>,
    pub ready: Arc<AtomicBool>,
    pub(crate) auth: AuthService,
    pub(crate) limiter: RateLimiter,
}

impl ApiState {
    pub fn new(
        node: NetworkNode,
        storage: Arc<RedbStorage>,
        config: &ApiConfig,
    ) -> std::result::Result<Self, ApiError> {
        config
            .validate()
            .map_err(|_| ApiError::validation("API configuration is invalid"))?;
        Ok(Self {
            node,
            storage,
            ready: Arc::new(AtomicBool::new(true)),
            auth: AuthService::new(&config.tokens)?,
            limiter: RateLimiter::new(config),
        })
    }
}

pub fn router(state: ApiState, config: &ApiConfig) -> std::result::Result<Router, ApiError> {
    let public = Router::new()
        .route("/api/v1/health", get(handlers::health))
        .route("/api/v1/ready", get(handlers::ready))
        .route("/api/v1/events", get(events::events))
        .layer(middleware::from_fn_with_state(state.clone(), limit_public));
    let protected = Router::new()
        .route("/api/v1/auth/whoami", get(handlers::whoami))
        .route("/api/v1/blocks", get(handlers::list_blocks))
        .route("/api/v1/blocks/{height}", get(handlers::get_block))
        .route(
            "/api/v1/blocks/{height}/verify",
            post(handlers::verify_block),
        )
        .route(
            "/api/v1/blocks/hash/{hash}",
            get(handlers::get_block_by_hash),
        )
        .route("/api/v1/transactions/{id}", get(handlers::get_transaction))
        .route(
            "/api/v1/transactions/{id}/status",
            get(handlers::transaction_status),
        )
        .route(
            "/api/v1/transactions",
            get(handlers::list_transactions).post(handlers::submit_transaction),
        )
        .route("/api/v1/records", post(handlers::create_record))
        .route("/api/v1/records/{id}", get(handlers::get_record))
        .route("/api/v1/records/{id}/verify", post(handlers::verify_record))
        .route(
            "/api/v1/records/{id}/history",
            get(handlers::record_history),
        )
        .route("/api/v1/validators", get(handlers::validators))
        .route("/api/v1/validators/{id}", get(handlers::get_validator))
        .route("/api/v1/storage/stats", get(handlers::storage_stats))
        .route("/api/v1/network/status", get(handlers::network_status))
        .route("/api/v1/network/peers", get(handlers::network_peers))
        .route(
            "/api/v1/network/consistency",
            get(handlers::network_consistency),
        )
        .route("/api/v1/network/sync/{peer}", post(handlers::network_sync))
        .route("/api/v1/network/ping/{peer}", post(handlers::network_ping))
        .route(
            "/api/v1/network/connect/{peer}",
            post(handlers::network_connect),
        )
        .route(
            "/api/v1/blockchain/validate",
            post(handlers::validate_chain),
        )
        .route("/api/v1/snapshots/create", post(handlers::create_snapshot))
        .route(
            "/api/v1/snapshots/restore",
            post(handlers::restore_snapshot),
        )
        .route("/api/v1/snapshots", get(handlers::list_snapshots))
        .route(
            "/api/v1/snapshots/{id}/verify",
            get(handlers::verify_snapshot),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            authenticate_and_limit,
        ));
    let cors = cors(config)?;
    Ok(Router::new()
        .merge(public)
        .merge(protected)
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(DefaultBodyLimit::max(config.max_body_bytes))
        .layer(cors)
        .layer(middleware::from_fn(request_id))
        .with_state(state))
}

pub async fn serve(listener: TcpListener, state: ApiState, config: &ApiConfig) -> Result<()> {
    let app = router(state, config).map_err(|_| MiniChainError::InvalidConfiguration)?;
    axum::serve(listener, app)
        .await
        .map_err(|_| MiniChainError::NetworkTimeout)
}

fn cors(config: &ApiConfig) -> std::result::Result<CorsLayer, ApiError> {
    if config.allowed_origins.iter().any(|origin| origin == "*") {
        return Err(ApiError::validation(
            "Wildcard CORS origins are not allowed",
        ));
    }
    let origins = config
        .allowed_origins
        .iter()
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .map_err(|_| ApiError::validation("Configured CORS origin is invalid"))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(CorsLayer::new()
        .allow_credentials(false)
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderNameExt::request_id(),
        ])
        .expose_headers([HeaderNameExt::request_id()])
        .max_age(Duration::from_secs(600)))
}

async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorEnvelope {
            error: ErrorBody {
                code: "NOT_FOUND",
                message: "The requested API route does not exist".to_owned(),
            },
        }),
    )
}

async fn method_not_allowed() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(ErrorEnvelope {
            error: ErrorBody {
                code: "METHOD_NOT_ALLOWED",
                message: "The HTTP method is not supported for this route".to_owned(),
            },
        }),
    )
}

struct HeaderNameExt;

impl HeaderNameExt {
    fn request_id() -> axum::http::HeaderName {
        axum::http::HeaderName::from_static("x-request-id")
    }
}
