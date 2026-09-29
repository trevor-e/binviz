//! `binviz asm` and `binviz m2c`: a MIPS function as assembler source, and m2c run on it.

use std::path::Path;
use std::process::{Command, Output};

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn binviz(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_binviz")).args(args).output().unwrap()
}

#[test]
fn a_function_as_assembler_source() {
    let out = binviz(&["asm", &fixture("tiny.z64"), "entry"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with(".set noat\n.set noreorder\n\n.section .text\n\nglabel entry\n"),
        "{text}"
    );
    assert!(text.contains("sw        $zero, %lo(VI_STATUS)($t0)\n"), "{text}");
    // Not MIPS.
    let out = binviz(&["asm", &fixture("tiny-elf-x64"), "main"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not MIPS code"));
}

#[cfg(unix)]
#[test]
fn m2c_runs_on_the_source_and_says_why_it_could_not() {
    // `cat` for m2c: what it's given comes back.
    let asm = binviz(&["asm", &fixture("tiny.z64"), "entry"]).stdout;
    let out = binviz(&["m2c", &fixture("tiny.z64"), "entry", "cat"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout, asm);
    // m2c ran, and gives its reason (in a C comment, on stdout) for not decompiling it.
    let out = binviz(&[
        "m2c",
        &fixture("tiny.z64"),
        "entry",
        "echo '/* no jump table */'; false",
    ]);
    assert!(!out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with(".set noat") && text.ends_with("/* no jump table */\n"),
        "{text}"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("reason is above"));
    // It didn't run.
    let out = binviz(&["m2c", &fixture("tiny.z64"), "entry", "/no/such/m2c"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("install m2c"));
}
