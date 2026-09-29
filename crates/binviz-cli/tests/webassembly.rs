//! A browser port's WebAssembly build from the command line: opened,
//! symbolicated with the debug info it names, compared with another build.

use std::path::Path;
use std::process::Command;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name);
    path.to_str().unwrap().to_string()
}

/// Runs the command line: (stdout, stderr).
fn binviz(args: &[&str]) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_binviz")).args(args).output().unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
    (text(&out.stdout), text(&out.stderr))
}

#[test]
fn a_module_is_opened_checked_and_compared() {
    let module = fixture("wasmdemo.wasm");
    let (out, _) = binviz(&["info", &module]);
    assert!(out.starts_with("WebAssembly Module wasm32"), "{out}");
    assert!(
        out.contains("Exports: 2 functions (step, crash); 1 memory (memory)"),
        "{out}"
    );
    let (out, _) = binviz(&["check", &module]);
    assert!(out.contains("0 unclaimed"), "{out}");
    let (out, _) = binviz(&["disasm", &module, "middle"]);
    assert!(out.contains("call     8  <check>"), "{out}");
    // The same code with DWARF 5: the functions are identical.
    let (out, _) = binviz(&["diff", &module, &fixture("wasmdemo.dwarf5.wasm"), "functions"]);
    assert!(out.starts_with("Functions: 10 identical"), "{out}");
    let (out, _) = binviz(&["diff", &fixture("wasmdemo.o"), &module]);
    assert!(out.contains("messages  (removed, was 12 B)"), "{out}");
}

#[test]
fn a_stack_trace_is_symbolicated_with_the_source_map_the_module_names() {
    // The stripped build names wasmdemo.wasm.map, which is beside it.
    let (out, err) = binviz(&["crash", &fixture("wasmdemo.stripped.wasm"), &fixture("wasmdemo.trace")]);
    assert!(err.contains("wasmdemo.wasm.map, which the module names"), "{err}");
    assert!(out.contains("check  wasmdemo.c:84:9"), "{out}");
    // With the DWARF, calls inlined where it trapped.
    let (out, _) = binviz(&[
        "crash",
        &fixture("wasmdemo.bare.wasm"),
        &fixture("wasmdemo.trace"),
        "--debug",
        &fixture("wasmdemo.wasm"),
    ]);
    assert!(out.contains("inlined into middle"), "{out}");
}
