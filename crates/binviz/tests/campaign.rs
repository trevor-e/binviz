use binviz::campaign::*;
use serde_json::json;

fn execution() -> Execution {
    serde_json::from_value(json!({"status":"executed","instructions":1,"returnWord":"0xffff8001","ram":[{"id":"ram","start":"0x1000","data":"0x0102"}],"registers":{},"events":[{"kind":"call","pc":"0x100","provider":"writer","profile":"real","arguments":["0x2000"],"delaySlot":true},{"kind":"return","pc":"0x104"}],"checkpoints":{},"coverage":["writer"]})).unwrap()
}
fn config() -> Comparison {
    serde_json::from_value(json!({"returnMask":"0xffffffff","ram":["ram"],"registers":[],"eventKinds":["call","return","device"],"requiredCheckpoints":[],"expectedFrontiers":[]})).unwrap()
}
#[test]
fn provider_state_comparison_is_explicit_complete_and_keeps_authority_separate() {
    let mut a = execution();
    let mut b = a.clone();
    let mut c = config();
    c.provider_states = vec!["gte".into()];
    a.provider_state.insert(
        "gte".into(),
        json!({"authority":"software-model","data":(vec![0u32;32]),"control":(vec![0u32;32])}),
    );
    b.provider_state.insert(
        "gte".into(),
        json!({"authority":"hardware-oracle","data":(vec![0u32;32]),"control":(vec![0u32;32])}),
    );
    assert_eq!(compare(&c, &a, &b).unwrap().state, "passed");
    b.provider_state.get_mut("gte").unwrap()["data"][3] = json!(1);
    assert_eq!(compare(&c, &a, &b).unwrap().state, "failed");
    b.provider_state.clear();
    assert!(compare(&c, &a, &b).is_err());
}
#[test]
fn exceptions_compare_the_checkpoint_ram_and_effect_order_without_return_bits() {
    let mut a = execution();
    a.status = "exception".into();
    a.return_word = None;
    a.participant = Some(Participant {
        role: "baseline".into(),
        architecture: "ps1".into(),
        runner_kind: "native".into(),
    });
    a.exception = Some(Exception {
        kind: "division-by-zero".into(),
        operation: "cursor/divisor".into(),
        guest_pc: Some("0x80010000".into()),
        checkpoint: "trap".into(),
    });
    a.checkpoints.insert("trap".into(), json!({"cursor":"0x1"}));
    a.events.clear();
    let c = config();
    assert_eq!(compare(&c, &a, &a).unwrap().state, "passed");
    let mut b = a.clone();
    b.participant.as_mut().unwrap().architecture = "wasm32".into();
    b.exception.as_mut().unwrap().guest_pc = None;
    b.ram[0].data = "0x0202".into();
    assert_eq!(compare(&c, &a, &b).unwrap().state, "failed");
    b = a.clone();
    b.checkpoints.clear();
    assert!(compare(&c, &a, &b).unwrap_err().contains("exception-point"));
    b = a.clone();
    b.exception.as_mut().unwrap().operation = "later/divisor".into();
    assert_eq!(compare(&c, &a, &b).unwrap().state, "failed");
}
#[test]
fn full_word_ram_and_ordered_provider_events_have_concrete_negative_controls() {
    let a = execution();
    let c = config();
    assert_eq!(compare(&c, &a, &a).unwrap().state, "passed");
    let mut b = a.clone();
    b.return_word = Some("0x7fff8001".into());
    assert!(
        compare(&c, &a, &b)
            .unwrap()
            .differences
            .iter()
            .any(|d| d["kind"] == "return")
    );
    b = a.clone();
    b.ram[0].data = "0x0103".into();
    assert!(
        compare(&c, &a, &b)
            .unwrap()
            .differences
            .iter()
            .any(|d| d["kind"] == "ram-byte")
    );
    b = a.clone();
    b.events[0].delay_slot = false;
    assert!(
        compare(&c, &a, &b)
            .unwrap()
            .differences
            .iter()
            .any(|d| d["kind"] == "ordered-events")
    );
    b = a.clone();
    b.events[0].profile = Some("controlled".into());
    assert_eq!(compare(&c, &a, &b).unwrap().state, "failed");
    let mut required = c.clone();
    required.required_checkpoints.push("after-delay-slot".into());
    assert!(compare(&required, &a, &a).unwrap_err().contains("checkpoint"));
}
#[test]
fn narrow_local_correspondences_keep_extent_initialization_and_alias_checks() {
    let mut a = execution();
    let mut b = a.clone();
    b.events[0].arguments[0] = "0x4000".into();
    let object = |base: &str| LocalObject {
        id: "local".into(),
        base: base.into(),
        bytes: "0x4".into(),
        data: "0x1234abcd".into(),
        initialized: vec![ByteRange {
            offset: "0x0".into(),
            bytes: "0x2".into(),
        }],
        first_event: 0,
        last_event: 2,
    };
    a.local_objects.push(object("0x2000"));
    b.local_objects.push(object("0x4000"));
    let mut c = config();
    c.local_objects.push(CorrespondingObject {
        id: "local".into(),
        max_bytes: "0x4".into(),
        rationale: "only this stack object at this exact argument".into(),
        pointer_arguments: vec![PointerArgument { event: 0, argument: 0 }],
    });
    let r = compare(&c, &a, &b).unwrap();
    assert_eq!(r.state, "passed");
    assert_eq!(r.unspecified_local_bytes["local"], "0x2");
    b.local_objects[0].data = "0x12340000".into();
    assert_eq!(compare(&c, &a, &b).unwrap().state, "passed");
    b.local_objects[0].data = "0xff340000".into();
    assert_eq!(compare(&c, &a, &b).unwrap().state, "failed");
    c.local_objects[0].pointer_arguments[0].event = 7;
    assert!(compare(&c, &a, &b).unwrap_err().contains("event missing"));
}
#[test]
fn unsupported_cases_are_frontiers_and_never_executed_coverage() {
    let mut a = execution();
    a.status = "unsupported".into();
    a.reason = Some("unknown-device".into());
    a.instructions = 0;
    a.return_word = None;
    a.events.clear();
    let mut c = config();
    c.expected_frontiers.push("unknown-device".into());
    let r = compare(&c, &a, &a).unwrap();
    assert_eq!(r.state, "expected-frontier");
    assert!(r.coverage.is_empty());
    assert!(!r.native_executed);
    c.local_objects.push(CorrespondingObject {
        id: "not-observed".into(),
        max_bytes: "0x4".into(),
        rationale: "an expected frontier executes no observation".into(),
        pointer_arguments: vec![PointerArgument { event: 0, argument: 0 }],
    });
    assert_eq!(compare(&c, &a, &a).unwrap().state, "expected-frontier");
    a.reason = Some("another-device".into());
    assert_eq!(compare(&c, &a, &a).unwrap().state, "refused");
}
