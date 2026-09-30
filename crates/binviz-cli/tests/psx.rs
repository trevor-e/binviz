//! `--psx-exe`: an overlay opened with the boot executable's names, its own notes' included.

use std::path::Path;
use std::process::{Command, Output};

fn binviz(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_binviz")).args(args).output().unwrap()
}

#[test]
fn an_overlay_takes_the_executables_noted_names() {
    let dir = std::env::temp_dir().join(format!("binviz-cli-psx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("game.exe");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin/tiny-psx.exe"),
        &exe,
    )
    .unwrap();
    // The executable's own notes name its entry; a journal line adds a second name.
    std::fs::write(
        dir.join("game.exe.binviz-notes.json"),
        r#"[{ "address": "0x80010000", "name": "boot_main" }]"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("game.exe.binviz-notes.json.journal"),
        "{\"t\":1,\"by\":\"agent\",\"set\":{\"address\":\"0x80010020\",\"name\":\"from_journal\"}}\n",
    )
    .unwrap();
    // The executable's own bytes stand in for an overlay, loaded elsewhere.
    let exe = exe.to_str().unwrap();
    let out = binviz(&["symbols", exe, "--overlay-at", "0x80100000", "--psx-exe", exe]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("boot_main") && text.contains("from_journal"), "{text}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("2 notes from"));
    let _ = std::fs::remove_dir_all(&dir);
}
