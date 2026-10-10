use binviz::{Binary, analysisobservation, evidence::sha256};
use serde_json::{Value, json};

fn fixture() -> (Binary, Value) {
    let binary = Binary::parse(include_bytes!("../../../tests/fixtures/bin/tiny-elf-x64").to_vec()).unwrap();
    let entry = binary.summary().entry.unwrap();
    let offset = binary.address_to_offset(entry).unwrap() as usize;
    let observation = json!({
        "format":"binviz-analysis-observation", "schemaVersion":1,
        "provider":{"name":"fixture-ghidra","version":"test","profileSha256":sha256(b"profile")},
        "targetSha256":sha256(binary.data()), "architecture":binary.summary().arch,
        "addressSpace":"default", "entry":format!("{entry:#x}"), "start":format!("{entry:#x}"), "bytes":"0x1",
        "nativeSha256":sha256(&binary.data()[offset..offset+1]), "pseudocode":"provider text",
        "dataflow":null, "evidenceIds":["fixture-evidence"], "unknowns":["Not executed"]
    });
    (binary, observation)
}

#[test]
fn verified_bytes_retain_provider_authority_and_do_not_change_native_analysis() {
    let (binary, observation) = fixture();
    let before = binary.summary().fingerprint.clone();
    let count = binary.symbols().functions().count();
    let report = analysisobservation::inspect(&binary, &serde_json::to_vec(&observation).unwrap()).unwrap();
    assert_eq!(report.identity_state, "verified");
    assert_eq!(report.authority, "provider-observation");
    assert_eq!(report.observation.evidence_ids, ["fixture-evidence"]);
    assert_eq!(report.observation.unknowns, ["Not executed"]);
    assert_eq!(binary.summary().fingerprint, before);
    assert_eq!(binary.symbols().functions().count(), count);
    assert!(binary.annotations().is_empty());
}

#[test]
fn stale_target_extent_mapping_and_unsupported_identity_are_rejected() {
    let (binary, observation) = fixture();
    for (key, value) in [
        ("targetSha256", json!(sha256(b"other target"))),
        ("nativeSha256", json!(sha256(b"other extent"))),
        ("start", json!("0x0")),
        ("entry", json!("0x0")),
        ("bytes", json!("0x0")),
        ("bytes", json!("0xffffffffffffffff")),
        ("addressSpace", json!("EXTERNAL")),
        ("architecture", json!("wrong")),
        ("schemaVersion", json!(2)),
        ("matchingCredit", json!(true)),
    ] {
        let mut invalid = observation.clone();
        invalid[key] = value;
        assert!(
            analysisobservation::inspect(&binary, &serde_json::to_vec(&invalid).unwrap()).is_err(),
            "accepted {key}"
        );
    }
    let mut changed = binary.data().to_vec();
    *changed.last_mut().unwrap() ^= 1;
    let changed = Binary::parse(changed).unwrap();
    assert!(analysisobservation::inspect(&changed, &serde_json::to_vec(&observation).unwrap()).is_err());
}

#[test]
fn internal_mapping_holes_refuse_even_when_endpoint_addresses_are_backed() {
    let (binary, mut observation) = fixture();
    let segments: Vec<_> = binary
        .segments()
        .iter()
        .filter(|s| s.mapped && s.file_size > 0)
        .collect();
    let start = segments[0].address;
    let last = segments.last().unwrap();
    let end = last.address + last.file_size;
    observation["start"] = json!(format!("{start:#x}"));
    observation["entry"] = observation["start"].clone();
    observation["bytes"] = json!(format!("{:#x}", end - start));
    let error = analysisobservation::inspect(&binary, &serde_json::to_vec(&observation).unwrap()).unwrap_err();
    assert!(error.contains("not contiguously file-backed"), "{error}");
}
