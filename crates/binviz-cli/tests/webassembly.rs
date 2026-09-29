//! A browser port's WebAssembly build from the command line: opened,
//! symbolicated with the debug info it names, compared with another build,
//! found in a folder with its debug files.

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

/// A custom section holding a string, as `emcc` appends one to a module.
fn custom_str(name: &str, s: &str) -> Vec<u8> {
    fn leb(out: &mut Vec<u8>, mut n: usize) {
        while n >= 0x80 {
            out.push(n as u8 | 0x80);
            n >>= 7;
        }
        out.push(n as u8);
    }
    let mut contents = Vec::new();
    for part in [name, s] {
        leb(&mut contents, part.len());
        contents.extend_from_slice(part.as_bytes());
    }
    let mut section = vec![0];
    leb(&mut section, contents.len());
    section.extend(contents);
    section
}

#[test]
fn a_web_build_folder_pairs_its_modules_with_their_debug_files() {
    let dir = std::env::temp_dir().join(format!("binviz-cli-web-build-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let put = |rel: &str, data: &[u8]| {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    };
    let read = |name: &str| std::fs::read(fixture(name)).unwrap();
    // As `emcc -gseparate-dwarf` leaves a build: the module without its
    // DWARF, naming (by URL) the module with it, kept elsewhere.
    let mut module = read("wasmdemo.bare.wasm");
    module.extend(custom_str(
        "external_debug_info",
        "https://cdn.example.com/symbols/wasmdemo.wasm.debug.wasm?v=3",
    ));
    put("game/wasmdemo.wasm", &module);
    put("symbols/wasmdemo.wasm.debug.wasm", &read("wasmdemo.wasm"));
    put(
        "game/index.html",
        b"<!doctype html><script src=\"wasmdemo.js\"></script>",
    );
    put(
        "game/wasmdemo.js",
        b"WebAssembly.instantiateStreaming(fetch(\"wasmdemo.wasm\"));",
    );
    // As `emcc -gsource-map` leaves one: the module names its map, beside it.
    put("game/worker/wasmdemo.stripped.wasm", &read("wasmdemo.stripped.wasm"));
    put("game/worker/wasmdemo.wasm.map", &read("wasmdemo.wasm.map"));
    let d = dir.to_str().unwrap();

    let (info, _) = binviz(&["info", d]);
    assert!(info.contains("2 binaries"), "{info}");
    assert!(info.contains("game/wasmdemo.wasm  [WebAssembly wasm32"), "{info}");
    assert!(info.contains("debug file symbols/wasmdemo.wasm.debug.wasm"), "{info}");
    assert!(info.contains("debug file game/worker/wasmdemo.wasm.map"), "{info}");
    assert!(info.contains("Debug symbols"), "{info}");
    // A stack trace finds the module by name, and its DWARF through the link.
    let (out, _) = binviz(&["crash", d, &fixture("wasmdemo.trace")]);
    assert!(out.contains("inlined into middle"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}
