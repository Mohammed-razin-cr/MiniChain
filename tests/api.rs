use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use minichain::{
    Block, Operation, Transaction, ValidatorIdentity,
    api::{ApiState, router},
    crypto::{encode_hex, sha256},
    mempool::Mempool,
    network::{ApiConfig, ApiRole, ApiTokenConfig, NetworkNode, NodeConfig, RunningNode},
    storage::{RedbStorage, Storage},
};
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

const VIEWER: &str = "viewer-test-token";
const OPERATOR: &str = "operator-test-token";
const ADMIN: &str = "admin-test-token";

struct Fixture {
    app: Router,
    storage: Arc<RedbStorage>,
    running: RunningNode,
    _directory: TempDir,
}

#[tokio::test]
async fn major_endpoints_use_real_node_state_and_enforce_roles() {
    let fixture = fixture(100, 100, 100, 1_000, 128 * 1024).await;

    let health = send(&fixture.app, "GET", "/api/v1/health", None, None).await;
    assert_eq!(health.status(), StatusCode::OK);
    assert!(health.headers().contains_key("x-request-id"));
    assert_eq!(body(health).await["height"], 0);

    let request_id = Uuid::new_v4().to_string();
    let cors_response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .header("origin", "http://localhost:5173")
                .header("x-request-id", &request_id)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cors_response.headers()["x-request-id"], request_id);
    assert_eq!(
        cors_response.headers()["access-control-allow-origin"],
        "http://localhost:5173"
    );

    assert_eq!(
        send(&fixture.app, "GET", "/api/v1/ready", None, None)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(&fixture.app, "GET", "/api/v1/blocks", None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&fixture.app, "GET", "/api/v1/blocks", Some("wrong"), None,)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let identity = body(
        send(
            &fixture.app,
            "GET",
            "/api/v1/auth/whoami",
            Some(VIEWER),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(identity["identity"], "viewer");
    assert_eq!(identity["role"], "viewer");

    let blocks = body(
        send(
            &fixture.app,
            "GET",
            "/api/v1/blocks?page=1&limit=10",
            Some(VIEWER),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(blocks["total"], 1);
    let genesis = fixture.storage.get_block(0).unwrap();
    assert_eq!(
        send(&fixture.app, "GET", "/api/v1/blocks/0", Some(VIEWER), None,)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &fixture.app,
            "GET",
            &format!("/api/v1/blocks/hash/{}", genesis.hash),
            Some(VIEWER),
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );

    let signer = ValidatorIdentity::from_secret_bytes("records-client", [121; 32]);
    let transaction = Transaction::new(
        Operation::CreateRecord,
        "record-api-1",
        json!({"type": "certificate", "owner": "student-01"}),
        Default::default(),
        &signer,
    );
    assert_eq!(
        send(
            &fixture.app,
            "POST",
            "/api/v1/records",
            Some(VIEWER),
            Some(serde_json::to_vec(&transaction).unwrap()),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let accepted = send(
        &fixture.app,
        "POST",
        "/api/v1/records",
        Some(OPERATOR),
        Some(serde_json::to_vec(&transaction).unwrap()),
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    assert_eq!(fixture.running.node.mempool_len().await, 1);

    let block = Block::new(1, genesis.hash, vec![transaction.clone()], "node-api").unwrap();
    fixture.storage.commit_block(block.clone()).unwrap();
    fixture
        .running
        .node
        .broadcast_block(block.clone())
        .await
        .unwrap();

    for (method, path, token) in [
        ("GET", "/api/v1/records/record-api-1", VIEWER),
        ("POST", "/api/v1/records/record-api-1/verify", VIEWER),
        (
            "GET",
            &format!("/api/v1/transactions/{}", transaction.id),
            VIEWER,
        ),
        ("GET", "/api/v1/blocks/1", VIEWER),
        ("GET", "/api/v1/validators?active=true", VIEWER),
        ("GET", "/api/v1/network/status", VIEWER),
        ("GET", "/api/v1/network/peers", VIEWER),
        ("GET", "/api/v1/network/consistency", VIEWER),
        ("POST", "/api/v1/blockchain/validate", OPERATOR),
    ] {
        let response = send(&fixture.app, method, path, Some(token), None).await;
        assert_eq!(response.status(), StatusCode::OK, "{method} {path}");
    }

    assert_eq!(
        send(
            &fixture.app,
            "POST",
            "/api/v1/snapshots/create",
            Some(OPERATOR),
            None,
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let created = send(
        &fixture.app,
        "POST",
        "/api/v1/snapshots/create",
        Some(ADMIN),
        None,
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let snapshot_id = body(created).await["id"].as_str().unwrap().to_owned();
    assert_eq!(
        send(&fixture.app, "GET", "/api/v1/snapshots", Some(ADMIN), None,)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &fixture.app,
            "GET",
            &format!("/api/v1/snapshots/{snapshot_id}/verify"),
            Some(ADMIN),
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &fixture.app,
            "POST",
            "/api/v1/snapshots/restore",
            Some(VIEWER),
            Some(serde_json::to_vec(&json!({"id": snapshot_id, "force": true})).unwrap()),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &fixture.app,
            "POST",
            "/api/v1/snapshots/restore",
            Some(ADMIN),
            Some(serde_json::to_vec(&json!({"id": snapshot_id, "force": true})).unwrap()),
        )
        .await
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn invalid_inputs_and_oversized_bodies_are_structured() {
    let fixture = fixture(100, 100, 100, 1_000, 1_024).await;
    for (method, path, payload) in [
        ("GET", "/api/v1/blocks/not-a-height", None),
        ("GET", "/api/v1/blocks/hash/not-a-hash", None),
        ("GET", "/api/v1/transactions/not-a-uuid", None),
        ("POST", "/api/v1/transactions", Some(Vec::new())),
        ("POST", "/api/v1/transactions", Some(b"{".to_vec())),
        (
            "POST",
            "/api/v1/transactions",
            Some(br#"{"unknown":true}"#.to_vec()),
        ),
    ] {
        let response = send(&fixture.app, method, path, Some(OPERATOR), payload).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{method} {path}"
        );
        assert_eq!(body(response).await["error"]["code"], "VALIDATION_ERROR");
    }

    let signer = ValidatorIdentity::from_secret_bytes("invalid-client", [122; 32]);
    let mut malformed = Transaction::new(
        Operation::CreateRecord,
        "record-invalid",
        json!({"valid": true}),
        Default::default(),
        &signer,
    );
    malformed.signature = vec![1];
    malformed.hash = malformed.calculate_hash();
    assert_eq!(
        send(
            &fixture.app,
            "POST",
            "/api/v1/transactions",
            Some(OPERATOR),
            Some(serde_json::to_vec(&malformed).unwrap()),
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let valid = Transaction::new(
        Operation::CreateRecord,
        "invalid-enum-record",
        json!({"valid": true}),
        Default::default(),
        &signer,
    );
    let mut invalid_enum = serde_json::to_value(valid).unwrap();
    invalid_enum["operation"] = json!("NOT_AN_OPERATION");
    assert_eq!(
        send(
            &fixture.app,
            "POST",
            "/api/v1/transactions",
            Some(OPERATOR),
            Some(serde_json::to_vec(&invalid_enum).unwrap()),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(
            &fixture.app,
            "GET",
            "/api/v1/blocks?page=0&limit=101",
            Some(VIEWER),
            None,
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(&fixture.app, "DELETE", "/api/v1/blocks", Some(ADMIN), None,)
            .await
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );

    let oversized = vec![b'a'; 2_048];
    let response = send(
        &fixture.app,
        "POST",
        "/api/v1/transactions",
        Some(OPERATOR),
        Some(oversized),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body(response).await["error"]["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn rate_limit_rejects_then_resets_after_window() {
    let fixture = fixture(2, 10, 10, 100, 128 * 1024).await;
    for _ in 0..2 {
        assert_eq!(
            send(&fixture.app, "GET", "/api/v1/blocks", Some(VIEWER), None,)
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        send(&fixture.app, "GET", "/api/v1/blocks", Some(VIEWER), None,)
            .await
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(
        send(&fixture.app, "GET", "/api/v1/blocks", Some(VIEWER), None,)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "manual release-mode API measurement"]
async fn measure_api_lookup_latency() {
    let fixture = fixture(10_000, 100, 100, 1_000, 128 * 1024).await;
    let signer = ValidatorIdentity::from_secret_bytes("performance-client", [123; 32]);
    let transaction = Transaction::new(
        Operation::CreateRecord,
        "performance-record",
        json!({"type": "benchmark"}),
        Default::default(),
        &signer,
    );
    let genesis = fixture.storage.get_block(0).unwrap();
    let block = Block::new(1, genesis.hash, vec![transaction.clone()], "node-api").unwrap();
    fixture.storage.commit_block(block).unwrap();
    let transaction_path = format!("/api/v1/transactions/{}", transaction.id);
    let cases = [
        ("health", "/api/v1/health", None),
        ("block", "/api/v1/blocks/1", Some(VIEWER)),
        ("transaction", transaction_path.as_str(), Some(VIEWER)),
        ("record", "/api/v1/records/performance-record", Some(VIEWER)),
        ("pagination", "/api/v1/blocks?page=1&limit=20", Some(VIEWER)),
    ];
    for (name, path, token) in cases {
        let mut samples = Vec::new();
        for _ in 0..100 {
            let started = std::time::Instant::now();
            let response = send(&fixture.app, "GET", path, token, None).await;
            assert_eq!(response.status(), StatusCode::OK);
            samples.push(started.elapsed());
        }
        samples.sort_unstable();
        let average = samples.iter().sum::<Duration>() / samples.len() as u32;
        println!(
            "{name}: average={average:?} p95={:?}",
            samples[samples.len() * 95 / 100]
        );
    }
}

async fn fixture(
    read_limit: u32,
    write_limit: u32,
    admin_limit: u32,
    window_ms: u64,
    max_body_bytes: usize,
) -> Fixture {
    let directory = TempDir::new().unwrap();
    let identity = ValidatorIdentity::from_secret_bytes("node-api", [120; 32]);
    let api_config = ApiConfig {
        listen_address: "127.0.0.1:0".to_owned(),
        allowed_origins: vec!["http://localhost:5173".to_owned()],
        max_body_bytes,
        rate_window_ms: window_ms,
        read_requests_per_window: read_limit,
        write_requests_per_window: write_limit,
        admin_requests_per_window: admin_limit,
        tokens: vec![
            token("viewer", ApiRole::Viewer, VIEWER),
            token("operator", ApiRole::Operator, OPERATOR),
            token("admin", ApiRole::Admin, ADMIN),
        ],
    };
    let chain_id = Uuid::new_v4();
    let storage = Arc::new(
        RedbStorage::open(directory.path().join("api.redb"), chain_id, "api-test").unwrap(),
    );
    let config = NodeConfig {
        node_id: "node-api".to_owned(),
        listen_address: available_address(),
        trusted_peers: HashMap::new(),
        max_peers: 8,
        chain_id,
        network_id: "api-test".to_owned(),
        storage_path: directory.path().join("api.redb"),
        identity_path: directory.path().join("api.key"),
        heartbeat_interval_ms: 60_000,
        api: Some(api_config.clone()),
    };
    let node = NetworkNode::new(
        config,
        identity,
        Arc::clone(&storage),
        Mempool::new(1_000, Duration::from_secs(300)),
    )
    .unwrap();
    let running = node.start().await.unwrap();
    let state = ApiState::new(running.node.clone(), Arc::clone(&storage), &api_config).unwrap();
    let app = router(state, &api_config).unwrap();
    Fixture {
        app,
        storage,
        running,
        _directory: directory,
    }
}

fn token(identity: &str, role: ApiRole, plaintext: &str) -> ApiTokenConfig {
    ApiTokenConfig {
        identity: identity.to_owned(),
        role,
        token_sha256: encode_hex(&sha256(plaintext.as_bytes())),
    }
}

fn available_address() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    payload: Option<Vec<u8>>,
) -> Response<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if payload.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    app.clone()
        .oneshot(
            builder
                .body(Body::from(payload.unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn body(response: Response<Body>) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}
