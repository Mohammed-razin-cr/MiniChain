use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use minichain::{
    ValidatorIdentity,
    crypto::{encode_hex, sha256},
    network::{ApiConfig, ApiRole, ApiTokenConfig, NodeConfig, TrustedPeer},
    storage::{RedbStorage, Storage},
};
use tempfile::TempDir;
use uuid::Uuid;

fn available_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

#[test]
fn three_cli_node_processes_start_with_persistent_identity_and_storage() {
    let directory = TempDir::new().unwrap();
    let chain_id = Uuid::new_v4();
    let network_id = "process-smoke-test";
    let addresses = [
        available_address(),
        available_address(),
        available_address(),
    ];
    let api_addresses = [
        available_address(),
        available_address(),
        available_address(),
    ];
    let identities = [
        ValidatorIdentity::from_secret_bytes("node-01", [101; 32]),
        ValidatorIdentity::from_secret_bytes("node-02", [102; 32]),
        ValidatorIdentity::from_secret_bytes("node-03", [103; 32]),
    ];
    let mut config_paths = Vec::new();

    for index in 0..3 {
        let node_id = format!("node-{:02}", index + 1);
        let trusted_peers = (0..3)
            .filter(|peer| *peer != index)
            .map(|peer| {
                (
                    format!("node-{:02}", peer + 1),
                    TrustedPeer {
                        address: addresses[peer].clone(),
                        public_key: identities[peer].public_key().to_vec(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let config = NodeConfig {
            node_id,
            listen_address: addresses[index].clone(),
            trusted_peers,
            max_peers: 8,
            chain_id,
            network_id: network_id.to_owned(),
            storage_path: directory.path().join(format!("node-{index}.redb")),
            identity_path: directory.path().join(format!("node-{index}.key")),
            heartbeat_interval_ms: 200,
            api: Some(ApiConfig {
                listen_address: api_addresses[index].clone(),
                allowed_origins: vec!["http://localhost:5173".to_owned()],
                max_body_bytes: 128 * 1024,
                rate_window_ms: 1_000,
                read_requests_per_window: 1_000,
                write_requests_per_window: 100,
                admin_requests_per_window: 100,
                tokens: vec![ApiTokenConfig {
                    identity: "process-admin".to_owned(),
                    role: ApiRole::Admin,
                    token_sha256: encode_hex(&sha256(b"process-viewer-token")),
                }],
            }),
        };
        fs::write(&config.identity_path, [101 + index as u8; 32]).unwrap();
        let config_path = directory.path().join(format!("node-{index}.toml"));
        fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
        config_paths.push((config_path, config));
    }

    let executable = env!("CARGO_BIN_EXE_minichain");
    let mut children = config_paths
        .iter()
        .map(|(path, _)| {
            Command::new(executable)
                .args(["node", "start", "--config"])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();

    thread::sleep(Duration::from_secs(2));
    let early_exits = children
        .iter_mut()
        .map(|child| child.try_wait().unwrap())
        .collect::<Vec<_>>();
    for address in &api_addresses {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .write_all(
                b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    }
    let first_config = &config_paths[0].0;
    for arguments in [
        vec!["node", "status"],
        vec!["chain", "status"],
        vec!["block", "latest"],
        vec!["network", "status"],
        vec!["network", "peers"],
        vec!["network", "consistency"],
        vec!["network", "ping", "node-02"],
        vec!["network", "sync", "node-02"],
        vec!["validator", "list"],
        vec!["diagnostics"],
        vec!["config", "show"],
    ] {
        let output = cli_output(executable, first_config, &arguments, true);
        assert!(
            output.status.success(),
            "{:?}: {}",
            arguments,
            String::from_utf8_lossy(&output.stderr)
        );
        let _: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    }

    let payload_path = directory.path().join("record.json");
    fs::write(&payload_path, br#"{"type":"cli-process-test"}"#).unwrap();
    let created = cli_output(
        executable,
        first_config,
        &[
            "record",
            "create",
            "CLI-RECORD",
            payload_path.to_str().unwrap(),
        ],
        true,
    );
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    let transaction_id = created["transaction_id"].as_str().unwrap();
    let pending = cli_output(
        executable,
        first_config,
        &["transaction", "status", transaction_id],
        true,
    );
    assert!(pending.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&pending.stdout).unwrap()["status"],
        "pending"
    );

    let snapshot = cli_output(executable, first_config, &["snapshot", "create"], true);
    assert!(snapshot.status.success());
    let snapshot: serde_json::Value = serde_json::from_slice(&snapshot.stdout).unwrap();
    let snapshot_id = snapshot["id"].as_str().unwrap();
    assert!(
        cli_output(
            executable,
            first_config,
            &["snapshot", "verify", snapshot_id],
            true,
        )
        .status
        .success()
    );
    let cancelled = cli_output(
        executable,
        first_config,
        &["snapshot", "restore", snapshot_id, "--force"],
        false,
    );
    assert_eq!(cancelled.status.code(), Some(1));

    let unauthorized = Command::new(executable)
        .args(["--config"])
        .arg(first_config)
        .args(["--json", "node", "status"])
        .env_remove("MINICHAIN_API_TOKEN")
        .output()
        .unwrap();
    assert_eq!(unauthorized.status.code(), Some(4));
    for (arguments, expected_code) in [
        (vec!["block", "show", "999"], 1),
        (
            vec![
                "transaction",
                "show",
                "00000000-0000-0000-0000-000000000999",
            ],
            1,
        ),
        (vec!["record", "get", "MISSING-RECORD"], 1),
        (vec!["snapshot", "verify", "not-a-uuid"], 3),
    ] {
        assert_eq!(
            cli_output(executable, first_config, &arguments, true)
                .status
                .code(),
            Some(expected_code),
            "{arguments:?}"
        );
    }
    for child in &mut children {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert_eq!(
        cli_output(executable, first_config, &["node", "status"], true)
            .status
            .code(),
        Some(4)
    );
    assert!(
        early_exits.iter().all(Option::is_none),
        "one or more node processes exited before the smoke-test window ended: {early_exits:?}"
    );

    for (_, config) in config_paths {
        let storage =
            RedbStorage::open(&config.storage_path, config.chain_id, &config.network_id).unwrap();
        assert_eq!(storage.recover().unwrap().height(), 0);
        assert_eq!(fs::metadata(config.identity_path).unwrap().len(), 32);
    }
}

fn cli_output(
    executable: &str,
    config: &std::path::Path,
    arguments: &[&str],
    authenticated: bool,
) -> std::process::Output {
    let mut command = Command::new(executable);
    command
        .args(["--config"])
        .arg(config)
        .arg("--json")
        .args(arguments)
        .stdin(Stdio::null());
    if authenticated {
        command.env("MINICHAIN_API_TOKEN", "process-viewer-token");
    } else {
        command.env_remove("MINICHAIN_API_TOKEN");
    }
    command.output().unwrap()
}
