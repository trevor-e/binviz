use std::{fs, path::Path, process::Command};

#[test]
fn progress_images_load_sidecar_notes_and_allow_an_explicit_override() {
    let dir = std::env::temp_dir().join(format!("binviz-cli-progress-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("game.exe");
    let notes = dir.join("game.exe.binviz-notes.json");
    let image = dir.join("progress.svg");
    let empty = dir.join("empty.json");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin/tiny-psx.exe"),
        &exe,
    )
    .unwrap();
    fs::write(
        &notes,
        r#"[{"address":"0x80010000","size":"0x2c","decomp":{"state":"matched"}}]"#,
    )
    .unwrap();
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_binviz"))
            .args(["progress", exe.to_str().unwrap(), "--svg", image.to_str().unwrap()])
            .args(extra)
            .output()
            .unwrap()
    };
    let result = run(&[]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let svg = fs::read_to_string(&image).unwrap();
    assert!(svg.contains("1 of 1 game functions matched; 44 of 44 game code bytes (100.0%)"));
    assert!(!run(&["--width", "0"]).status.success());
    assert_eq!(fs::read_to_string(&image).unwrap(), svg);
    fs::write(&empty, "[]").unwrap();
    assert!(run(&["--notes", empty.to_str().unwrap()]).status.success());
    assert!(
        fs::read_to_string(&image)
            .unwrap()
            .contains("0 of 2 game functions matched")
    );
    for path in [exe, notes, image, empty] {
        fs::remove_file(path).unwrap();
    }
    fs::remove_dir(dir).unwrap();
}
