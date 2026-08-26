use std::{collections::HashMap, fs};

use minichain::{
    MiniChainError, ValidatorIdentity,
    crypto::{encode_hex, sha256},
    network::{ApiConfig, ApiRole, ApiTokenConfig, NodeConfig, TrustedPeer},
};
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn toml_configuration_and_identity_survive_restart() {
    let directory = TempDir::new().unwrap();
    let peer = ValidatorIdentity::from_secret_bytes("node-02", [92; 32]);
    let config = NodeConfig {
        node_id: "node-01".to_owned(),
        listen_address: "127.0.0.1:9101".to_owned(),
        trusted_peers: HashMap::from([(
            "node-02".to_owned(),
            TrustedPeer {
                address: "127.0.0.1:9102".to_owned(),
                public_key: peer.public_key().to_vec(),
            },
        )]),
        max_peers: 8,
        chain_id: Uuid::new_v4(),
        network_id: "configuration-test".to_owned(),
        storage_path: directory.path().join("node.redb"),
        identity_path: directory.path().join("node.key"),
        heartbeat_interval_ms: 1_000,
        api: None,
    };
    let config_path = directory.path().join("node.toml");
    fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
    let loaded = NodeConfig::from_file(&config_path).unwrap();
    assert_eq!(loaded.node_id, config.node_id);
    assert_eq!(loaded.trusted_peers, config.trusted_peers);

    let first = ValidatorIdentity::load_or_create(&loaded.node_id, &loaded.identity_path).unwrap();
    let second = ValidatorIdentity::load_or_create(&loaded.node_id, &loaded.identity_path).unwrap();
    assert_eq!(first.public_key(), second.public_key());
    assert_eq!(fs::metadata(&loaded.identity_path).unwrap().len(), 32);
}

#[test]
fn malformed_or_unsafe_configuration_is_rejected() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("bad.toml");
    fs::write(&path, "node_id = 'node-01'\nunknown = true").unwrap();
    assert!(matches!(
        NodeConfig::from_file(&path),
        Err(MiniChainError::InvalidConfiguration)
    ));
}

#[test]
fn checked_in_api_configurations_are_strict_and_complete() {
    for node in ["node-01", "node-02", "node-03"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("config")
            .join(format!("{node}.toml"));
        let config = NodeConfig::from_file(path).unwrap();
        let api = config.api.expect("sample nodes expose the API");
        assert_eq!(api.tokens.len(), 3);
        assert_eq!(
            api.allowed_origins,
            ["http://localhost:3000", "http://localhost:5173"]
        );
    }
}

#[test]
fn api_configuration_rejects_wildcards_and_duplicate_credentials() {
    let digest = encode_hex(&sha256(b"configuration-token"));
    let mut config = ApiConfig {
        listen_address: "127.0.0.1:9200".to_owned(),
        allowed_origins: vec!["*".to_owned()],
        max_body_bytes: 1_024,
        rate_window_ms: 1_000,
        read_requests_per_window: 10,
        write_requests_per_window: 5,
        admin_requests_per_window: 2,
        tokens: vec![ApiTokenConfig {
            identity: "viewer".to_owned(),
            role: ApiRole::Viewer,
            token_sha256: digest.clone(),
        }],
    };
    assert_eq!(config.validate(), Err(MiniChainError::InvalidConfiguration));
    config.allowed_origins = vec!["http://localhost:5173".to_owned()];
    config.tokens.push(ApiTokenConfig {
        identity: "viewer-copy".to_owned(),
        role: ApiRole::Admin,
        token_sha256: digest,
    });
    assert_eq!(config.validate(), Err(MiniChainError::InvalidConfiguration));
}
