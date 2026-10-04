use binviz::{
    evidence::{ArtifactRef, sha256},
    workspace::Workspace,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn fixture() -> (Workspace, BTreeMap<String, Vec<u8>>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/acceptance");
    let facts: Value = serde_json::from_slice(&std::fs::read(root.join("facts.json")).unwrap()).unwrap();
    let evidence: Value = serde_json::from_slice(&std::fs::read(root.join("evidence.json")).unwrap()).unwrap();
    let object = std::fs::read(root.join("baseline.o")).unwrap();
    let function = binviz::matching::object_functions(&object).unwrap().remove(0);
    let bytes = format!("0x{:x}", function.code.len());
    let inventory = json!({"schemaVersion":1,"units":[{"identity":{"id":"main","asset":"native","memberOffset":"0x0","memberSize":format!("0x{:x}",std::fs::metadata(root.join("native.bin")).unwrap().len()),"loadAddress":"0x80010000","context":"original"},"memberArtifact":"native","architecture":"ps1-mipsel","functions":[{"id":"main:provider","name":"provider","identity":{"unit":"main","entry":"0x80010000","role":"primary","analysisExtent":{"start":"0x80010000","bytes":bytes,"exact":true},"matchingExtent":{"start":"0x80010000","bytes":bytes,"exact":true}},"analysisArtifact":"native-body","matchingArtifact":"native-body","owner":null}],"gaps":[]}]});
    let mut w:Workspace=serde_json::from_value(json!({"format":"binviz-workspace","schemaVersion":1,"evidence":evidence,"compilerFacts":facts,"inventory":inventory})).unwrap();
    let mut files = w.evidence.read_artifacts(&root);
    let compiler = b"fixture compiler identity".to_vec();
    for e in [&mut w.evidence, &mut w.compiler_facts.evidence] {
        e.artifacts.iter_mut().find(|a| a.id == "compiler").unwrap().sha256 = sha256(&compiler);
    }
    files.insert("compiler".into(), compiler);
    add(&mut w, &mut files, "native-body", function.code);
    add(
        &mut w,
        &mut files,
        "notes",
        binviz::notes::document(
            "main",
            "frozen-native",
            &[
                binviz::Annotation {
                    address: 0x80010000,
                    ..Default::default()
                },
                binviz::Annotation {
                    address: 0x80010100,
                    comment: "outside publication scope".into(),
                    ..Default::default()
                },
            ],
        )
        .into_bytes(),
    );
    (w, files)
}
fn add(w: &mut Workspace, f: &mut BTreeMap<String, Vec<u8>>, id: &str, b: Vec<u8>) {
    w.evidence.artifacts.push(ArtifactRef {
        id: id.into(),
        role: "fixture".into(),
        location: id.into(),
        sha256: sha256(&b),
        normalized_sha256: None,
    });
    f.insert(id.into(), b);
}
fn pins(f: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
    f.iter().map(|(id, b)| (id.clone(), sha256(b))).collect()
}
#[test]
fn selected_matches_publish_only_selected_rows_and_refuse_notes_or_producer_drift() {
    let (mut w, mut f) = fixture();
    let definition = w
        .compiler_facts
        .definitions
        .iter()
        .find(|d| d.unit == "p" && d.has_body)
        .unwrap()
        .id
        .clone();
    let r = binviz::publication::Request {
        id: "selected".into(),
        unit: "main".into(),
        notes_artifact: "notes".into(),
        notes_sha256: sha256(&f["notes"]),
        review_artifact: "publication-review".into(),
        dependencies: pins(&f),
        functions: vec![binviz::publication::Selection {
            function: "main:provider".into(),
            definition,
            source: "p:prepared".into(),
            object: "baseline-object".into(),
            producer: "baseline-compile".into(),
            symbol: "provider".into(),
        }],
        build: "compiler=Clang 14,flags=-O2 -mips1,sdk=fixture".into(),
        now: 123,
    };
    add(&mut w, &mut f, "publication-review", serde_json::to_vec(&r).unwrap());
    let p = binviz::publication::plan(&w, &r, &f).unwrap();
    assert_eq!(p["uniquePhysicalFunctions"], 1);
    assert_eq!(p["untouchedNoteRows"], 1);
    assert_eq!(p["newlyMatched"], 1);
    let notes = binviz::notes::parse(p["proposedNotes"].as_str().unwrap()).unwrap().0;
    assert_eq!(notes[1].comment, "outside publication scope");
    assert!(notes[1].decomp.is_none());
    f.get_mut("notes").unwrap().push(b' ');
    assert!(binviz::publication::plan(&w, &r, &f).is_err());
}
#[test]
fn readability_preserves_complete_objects_scores_and_source_namespaces() {
    let (mut w, mut f) = fixture();
    let d: Vec<_> = w
        .compiler_facts
        .definitions
        .iter()
        .filter(|d| d.has_body)
        .map(|d| d.id.clone())
        .collect();
    let r = binviz::readability::Request {
        id: "rename".into(),
        review_artifact: "readability-review".into(),
        dependencies: pins(&f),
        pairs: vec![binviz::readability::Pair {
            function: "main:provider".into(),
            baseline_definition: d[0].clone(),
            candidate_definition: d[1].clone(),
            baseline_object: "baseline-object".into(),
            candidate_object: "candidate-object".into(),
            baseline_stage: "baseline-compile".into(),
            candidate_stage: "candidate-compile".into(),
            baseline_percent: 100.0,
        }],
        affected_callers: d,
    };
    add(&mut w, &mut f, "readability-review", serde_json::to_vec(&r).unwrap());
    w.readability_batches.push(r);
    let p = binviz::readability::plans(&w, &f);
    assert_eq!(p[0]["state"], "ready-for-review", "{p:?}");
    assert_eq!(p[0]["pairs"][0]["matchingCreditChange"], 0);
    f.get_mut("candidate-object").unwrap().push(0);
    assert_eq!(binviz::readability::plans(&w, &f)[0]["state"], "refused");
}
