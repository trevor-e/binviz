//! Folders and zips work wherever a file does: no command of their own.

use std::path::Path;
use std::process::Command;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Runs the command line: (stdout, stderr).
fn binviz(args: &[&str]) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_binviz")).args(args).output().unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
    (text(&out.stdout), text(&out.stderr))
}

#[test]
fn a_folder_works_like_a_file() {
    let dir = std::env::temp_dir().join(format!("binviz-cli-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let put = |rel: &str, data: &[u8]| {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    };
    // A stripped app with a framework and its "dSYM" (the unstripped copy,
    // marked MH_DSYM), and a stripped ELF executable next to its debug file.
    put("Shop.app/ShopApp", &fixture("imports-macho-a64.chained.stripped"));
    put(
        "Shop.app/Info.plist",
        b"<plist version=\"1.0\"><dict><key>CFBundleExecutable</key><string>ShopApp</string></dict></plist>",
    );
    put("Shop.app/Frameworks/Tiny.framework/Tiny", &fixture("libtiny.dylib"));
    let mut dsym = fixture("imports-macho-a64.chained");
    dsym[12..16].copy_from_slice(&10u32.to_le_bytes());
    put("Shop.app.dSYM/Contents/Resources/DWARF/ShopApp", &dsym);
    put("linux/tiny", &fixture("tiny-elf-x64.stripped"));
    put("linux/tiny-elf-x64.debug", &fixture("tiny-elf-x64.debug"));
    let d = dir.to_str().unwrap();

    // info: every binary with its debug file (the ELF's through its debug
    // link), and their code summed by owner.
    let (info, _) = binviz(&["info", d]);
    assert!(info.contains("3 binaries"), "{info}");
    assert!(info.contains("debug file Shop.app.dSYM/"), "{info}");
    assert!(info.contains("debug file linux/tiny-elf-x64.debug"), "{info}");
    assert!(info.contains("owners across 3 binaries"), "{info}");
    // Other commands work on the app's own binary, named by its dSYM.
    let (symbols, which) = binviz(&["symbols", d, "MESSAGES"]);
    assert!(which.contains("Shop.app/ShopApp (debug file"), "{which}");
    assert!(symbols.contains("imports::MESSAGES"), "{symbols}");
    // --member picks another, by name; its debug file comes along.
    let (dwarf, _) = binviz(&["dwarf", d, "--member", "tiny"]);
    assert!(dwarf.contains("source: linux/tiny-elf-x64.debug"), "{dwarf}");
    // search looks through them all.
    let (found, _) = binviz(&["search", d, "fib"]);
    assert!(found.contains("## Tiny (") && found.contains("## tiny ("), "{found}");
    assert!(found.contains("No matches in: ShopApp"), "{found}");
    let _ = std::fs::remove_dir_all(&dir);
}
