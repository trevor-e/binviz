use binviz::{
    adapters::*,
    adoption::*,
    compilerfacts::SourceSpan,
    evidence::{ArtifactRef, sha256},
    workspace::Workspace,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
fn fixture() -> (Workspace, BTreeMap<String, Vec<u8>>) {
    let mut w = Workspace::parse(include_bytes!("../../../tests/fixtures/workspace/workspace.json")).unwrap();
    let mut f = w
        .evidence
        .read_artifacts(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/workspace"));
    let compiler = b"test compiler identity, not executable".to_vec();
    for e in [&mut w.evidence, &mut w.compiler_facts.evidence] {
        e.artifacts.iter_mut().find(|a| a.id == "compiler").unwrap().sha256 = sha256(&compiler);
    }
    f.insert("compiler".into(), compiler);
    (w, f)
}
fn add(w: &mut Workspace, f: &mut BTreeMap<String, Vec<u8>>, id: &str, b: Vec<u8>) {
    w.evidence.artifacts.retain(|a| a.id != id);
    w.evidence.artifacts.push(ArtifactRef {
        id: id.into(),
        role: "linked-module".into(),
        location: id.into(),
        sha256: sha256(&b),
        normalized_sha256: None,
    });
    f.insert(id.into(), b);
}
fn request(w: &mut Workspace, f: &mut BTreeMap<String, Vec<u8>>, claim: Claim) {
    let r = Request {
        id: "adoption".into(),
        dependencies: f
            .iter()
            .filter(|(id, _)| *id != "adoption-review")
            .map(|(id, b)| (id.clone(), sha256(b)))
            .collect(),
        review_artifact: "adoption-review".into(),
        claim,
    };
    add(w, f, "adoption-review", serde_json::to_vec(&r).unwrap());
    w.adoption_requests = vec![r];
}
fn state(w: &Workspace, f: &BTreeMap<String, Vec<u8>>) -> Value {
    let report = w.analyze(Some(f)).unwrap();
    report.adoption[0].clone()
}
#[test]
fn frozen_overlay_requires_complete_inventory_and_actual_completed_outputs() {
    let (mut w, mut f) = fixture();
    add(
        &mut w,
        &mut f,
        "snapshot-index",
        serde_json::to_vec(&json!(["src/provider.i", "build/provider.o"])).unwrap(),
    );
    request(
        &mut w,
        &mut f,
        Claim::SnapshotOverlay {
            inventory: "snapshot-index".into(),
            members: vec![
                SnapshotMember {
                    path: "src/provider.i".into(),
                    input: "p:prepared".into(),
                    completed_output: None,
                },
                SnapshotMember {
                    path: "build/provider.o".into(),
                    input: "provider-object".into(),
                    completed_output: Some("provider-object".into()),
                },
            ],
        },
    );
    // The frozen workspace's actual object has a completed compiler producer.
    assert_eq!(state(&w, &f)["state"], "verified", "{}", state(&w, &f));
    f.get_mut("p:prepared").unwrap().push(b' ');
    assert_eq!(state(&w, &f)["state"], "refused");
    let (mut w, mut f) = fixture();
    add(
        &mut w,
        &mut f,
        "snapshot-index",
        serde_json::to_vec(&json!(["src/provider.i", "unlisted"])).unwrap(),
    );
    request(
        &mut w,
        &mut f,
        Claim::SnapshotOverlay {
            inventory: "snapshot-index".into(),
            members: vec![SnapshotMember {
                path: "src/provider.i".into(),
                input: "p:prepared".into(),
                completed_output: None,
            }],
        },
    );
    assert_eq!(state(&w, &f)["state"], "refused");
}
#[test]
fn stamp_transition_checks_json_fields_semantic_closure_and_consumers() {
    let (mut w, mut f) = fixture();
    for (id, b) in [
        ("old-proof", json!({"semantic":"same","dependencyStamp":"old"})),
        ("new-proof", json!({"semantic":"same","dependencyStamp":"new"})),
        ("proof-recipe", json!({"tool":"reviewed"})),
        ("consumer", json!({"proof":"old"})),
    ] {
        add(&mut w, &mut f, id, serde_json::to_vec(&b).unwrap());
    }
    for (id, output) in [("old-producer", "old-proof"), ("new-producer", "new-proof")] {
        w.evidence.stages.push(
            serde_json::from_value(
                json!({"id":id,"parents":["p:source"],"outputs":[output],"recipe":"proof-recipe","result":"success"}),
            )
            .unwrap(),
        );
    }
    w.evidence.stages.push(serde_json::from_value(json!({"id":"consumer-stage","parents":["old-proof"],"outputs":["consumer"],"recipe":"proof-recipe","result":"success"})).unwrap());
    let claim = Claim::DependencyTransition {
        before: "old-proof".into(),
        after: "new-proof".into(),
        changed_fields: vec!["/dependencyStamp".into()],
        semantic_inputs: vec![
            ArtifactPair {
                before: "p:source".into(),
                after: "p:source".into(),
            },
            ArtifactPair {
                before: "proof-recipe".into(),
                after: "proof-recipe".into(),
            },
        ],
        consumers: vec!["consumer-stage".into()],
        provider_change: None,
    };
    request(&mut w, &mut f, claim.clone());
    assert_eq!(state(&w, &f)["state"], "verified", "{}", state(&w, &f));
    add(
        &mut w,
        &mut f,
        "new-proof",
        serde_json::to_vec(&json!({"semantic":"different","dependencyStamp":"new"})).unwrap(),
    );
    request(&mut w, &mut f, claim);
    assert_eq!(state(&w, &f)["state"], "refused");
}
fn edit(start: u64, end: u64, before: &str, replacement: &str) -> Edit {
    Edit {
        start: format!("0x{start:x}"),
        end: format!("0x{end:x}"),
        before: before.into(),
        replacement: replacement.into(),
        kind: "fixture".into(),
        calls: vec![],
        policies: vec![],
    }
}
fn plan(artifact: &str, basis: &[u8], edits: Vec<Edit>) -> EditPlan {
    let mut source = String::from_utf8(basis.to_vec()).unwrap();
    for e in edits.iter().rev() {
        source.replace_range(
            binviz::evidence::hex(&e.start).unwrap() as usize..binviz::evidence::hex(&e.end).unwrap() as usize,
            &e.replacement,
        );
    }
    EditPlan {
        format: "binviz-edit-plan".into(),
        schema_version: 1,
        candidates: vec![Candidate {
            artifact: artifact.into(),
            before_sha256: sha256(basis),
            after_sha256: sha256(source.as_bytes()),
            edits,
            source,
        }],
        refused: vec![],
        already_applied: vec![],
        dependencies: BTreeMap::from([(artifact.into(), sha256(basis))]),
        references: vec![],
    }
}
#[test]
fn source_mapping_and_composition_use_one_exact_basis() {
    use binviz::sourceplan::*;
    let a = b"one\n\ntwo\n".to_vec();
    let b = b"one\ntwo\n".to_vec();
    let mut files = BTreeMap::from([("raw".into(), a.clone()), ("prepared".into(), b.clone())]);
    let map = BufferMap {
        from: "prepared".into(),
        to: "raw".into(),
        from_sha256: sha256(&b),
        to_sha256: sha256(&a),
        ranges: vec![
            Mapping {
                from_start: 0,
                from_end: 4,
                to_start: 0,
                to_end: 4,
            },
            Mapping {
                from_start: 4,
                from_end: 8,
                to_start: 5,
                to_end: 9,
            },
        ],
        review_artifact: "map-review".into(),
    };
    files.insert("map-review".into(), serde_json::to_vec(&map).unwrap());
    let span = SourceSpan {
        artifact: "prepared".into(),
        start: 4,
        end: 7,
        editable: true,
    };
    assert_eq!(map_span(&map, &span, &files).unwrap().start, 5);
    let p = plan("prepared", &b, vec![edit(4, 7, "two", "next")]);
    assert_eq!(
        map_plan(&p, &map, &files).unwrap().candidates[0].source,
        "one\n\nnext\n"
    );
    let x = plan("prepared", &b, vec![edit(0, 3, "one", "first")]);
    let merged = compose(&[p.clone(), x], &files).unwrap();
    assert_eq!(merged.candidates[0].source, "first\nnext\n");
    assert!(compose(&[p, plan("prepared", &b, vec![edit(4, 7, "two", "other")])], &files).is_err());
    assert_eq!(
        compose(
            &[
                plan("prepared", &b, vec![edit(0, 0, "", "helper1\n")]),
                plan("prepared", &b, vec![edit(0, 0, "", "helper2\n")])
            ],
            &files
        )
        .unwrap()
        .candidates[0]
            .source,
        "helper1\nhelper2\none\ntwo\n"
    );
    files.get_mut("raw").unwrap()[5] = b'X';
    assert!(map_span(&map, &span, &files).is_err());
}
#[test]
fn actual_objects_refuse_zero_data_and_accept_allocated_backing() {
    let (mut w, mut files) = fixture();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/adoption");
    for (id, name) in [
        ("data-module", "void.wasm"),
        ("data-object", "void.o"),
        ("undefined-module", "unresolved.wasm"),
        ("undefined-object", "unresolved.o"),
    ] {
        add(&mut w, &mut files, id, std::fs::read(root.join(name)).unwrap());
    }
    for (module, object, bindings, valid) in [
        (
            "data-module",
            "data-object",
            vec![json!({"symbol":"state","export":"state","bytes":"0x4","externalProfile":null,"aliasOf":null})],
            true,
        ),
        (
            "undefined-module",
            "undefined-object",
            vec![
                json!({"symbol":"missing_a","export":"missing_a","bytes":"0x4","externalProfile":null,"aliasOf":null}),
                json!({"symbol":"missing_b","export":"missing_b","bytes":"0x4","externalProfile":null,"aliasOf":null}),
            ],
            false,
        ),
    ] {
        let r:binviz::proofclosure::Request=serde_json::from_value(json!({"id":"data","dependencies":files.iter().filter(|(id,_)|*id!="data-review").map(|(id,b)|(id.clone(),sha256(b))).collect::<BTreeMap<_,_>>(),"reviewArtifact":"data-review","claim":{"kind":"data-closure","module":module,"objects":[object],"bindings":bindings,"obligations":[]}})).unwrap();
        add(&mut w, &mut files, "data-review", serde_json::to_vec(&r).unwrap());
        w.proof_closures = vec![r];
        let report = w.analyze(Some(&files)).unwrap();
        assert_eq!(
            report.proof_closures[0]["state"] == "verified",
            valid,
            "{}",
            report.proof_closures[0]
        );
        if !valid {
            assert!(
                report.proof_closures[0]["reasons"][0]
                    .as_str()
                    .unwrap()
                    .contains("DATA"),
                "{}",
                report.proof_closures[0]
            );
        }
    }
}
#[test]
fn sdk_catalog_joins_existing_matcher_source_abi_and_ambiguity() {
    let (mut w, mut f) = fixture();
    let code = f["main:member"].clone();
    // Genuine Psy-Q object record stream, fed to the existing shared parser.
    let mut obj = b"LNK\x02".to_vec();
    obj.extend([46, 7, 16, 1, 0, 0, 0, 3, 5]);
    obj.extend(b".text");
    obj.extend([6, 1, 0, 2]);
    obj.extend((code.len() as u16).to_le_bytes());
    obj.extend(code);
    for (i, name) in ["provider", "provider_alias"].into_iter().enumerate() {
        obj.push(12);
        obj.extend((i as u16 + 1).to_le_bytes());
        obj.extend(1u16.to_le_bytes());
        obj.extend(0u32.to_le_bytes());
        obj.push(name.len() as u8);
        obj.extend(name.bytes());
    }
    obj.push(0);
    add(&mut w, &mut f, "sdk-library", obj);
    let raw = serde_json::to_vec(&w.compiler_facts).unwrap();
    add(&mut w, &mut f, "sdk-facts", raw);
    add(&mut w, &mut f, "upstream", b"reviewed recovered source".to_vec());
    add(&mut w, &mut f, "license", b"reviewed license fixture".to_vec());
    let d = w
        .compiler_facts
        .definitions
        .iter()
        .find(|d| d.has_body && d.name == "provider")
        .unwrap()
        .id
        .clone();
    request(
        &mut w,
        &mut f,
        Claim::SdkCatalog {
            unit: "main".into(),
            libraries: BTreeMap::from([("sdk-library".into(), "psyq-4.3/LIBFIX.OBJ".into())]),
            entries: vec![SdkEntry {
                native: "main:provider".into(),
                recovered_source: "p:source".into(),
                compiler_facts: "sdk-facts".into(),
                definition: d,
                upstream_revision: "a".repeat(40),
                upstream_file: "src/provider.c".into(),
                upstream_source: "upstream".into(),
                license: "license".into(),
                portable: None,
            }],
        },
    );
    let result = state(&w, &f);
    assert_eq!(result["state"], "verified", "{result}");
    assert_eq!(result["details"]["entries"][0]["ambiguous"], true);
    f.get_mut("license").unwrap().push(b'!');
    assert_eq!(state(&w, &f)["state"], "refused");
}
#[test]
fn typed_import_inventory_retains_same_name_distinct_signatures_and_requires_authority() {
    let (mut w, mut f) = fixture();
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    module.extend([1, 10, 2, 0x60, 0, 1, 0x7f, 0x60, 1, 0x7f, 1, 0x7f]);
    let mut imports = vec![2];
    for ty in 0..2 {
        imports.extend([3]);
        imports.extend(b"env");
        imports.push(8);
        imports.extend(b"DrawSync");
        imports.extend([0, ty]);
    }
    module.extend([2, imports.len() as u8]);
    module.extend(imports);
    add(&mut w, &mut f, "typed-imports", module);
    let mut bindings = vec![];
    for n in 0..2 {
        let b = HostBinding {
            identity: binviz::linkevidence::ImportIdentity {
                module: "env".into(),
                field: "DrawSync".into(),
                parameters: if n == 0 { vec![] } else { vec!["i32".into()] },
                results: vec!["i32".into()],
            },
            authority: "controlled-fixture".into(),
            profile_artifact: format!("host-{n}"),
        };
        add(&mut w, &mut f, &b.profile_artifact, serde_json::to_vec(&b).unwrap());
        bindings.push(b);
    }
    request(
        &mut w,
        &mut f,
        Claim::ImportInventory {
            module: "typed-imports".into(),
            bindings: bindings.clone(),
            internally_resolved: vec![],
        },
    );
    let result = state(&w, &f);
    assert_eq!(result["state"], "verified", "{result}");
    assert_eq!(result["details"]["multiplicity"]["env.DrawSync"], 2);
    assert_eq!(result["details"]["imports"][1]["typeIndex"], 1);
    bindings.pop();
    request(
        &mut w,
        &mut f,
        Claim::ImportInventory {
            module: "typed-imports".into(),
            bindings,
            internally_resolved: vec![],
        },
    );
    assert_eq!(state(&w, &f)["state"], "refused");
}
#[test]
fn preflight_aggregates_dependency_drift_with_producer_owners() {
    let (w, mut f) = fixture();
    assert_eq!(w.preflight(&f).unwrap()["state"], "verified");
    f.get_mut("p:prepared").unwrap().push(b' ');
    f.get_mut("a:source").unwrap().push(b' ');
    let report = w.preflight(&f).unwrap();
    assert_eq!(report["state"], "refused");
    let items = report["mismatches"].as_array().unwrap();
    assert!(items.iter().any(|i| i["id"] == "p:prepared"));
    assert!(items.iter().any(|i| i["id"] == "a:source"));
    assert!(
        items
            .iter()
            .filter(|i| i["currentSha256"].is_string())
            .all(|i| i["owners"].as_array().is_some_and(|a| !a.is_empty()))
    );
}
