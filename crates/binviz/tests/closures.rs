use binviz::{
    evidence::{ArtifactRef, sha256},
    linkevidence,
    proofclosure::*,
    workspace::Workspace,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn leb(mut n: u32) -> Vec<u8> {
    let mut v = vec![];
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        v.push(b | if n > 0 { 128 } else { 0 });
        if n == 0 {
            return v;
        }
    }
}
fn name(s: &str) -> Vec<u8> {
    let mut v = leb(s.len() as u32);
    v.extend(s.bytes());
    v
}
fn section(v: &mut Vec<u8>, id: u8, b: Vec<u8>) {
    v.push(id);
    v.extend(leb(b.len() as u32));
    v.extend(b);
}
fn module() -> Vec<u8> {
    module_with_root(&[0, 0x10, 0, 0x0b])
}
fn module_with_root(root: &[u8]) -> Vec<u8> {
    let mut v = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut v,
        1,
        vec![
            2, 0x60, 0, 1, 0x7f, 0x60, 6, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 1, 0x7f,
        ],
    );
    let mut imports = vec![3];
    imports.extend(name("env"));
    imports.extend(name("memory"));
    imports.extend([2, 1, 1, 2]);
    for ty in [0, 1] {
        imports.extend(name("hle"));
        imports.extend(name("ff9_cd_busy"));
        imports.extend([0, ty]);
    }
    section(&mut v, 2, imports);
    section(&mut v, 3, vec![2, 0, 0]);
    section(&mut v, 4, vec![1, 0x70, 1, 2, 2]);
    section(
        &mut v,
        6,
        vec![
            3, 0x7f, 1, 0x41, 0x80, 2, 0x0b, 0x7f, 0, 0x41, 0x10, 0x0b, 0x7f, 0, 0x41, 0x20, 0x0b,
        ],
    );
    let mut exports = vec![5];
    for (s, kind, index) in [
        ("root", 0, 2),
        ("callback", 0, 3),
        ("__stack_pointer", 3, 0),
        ("__data_end", 3, 1),
        ("__heap_base", 3, 2),
    ] {
        exports.extend(name(s));
        exports.extend([kind, index]);
    }
    section(&mut v, 7, exports);
    section(&mut v, 9, vec![1, 0, 0x41, 0, 0x0b, 2, 0, 2]);
    let mut bodies = vec![2];
    bodies.extend(leb(root.len() as u32));
    bodies.extend(root);
    bodies.extend([7, 0, 0x41, 0, 0x11, 0, 0, 0x0b]);
    section(&mut v, 10, bodies);
    section(&mut v, 11, vec![1, 0, 0x41, 0x10, 0x0b, 4, 1, 2, 3, 4]);
    v
}
#[test]
fn frames_require_the_real_stack_operand_and_restoration_before_returns() {
    for (body, valid) in [
        (
            vec![
                0, 0x23, 0, 0x41, 16, 0x6b, 0x24, 0, 0x10, 0, 0x23, 0, 0x41, 16, 0x6a, 0x24, 0, 0x0b,
            ],
            true,
        ),
        (
            vec![
                0, 0x41, 48, 0x41, 16, 0x6b, 0x24, 0, 0x10, 0, 0x23, 0, 0x41, 16, 0x6a, 0x24, 0, 0x0b,
            ],
            false,
        ),
        (vec![0, 0x23, 0, 0x41, 16, 0x6b, 0x24, 0, 0x10, 0, 0x0b], false),
        (
            vec![
                0, 0x23, 0, 0x41, 16, 0x6b, 0x24, 0, 0x02, 0x40, 0x0b, 0x10, 0, 0x23, 0, 0x41, 16, 0x6a, 0x24, 0, 0x0b,
            ],
            false,
        ),
    ] {
        let (mut w, mut f) = fixture();
        add(&mut w, &mut f, "selected", module_with_root(&body));
        review(
            &mut w,
            &mut f,
            Claim::CallbackStack {
                module: "selected".into(),
                roots: vec![2],
                indirect_targets: vec![],
                external_frames: vec![ExternalFrame {
                    function: 0,
                    bytes: "0x10".into(),
                    profile: "host-profile".into(),
                }],
                recursion_bounds: BTreeMap::new(),
                interrupt_roots: vec![],
                interrupt_depth: 0,
                stack_bytes: "0x80".into(),
            },
        );
        assert_eq!(inspect(&w, &f)["state"] == "verified", valid, "{}", inspect(&w, &f));
    }
}
fn fixture() -> (Workspace, BTreeMap<String, Vec<u8>>) {
    let mut w = Workspace::parse(include_bytes!("../../../tests/fixtures/workspace/workspace.json")).unwrap();
    let mut files = w
        .evidence
        .read_artifacts(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/workspace"));
    add(&mut w, &mut files, "selected", module());
    add(
        &mut w,
        &mut files,
        "host-profile",
        serde_json::to_vec(
            &json!({"function":0,"parameters":[],"results":["i32"],"stackBytes":"0x10","restoresStack":true}),
        )
        .unwrap(),
    );
    w.layouts.push(serde_json::from_value(json!({"module":"selected","guestRam":{"name":"guest","start":"0x0","bytes":"0x80"},"reserved":[{"name":"stack","start":"0x80","bytes":"0x80"}],"stackPointerExport":"__stack_pointer","stackPointer":"0x100","dataEndExport":"__data_end","dataEnd":"0x10","heapBaseExport":"__heap_base","heapBase":"0x20","memoryImport":"env.memory","tableImport":null,"sharedMemory":false,"requiredImports":["env.memory:memory","hle.ff9_cd_busy:function","hle.ff9_cd_busy:function"],"data":[]})).unwrap());
    (w, files)
}
fn add(w: &mut Workspace, f: &mut BTreeMap<String, Vec<u8>>, id: &str, bytes: Vec<u8>) {
    w.evidence.artifacts.retain(|a| a.id != id);
    w.evidence.artifacts.push(ArtifactRef {
        id: id.into(),
        role: "fixture".into(),
        location: id.into(),
        sha256: sha256(&bytes),
        normalized_sha256: None,
    });
    f.insert(id.into(), bytes);
}
fn review(w: &mut Workspace, f: &mut BTreeMap<String, Vec<u8>>, claim: Claim) {
    let layout = serde_json::to_vec(w.layouts.iter().find(|l| l.module == "selected").unwrap()).unwrap();
    add(w, f, "selected-layout", layout);
    let r = Request {
        id: "closure".into(),
        review_artifact: "review".into(),
        dependencies: BTreeMap::from([
            ("selected".into(), sha256(&f["selected"])),
            ("host-profile".into(), sha256(&f["host-profile"])),
            ("selected-layout".into(), sha256(&f["selected-layout"])),
        ]),
        claim,
    };
    add(w, f, "review", serde_json::to_vec(&r).unwrap());
    w.proof_closures = vec![r];
}
fn inspect(w: &Workspace, f: &BTreeMap<String, Vec<u8>>) -> Value {
    json!(
        binviz::proofclosure::inspect(
            w,
            f,
            &BTreeMap::from([("selected".into(), linkevidence::inspect(&f["selected"]).unwrap())])
        )
        .unwrap()[0]
    )
}
#[test]
fn actual_import_signatures_initializers_and_callback_closure_are_scoped() {
    let (mut w, mut f) = fixture();
    let m = linkevidence::inspect(&f["selected"]).unwrap();
    assert_eq!(m.functions[0].import_identity.as_ref().unwrap().parameters.len(), 0);
    assert_eq!(m.functions[1].import_identity.as_ref().unwrap().parameters.len(), 6);
    assert_eq!(m.memory_max_bytes, vec![Some("0x20000".into())]);
    assert_eq!(m.initialization_writes[0].address.as_deref(), Some("0x10"));
    let offset = m.functions[3].indirect_calls[0].clone();
    let claim = Claim::CallbackStack {
        module: "selected".into(),
        roots: vec![3],
        indirect_targets: vec![Indirect {
            caller: 3,
            offset: offset.clone(),
            targets: vec![0, 2],
            return_use: "full-word".into(),
        }],
        external_frames: vec![ExternalFrame {
            function: 0,
            bytes: "0x10".into(),
            profile: "host-profile".into(),
        }],
        recursion_bounds: BTreeMap::new(),
        interrupt_roots: vec![],
        interrupt_depth: 0,
        stack_bytes: "0x80".into(),
    };
    review(&mut w, &mut f, claim.clone());
    assert_eq!(inspect(&w, &f)["state"], "verified");
    let mut missing = claim.clone();
    if let Claim::CallbackStack { indirect_targets, .. } = &mut missing {
        indirect_targets[0].targets = vec![0];
    }
    review(&mut w, &mut f, missing);
    assert!(inspect(&w, &f)["reasons"][0].as_str().unwrap().contains("omits actual"));
    let mut wrong = claim;
    if let Claim::CallbackStack { indirect_targets, .. } = &mut wrong {
        indirect_targets[0].targets = vec![1];
    }
    review(&mut w, &mut f, wrong);
    assert!(inspect(&w, &f)["reasons"][0].as_str().unwrap().contains("wire type"));
}
#[test]
fn active_initialization_cannot_overwrite_live_memory_on_a_checkpoint_name_alone() {
    let (mut w, mut f) = fixture();
    review(
        &mut w,
        &mut f,
        Claim::SharedInitialization {
            modules: vec!["selected".into()],
            memory_import: "env.memory".into(),
            memory_bytes: "0x10000".into(),
            maximum_bytes: "0x20000".into(),
            live_regions: vec![],
            obligations: vec![],
        },
    );
    assert_eq!(inspect(&w, &f)["state"], "verified");
    let mut claim = w.proof_closures[0].claim.clone();
    if let Claim::SharedInitialization { live_regions, .. } = &mut claim {
        live_regions.push(LiveRegion {
            module: "selected".into(),
            region: linkevidence::Region {
                name: "guest".into(),
                start: "0x10".into(),
                bytes: "0x4".into(),
            },
            established_before: "selected".into(),
            restoration: None,
        });
    }
    review(&mut w, &mut f, claim);
    assert!(
        inspect(&w, &f)["reasons"][0]
            .as_str()
            .unwrap()
            .contains("overwrites live")
    );
    f.get_mut("selected").unwrap().push(0);
    assert_eq!(inspect(&w, &f)["state"], "refused");
}
