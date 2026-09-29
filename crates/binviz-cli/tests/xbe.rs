//! An original Xbox executable on the command line: every byte explained, its
//! kernel calls named.

use std::path::Path;
use std::process::Command;

/// Runs the command line on the XBE fixture: its output.
fn binviz(args: &[&str]) -> String {
    let xbe = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin/tiny.xbe");
    let out = Command::new(env!("CARGO_BIN_EXE_binviz"))
        .arg(args[0])
        .arg(&xbe)
        .args(&args[1..])
        .output()
        .unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
    text(&out.stdout)
}

#[test]
fn an_xbe_is_explained_to_the_last_byte() {
    let info = binviz(&["info"]);
    assert!(info.starts_with("XBE Executable (retail) x86 (32-bit"), "{info}");
    assert!(info.contains("Built with: XDK 5849"), "{info}");
    let check = binviz(&["check"]);
    assert!(check.contains("ok: 20524 bytes covered, 0 unclaimed"), "{check}");
    let code = binviz(&["disasm", "0x11090"]);
    assert!(
        code.contains("call     dword ptr [0x110f0]  <__imp_KeBugCheck>"),
        "{code}"
    );
}
