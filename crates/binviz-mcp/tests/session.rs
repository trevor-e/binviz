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
