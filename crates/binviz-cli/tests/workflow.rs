use serde_json::Value;
use std::{path::Path, process::Command};

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_binviz")).args(args).output().unwrap()
}

#[test]
fn analysis_import_reports_identity_and_refuses_a_stale_target() {
    let binary_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin/tiny-elf-x64");
    let binary = binviz::Binary::parse(std::fs::read(&binary_path).unwrap()).unwrap();
    let entry = binary.summary().entry.unwrap();
    let offset = binary.address_to_offset(entry).unwrap() as usize;
    let mut observation = serde_json::json!({
        "format":"binviz-analysis-observation","schemaVersion":1,
        "provider":{"name":"test-provider","version":"test","profileSha256":binviz::evidence::sha256(b"profile")},
        "targetSha256":binviz::evidence::sha256(binary.data()),"architecture":binary.summary().arch,"addressSpace":"default",
        "entry":format!("{entry:#x}"),"start":format!("{entry:#x}"),"bytes":"0x1",
        "nativeSha256":binviz::evidence::sha256(&binary.data()[offset..offset+1]),"pseudocode":"CLI fixture","dataflow":null,"evidenceIds":[],"unknowns":[]
    });
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/cli-observation-{}-{nonce}.json",
        std::process::id()
    ));
    std::fs::write(&path, observation.to_string()).unwrap();
    let valid = run(&["analysis", binary_path.to_str().unwrap(), path.to_str().unwrap()]);
    observation["targetSha256"] = serde_json::json!(binviz::evidence::sha256(b"stale"));
    std::fs::write(&path, observation.to_string()).unwrap();
    let stale = run(&["analysis", binary_path.to_str().unwrap(), path.to_str().unwrap()]);
    std::fs::remove_file(&path).unwrap();
    assert!(valid.status.success(), "{}", String::from_utf8_lossy(&valid.stderr));
    let report: Value = serde_json::from_slice(&valid.stdout).unwrap();
    assert_eq!(report["identityState"], "verified");
    assert_eq!(report["authority"], "provider-observation");
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("target digest mismatch"));
}

#[test]
fn next_actions_are_json_with_resolved_safe_argv_and_existing_evidence() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/workspace/workspace.json");
    let path = path.to_str().unwrap();
    let full = run(&["workspace", path, "--json"]);
    assert!(full.status.success(), "{}", String::from_utf8_lossy(&full.stderr));
    let full: Value = serde_json::from_slice(&full.stdout).unwrap();
    let result = run(&["workspace", path, "--next-actions", "--json"]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["state"], "needs-attention");
    let summary = run(&["workspace", path, "--next-actions", "--json", "--limit", "0"]);
    assert!(summary.status.success());
    let summary: Value = serde_json::from_slice(&summary.stdout).unwrap();
    assert_eq!(summary["state"], result["state"]);
    assert_eq!(summary["actions"], serde_json::json!([]));
    assert_eq!(summary["omittedActions"], summary["totalActions"]);
    assert!(summary["totalActions"].as_u64().unwrap() > 0);
    for action in result["actions"].as_array().unwrap() {
        assert_eq!(action["cli"][1], path);
        for pointer in action["evidence"].as_array().unwrap() {
            assert!(full.pointer(pointer.as_str().unwrap()).is_some());
        }
        let args: Vec<_> = action["cli"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        let follow = run(&args);
        // Preflight refuses missing/stale bytes; every command still emits valid diagnostic JSON.
        assert!(
            serde_json::from_slice::<Value>(&follow.stdout).is_ok(),
            "{args:?}: {}",
            String::from_utf8_lossy(&follow.stderr)
        );
    }
    let missing = run(&["workspace", path, "--next-actions", "--caller", "missing", "--json"]);
    assert!(!missing.status.success());
    let strict = run(&["workspace", path, "--next-actions", "--json", "--strict"]);
    assert!(!strict.status.success());
}
