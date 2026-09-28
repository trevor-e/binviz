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
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bin")
        .join(name);
    let dir = std::env::temp_dir().join(format!("binviz-mcp-test-{}-{name}", std::process::id()));
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
