//! `binviz header`: a binary's types as C, from its DWARF or its PDB.

use std::path::Path;
use std::process::{Command, Output};

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name);
    path.to_string_lossy().into_owned()
}

fn binviz(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_binviz")).args(args).output().unwrap()
}

#[test]
fn header_writes_the_types_a_pdb_describes() {
    let (exe, pdb) = (fixture("x86demo.exe"), fixture("x86demo.pdb"));
    let out = binviz(&["header", &exe, "Labelled", "mix", "--debug", &pdb]);
    let (text, note) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{note}");
    // The class, what it holds before it, and the function.
    for line in [
        "struct Shape { /* 0x8 bytes */",
        "struct Labelled { /* 0x14 bytes */",
        "/* 0x0c */ struct Named base_Named; /* base class Named */",
        "_Static_assert(sizeof(struct Labelled) == 0x14, \"struct Labelled is 0x14 bytes\");",
        "int BINVIZ_STDCALL mix(int a, int b);",
    ] {
        assert!(text.contains(line), "{line:?} missing in:\n{text}");
    }
    assert!(text.find("struct Shape {") < text.find("struct Labelled {"));
    assert!(note.contains("4 structures and unions"), "{note}");
    // Without the PDB there is nothing to write; a name nothing has is an error.
    assert!(!binviz(&["header", &exe]).status.success());
    let out = binviz(&["header", &exe, "NoSuchType", "--debug", &pdb]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no type or function named NoSuchType"));
}
