//! Drives the MCP server the way an agent would: JSON-RPC over stdio.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Session {
    fn start() -> Session {
        let mut child = Command::new(env!("CARGO_BIN_EXE_binviz-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start binviz-mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut s = Session {
            child,
            stdin,
            stdout,
            next: 0,
        };
        let init = s.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "binviz");
        s.notify("notifications/initialized");
        s
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str) {
        self.send(json!({ "jsonrpc": "2.0", "method": method }));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": self.next, "method": method, "params": params }));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
        assert_eq!(reply["id"], self.next, "{reply}");
        reply
    }

    /// Calls a tool; returns (text, is_error).
    fn call(&mut self, name: &str, args: Value) -> (String, bool) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        let text = result["content"][0]["text"].as_str().unwrap_or_default().to_string();
        (text, result["isError"].as_bool().unwrap_or(false))
    }

    fn ok(&mut self, name: &str, args: Value) -> String {
        let (text, error) = self.call(name, args);
        assert!(!error, "{name} failed: {text}");
        text
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// A private copy of a fixture, so notes files land in a temp dir.
fn fixture_copy(name: &str) -> PathBuf {
    fixture_copy_for(name, "")
}

/// A copy of a fixture in a folder of its own for one test (`test`), which may write next to it.
fn fixture_copy_for(name: &str, test: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name);
    let dir = std::env::temp_dir().join(format!("binviz-mcp-test-{}-{test}{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join(name);
    std::fs::copy(src, &dst).unwrap();
    let _ = std::fs::remove_file(dst.with_file_name(format!("{name}.binviz-notes.json")));
    dst
}

#[test]
fn an_agent_session() {
    let path = fixture_copy("shapes-pe.exe");
    let mut s = Session::start();

    let tools = s.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for want in [
        "open_binary",
        "size_report",
        "search",
        "inspect",
        "disassemble",
        "annotate",
        "coverage",
    ] {
        assert!(names.contains(&want), "missing {want}: {names:?}");
    }

    // Nothing open yet: a tool error, not a protocol error.
    let (text, error) = s.call("binary_summary", json!({}));
    assert!(error && text.contains("open_binary"), "{text}");

    let opened = s.ok("open_binary", json!({ "path": path.to_str().unwrap() }));
    assert!(opened.contains("PE32+") && opened.contains("DWARF"), "{opened}");

    let size = s.ok("size_report", json!({ "top": 5 }));
    assert!(
        size.contains("Largest functions") && size.contains("shapes.cpp"),
        "{size}"
    );

    let found = s.ok("search", json!({ "query": "area", "limit": 3 }));
    assert!(found.contains("geo::Rect::area() const"), "{found}");

    let here = s.ok("inspect", json!({ "at": "main" }));
    assert!(here.contains("Symbol: main") && here.contains("shapes.cpp:"), "{here}");

    let code = s.ok("disassemble", json!({ "at": "total_area", "max_instructions": 20 }));
    assert!(code.contains("total_area") && code.contains("; shapes.cpp:"), "{code}");

    let hex = s.ok("hexdump", json!({ "at": "@0x0", "length": 16 }));
    assert!(hex.contains("4d 5a"), "{hex}");

    // Following the code.
    let info = s.ok("function_info", json!({ "at": "main" }));
    assert!(info.contains("Indexed") && info.contains("total_area"), "{info}");
    assert!(info.contains("%s: %d shapes") && info.contains("g_counter"), "{info}");
    let callers = s.ok("callers", json!({ "at": "total_area" }));
    assert!(
        callers.contains("called by 1 function") && callers.contains("main"),
        "{callers}"
    );
    let graph = s.ok("call_graph", json!({ "at": "main", "up": 1, "down": 1 }));
    assert!(graph.contains("→ total_area") && graph.contains("← "), "{graph}");
    let chain = s.ok("call_path", json!({ "from": "main", "to": "total_area" }));
    assert!(chain.starts_with("1 call:"), "{chain}");
    let refs = s.ok("xrefs", json!({ "at": "g_counter", "kind": "read" }));
    assert!(refs.contains("from main+"), "{refs}");

    // DWARF.
    let units = s.ok("dwarf_units", json!({}));
    assert!(units.contains("[0]") && units.contains("shapes.cpp"), "{units}");
    let found = s.ok("dwarf_search", json!({ "query": "Rect" }));
    assert!(found.contains("class_type geo::Rect"), "{found}");
    let die = s.ok("dwarf_die", json!({ "die": "geo::Rect" }));
    assert!(die.contains("Layout:") && die.contains("Point min"), "{die}");
    assert!(die.contains("Declared at") && die.contains("class Rect"), "{die}");
    let listed = s.ok(
        "dwarf_dies",
        json!({ "unit": 0, "tags": "variables", "name": "g_counter" }),
    );
    assert!(listed.contains("variable g_counter"), "{listed}");
    let at = s.ok("dwarf_at", json!({ "at": "total_area" }));
    assert!(at.contains("parameter shapes") && at.contains("DW_OP_reg"), "{at}");
    let check = s.ok("dwarf_check", json!({}));
    assert!(check.contains("0 error(s), 0 warning(s)"), "{check}");
    let offset = die.split("<0x").nth(1).and_then(|r| r.split('>').next()).unwrap();
    let by_offset = s.ok("dwarf_die", json!({ "die": format!("0x{offset}") }));
    assert!(by_offset.starts_with("DW_TAG_class_type Rect"), "{by_offset}");

    // Map something out; the name becomes a symbol and is saved.
    let sub = s.ok(
        "annotate",
        json!({ "at": "total_area", "name": "sum_of_areas", "comment": "adds up shape areas", "reviewed": true }),
    );
    assert!(sub.contains("saved to"), "{sub}");
    let renamed = s.ok("search", json!({ "query": "sum_of_areas", "kind": "symbol" }));
    assert!(renamed.contains("your name"), "{renamed}");
    let cov = s.ok("coverage", json!({}));
    assert!(cov.contains("notes: 1 (1 reviewed)"), "{cov}");

    let (text, error) = s.call("inspect", json!({ "at": "no_such_symbol" }));
    assert!(error && text.contains("not an address"), "{text}");
    drop(s);

    // A new session picks the notes up again.
    let mut s = Session::start();
    let opened = s.ok("open_binary", json!({ "path": path.to_str().unwrap() }));
    assert!(opened.contains("Loaded 1 notes"), "{opened}");
    let notes = s.ok("list_annotations", json!({}));
    assert!(
        notes.contains("sum_of_areas") && notes.contains("[reviewed]"),
        "{notes}"
    );
    let removed = s.ok("remove_annotation", json!({ "at": "sum_of_areas" }));
    assert!(removed.contains("0 left"), "{removed}");
}

#[test]
fn universal_binaries_pick_a_slice() {
    // A fat file built on the fly from two thin slices.
    let dir = std::env::temp_dir().join(format!("binviz-mcp-fat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin");
    let slice = std::fs::read(bin.join("tiny-macho-a64")).unwrap();
    let mut fat = Vec::new();
    let align = 0x4000u32;
    fat.extend_from_slice(&0xcafebabe_u32.to_be_bytes());
    fat.extend_from_slice(&1u32.to_be_bytes());
    fat.extend_from_slice(&0x0100000c_u32.to_be_bytes()); // CPU_TYPE_ARM64
    fat.extend_from_slice(&0u32.to_be_bytes());
    fat.extend_from_slice(&align.to_be_bytes()); // offset
    fat.extend_from_slice(&(slice.len() as u32).to_be_bytes());
    fat.extend_from_slice(&14u32.to_be_bytes());
    fat.resize(align as usize, 0);
    fat.extend_from_slice(&slice);
    let path = dir.join("fat-app");
    std::fs::write(&path, fat).unwrap();

    let mut s = Session::start();
    let opened = s.ok("open_binary", json!({ "path": path.to_str().unwrap() }));
    assert!(
        opened.contains("using [0] arm64") && opened.contains("Mach-O"),
        "{opened}"
    );
}

#[test]
fn a_folder_opens_every_binary() {
    // An .app folder with a framework and an extension, and a matching "dSYM".
    let dir = std::env::temp_dir().join(format!("binviz-mcp-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin");
    let put = |rel: &str, data: &[u8]| {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    };
    let plist = |exe: &str, id: &str| {
        format!(
            "<plist version=\"1.0\"><dict><key>CFBundleExecutable</key><string>{exe}</string>\
             <key>CFBundleIdentifier</key><string>{id}</string></dict></plist>"
        )
    };
    let main = std::fs::read(bin.join("imports-macho-a64.chained")).unwrap();
    put("Shop.app/ShopApp", &main);
    put("Shop.app/Info.plist", plist("ShopApp", "com.example.shop").as_bytes());
    put(
        "Shop.app/Frameworks/Tiny.framework/Tiny",
        &std::fs::read(bin.join("libtiny.dylib")).unwrap(),
    );
    put(
        "Shop.app/PlugIns/Widget.appex/Widget",
        &std::fs::read(bin.join("tiny-macho-a64")).unwrap(),
    );
    put("Shop.app/Assets.car", &[1; 5000]);
    let mut dsym = main.clone();
    dsym[12..16].copy_from_slice(&10u32.to_le_bytes()); // MH_DSYM
    put("Shop.app.dSYM/Contents/Resources/DWARF/ShopApp", &dsym);

    let mut s = Session::start();
    let opened = s.ok("open_binary", json!({ "path": dir.to_str().unwrap() }));
    assert!(opened.contains("Binaries (3"), "{opened}");
    assert!(
        opened.contains("`ShopApp`") && opened.contains("`Tiny`") && opened.contains("`Widget`"),
        "{opened}"
    );
    assert!(
        opened.contains("com.example.shop") && opened.contains("debug file Shop.app.dSYM"),
        "{opened}"
    );
    // The app binary is current; using it attaches its dSYM.
    let summary = s.ok("binary_summary", json!({}));
    assert!(
        summary.contains("ShopApp") && summary.contains("debug file attached"),
        "{summary}"
    );
    // Another binary by id.
    let tiny = s.ok("binary_summary", json!({ "binary": "Tiny" }));
    assert!(tiny.contains("Tiny.framework/Tiny"), "{tiny}");
    // Every binary at once.
    let found = s.ok("search", json!({ "query": "fib", "binary": "all" }));
    assert!(found.contains("## `Tiny`") && found.contains("## `Widget`"), "{found}");
    // A crash report symbolicates with the folder's binaries, found by UUID.
    let main_at = binviz::Binary::parse(main.clone())
        .unwrap()
        .symbols()
        .by_name("_main")
        .expect("_main")
        .address;
    let ips = format!(
        "{{\"bug_type\":\"309\"}}\n{}",
        json!({
            "procName": "ShopApp",
            "threads": [{ "triggered": true, "frames": [{ "imageOffset": main_at - 0x1_0000_0000 + 4, "imageIndex": 0 }] }],
            "usedImages": [{ "base": 0x1_0400_0000u64, "size": 0x10000, "uuid": "4c4c44b4-5555-3144-a10f-328b2873456c", "name": "ShopApp", "path": "/var/ShopApp.app/ShopApp" }],
        })
    );
    let crash = s.ok("symbolicate", json!({ "report": ips }));
    assert!(
        crash.contains("symbolicated with `ShopApp`") && crash.contains("_main + 4"),
        "{crash}"
    );
    // Two builds compared without opening them.
    let diff = s.ok(
        "size_diff",
        json!({
            "old": bin.join("imports-macho-a64.chained").to_str().unwrap(),
            "new": bin.join("imports-macho-a64").to_str().unwrap(),
        }),
    );
    assert!(diff.contains("Sections:") && diff.contains("__stub_helper"), "{diff}");
    let app = s.ok("folder_summary", json!({ "analyze": true }));
    assert!(
        app.contains("Asset catalogs") && app.contains("Largest owners"),
        "{app}"
    );
    let listed = s.request("tools/call", json!({ "name": "list_binaries", "arguments": {} }));
    let text = listed["result"]["content"][0]["text"].as_str().unwrap_or_default();
    assert_eq!(text.lines().count(), 3, "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn objective_c_classes_and_senders() {
    let path = fixture_copy("objc-macho-a64.chained.stripped");
    let mut s = Session::start();
    let opened = s.ok("open_binary", json!({ "path": path.to_str().unwrap() }));
    assert!(opened.contains("Objective-C: 1 class, 1 category"), "{opened}");
    let list = s.ok("objc", json!({}));
    assert!(
        list.contains("Greeter : NSObject") && list.contains("NSObject (Extras)"),
        "{list}"
    );
    let class = s.ok("objc", json!({ "name": "Greeter" }));
    assert!(class.contains("- (void)greetWith:(id)arg1 times:(int)arg2;"), "{class}");
    let selector = s.ok("objc", json!({ "name": "wave" }));
    assert!(
        selector.contains("-[NSObject(Extras) wave]") && selector.contains("sent by 1 function"),
        "{selector}"
    );
    // The recovered names work like any other.
    let code = s.ok("disassemble", json!({ "at": "-[Greeter hello]" }));
    assert!(code.contains("-[Greeter hello]"), "{code}");
    let (text, error) = s.call("objc", json!({ "name": "Greet" }));
    assert!(error && text.contains("similar: Greeter"), "{text}");
}

#[test]
fn agents_work_through_a_decompilation() {
    let path = fixture_copy_for("x86demo.exe", "decomp-");
    let dir = path.parent().unwrap().to_path_buf();
    let pdb = dir.join("x86demo.pdb");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin/x86demo.pdb"),
        &pdb,
    )
    .unwrap();
    let open = json!({ "path": path.to_str().unwrap(), "debug_file": pdb.to_str().unwrap() });
    let mut s = Session::start();
    s.ok("open_binary", open.clone());

    // Two agents claim, and don't get the same function.
    let a = s.ok("next_functions", json!({ "claim": true, "agent": "a" }));
    assert!(
        a.contains("Claimed fatal") && a.contains("0 of 42 functions matched"),
        "{a}"
    );
    let b = s.ok("next_functions", json!({ "claim": true, "agent": "b", "count": 3 }));
    assert!(
        b.contains("1 claimed") && b.contains("Claimed ") && !b.contains("Claimed fatal"),
        "{b}"
    );

    // A match says what it made ready; a try that didn't match keeps its best.
    let done = s.ok(
        "mark",
        json!({ "at": "fatal", "state": "matched", "source": "src/main.c" }),
    );
    assert!(
        done.contains("matched in src/main.c") && done.contains("Now ready") && done.contains("checked_div"),
        "{done}"
    );
    let tried = s.ok(
        "mark",
        json!({ "at": "clamp_ammo", "state": "attempted", "percent": 62.5 }),
    );
    assert!(tried.contains("tried 1×, best 62.5%"), "{tried}");

    // Matching a function points at its copy, which then comes first.
    let health = s.ok(
        "mark",
        json!({ "at": "clamp_health", "state": "matched", "source": "src/stats.c" }),
    );
    assert!(health.contains("likely the same C: clamp_ammo"), "{health}");
    let next = s.ok("next_functions", json!({ "count": 1 }));
    assert!(
        next.contains("clamp_ammo") && next.contains("like a done one"),
        "{next}"
    );
    let like = s.ok("similar_functions", json!({ "at": "clamp_ammo", "done_only": true }));
    assert!(
        like.contains("clamp_health") && like.contains("matched in src/stats.c"),
        "{like}"
    );
    let info = s.ok("function_info", json!({ "at": "fatal" }));
    assert!(info.contains("Decompilation: matched in src/main.c"), "{info}");
    let context = s.ok("decomp_context", json!({ "at": "clamp_ammo" }));
    assert!(
        context.contains("Worked examples") && context.contains("clamp_health  matched in src/stats.c"),
        "{context}"
    );

    // objdiff's report, recorded: a new match, a match lost, a percent.
    let report = dir.join("report.json");
    let unit = |functions: &str| format!(r#"{{"units": [{{"name": "main/demo", "functions": [{functions}]}}]}}"#);
    std::fs::write(
        &report,
        unit(
            r#"{"name": "sum3", "size": 16, "fuzzy_match_percent": 100.0},
               {"name": "mix", "size": 14, "fuzzy_match_percent": 80.5},
               {"name": "fatal", "size": 10, "fuzzy_match_percent": 90.0}"#,
        ),
    )
    .unwrap();
    let placed = s.ok("place_report", json!({ "report": report.to_str().unwrap() }));
    assert!(
        placed.contains("1 newly matched") && placed.contains("1 no longer match: fatal"),
        "{placed}"
    );
    let mix = s.ok("function_info", json!({ "at": "mix" }));
    assert!(mix.contains("Decompilation: todo in main/demo, best 80.5%"), "{mix}");
    let (text, error) = s.call("mark", json!({ "at": "fatal", "state": "done" }));
    assert!(error && text.contains("unknown state"), "{text}");
    drop(s);

    // The notes file keeps it all, for the next session and the web UI.
    let notes = std::fs::read_to_string(dir.join("x86demo.exe.binviz-notes.json")).unwrap();
    assert!(
        notes.contains("\"state\": \"matched\"") && notes.contains("\"source\": \"src/main.c\""),
        "{notes}"
    );
    let mut s = Session::start();
    s.ok("open_binary", open);
    let cov = s.ok("coverage", json!({ "gaps": 0 }));
    assert!(cov.contains("Decompilation: 2 of 42 functions matched"), "{cov}");
    let listed = s.ok("list_annotations", json!({}));
    assert!(listed.contains("[matched in src/stats.c]"), "{listed}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn x86_objects_are_matched_project_by_project() {
    let path = fixture_copy_for("x86match.exe", "match-");
    let dir = path.parent().unwrap().to_path_buf();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin");
    let pdb = dir.join("x86match.pdb");
    std::fs::copy(bin.join("x86match.pdb"), &pdb).unwrap();
    // A build folder: the game's object (edited, not matching yet) and the library's.
    let build = dir.join("build");
    for (from, to) in [
        ("x86match-edited.obj", "src/game.obj"),
        ("x86lib-a.obj", "lib/x86lib-a.obj"),
        ("x86lib-b.obj", "lib/x86lib-b.obj"),
    ] {
        let to = build.join(to);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(bin.join(from), to).unwrap();
    }
    let edited = bin.join("x86match-edited.obj");
    let mut s = Session::start();
    s.ok(
        "open_binary",
        json!({ "path": path.to_str().unwrap(), "debug_file": pdb.to_str().unwrap() }),
    );

    // One function, by its C name or as the object decorates it.
    let diff = s.ok(
        "match_function",
        json!({ "object": edited.to_str().unwrap(), "symbol": "diff" }),
    );
    assert!(
        diff.contains("diff: 33.3%") && diff.contains("stack slot offset differs"),
        "{diff}"
    );
    let damage = s.ok(
        "match_function",
        json!({ "object": edited.to_str().unwrap(), "symbol": "_damage@8" }),
    );
    assert!(
        damage.contains("immediate differs: -0x5 in the original, -0x7 in the rebuild"),
        "{damage}"
    );
    let all = s.ok(
        "match_object",
        json!({ "object": bin.join("x86match.obj").to_str().unwrap() }),
    );
    assert!(all.contains("15 functions compared, 15 match exactly"), "{all}");

    // The whole project, recorded in the notes.
    let project = s.ok(
        "match_project",
        json!({ "paths": [build.to_str().unwrap()], "record": true }),
    );
    assert!(
        project.contains("3 objects, 20 functions compared")
            && project.contains("src/game.obj")
            && project.contains("Recorded in the notes: 12 newly matched"),
        "{project}"
    );
    let info = s.ok("function_info", json!({ "at": "_lib_checksum" }));
    assert!(info.contains("Decompilation: matched in lib/x86lib-a.obj"), "{info}");
    let info = s.ok("function_info", json!({ "at": "diff" }));
    assert!(
        info.contains("Decompilation: todo in src/game.obj, best 33.3%"),
        "{info}"
    );

    // The library's functions named in a copy with no PDB, by their signatures.
    let stripped = fixture_copy_for("x86match.exe", "match-sdk-");
    s.ok("open_binary", json!({ "path": stripped.to_str().unwrap() }));
    let sdk = s.ok(
        "identify_sdk",
        json!({ "paths": [bin.join("x86lib.lib").to_str().unwrap()], "apply": true }),
    );
    assert!(
        sdk.contains("skipped 6 import library members")
            && sdk.contains("4 of")
            && sdk.contains("lib_checksum  (x86lib.lib/x86lib-a.obj"),
        "{sdk}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(stripped.parent().unwrap());
}

#[test]
fn a_rom_with_an_emulators_log_and_labels() {
    let path = fixture_copy_for("tiny.nes", "emulators-");
    let dir = path.parent().unwrap().to_path_buf();
    // FCEUX's log: bank 1's code ran at $8000; Mesen's labels name it and a RAM byte.
    let mut log = vec![0u8; 0x12000];
    log[0x4000..0x4007].fill(1);
    std::fs::write(dir.join("tiny.cdl"), &log).unwrap();
    std::fs::write(
        dir.join("tiny.mlb"),
        "P:4000:BankOne:switched in by reset\nR:0010:FrameCount\n",
    )
    .unwrap();
    let mut s = Session::start();
    s.ok("open_binary", json!({ "path": path.to_str().unwrap() }));
    let read = s.ok("code_log", json!({ "path": dir.join("tiny.cdl").to_str().unwrap() }));
    assert!(read.contains("FCEUX code/data log: 7 bytes of code"), "{read}");
    let code = s.ok("disassemble", json!({ "at": "0x18000" }));
    assert!(code.contains("jsr") && code.contains("0x18006"), "{code}");
    let imported = s.ok("labels", json!({ "import": dir.join("tiny.mlb").to_str().unwrap() }));
    assert!(imported.contains("2 notes added"), "{imported}");
    let named = s.ok("disassemble", json!({ "at": "BankOne" }));
    assert!(named.contains("BankOne"), "{named}");
    let written = s.ok("labels", json!({ "export": "nl" }));
    assert!(
        written.contains("tiny.nes.1.nl") && written.contains("tiny.nes.ram.nl"),
        "{written}"
    );
    let bank = std::fs::read_to_string(dir.join("tiny.nes.1.nl")).unwrap();
    assert_eq!(bank, "$8000#BankOne#switched in by reset\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_a_patch_changes() {
    let path = fixture_copy_for("tiny.nes", "patch-");
    let dir = path.parent().unwrap().to_path_buf();
    let mut hacked = std::fs::read(&path).unwrap();
    hacked[0xC016] = 1;
    std::fs::write(dir.join("hacked.nes"), &hacked).unwrap();
    let mut s = Session::start();
    s.ok("open_binary", json!({ "path": path.to_str().unwrap() }));
    let ips = dir.join("hack.ips");
    let made = s.ok(
        "patch",
        json!({ "target": dir.join("hacked.nes").to_str().unwrap(), "out": ips.to_str().unwrap() }),
    );
    assert!(made.contains("IPS patch"), "{made}");
    let applied = s.ok("patch", json!({ "apply": ips.to_str().unwrap() }));
    assert!(
        applied.contains("1 run of changes, 1 byte differ")
            && applied.contains("PRG bank 3 (fixed) 0x3c006 in reset+0x6"),
        "{applied}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_builds_function_by_function() {
    let old = fixture_copy_for("shapes-pe.exe", "functions-");
    let new = fixture_copy_for("shapes-pe.stripped.exe", "functions-");
    let mut s = Session::start();
    let text = s.ok(
        "diff_functions",
        json!({ "old": old.to_str().unwrap(), "new": new.to_str().unwrap() }),
    );
    assert!(text.contains("identical") && text.contains("0 changed"), "{text}");
    let code = s.ok(
        "diff_functions",
        json!({ "old": old.to_str().unwrap(), "new": new.to_str().unwrap(), "function": "total_area" }),
    );
    assert!(code.contains("Identical") && code.contains("= 0x"), "{code}");
}

#[test]
fn an_original_xbox_executable() {
    let path = fixture_copy_for("tiny.xbe", "xbe-");
    let dir = path.parent().unwrap().to_path_buf();
    let mut s = Session::start();
    let opened = s.ok("open_binary", json!({ "path": path.to_str().unwrap() }));
    assert!(opened.contains("XBE"), "{opened}");
    let summary = s.ok("binary_summary", json!({}));
    assert!(
        summary.contains("XBE executable (retail)") && summary.contains("Built with: XDK 5849"),
        "{summary}"
    );
    // The kernel's calls read as such, named after the ordinals in the thunk table.
    let code = s.ok("disassemble", json!({ "at": "0x11000" }));
    assert!(
        code.contains("<__imp_KeTickCount>") && code.contains("<__imp_HalReturnToFirmware>"),
        "{code}"
    );
    let info = s.ok("function_info", json!({ "at": "0x110a0" }));
    assert!(info.contains("__imp_NtClose"), "{info}");
    let refs = s.ok("xrefs", json!({ "at": "__imp_KeBugCheck" }));
    assert!(refs.contains("0x11094"), "{refs}");
    let context = s.ok("decomp_context", json!({ "at": "0x11050" }));
    assert!(context.contains("dd 0x11060"), "{context}");
    let cov = s.ok("coverage", json!({ "gaps": 0 }));
    assert!(cov.contains(".text") && cov.contains("D3D"), "{cov}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn types_as_c_and_members_at_offsets() {
    let path = fixture_copy_for("layouts-pe-x86.exe", "types-");
    let pdb = path.with_file_name("layouts-pe-x86.pdb");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin/layouts-pe-x86.pdb"),
        &pdb,
    )
    .unwrap();
    let mut s = Session::start();
    s.ok(
        "open_binary",
        json!({ "path": path.to_str().unwrap(), "debug_file": pdb.to_str().unwrap() }),
    );
    // A structure with what it holds, from the PDB's types.
    let header = s.ok("c_header", json!({ "names": ["entity"] }));
    assert!(header.starts_with("4 structures and unions, 3 enums"), "{header}");
    assert!(
        header.contains("struct entity { /* 0x108 bytes */") && header.contains("union odd {"),
        "{header}"
    );
    assert!(
        header.contains("_Static_assert(BINVIZ_OFFSETOF(struct entity, enemy) == 0x54"),
        "{header}"
    );
    // Everything, prototypes included.
    let all = s.ok("c_header", json!({}));
    assert!(all.contains("int BINVIZ_STDCALL list_length(struct list *l);"), "{all}");
    let (text, error) = s.call("c_header", json!({ "names": ["no_such_type"] }));
    assert!(
        error && text.contains("no type or function named no_such_type"),
        "{text}"
    );
    // Which member an offset is: through a typedef, into arrays and unions.
    let fields = s.ok("struct_field", json!({ "type": "list_t", "offsets": ["0x0", "4"] }));
    assert!(fields.contains("0x0  list.head: struct node *"), "{fields}");
    assert!(fields.contains("0x4  list.length: int"), "{fields}");
    let fields = s.ok(
        "struct_field",
        json!({ "type": "entity", "offsets": ["0x3c", "0x80", "0x85", "0xa4", "0x200"] }),
    );
    for line in [
        "0x3c  entity.matrix[1][2]: float",
        "0x80  entity.count: int",
        "0x85  entity+0x85 (padding)",
        "0xa4  entity.fl.d: unsigned short, bits 1312..1316",
        "0x200  past its end (0x108 bytes)",
    ] {
        assert!(fields.contains(line), "{line:?} missing in:\n{fields}");
    }
}

#[test]
fn a_webassembly_build_and_its_stack_trace() {
    // The stripped build names its source map, which is beside it.
    let module = fixture_copy_for("wasmdemo.stripped.wasm", "wasm-");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bin");
    std::fs::copy(
        src.join("wasmdemo.wasm.map"),
        module.with_file_name("wasmdemo.wasm.map"),
    )
    .unwrap();
    let mut s = Session::start();
    let opened = s.ok("open_binary", json!({ "path": module.to_str().unwrap() }));
    assert!(opened.contains("wasmdemo.wasm.map, which the module names"), "{opened}");
    let code = s.ok("disassemble", json!({ "at": "middle" }));
    assert!(code.contains("<check>"), "{code}");
    let trace = src.join("wasmdemo.trace");
    let crash = s.ok("symbolicate", json!({ "report_file": trace.to_str().unwrap() }));
    assert!(crash.contains("check  wasmdemo.c:84:9"), "{crash}");
    let _ = std::fs::remove_dir_all(module.parent().unwrap());
}
