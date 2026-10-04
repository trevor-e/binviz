use binviz::{
    evidence::{ArtifactRef, StageRecord, sha256},
    linkevidence,
    workspace::{BuildRecord, PolicyApplication, Workspace},
};
use std::{collections::BTreeMap, path::Path};

fn fixture() -> (Workspace, BTreeMap<String, Vec<u8>>) {
    let mut w = Workspace::parse(include_bytes!("../../../tests/fixtures/workspace/workspace.json")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/workspace");
    let mut files = w.evidence.read_artifacts(&root);
    // The actual frozen Clang executable is intentionally not redistributed.
    // Only tests substitute its identity; production uses the original digest.
    let bytes = b"test compiler identity, not an executable".to_vec();
    for e in [&mut w.evidence, &mut w.compiler_facts.evidence] {
        e.artifacts.iter_mut().find(|a| a.id == "compiler").unwrap().sha256 = sha256(&bytes);
    }
    files.insert("compiler".into(), bytes);
    (w, files)
}
fn replace(w: &mut Workspace, files: &mut BTreeMap<String, Vec<u8>>, id: &str, bytes: Vec<u8>) {
    w.evidence.artifacts.iter_mut().find(|a| a.id == id).unwrap().sha256 = sha256(&bytes);
    files.insert(id.into(), bytes);
}
fn add(w: &mut Workspace, files: &mut BTreeMap<String, Vec<u8>>, id: &str, bytes: Vec<u8>) {
    w.evidence.artifacts.push(ArtifactRef {
        id: id.into(),
        role: "test".into(),
        location: id.into(),
        sha256: sha256(&bytes),
        normalized_sha256: None,
    });
    files.insert(id.into(), bytes);
}
#[test]
fn mixed_kill_and_preserve_callee_certificates_retain_the_word_and_bind_native_bytes() {
    use binviz::mipsaudit::{AuditPolicy, CalleeEffect, Endpoint, ExactExtent, Outcome};
    let (mut w, mut files) = fixture();
    w.callee_certificates.clear();
    // Unknown T0 selects either a killed A1 or an unchanged A1 return.
    let words = [0x11000002u32, 0, 0x24050000, 0x03e00008, 0];
    let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let u = w.inventory.units.iter_mut().find(|u| u.identity.id == "main").unwrap();
    u.identity.member_size = "0x14".into();
    u.functions[0].identity.analysis_extent.as_mut().unwrap().bytes = "0x14".into();
    for id in ["main:asset", "main:member", "main:native"] {
        replace(&mut w, &mut files, id, bytes.clone());
    }
    let proof = binviz::calleeproof::Request {
        loop_bounds: vec![],
        indirect_targets: BTreeMap::new(),
        id: "mixed-a1".into(),
        provider: "main:provider".into(),
        register: 5,
        dependencies: vec!["main:native".into()],
        review_artifact: "mixed-review".into(),
        native_sha256: sha256(&bytes),
        extent: w.inventory.units[2].functions[0]
            .identity
            .analysis_extent
            .clone()
            .unwrap(),
        callees: vec![],
        mode: Default::default(),
    };
    add(&mut w, &mut files, "mixed-review", serde_json::to_vec(&proof).unwrap());
    w.callee_certificates.push(proof);
    let r = w.analyze(Some(&files)).unwrap();
    let c = &r.callee_certificates[0];
    assert_eq!(c.state, "verified", "{:?}", c.reasons);
    assert_eq!(c.summary.as_ref().unwrap().effect, CalleeEffect::NotConsumedMayWrite);
    let endpoints = &c.audit.as_ref().unwrap().endpoints;
    assert!(endpoints.iter().any(|w| w.endpoint == Some(Endpoint::Killed)));
    assert!(endpoints.iter().any(|w| w.endpoint == Some(Endpoint::DiscardedReturn)));
    let mut p = AuditPolicy::default();
    p.callees.insert(0x80020000, c.summary.clone().unwrap());
    let caller = [0x0c008000, 0, 0xafa50000, 0x03e00008, 0];
    let after = binviz::mipsaudit::audit(
        ExactExtent {
            start: 0x80010000,
            words: &caller,
        },
        0x80010000,
        5,
        &p,
    )
    .unwrap();
    assert_eq!(
        after.outcome,
        Outcome::Consumed,
        "may-write must not suppress a post-call read"
    );
    // Composition through overlay-a's call is complete only with the child's
    // review/native dependency closure; its independently killed continuation remains.
    let outer = binviz::calleeproof::Request {
        loop_bounds: vec![],
        indirect_targets: BTreeMap::new(),
        id: "outer-a1".into(),
        provider: "overlay-a:caller_a".into(),
        register: 5,
        dependencies: vec!["overlay-a:native".into(), "main:native".into(), "mixed-review".into()],
        review_artifact: "outer-review".into(),
        native_sha256: sha256(&files["overlay-a:native"]),
        extent: w.inventory.units[0].functions[0]
            .identity
            .analysis_extent
            .clone()
            .unwrap(),
        callees: vec!["mixed-a1".into()],
        mode: Default::default(),
    };
    add(&mut w, &mut files, "outer-review", serde_json::to_vec(&outer).unwrap());
    w.callee_certificates.push(outer);
    let r = w.analyze(Some(&files)).unwrap();
    assert_eq!(
        r.callee_certificates[1].state, "verified",
        "{:?}",
        r.callee_certificates[1].reasons
    );
    assert_eq!(
        r.callee_certificates[1].summary.as_ref().unwrap().effect,
        CalleeEffect::Killed
    );
    // A new manifest digest cannot reuse a review of different native bytes.
    let mut changed = files.clone();
    let mut changed_workspace = w.clone();
    let mut changed_bytes = bytes.clone();
    changed_bytes[0] ^= 1;
    for id in ["main:asset", "main:member", "main:native"] {
        replace(&mut changed_workspace, &mut changed, id, changed_bytes.clone());
    }
    let changed_report = changed_workspace.analyze(Some(&changed)).unwrap();
    assert!(
        changed_report.callee_certificates[0]
            .reasons
            .iter()
            .any(|r| r.contains("reviewed native hash/extent"))
    );
    assert_eq!(changed_report.callee_certificates[1].state, "refused");
    files.get_mut("mixed-review").unwrap().push(b' ');
    let stale = w.analyze(Some(&files)).unwrap();
    assert_eq!(stale.callee_certificates[0].state, "refused");
    assert_eq!(stale.callee_certificates[1].state, "refused");
    w.callee_certificates[0].callees.push("outer-a1".into());
    assert!(w.validate().unwrap_err().contains("recursive"));
}
#[test]
fn reviewed_indirect_callee_targets_bind_the_complete_native_extent_and_delay_slot() {
    use binviz::mipsaudit::{CalleeEffect, ReviewedTargets};
    let (mut w, mut files) = fixture();
    w.callee_certificates.clear();
    // JR target is captured before the slot overwrites A0; A1 survives unread.
    let words = [
        0x00800008u32,
        0x24040000,
        0x03e00008,
        0,
        0x03e00008,
        0,
        0x03e00008,
        0,
        0x03e00008,
        0,
    ];
    let raw: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    for id in ["main:asset", "main:member", "main:native"] {
        replace(&mut w, &mut files, id, raw.clone());
    }
    w.inventory.units[2].identity.member_size = "0x28".into();
    w.inventory.units[2].functions[0]
        .identity
        .analysis_extent
        .as_mut()
        .unwrap()
        .bytes = "0x28".into();
    let extent = w.inventory.units[2].functions[0]
        .identity
        .analysis_extent
        .clone()
        .unwrap();
    let targets = ReviewedTargets {
        targets: vec![0x80020008, 0x80020010, 0x80020018, 0x80020020],
        evidence: "target-review".into(),
    };
    add(&mut w,&mut files,"target-review",serde_json::to_vec(&serde_json::json!({"provider":"main:provider","nativeSha256":sha256(&raw),"extent":extent,"pc":"0x80020000","targets":targets.targets})).unwrap());
    let proof = binviz::calleeproof::Request {
        id: "indirect-a1".into(),
        provider: "main:provider".into(),
        register: 5,
        dependencies: vec!["main:native".into(), "target-review".into()],
        review_artifact: "indirect-review".into(),
        native_sha256: sha256(&raw),
        extent,
        callees: vec![],
        mode: Default::default(),
        loop_bounds: vec![],
        indirect_targets: BTreeMap::from([("0x80020000".into(), targets)]),
    };
    add(
        &mut w,
        &mut files,
        "indirect-review",
        serde_json::to_vec(&proof).unwrap(),
    );
    w.callee_certificates = vec![proof];
    let report = w.analyze(Some(&files)).unwrap();
    let c = &report.callee_certificates[0];
    assert_eq!(c.state, "verified", "{:?}", c.reasons);
    assert_eq!(c.summary.as_ref().unwrap().effect, CalleeEffect::NotConsumedMayWrite);
    assert_eq!(c.explanation["derivedFrom"], "recomputed-native-audit");
    files.get_mut("target-review").unwrap().push(b' ');
    assert_eq!(w.analyze(Some(&files)).unwrap().callee_certificates[0].state, "refused");
}
#[test]
fn physical_overlays_complete_calls_and_exact_policy_scope() {
    let (w, files) = fixture();
    let report = w.analyze(Some(&files)).unwrap();
    assert!(report.inventory.collisions.is_empty());
    assert_eq!(report.callers.len(), 2);
    assert_eq!(report.raw_observations, 2);
    let a = &report.callers[0].calls[0];
    assert!(a.reasons.is_empty(), "{:?}", a.reasons);
    assert_eq!(a.policies[0].state, "eligible", "{:?}", a.policies);
    assert_eq!(a.original_instructions.len(), 2);
    assert_eq!(a.audits.len(), 1);
    assert_eq!(
        a.audits[0].provider.as_ref().unwrap().outcome,
        binviz::mipsaudit::Outcome::Dead
    );
    assert_eq!(
        a.audits[0].continuation.as_ref().unwrap().outcome,
        binviz::mipsaudit::Outcome::Dead
    );
    let b = &report.callers[1].calls[0];
    assert_eq!(b.policies[0].state, "refused");
    assert!(b.policies[0].reasons.iter().any(|r| r.contains("allowedCallers")));
    assert!(b.policies[0].reasons.iter().any(|r| r.contains("allowedSites")));
    assert_eq!(report.blockers[0].callers.len(), 2);
    assert_eq!(
        w.analyze(None).unwrap().callers[0].calls[0].policies[0].state,
        "refused"
    );
}
#[test]
fn queue_joins_physical_units_without_changing_completed_or_claim_filters() {
    use binviz::{Binary, NextQuery};
    let (w, files) = fixture();
    let member = &w.inventory.units[0].member_artifact;
    let binary = Binary::parse_psx_overlay(files[member].clone(), 0x80010000, None).unwrap();
    let q = NextQuery::default();
    let next = binary
        .next_functions_with_workspace(&q, &w, &files, &w.inventory.units[0].identity.id)
        .unwrap();
    assert!(!next.functions.is_empty());
    let joined = next.functions.iter().find(|f| f.address == 0x80010000).unwrap();
    assert_eq!(joined.contract_work.as_ref().unwrap().caller_packages.len(), 1);
    assert!(
        binary
            .next_functions_with_workspace(&q, &w, &files, &w.inventory.units[1].identity.id)
            .is_err()
    );
    let note = binviz::Annotation {
        address: 0x80010000,
        decomp: Some(binviz::Decomp {
            state: binviz::DecompState::Matched,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut done = binary;
    done.set_annotations(vec![note]);
    assert!(
        done.next_functions_with_workspace(&q, &w, &files, &w.inventory.units[0].identity.id)
            .unwrap()
            .functions
            .iter()
            .all(|f| f.address != 0x80010000)
    );
    done.set_annotations(vec![binviz::Annotation {
        address: 0x80010000,
        decomp: Some(binviz::Decomp {
            state: binviz::DecompState::InProgress,
            by: "other worker".into(),
            since: 100,
            ..Default::default()
        }),
        ..Default::default()
    }]);
    let mut q = q;
    q.now = 120;
    assert!(
        done.next_functions_with_workspace(&q, &w, &files, &w.inventory.units[0].identity.id)
            .unwrap()
            .functions
            .iter()
            .all(|f| f.address != 0x80010000)
    );
    q.include_claimed = true;
    assert!(
        done.next_functions_with_workspace(&q, &w, &files, &w.inventory.units[0].identity.id)
            .unwrap()
            .functions
            .iter()
            .any(|f| f.address == 0x80010000)
    );
}
#[test]
fn storage_size_is_independent_of_reserved_space_and_native_matching() {
    use binviz::storage::{Access, Object, StorageDescription};
    let (mut w, mut files) = fixture();
    let b = &w.bindings[0];
    let d = StorageDescription {
        id: "small-buffer".into(),
        caller: w.compiler_facts.calls[0].caller.clone(),
        artifact: "storage-review".into(),
        dependencies: vec![b.object.clone(), b.module.clone(), b.correspondence_artifact.clone()],
        objects: vec![Object {
            id: "buffer".into(),
            source: "actual C object".into(),
            bytes: "0x2".into(),
            reserved_bytes: "0x10".into(),
            alignment: "0x2".into(),
            initialized: vec![],
            lifetime: "during exact writer call".into(),
        }],
        accesses: vec![Access {
            object: "buffer".into(),
            call: b.call.clone(),
            provider: b.provider.clone(),
            kind: "write".into(),
            offset: 0,
            bytes: Some("0x4".into()),
            rationale: "reviewed selected writer writes one word".into(),
        }],
        frontiers: vec!["writer observations are not complete SDK closure".into()],
    };
    add(&mut w, &mut files, "storage-review", serde_json::to_vec(&d).unwrap());
    w.storage.push(d);
    let r = w.analyze(Some(&files)).unwrap();
    assert_eq!(r.storage["objects"][0]["accesses"][0]["state"], "out-of-bounds");
    assert_eq!(r.storage["objects"][0]["identityState"], "verified");
    assert_eq!(r.storage["closureComplete"], false);
    w.storage[0].objects[0].bytes = "0x4".into();
    let bytes = serde_json::to_vec(&w.storage[0]).unwrap();
    replace(&mut w, &mut files, "storage-review", bytes);
    assert_eq!(
        w.analyze(Some(&files)).unwrap().storage["objects"][0]["accesses"][0]["state"],
        "within-described-object"
    );
    files.get_mut("storage-review").unwrap().push(b' ');
    assert_eq!(
        w.analyze(Some(&files)).unwrap().storage["objects"][0]["identityState"],
        "unverified"
    );
}
#[test]
fn promotion_preserves_history_and_refuses_recipe_drift_or_missing_consumers() {
    use binviz::promotion::{Replacement, Request};
    let (mut w, mut files) = fixture();
    add(
        &mut w,
        &mut files,
        "candidate-prepared",
        b"candidate four-byte object".to_vec(),
    );
    add(
        &mut w,
        &mut files,
        "candidate-recipe",
        b"actual candidate flags and namespace".to_vec(),
    );
    w.evidence.stages.push(StageRecord {
        id: "candidate-prepare".into(),
        parents: vec!["a:source".into()],
        outputs: vec!["candidate-prepared".into()],
        recipe: "candidate-recipe".into(),
        result: "success".into(),
    });
    let baseline = w
        .evidence
        .stages
        .iter()
        .find(|s| s.outputs.contains(&"a:prepared".into()))
        .unwrap();
    let baseline_id = baseline.id.clone();
    let baseline_recipe = w
        .evidence
        .artifacts
        .iter()
        .find(|a| a.id == baseline.recipe)
        .unwrap()
        .sha256
        .clone();
    w.promotions.push(Request{id:"buffer-correction".into(),replacements:vec![Replacement{accepted:"a:prepared".into(),candidate:"candidate-prepared".into(),baseline_stage:baseline_id,candidate_stage:"candidate-prepare".into(),baseline_recipe_sha256:baseline_recipe,candidate_recipe_sha256:sha256(files.get("candidate-recipe").unwrap())}],prerequisites:vec!["a:source".into()],affected_consumers:vec!["a:extract".into()],scope_changes:serde_json::json!({"objectBytes":{"before":"0x2","after":"0x4"},"proofScope":"writer invocation remains required"})});
    let r = w.analyze(Some(&files)).unwrap();
    assert_eq!(
        r.promotion_plans[0]["state"], "ready-for-review",
        "{}",
        r.promotion_plans[0]
    );
    assert_eq!(r.promotion_plans[0]["historicalArtifactsRetained"], true);
    w.promotions[0].affected_consumers.clear();
    assert_eq!(w.analyze(Some(&files)).unwrap().promotion_plans[0]["state"], "refused");
    w.promotions[0].affected_consumers.push("a:extract".into());
    w.promotions[0].replacements[0].candidate_recipe_sha256 = "0".repeat(64);
    assert_eq!(w.analyze(Some(&files)).unwrap().promotion_plans[0]["state"], "refused");
}
#[test]
fn stale_inputs_count_drift_and_wrong_linked_instruction_refuse() {
    let (mut w, mut files) = fixture();
    files.get_mut("a:prepared").unwrap().push(b' ');
    let r = w.analyze(Some(&files)).unwrap();
    assert_eq!(r.callers[0].calls[0].policies[0].state, "refused");
    let (mut fresh, fresh_files) = fixture();
    fresh.policies[0]
        .expected_counts
        .insert(fresh.bindings[0].call.clone(), 3);
    let r = fresh.analyze(Some(&fresh_files)).unwrap();
    assert!(
        r.callers[0].calls[0].policies[0]
            .reasons
            .iter()
            .any(|r| r.contains("count"))
    );
    w.bindings[0].linked_call_offset = "0xa9".into();
    let binding = serde_json::to_vec(&w.bindings[0]).unwrap();
    replace(&mut w, &mut files, "a:correspondence", binding);
    assert!(
        w.analyze(Some(&files)).unwrap().callers[0].calls[0]
            .reasons
            .iter()
            .any(|r| r.contains("instruction"))
    );
}
#[test]
fn generated_edit_is_not_applied_until_current_compile_and_link() {
    let (mut w, mut files) = fixture();
    add(&mut w, &mut files, "edited", b"reviewed candidate".to_vec());
    w.evidence.stages.push(StageRecord {
        id: "edit".into(),
        parents: vec!["a:policy".into(), "a:prepared".into()],
        outputs: vec!["edited".into()],
        recipe: "build-recipe".into(),
        result: "success".into(),
    });
    w.applications.push(PolicyApplication {
        policy: w.policies[0].id.clone(),
        call: w.bindings[0].call.clone(),
        stage: "edit".into(),
        transformed_artifact: "edited".into(),
    });
    assert_eq!(
        w.analyze(Some(&files)).unwrap().callers[0].calls[0].policies[0].state,
        "refused"
    );
    add(
        &mut w,
        &mut files,
        "caller-object",
        include_bytes!("../../../tests/fixtures/workspace/caller.o").to_vec(),
    );
    w.evidence.stages.push(StageRecord {
        id: "caller-compile".into(),
        parents: vec!["edited".into()],
        outputs: vec!["caller-object".into()],
        recipe: "build-recipe".into(),
        result: "success".into(),
    });
    w.evidence
        .stages
        .iter_mut()
        .find(|s| s.id == "module-link")
        .unwrap()
        .parents
        .push("caller-object".into());
    w.builds.push(BuildRecord {
        unit: "a".into(),
        stage: "caller-compile".into(),
        phase: "compile".into(),
    });
    let r = w.analyze(Some(&files)).unwrap();
    assert_eq!(r.callers[0].calls[0].state, "verified-and-applied");
    assert_eq!(r.raw_observations, 2);
    assert_eq!(r.blockers[0].callers.len(), 1);
    files.get_mut("caller-object").unwrap().push(0);
    assert_eq!(w.analyze(Some(&files)).unwrap().callers[0].calls[0].state, "unresolved");
}
#[test]
fn failed_build_counts_only_current_inputs_and_raw_observations_stay_separate() {
    let (mut w, mut files) = fixture();
    add(
        &mut w,
        &mut files,
        "diagnostic",
        b"strict compiler rejected caller".to_vec(),
    );
    w.evidence.stages.push(StageRecord {
        id: "strict".into(),
        parents: vec!["a:prepared".into()],
        outputs: vec!["diagnostic".into()],
        recipe: "build-recipe".into(),
        result: "rejected".into(),
    });
    w.builds.push(BuildRecord {
        unit: "a".into(),
        stage: "strict".into(),
        phase: "compile".into(),
    });
    let r = w.analyze(Some(&files)).unwrap();
    assert_eq!(r.rejected_callers, 1);
    assert_eq!(r.raw_observations, 2);
    files.get_mut("a:prepared").unwrap().push(0);
    assert_eq!(w.analyze(Some(&files)).unwrap().rejected_callers, 0);
}
#[test]
fn actual_common_data_layout_and_aliases_are_checked() {
    let (w, _) = fixture();
    let actual = linkevidence::inspect(include_bytes!("../../../tests/fixtures/workspace/linked.wasm")).unwrap();
    let mut layout = w.layouts[0].clone();
    assert!(layout.verify(&actual).unwrap().is_empty());
    layout.data[0].address = "0x0".into();
    let reasons = layout.verify(&actual).unwrap();
    assert!(reasons.iter().any(|r| r.contains("unallocated")));
    assert!(reasons.iter().any(|r| r.contains("actual linked")));
    layout = w.layouts[0].clone();
    layout.stack_pointer = "0x10400".into();
    assert!(!layout.verify(&actual).unwrap().is_empty());
    let mut alias = layout.data[0].clone();
    alias.name = "alias".into();
    alias.alias_of = Some("absent".into());
    layout.data.push(alias);
    assert!(layout.verify(&actual).unwrap().iter().any(|r| r.contains("lineage")));
}

#[test]
fn adapter_plans_preserve_expressions_and_refuse_drift_or_unlisted_sites() {
    let (w, files) = fixture();
    let plan = binviz::adapters::plan(&w, &files, &[]).unwrap();
    assert_eq!(plan.candidates.len(), 1, "{:?}", plan.refused);
    assert_eq!(plan.refused.len(), 1);
    let c = &plan.candidates[0];
    assert_eq!(c.artifact, "a:prepared");
    assert!(c.source.contains("extern int provider(int);"));
    assert!(c.source.contains("(void)arg1;"));
    assert!(c.source.contains("(1, 7)"));
    let (first, already) = binviz::adapters::apply(c, &files["a:prepared"]).unwrap();
    assert!(!already);
    let (second, already) = binviz::adapters::apply(c, first.as_bytes()).unwrap();
    assert!(already);
    assert_eq!(first, second);
    let mut drift = files["a:prepared"].clone();
    drift.push(0);
    assert!(binviz::adapters::apply(c, &drift).is_err());
    let mut corrupt = c.clone();
    corrupt.edits[0].replacement.push(' ');
    assert!(binviz::adapters::apply(&corrupt, &files["a:prepared"]).is_err());
}
