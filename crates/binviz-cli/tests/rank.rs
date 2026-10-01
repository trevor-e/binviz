//! `binviz match --json`, `--record`, `rank`, `resolve` and `flags --jobs`: batches of
//! objects scored in one process, with a distance to climb.

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

fn stdout(out: &Output) -> String {
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_folder_of_objects_scores_as_json_and_records_into_the_notes() {
    let dir = std::env::temp_dir().join(format!("binviz-cli-rank-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("build")).unwrap();
    std::fs::copy(fixture("x86match.exe"), dir.join("x86match.exe")).unwrap();
    std::fs::copy(fixture("x86match.pdb"), dir.join("x86match.pdb")).unwrap();
    std::fs::copy(fixture("x86match-edited.obj"), dir.join("build/game.obj")).unwrap();
    let exe = dir.join("x86match.exe").to_string_lossy().into_owned();
    let build = dir.join("build").to_string_lossy().into_owned();
    let pdb = dir.join("x86match.pdb").to_string_lossy().into_owned();
    let cache = dir.join("cache");
    let run = |args: &[&str]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_binviz"));
        cmd.args(args).env("BINVIZ_CACHE", &cache);
        cmd.output().unwrap()
    };

    // JSON: one entry per function, with distance and the kinds of difference.
    let out = run(&["match", &exe, &build, "--json", "--debug", &pdb]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let scores = v["scores"].as_array().unwrap();
    assert_eq!(scores.len(), 15, "{v}");
    let diff = scores.iter().find(|f| f["name"] == "diff").unwrap();
    assert!(
        diff["distance"].as_u64().unwrap() > 0 && diff["unit"] == "game.obj",
        "{diff}"
    );
    assert!(std::fs::read_dir(&cache).unwrap().count() == 15, "scores cached");
    // Scored again from the cache: the same JSON.
    let again: serde_json::Value =
        serde_json::from_str(&stdout(&run(&["match", &exe, &build, "--json", "--debug", &pdb]))).unwrap();
    assert_eq!(again["scores"], v["scores"]);

    // Recorded into the notes beside the executable, through the journal.
    let out = run(&[
        "match",
        &exe,
        &build,
        "--record",
        "--meta",
        "compiler=msvc 6,flags=/O2",
        "--debug",
        &pdb,
    ]);
    stdout(&out);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Recorded in"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let notes = std::fs::read_to_string(dir.join("x86match.exe.binviz-notes.json")).unwrap();
    assert!(
        notes.contains("\"matched\"") && notes.contains("msvc 6") && notes.contains("game.obj"),
        "{notes}"
    );
    assert!(dir.join("x86match.exe.binviz-notes.json.journal").exists());

    // Variants ranked: the clean object is at distance 0, the edited one after it.
    let text = stdout(&run(&[
        "rank",
        &exe,
        &fixture("x86match.obj"),
        &fixture("x86match-edited.obj"),
        "--function",
        "diff",
        "--debug",
        &pdb,
    ]));
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("d=0 ") && lines[0].ends_with("x86match.obj"),
        "{text}"
    );
    assert!(lines[1].ends_with("x86match-edited.obj") && lines.len() == 2, "{text}");
    let text = stdout(&run(&["rank", &exe, &build, "--top", "3", "--json", "--debug", &pdb]));
    let r: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(r["scores"].as_array().unwrap().len(), 3);

    // Where a name is.
    let text = stdout(&run(&["resolve", &exe, "diff", "--debug", &pdb]));
    assert!(
        text.starts_with("diff is 0x") && text.contains("the start of diff"),
        "{text}"
    );

    // Two runs set against each other: the clean object after the edited one.
    let before = dir.join("before.json");
    let after = dir.join("after.json");
    std::fs::write(
        &before,
        stdout(&run(&["match", &exe, &build, "--json", "--debug", &pdb])),
    )
    .unwrap();
    std::fs::copy(fixture("x86match.obj"), dir.join("build/game.obj")).unwrap();
    std::fs::write(
        &after,
        stdout(&run(&["match", &exe, &build, "--json", "--debug", &pdb])),
    )
    .unwrap();
    let (before, after) = (before.to_str().unwrap(), after.to_str().unwrap());
    let text = stdout(&run(&["scores", before, after]));
    assert!(
        text.contains("8 closer, 0 further") && text.contains("exact 7 -> 15") && text.contains("UP   diff"),
        "{text}"
    );
    let text = stdout(&run(&["scores", before, after, "--json"]));
    let d: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(d["exactAfter"], 15, "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn many_functions_export_as_assembler_in_one_process() {
    let dir = std::env::temp_dir().join(format!("binviz-cli-asm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // A list the way a worker keeps one: the first word of each line names the function.
    std::fs::write(dir.join("lanes.txt"), "# wave\nentry 0x40 entry boot\n0x12345678 bad\n").unwrap();
    let out = dir.join("asm");
    let run = binviz(&[
        "asm",
        &fixture("tiny.z64"),
        "--list",
        dir.join("lanes.txt").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--bare",
    ]);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("1 functions written to") && err.contains("1 not written"),
        "{err}"
    );
    let text = std::fs::read_to_string(out.join("entry.s")).unwrap();
    assert!(
        text.starts_with(".set noat") && text.contains("glabel entry\n") && !text.contains("/* "),
        "{text}"
    );
    // Several to stdout, one after another; the comments kept.
    let text = stdout(&binviz(&["asm", &fixture("tiny.z64"), "entry", "entry"]));
    assert_eq!(
        text.matches("glabel entry").count(),
        1,
        "the same function once: {text}"
    );
    assert!(text.contains("/* "));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn variants_compile_in_parallel_and_in_one_batch() {
    let dir = std::env::temp_dir().join(format!("binviz-cli-flags-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // "Sources" that the fake compiler copies: two variants of the unit.
    std::fs::copy(fixture("x86match.obj"), dir.join("src/clean.c")).unwrap();
    std::fs::copy(fixture("x86match-edited.obj"), dir.join("src/edited.c")).unwrap();
    let src = dir.join("src").to_string_lossy().into_owned();
    // Each file its own command, two at a time, ranked by distance.
    let out = binviz(&[
        "flags",
        &fixture("x86match.exe"),
        &src,
        "cp {src} {out}",
        "--jobs",
        "2",
        "--function",
        "diff",
        "--debug",
        &fixture("x86match.pdb"),
    ]);
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("d=0 ") && lines[0].ends_with("clean") && lines[1].ends_with("edited"),
        "{text}"
    );
    // One command for the whole folder: the caller fans out inside it.
    let out = binviz(&[
        "flags",
        &fixture("x86match.exe"),
        &src,
        "for f in {srcdir}/*.c; do cp \"$f\" {outdir}/$(basename \"$f\" .c).o; done",
        "--function",
        "diff",
        "--json",
        "--debug",
        &fixture("x86match.pdb"),
    ]);
    let r: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let units: Vec<&str> = r["scores"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["unit"].as_str().unwrap())
        .collect();
    assert_eq!(units, ["clean", "edited"], "{r}");
    // Recorded: the best of the variants (the clean one, a match) into the notes given.
    let notes = dir.join("notes.json");
    let out = binviz(&[
        "flags",
        &fixture("x86match.exe"),
        &src,
        "cp {src} {out}",
        "--function",
        "diff",
        "--record",
        "--meta",
        "compiler=msvc 6",
        "--notes",
        notes.to_str().unwrap(),
        "--debug",
        &fixture("x86match.pdb"),
    ]);
    stdout(&out);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("1 newly matched"), "{err}");
    let text = std::fs::read_to_string(&notes).unwrap();
    assert!(
        text.contains("\"matched\"") && text.contains("msvc 6") && text.contains("\"source\": \"clean\""),
        "{text}"
    );
    // The old way still works: one source, several flag sets.
    let out = binviz(&[
        "flags",
        &fixture("x86match.exe"),
        "unused.c",
        "cp {flags} {out}",
        &fixture("x86match-edited.obj"),
        &fixture("x86match.obj"),
        "--debug",
        &fixture("x86match.pdb"),
    ]);
    assert!(stdout(&out).lines().next().unwrap().ends_with("x86match.obj"));
    let _ = std::fs::remove_dir_all(&dir);
}
