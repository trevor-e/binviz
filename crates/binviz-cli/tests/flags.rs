//! `binviz flags`: a unit's builds, compiled by a command, ranked by how they match.

use std::path::Path;
use std::process::Command;

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[cfg(unix)]
#[test]
fn builds_rank_best_first() {
    // The "compiler" copies a prebuilt object named by the flags.
    let out = Command::new(env!("CARGO_BIN_EXE_binviz"))
        .args([
            "flags",
            &fixture("x86match.exe"),
            "unused.c",
            "cp {flags} {out}",
            &fixture("x86match-edited.obj"),
            &fixture("x86match.obj"),
            "/no/such/object.o",
            "--debug",
            &fixture("x86match.pdb"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("100.0%    15 of   15 exact") && lines[0].ends_with("x86match.obj"), "{text}");
    assert!(lines[1].ends_with("x86match-edited.obj"), "{text}");
    assert!(lines[2].contains("didn't compile"), "{text}");
}
