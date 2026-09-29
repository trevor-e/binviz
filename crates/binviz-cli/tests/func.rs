//! `binviz func`: each list says how long it is.

use std::path::Path;
use std::process::Command;

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn binviz(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_binviz")).args(args).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn an_empty_list_says_so() {
    // gamedemo.dll's player_pain: a bare `ret`, only its address taken.
    let text = binviz(&["func", &fixture("gamedemo.dll"), "0x20001030"]);
    assert_eq!(
        text,
        "sub_20001030 at 0x20001030, 1 bytes\nreferenced by: 1 address taken\n\
         no callers\nno callees\nno strings\nno globals\n"
    );
}
