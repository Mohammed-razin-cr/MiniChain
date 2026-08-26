use std::process::Command;

#[test]
fn help_invalid_usage_json_config_and_safe_demos_work() {
    let executable = env!("CARGO_BIN_EXE_minichain");
    assert!(
        Command::new(executable)
            .arg("--help")
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        Command::new(executable)
            .args(["block", "show"])
            .status()
            .unwrap()
            .code(),
        Some(2)
    );

    for demo in [
        "seed",
        "run",
        "blockchain",
        "tamper",
        "recovery",
        "snapshot",
        "network",
    ] {
        let output = Command::new(executable)
            .args(["--json", "demo", demo])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{demo}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["demo"], if demo == "run" { "release" } else { demo });
        if demo == "tamper" {
            assert_eq!(value["tampering_detected"], true);
        }
        if demo == "run" {
            assert_eq!(value["network"], "CONSISTENT");
            assert_eq!(value["chain"], "VALID");
            assert_eq!(value["temporary_storage"], true);
        }
    }

    let config = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/node-01.toml");
    let shown = Command::new(executable)
        .args(["--config"])
        .arg(&config)
        .args(["--json", "config", "show"])
        .output()
        .unwrap();
    assert!(shown.status.success());
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("********"));
    assert!(!text.contains("d08219426d69283f"));
}
