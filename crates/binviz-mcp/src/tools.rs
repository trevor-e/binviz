//! The tools an agent can call, and how their results read.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use binviz::{Annotation, Binary, Container, HitKind, SymbolQuery, Target};
use serde_json::{Value, json};

use crate::notes;

pub const INSTRUCTIONS: &str = "binviz explains ELF, Mach-O and PE binaries down to every byte, maps code back to source through DWARF, and keeps binaries loaded between calls, so exploring a large file stays fast. \
Start with open_binary (a path; universal binaries pick arm64 unless you pass member). Then: binary_summary for the overview, size_report to see where the bytes go (sections, largest functions, and owners: Swift modules, Objective-C classes, C++ namespaces, C prefixes), search for anything (names, strings, addresses, byte patterns like `48 8b ?? 05`, \"exact text\", file.c:42), inspect to learn what is at an address or file offset, disassemble a function, list_symbols / list_strings to page through tables, hexdump for raw bytes. \
To follow code: function_info gives a function's callers, callees, strings and data at a glance; callers / callees list call sites; call_graph draws the neighbourhood; call_path finds a chain of calls from one function to another; xrefs lists every reference to an address (calls, reads, writes, address-taken, pointers stored in data — e.g. who uses a string or a global). The reference index is built on first use (about a second per 100 MB of code). Calls through import stubs, PLT entries and GOT/IAT slots show the imported function's name. \
To map a binary out: annotate names functions, comments addresses and marks code reviewed (names show up in disassembly and search); coverage shows how much is named, recovered, reviewed or still unexplored, with the largest unexplored gaps. Notes persist in <binary>.binviz-notes.json, which the binviz web UI can import. \
Addresses can be written 0x401000 (hex, also without 0x), a symbol name, name+0x10, or @0x200 for a file offset.";

const MAX_OUTPUT: usize = 60_000;

struct Open {
    id: String,
    path: PathBuf,
    label: String,
    bin: Binary,
    notes: Option<PathBuf>,
}

#[derive(Default)]
pub struct Server {
    open: Vec<Open>,
    current: Option<usize>,
}

// --- Tool definitions --------------------------------------------------------

fn binary_param() -> Value {
    json!({ "type": "string", "description": "Which open binary (the id open_binary returned). Defaults to the most recently opened." })
}

fn tool(name: &str, title: &str, description: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    let mut props = props;
    if name != "open_binary" && name != "list_binaries" {
        props["binary"] = binary_param();
    }
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": { "type": "object", "properties": props, "required": required },
        "annotations": { "readOnlyHint": read_only, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
    })
}

pub fn definitions() -> Vec<Value> {
    let address = |what: &str| json!({ "type": "string", "description": format!("{what}: 0x401000 (or 401000), a symbol name, name+0x10, or @0x200 for a file offset.") });
    vec![
        tool(
            "open_binary",
            "Open a binary",
            "Loads an ELF, Mach-O or PE file (also universal/fat binaries and .a archives) and keeps it in memory for the other tools. Loads notes from <path>.binviz-notes.json if present. Returns an id and a summary.",
            json!({
                "path": { "type": "string", "description": "Path to the binary." },
                "member": { "type": "string", "description": "For universal binaries or archives: the slice/member index or architecture (e.g. arm64, x86_64). Universal binaries default to arm64." },
                "debug_file": { "type": "string", "description": "Separate DWARF to attach: a .dSYM's DWARF file (…/Contents/Resources/DWARF/<name>), an ELF .debug file, or an unstripped copy." },
                "notes_file": { "type": "string", "description": "Where to keep notes; defaults to <path>.binviz-notes.json." },
            }),
            &["path"],
            false,
        ),
        tool(
            "list_binaries",
            "List open binaries",
            "Binaries currently loaded, with their ids.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "close_binary",
            "Close a binary",
            "Unloads a binary to free memory.",
            json!({}),
            &[],
            false,
        ),
        tool(
            "binary_summary",
            "Summarize a binary",
            "Format, architecture, entry point, build ID/UUID, platform facts, segments and sections, symbol counts, and DWARF (units, source files, producers) if present.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "size_report",
            "Where the bytes go",
            "Explains a binary's size: bytes by region kind (code, data, symbols, debug info…), largest sections, the owners of the code and data (Swift modules, Objective-C classes, C++ namespaces / Rust crates, C prefixes, unnamed functions), the largest functions and data, source files (with DWARF), and strings.",
            json!({ "top": { "type": "integer", "description": "How many entries per list (default 15, max 200)." } }),
            &[],
            true,
        ),
        tool(
            "search",
            "Search",
            "One search over addresses, file offsets (@0x…), symbols (raw and demangled), imports/exports, sections, source files and file:line, DWARF names, notes, strings, byte patterns with ?? wildcards (48 8b ?? 05) and \"exact text\". Best matches first, grouped by kind.",
            json!({
                "query": { "type": "string" },
                "kind": { "type": "string", "enum": ["address", "offset", "symbol", "import", "export", "section", "source", "dwarf", "note", "string", "bytes"], "description": "Only this kind of result." },
                "limit": { "type": "integer", "description": "Results per kind (default 10, max 500)." },
            }),
            &["query"],
            true,
        ),
        tool(
            "inspect",
            "What is here?",
            "Everything known about one location: the structures containing it (down to header fields and table entries), segment and section, symbol + offset, source file:line:column, the inlined call stack, the instruction, and any note.",
            json!({ "at": address("Location") }),
            &["at"],
            true,
        ),
        tool(
            "disassemble",
            "Disassemble a function",
            "Disassembles the function containing an address (x86, x86-64, AArch64, ARM), with branch targets named, source lines interleaved (DWARF) and notes shown as comments.",
            json!({
                "at": address("A function or address inside it"),
                "max_instructions": { "type": "integer", "description": "Default 400, max 5000." },
            }),
            &["at"],
            true,
        ),
        tool(
            "function_info",
            "Understand a function",
            "One-stop summary of the function containing an address: size, source file (DWARF), who calls it, what it calls (functions and imports), the strings it uses, the globals it reads and writes, and how many pointers to it are stored in data (vtables, callbacks).",
            json!({
                "at": address("A function or address inside it"),
                "limit": { "type": "integer", "description": "Entries per list (default 25, max 500)." },
            }),
            &["at"],
            true,
        ),
        tool(
            "xrefs",
            "References to an address",
            "Every reference to an address, a symbol (its whole extent) or a string: calls and tail calls, code reading, writing or taking its address, and pointers to it stored in data. Each with the function (or data) it comes from.",
            json!({
                "at": address("What is referenced"),
                "kind": { "type": "string", "enum": ["call", "jump", "read", "write", "address", "pointer"], "description": "Only this kind of reference." },
                "offset": { "type": "integer" },
                "limit": { "type": "integer", "description": "Default 100, max 2000." },
            }),
            &["at"],
            true,
        ),
        tool(
            "callers",
            "Who calls this function",
            "Functions that call (or tail-call) the function containing an address, most call sites first, with the first call site of each.",
            json!({
                "at": address("A function or address inside it"),
                "limit": { "type": "integer", "description": "Default 100, max 2000." },
            }),
            &["at"],
            true,
        ),
        tool(
            "callees",
            "What this function calls",
            "Functions and imports that the function containing an address calls (directly, through stubs/PLT/GOT/IAT, or as tail calls), most call sites first.",
            json!({
                "at": address("A function or address inside it"),
                "limit": { "type": "integer", "description": "Default 100, max 2000." },
            }),
            &["at"],
            true,
        ),
        tool(
            "call_graph",
            "Call graph around a function",
            "The call tree around a function: callers up to `up` levels and callees down to `down` levels, keeping the `fanout` neighbours with the most call sites per function.",
            json!({
                "at": address("The function at the centre"),
                "up": { "type": "integer", "description": "Levels of callers (default 1, max 4)." },
                "down": { "type": "integer", "description": "Levels of callees (default 2, max 4)." },
                "fanout": { "type": "integer", "description": "Neighbours per function (default 8, max 30)." },
            }),
            &["at"],
            true,
        ),
        tool(
            "call_path",
            "How does A reach B?",
            "A shortest chain of calls from one function to another (e.g. from main or an entry point to an interesting function), with each call site.",
            json!({
                "from": address("Where the path starts"),
                "to": address("Where it should end"),
                "max_depth": { "type": "integer", "description": "Longest chain to consider (default 10, max 20)." },
            }),
            &["from", "to"],
            true,
        ),
        tool(
            "list_symbols",
            "List symbols",
            "Pages through the symbol table: filter by name, kind (function, data, …), sort by address, name or size.",
            json!({
                "filter": { "type": "string", "description": "Case-insensitive substring of the raw or demangled name." },
                "kind": { "type": "string", "enum": ["function", "data", "label", "section", "file", "tls", "unknown", "undefined"] },
                "sort": { "type": "string", "enum": ["address", "name", "size"] },
                "descending": { "type": "boolean" },
                "offset": { "type": "integer" },
                "limit": { "type": "integer", "description": "Default 50, max 1000." },
            }),
            &[],
            true,
        ),
        tool(
            "list_strings",
            "List strings",
            "Printable ASCII and UTF-16 strings in the data sections, in file order, optionally filtered.",
            json!({
                "filter": { "type": "string" },
                "offset": { "type": "integer" },
                "limit": { "type": "integer", "description": "Default 50, max 1000." },
            }),
            &[],
            true,
        ),
        tool(
            "hexdump",
            "Hex dump",
            "Raw bytes at an address or file offset, with the structure they belong to.",
            json!({
                "at": address("Start"),
                "length": { "type": "integer", "description": "Bytes (default 256, max 4096)." },
            }),
            &["at"],
            true,
        ),
        tool(
            "coverage",
            "Reverse-engineering coverage",
            "How much of the code and data is mapped out: bytes that are reviewed, annotated, named (symbols/DWARF), format structure, recovered (functions from unwind info / function starts, strings), padding, or unexplored — per section — plus the largest unexplored gaps with a guess at what they hold.",
            json!({ "gaps": { "type": "integer", "description": "How many gaps to list (default 20, max 500)." } }),
            &[],
            true,
        ),
        tool(
            "annotate",
            "Add or update a note",
            "Names a function or range, comments an address, and/or marks it reviewed. Updates the note already at that address if there is one (only the fields you pass change). Names become symbols everywhere. Saved to the notes file.",
            json!({
                "at": address("Where the note starts"),
                "size": { "type": "integer", "description": "Bytes covered; 0 or omitted means the symbol or instruction there." },
                "name": { "type": "string" },
                "comment": { "type": "string" },
                "reviewed": { "type": "boolean", "description": "Mark as understood." },
            }),
            &["at"],
            false,
        ),
        tool(
            "remove_annotation",
            "Remove a note",
            "Deletes the note(s) starting at an address.",
            json!({ "at": address("Where the note starts") }),
            &["at"],
            false,
        ),
        tool(
            "list_annotations",
            "List notes",
            "Your notes, in address order, optionally filtered by name or comment.",
            json!({ "filter": { "type": "string" } }),
            &[],
            true,
        ),
    ]
}

// --- Formatting ------------------------------------------------------------------

fn human(n: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut x = n as f64 / 1024.0;
    let mut u = 0;
    while x >= 1024.0 && u < UNITS.len() - 1 {
        x /= 1024.0;
        u += 1;
    }
    format!("{x:.1} {}", UNITS[u])
}

fn count(n: impl Into<u64>) -> String {
    let s = n.into().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn pct(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "0%".into();
    }
    let p = part as f64 * 100.0 / whole as f64;
    if p >= 10.0 {
        format!("{p:.0}%")
    } else if p >= 0.1 {
        format!("{p:.1}%")
    } else if p > 0.0 {
        "<0.1%".into()
    } else {
        "0%".into()
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

fn finish(mut s: String) -> String {
    if s.len() > MAX_OUTPUT {
        let mut cut = MAX_OUTPUT;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("\n… (output truncated; narrow the query or use offset/limit)");
    }
    s
}

fn int(args: &Value, key: &str, default: u64, max: u64) -> u64 {
    args.get(key).and_then(Value::as_u64).unwrap_or(default).min(max)
}

fn string<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

enum Loc {
    Address(u64),
    Offset(u64),
}

/// `0x401000`, `401000`, `@0x200`, `main`, `main+0x10`.
fn resolve(bin: &Binary, text: &str) -> Result<Loc, String> {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix('@') {
        return binviz::search::parse_number(rest)
            .map(Loc::Offset)
            .ok_or_else(|| format!("not a file offset: {t}"));
    }
    if let Some(n) = binviz::search::parse_number(t) {
        return Ok(Loc::Address(n));
    }
    if let Some((name, off)) = t.rsplit_once('+')
        && let Some(off) = binviz::search::parse_number(off)
        && let Some(s) = bin.symbols().by_name(name.trim())
    {
        return Ok(Loc::Address(s.address + off));
    }
    if let Some(s) = bin.symbols().by_name(t) {
        return Ok(Loc::Address(s.address));
    }
    Err(format!(
        "{t:?} is not an address, a file offset (@0x…) or a symbol name; try search"
    ))
}

fn address_of(bin: &Binary, text: &str) -> Result<u64, String> {
    match resolve(bin, text)? {
        Loc::Address(a) => Ok(a),
        Loc::Offset(o) => bin
            .offset_to_address(o)
            .ok_or_else(|| format!("file offset {o:#x} is not loaded at any address")),
    }
}

// --- Dispatch ----------------------------------------------------------------------

impl Server {
    pub fn call(&mut self, name: &str, args: &Value) -> Result<String, String> {
        match name {
            "open_binary" => self.open_binary(args),
            "list_binaries" => Ok(self.list_binaries()),
            "close_binary" => self.close_binary(args),
            _ => {
                let o = self.get(args)?;
                let text = match name {
                    "binary_summary" => summary(o),
                    "size_report" => size_report(o, args),
                    "search" => search(o, args)?,
                    "inspect" => inspect(o, args)?,
                    "disassemble" => disassemble(o, args)?,
                    "list_symbols" => list_symbols(o, args),
                    "list_strings" => list_strings(o, args),
                    "hexdump" => hexdump(o, args)?,
                    "coverage" => coverage(o, args),
                    "annotate" => annotate(o, args)?,
                    "remove_annotation" => remove_annotation(o, args)?,
                    "list_annotations" => list_annotations(o, args),
                    "function_info" => function_info(o, args)?,
                    "xrefs" => xrefs(o, args)?,
                    "callers" => call_list(o, args, true)?,
                    "callees" => call_list(o, args, false)?,
                    "call_graph" => call_graph(o, args)?,
                    "call_path" => call_path(o, args)?,
                    _ => return Err(format!("unknown tool {name}")),
                };
                Ok(finish(text))
            }
        }
    }

    fn get(&mut self, args: &Value) -> Result<&mut Open, String> {
        if self.open.is_empty() {
            return Err("no binary is open: call open_binary first".into());
        }
        let i = match string(args, "binary") {
            Some(want) => self
                .open
                .iter()
                .position(|o| o.id == want || o.path.to_string_lossy() == want || o.label == want)
                .ok_or_else(|| format!("no open binary {want:?}; open: {}", self.ids()))?,
            None => self.current.unwrap_or(self.open.len() - 1),
        };
        Ok(&mut self.open[i])
    }

    fn ids(&self) -> String {
        self.open.iter().map(|o| o.id.as_str()).collect::<Vec<_>>().join(", ")
    }

    fn open_binary(&mut self, args: &Value) -> Result<String, String> {
        let path = PathBuf::from(string(args, "path").ok_or("path is required")?);
        let started = Instant::now();
        let data = binviz::read_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file_name = path
            .file_name()
            .map_or_else(|| "binary".into(), |n| n.to_string_lossy().into_owned());
        let mut note = String::new();
        let (bin, label) = if Container::is_container(&data) {
            let c = Container::parse(data).map_err(|e| e.to_string())?;
            let members = c.members();
            let want = string(args, "member");
            let pick = match want {
                Some(w) => members
                    .iter()
                    .find(|m| m.index.to_string() == w || m.arch.as_deref() == Some(w) || m.name == w),
                None if c.info().kind.contains("Universal") => members
                    .iter()
                    .find(|m| m.arch.as_deref() == Some("arm64"))
                    .or_else(|| members.first()),
                None => None,
            };
            let list = members
                .iter()
                .take(50)
                .map(|m| {
                    format!(
                        "  [{}] {} {} {}",
                        m.index,
                        m.name,
                        m.arch.as_deref().unwrap_or(""),
                        human(m.size)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let Some(m) = pick else {
                return Err(format!(
                    "{} is a {} with {} members; pass member (index or architecture):\n{list}",
                    path.display(),
                    c.info().kind,
                    members.len()
                ));
            };
            if want.is_none() {
                let _ = writeln!(
                    note,
                    "{} with {} slices; using [{}] {}. Pass member to pick another:\n{list}",
                    c.info().kind,
                    members.len(),
                    m.index,
                    m.arch.as_deref().unwrap_or(&m.name)
                );
            }
            let bin = c.open(m.index).map_err(|e| e.to_string())?;
            (bin, format!("{file_name} [{}]", m.arch.as_deref().unwrap_or(&m.name)))
        } else {
            (Binary::parse(data).map_err(|e| e.to_string())?, file_name.clone())
        };
        let mut open = Open {
            id: String::new(),
            path: path.clone(),
            label,
            bin,
            notes: None,
        };
        if let Some(debug) = string(args, "debug_file") {
            let data = binviz::read_file(debug).map_err(|e| format!("{debug}: {e}"))?;
            open.bin
                .attach_debug_file(debug, data)
                .map_err(|e| format!("{debug}: {e}"))?;
        }
        // Notes: <path>.binviz-notes.json unless told otherwise.
        let notes_path = string(args, "notes_file")
            .map(PathBuf::from)
            .unwrap_or_else(|| sidecar(&path, open.label.split(" [").nth(1).map(|s| s.trim_end_matches(']'))));
        if notes_path.exists() {
            match notes::load(&notes_path) {
                Ok((list, fingerprint)) => {
                    let n = list.len();
                    open.bin.set_annotations(list);
                    let _ = write!(note, "Loaded {n} notes from {}", notes_path.display());
                    if fingerprint.is_some_and(|f| f != open.bin.summary().fingerprint) {
                        note.push_str(
                            " (they were saved for a different build of this file: check they still line up)",
                        );
                    }
                    note.push('\n');
                }
                Err(e) => {
                    let _ = writeln!(note, "Could not read notes: {e}");
                }
            }
        } else {
            let _ = writeln!(note, "Notes will be saved to {}", notes_path.display());
        }
        open.notes = Some(notes_path);
        // A short, unique id: the file name, numbered if needed.
        let mut id = open.label.replace(' ', "");
        let base = id.clone();
        let mut k = 2;
        while self.open.iter().any(|o| o.id == id) {
            id = format!("{base}#{k}");
            k += 1;
        }
        open.id = id;
        let text = format!(
            "Opened {} as id `{}` in {:.2} s.\n{}{}",
            path.display(),
            open.id,
            started.elapsed().as_secs_f64(),
            note,
            summary(&open)
        );
        // Re-opening the same file replaces the old copy.
        if let Some(i) = self
            .open
            .iter()
            .position(|o| o.path == open.path && o.label == open.label)
        {
            let id = std::mem::take(&mut self.open[i].id);
            open.id = id;
            self.open[i] = open;
            self.current = Some(i);
        } else {
            self.open.push(open);
            self.current = Some(self.open.len() - 1);
        }
        Ok(finish(text))
    }

    fn list_binaries(&self) -> String {
        if self.open.is_empty() {
            return "No binaries open. Use open_binary.".into();
        }
        let mut out = String::new();
        for (i, o) in self.open.iter().enumerate() {
            let s = o.bin.summary();
            let _ = writeln!(
                out,
                "{} `{}` {} — {} {} {}, {} symbols{}",
                if Some(i) == self.current { "*" } else { " " },
                o.id,
                o.path.display(),
                s.format_name,
                s.arch,
                human(s.file_size),
                count(s.symbol_count),
                if s.has_dwarf { ", DWARF" } else { "" }
            );
        }
        out
    }

    fn close_binary(&mut self, args: &Value) -> Result<String, String> {
        let id = self.get(args)?.id.clone();
        let i = self.open.iter().position(|o| o.id == id).expect("found above");
        self.open.remove(i);
        self.current = if self.open.is_empty() {
            None
        } else {
            Some(self.open.len() - 1)
        };
        Ok(format!("Closed `{id}`."))
    }
}

/// `<path>.binviz-notes.json`, or `<path>.<arch>.binviz-notes.json` for a slice.
fn sidecar(path: &Path, member: Option<&str>) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    if let Some(m) = member {
        name.push(format!(".{m}"));
    }
    name.push(".binviz-notes.json");
    path.with_file_name(name)
}

// --- Tools -------------------------------------------------------------------------

fn summary(o: &Open) -> String {
    let b = &o.bin;
    let s = b.summary();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}: {} {} for {} ({}-bit, {}-endian), {}",
        o.label,
        s.format_name,
        s.kind.to_lowercase(),
        s.arch,
        s.bits,
        if s.little_endian { "little" } else { "big" },
        human(s.file_size)
    );
    if let Some(e) = s.entry {
        let _ = writeln!(
            out,
            "Entry point: {e:#x}{}",
            b.symbols()
                .lookup(e)
                .map(|f| format!(" ({})", f.demangled.unwrap_or(f.name)))
                .unwrap_or_default()
        );
    }
    if let Some(base) = s.image_base {
        let _ = writeln!(out, "Image base: {base:#x}");
    }
    if let Some(id) = &s.build_id {
        let _ = writeln!(out, "Build ID / UUID: {id}");
    }
    if let Some(l) = &s.debug_link {
        let _ = writeln!(out, "Debug info reference: {l}");
    }
    for p in &s.properties {
        let _ = writeln!(out, "{}: {}", p.key, clip(&p.value, 300));
    }
    let _ = writeln!(
        out,
        "Symbols: {} ({} functions)",
        count(s.symbol_count),
        count(b.symbols().functions().count() as u64)
    );
    if !b.imports().is_empty() || !b.exports().is_empty() {
        let _ = writeln!(
            out,
            "Imports: {} · Exports: {}",
            count(b.imports().len() as u64),
            count(b.exports().len() as u64)
        );
    }
    let notes = b.annotations();
    if !notes.is_empty() {
        let _ = writeln!(
            out,
            "Notes: {} ({} reviewed)",
            notes.len(),
            notes.iter().filter(|a| a.reviewed).count()
        );
    }
    match b.debug_info() {
        Some(d) => {
            let ds = d.summary();
            let _ = writeln!(
                out,
                "DWARF ({}): versions {:?}, {} units, {} source files, languages {}",
                ds.source,
                ds.versions,
                count(ds.unit_count),
                count(d.source_files().len() as u64),
                ds.languages.join(", ")
            );
            for p in ds.producers.iter().take(3) {
                let _ = writeln!(out, "  producer: {}", clip(p, 160));
            }
        }
        None => {
            let _ = writeln!(
                out,
                "No DWARF debug info{}.",
                if s.format == binviz::Format::MachO {
                    " (for Mach-O it usually lives in a .dSYM: pass its DWARF file as debug_file)"
                } else {
                    ""
                }
            );
        }
    }
    let segs: Vec<_> = b.segments().iter().filter(|g| g.mapped).collect();
    if !segs.is_empty() {
        let _ = writeln!(out, "\nSegments:");
        for g in segs.iter().take(64) {
            let _ = writeln!(
                out,
                "  {:<16} {:>4} {:#014x}..{:#014x}  file {:#x}..{:#x}  {}",
                g.name,
                g.perms,
                g.address,
                g.address + g.mem_size,
                g.file_offset,
                g.file_offset + g.file_size,
                human(g.mem_size)
            );
        }
    }
    let _ = writeln!(out, "\nSections ({}):", b.sections().len());
    for sec in b.sections().iter().take(200) {
        let name = match &sec.segment_name {
            Some(seg) if !seg.is_empty() => format!("{seg},{}", sec.name),
            _ => sec.name.clone(),
        };
        let place = if sec.loaded {
            format!("{:#014x}", sec.address)
        } else {
            "not loaded    ".into()
        };
        let _ = writeln!(
            out,
            "  [{:>3}] {:<28} {:<10} {} {:>10}  {}",
            sec.index,
            clip(&name, 28),
            format!("{:?}", sec.kind).to_lowercase(),
            place,
            human(sec.size),
            sec.perms
        );
    }
    if b.sections().len() > 200 {
        let _ = writeln!(out, "  … {} more", b.sections().len() - 200);
    }
    out
}

fn size_report(o: &Open, args: &Value) -> String {
    let top = int(args, "top", 15, 200) as usize;
    let r = o.bin.size_report(top);
    let total = r.file_size;
    let mut out = String::new();
    let _ = writeln!(out, "{} — {} ({} bytes)\n", o.label, human(total), count(total));
    let _ = writeln!(out, "Every byte of the file, by what it is:");
    for (kind, bytes) in &r.by_kind {
        let _ = writeln!(
            out,
            "  {:<18} {:>10}  {:>5}",
            format!("{kind:?}").to_lowercase(),
            human(*bytes),
            pct(*bytes, total)
        );
    }
    let _ = writeln!(out, "\nLargest sections (file bytes · memory bytes):");
    for s in r.sections.iter().take(top) {
        let _ = writeln!(
            out,
            "  {:<32} {:>10} · {:>10}  {}",
            clip(&s.name, 32),
            human(s.file_bytes),
            human(s.memory_bytes),
            format!("{:?}", s.kind).to_lowercase()
        );
    }
    if r.symbolized_bytes > 0 {
        let _ = writeln!(
            out,
            "\nSymbols cover {} of code and data. Who owns it:",
            human(r.symbolized_bytes)
        );
        for (kind, bytes) in &r.by_group_kind {
            let _ = writeln!(
                out,
                "  {:<28} {:>10}  {:>5}",
                kind.label(),
                human(*bytes),
                pct(*bytes, r.symbolized_bytes)
            );
        }
        let _ = writeln!(out, "\nBiggest owners ({} in total):", count(r.group_count));
        for g in &r.groups {
            let mut what = Vec::new();
            if g.functions > 0 {
                what.push(format!("{} in {} functions", human(g.code_bytes), count(g.functions)));
            }
            if g.data_symbols > 0 {
                what.push(format!(
                    "{} in {} data symbols",
                    human(g.data_bytes),
                    count(g.data_symbols)
                ));
            }
            let _ = writeln!(
                out,
                "  {:>10}  {:<40} {} · {}",
                human(g.code_bytes + g.data_bytes),
                clip(&g.name, 40),
                g.kind.label(),
                what.join(", ")
            );
        }
        let _ = writeln!(out, "\nLargest functions:");
        for f in &r.largest_functions {
            let _ = writeln!(
                out,
                "  {:>10}{} {:#x}  {}",
                human(f.size),
                if f.approximate { "~" } else { " " },
                f.address,
                clip(&f.name, 120)
            );
        }
        if !r.largest_data.is_empty() {
            let _ = writeln!(out, "\nLargest data:");
            for f in &r.largest_data {
                let _ = writeln!(
                    out,
                    "  {:>10}{} {:#x}  {}",
                    human(f.size),
                    if f.approximate { "~" } else { " " },
                    f.address,
                    clip(&f.name, 120)
                );
            }
        }
        let _ = writeln!(out, "(~ = size inferred from the next symbol)");
    } else {
        let _ = writeln!(out, "\nNo symbols with sizes: owners can't be worked out.");
    }
    if !r.source_files.is_empty() {
        let _ = writeln!(out, "\nSource files by code and data generated (DWARF):");
        for (path, bytes) in &r.source_files {
            let _ = writeln!(out, "  {:>10}  {}", human(*bytes), clip(path, 140));
        }
    }
    let _ = writeln!(
        out,
        "\nStrings: {} ({} of text) in the data sections.",
        count(r.strings),
        human(r.string_bytes)
    );
    out
}

fn search(o: &Open, args: &Value) -> Result<String, String> {
    let query = string(args, "query").ok_or("query is required")?;
    let limit = int(args, "limit", 10, 500) as u32;
    let only = match string(args, "kind") {
        Some(k) => Some(serde_json::from_value::<HitKind>(json!(k)).map_err(|_| format!("unknown kind {k}"))?),
        None => None,
    };
    let res = o.bin.search(query, limit, only);
    if res.hits.is_empty() {
        return Ok(format!(
            "Nothing matches {query:?}. (Name and string searches need at least 2 characters.)"
        ));
    }
    let mut out = String::new();
    let mut last = None;
    for h in &res.hits {
        if last != Some(h.kind) {
            last = Some(h.kind);
            let n = res.counts.iter().find(|c| c.kind == h.kind).map_or(0, |c| c.count);
            let shown = res.hits.iter().filter(|x| x.kind == h.kind).count();
            let _ = writeln!(
                out,
                "{}{:?} — {} match{}{}",
                if out.is_empty() { "" } else { "\n" },
                h.kind,
                count(n),
                if n == 1 { "" } else { "es" },
                if (n as usize) > shown {
                    format!(", best {shown}")
                } else {
                    String::new()
                }
            );
        }
        let at = match (h.address, h.offset) {
            (Some(a), _) => format!("{a:#x}"),
            (None, Some(off)) => format!("@{off:#x}"),
            _ => String::new(),
        };
        let _ = writeln!(out, "  {at:<16} {}  — {}", clip(&h.label, 140), clip(&h.detail, 200));
    }
    Ok(out)
}

fn inspect(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let target = match resolve(&o.bin, at)? {
        Loc::Address(a) => Target::Address(a),
        Loc::Offset(off) => Target::Offset(off),
    };
    let i = o.bin.inspect(target);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}{}",
        i.address
            .map(|a| format!("address {a:#x}"))
            .unwrap_or_else(|| "not mapped into memory".into()),
        i.offset
            .map(|off| format!(" · file offset {off:#x}"))
            .unwrap_or_else(|| " · no file bytes (zero-filled at load)".into())
    );
    if let Some(b) = i.byte {
        let _ = writeln!(out, "byte {b:#04x}");
    }
    if !i.path.is_empty() {
        let _ = writeln!(out, "\nContained in (outermost first):");
        for (depth, p) in i.path.iter().enumerate() {
            let _ = writeln!(
                out,
                "{}{} [{:#x}..{:#x}]{}",
                "  ".repeat(depth + 1),
                p.name,
                p.start,
                p.end,
                p.value
                    .as_deref()
                    .map(|v| format!(" = {}", clip(v, 160)))
                    .unwrap_or_default()
            );
        }
        if let Some(note) = i.path.last().and_then(|p| p.note.as_deref()) {
            let _ = writeln!(out, "  ({note})");
        }
    }
    if let Some(seg) = i.segment.and_then(|s| o.bin.segments().get(s as usize)) {
        let _ = writeln!(out, "\nSegment {} ({})", seg.name, seg.perms);
    }
    if let Some(sec) = i.section.and_then(|s| o.bin.sections().get(s as usize)) {
        let _ = writeln!(
            out,
            "Section {}{}",
            sec.segment_name.as_deref().map(|s| format!("{s},")).unwrap_or_default(),
            sec.name
        );
    }
    if let Some(s) = &i.symbol {
        let _ = writeln!(
            out,
            "Symbol: {}{} ({} bytes at {:#x})",
            s.demangled.as_deref().unwrap_or(&s.name),
            if s.offset > 0 {
                format!(" + {:#x}", s.offset)
            } else {
                String::new()
            },
            s.size,
            s.address
        );
    }
    if let Some(src) = &i.source {
        let _ = writeln!(out, "Source: {}:{}:{}", src.path, src.line, src.column);
    }
    if i.frames.len() > 1 {
        let _ = writeln!(out, "Inlined call stack (innermost first):");
        for f in &i.frames {
            let _ = writeln!(
                out,
                "  {} at {}:{}",
                f.demangled.as_deref().or(f.function.as_deref()).unwrap_or("??"),
                f.file.as_deref().unwrap_or("?"),
                f.line.unwrap_or(0)
            );
        }
    }
    if let Some(ins) = &i.instruction {
        let _ = writeln!(
            out,
            "Instruction: {} {}{}   [{}]",
            ins.mnemonic,
            ins.operands,
            ins.target_symbol
                .as_deref()
                .map(|t| format!("  → {t}"))
                .unwrap_or_default(),
            ins.bytes
        );
    }
    if let Some(address) = i.address
        && o.bin.xrefs_ready()
    {
        let c = o.bin.reference_counts(address, address + 1);
        if c.total() > 0 {
            let _ = writeln!(out, "Referenced by: {} (see xrefs)", ref_counts(&c));
        }
    }
    if let Some(a) = &i.annotation {
        let _ = writeln!(
            out,
            "Note{}: {}{}{}",
            if a.reviewed { " (reviewed)" } else { "" },
            a.name,
            if a.name.is_empty() || a.comment.is_empty() {
                ""
            } else {
                " — "
            },
            a.comment
        );
    }
    Ok(out)
}

fn disassemble(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let address = address_of(&o.bin, at)?;
    let max = int(args, "max_instructions", 400, 5000) as usize;
    let d = o.bin.disassemble_function(address, max);
    if !d.supported {
        return Err(format!("no disassembler for {}", o.bin.summary().arch));
    }
    if d.instructions.is_empty() {
        return Err(format!("{address:#x} is not in code with file bytes"));
    }
    let notes: std::collections::HashMap<u64, &Annotation> =
        o.bin.annotations().iter().map(|a| (a.address, a)).collect();
    let mut out = String::new();
    let name = d
        .function
        .as_ref()
        .map(|f| f.demangled.clone().unwrap_or_else(|| f.name.clone()))
        .unwrap_or_else(|| format!("{:#x}", d.start));
    let _ = writeln!(
        out,
        "{name}  [{:#x}..{:#x}, {} bytes, {} instructions{}]",
        d.start,
        d.end,
        d.end - d.start,
        d.instructions.len(),
        if d.truncated { ", truncated" } else { "" }
    );
    if let Some(a) = notes.get(&d.start)
        && !a.comment.is_empty()
    {
        let _ = writeln!(out, "; {}", a.comment.replace('\n', "\n; "));
    }
    let mut last_src: Option<(String, u32)> = None;
    for ins in &d.instructions {
        if let Some(src) = &ins.source {
            let key = (src.path.clone(), src.line);
            if last_src.as_ref() != Some(&key) {
                let file = src.path.rsplit(['/', '\\']).next().unwrap_or(&src.path);
                let _ = writeln!(out, "  ; {file}:{}", src.line);
                last_src = Some(key);
            }
        }
        let target = match (ins.target, &ins.target_symbol) {
            (_, Some(sym)) => format!("  <{sym}>"),
            _ => String::new(),
        };
        let comment = notes
            .get(&ins.address)
            .filter(|a| !a.comment.is_empty() && ins.address != d.start)
            .map(|a| format!("  ; {}", a.comment.lines().next().unwrap_or("")))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "{:#x}  {:<8} {}{target}{comment}",
            ins.address, ins.mnemonic, ins.operands
        );
    }
    Ok(out)
}

fn list_symbols(o: &Open, args: &Value) -> String {
    let q = SymbolQuery {
        filter: string(args, "filter").unwrap_or("").to_string(),
        kind: string(args, "kind").unwrap_or("").to_string(),
        sort: string(args, "sort").unwrap_or("address").to_string(),
        descending: args.get("descending").and_then(Value::as_bool).unwrap_or(false),
        defined_only: false,
        offset: int(args, "offset", 0, u32::MAX as u64) as u32,
        limit: int(args, "limit", 50, 1000) as u32,
    };
    let page = o.bin.symbols().query(&q);
    let mut out = format!(
        "{} symbols match; showing {}–{}:\n",
        count(page.total),
        page.offset as usize + (!page.symbols.is_empty()) as usize,
        page.offset as usize + page.symbols.len()
    );
    for s in &page.symbols {
        let _ = writeln!(
            out,
            "  {:<18} {:>9}{} {:<8} {:<9} {}",
            if s.defined {
                format!("{:#x}", s.address)
            } else {
                "undefined".into()
            },
            s.size,
            if s.size_inferred { "~" } else { " " },
            format!("{:?}", s.kind).to_lowercase(),
            s.binding,
            clip(s.display_name(), 160)
        );
    }
    out
}

fn list_strings(o: &Open, args: &Value) -> String {
    let filter = string(args, "filter").unwrap_or("");
    let page = o.bin.strings(
        filter,
        int(args, "offset", 0, u32::MAX as u64) as u32,
        int(args, "limit", 50, 1000) as u32,
    );
    let mut out = format!(
        "{} strings{}; showing {}:\n",
        count(page.total),
        if filter.is_empty() {
            String::new()
        } else {
            format!(" containing {filter:?}")
        },
        page.strings.len()
    );
    for s in &page.strings {
        let at = s
            .address
            .map_or_else(|| format!("@{:#x}", s.offset), |a| format!("{a:#x}"));
        let section = s
            .section
            .and_then(|i| o.bin.sections().get(i as usize))
            .map_or("", |sec| sec.name.as_str());
        let _ = writeln!(
            out,
            "  {at:<16} {section:<16}{} {:?}",
            if s.wide { " UTF-16" } else { "" },
            clip(&s.text, 200)
        );
    }
    out
}

fn hexdump(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let (offset, address) = match resolve(&o.bin, at)? {
        Loc::Offset(off) => (off, o.bin.offset_to_address(off)),
        Loc::Address(a) => (
            o.bin
                .address_to_offset(a)
                .ok_or_else(|| format!("{a:#x} has no file bytes (not mapped, or zero-filled)"))?,
            Some(a),
        ),
    };
    let data = o.bin.data();
    if offset >= data.len() as u64 {
        return Err(format!(
            "offset {offset:#x} is past the end of the file ({:#x})",
            data.len()
        ));
    }
    let len = int(args, "length", 256, 4096) as usize;
    let end = (offset as usize + len).min(data.len());
    let mut out = String::new();
    let path = o.bin.describe_offset(offset);
    let names: Vec<&str> = path.iter().map(|p| p.name.as_str()).collect();
    let _ = writeln!(
        out,
        "file offset {offset:#x}{} — in {}",
        address.map(|a| format!(" (address {a:#x})")).unwrap_or_default(),
        names.join(" › ")
    );
    for row in (offset as usize..end).step_by(16) {
        let bytes = &data[row..(row + 16).min(end)];
        let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = bytes
            .iter()
            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
            .collect();
        let va = o
            .bin
            .offset_to_address(row as u64)
            .map(|a| format!("{a:#014x} "))
            .unwrap_or_default();
        let _ = writeln!(out, "{row:08x}  {va}{:<48} {ascii}", hex.join(" "));
    }
    Ok(out)
}

fn coverage(o: &Open, args: &Value) -> String {
    let c = o.bin.coverage(int(args, "gaps", 20, 500) as u32);
    let t = &c.totals;
    let total = t.reviewed + t.annotated + t.named + t.structure + t.recovered + t.padding + t.unexplored;
    let denom = total - t.padding;
    let mapped = total - t.padding - t.unexplored;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Mapped out: {} of {} code and data bytes ({}; padding excluded)",
        human(mapped),
        human(denom),
        pct(mapped, denom)
    );
    let _ = writeln!(
        out,
        "  reviewed {} · annotated {} · named {} · format structure {} · recovered {} · padding {} · unexplored {}",
        human(t.reviewed),
        human(t.annotated),
        human(t.named),
        human(t.structure),
        human(t.recovered),
        human(t.padding),
        human(t.unexplored)
    );
    let _ = writeln!(
        out,
        "Functions: {} named, {} recovered (unnamed), {} named by you · notes: {} ({} reviewed)",
        count(c.functions.named),
        count(c.functions.recovered),
        count(c.functions.user),
        count(c.annotations),
        count(c.reviewed)
    );
    let _ = writeln!(
        out,
        "\nPer section (size: reviewed/annotated/named/structure/recovered/padding/unexplored):"
    );
    for s in &c.sections {
        let b = &s.bytes;
        let _ = writeln!(
            out,
            "  {:<20} {:>10}: {}/{}/{}/{}/{}/{}/{}",
            clip(&s.name, 20),
            human(s.size),
            pct(b.reviewed, s.size),
            pct(b.annotated, s.size),
            pct(b.named, s.size),
            pct(b.structure, s.size),
            pct(b.recovered, s.size),
            pct(b.padding, s.size),
            pct(b.unexplored, s.size)
        );
    }
    if c.gaps.is_empty() {
        let _ = writeln!(out, "\nNo unexplored bytes.");
    } else {
        let _ = writeln!(out, "\nLargest unexplored gaps ({} in total):", count(c.gap_count));
        for g in &c.gaps {
            let sec = o
                .bin
                .sections()
                .get(g.section as usize)
                .map_or("?", |s| s.name.as_str());
            let _ = writeln!(
                out,
                "  {:#x}..{:#x} {:>10} in {sec:<14} looks like {:<14} after {}  [{}]",
                g.start,
                g.end,
                human(g.end - g.start),
                g.hint,
                g.after.as_deref().unwrap_or("-"),
                g.preview
            );
        }
    }
    out
}

fn save_notes(o: &Open) -> String {
    match &o.notes {
        Some(path) => match notes::save(path, &o.label, &o.bin.summary().fingerprint, o.bin.annotations()) {
            Ok(()) => format!("saved to {}", path.display()),
            Err(e) => format!("NOT saved ({e}); kept in memory for this session"),
        },
        None => "kept in memory".into(),
    }
}

fn annotate(o: &mut Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let address = address_of(&o.bin, at)?;
    let size = args.get("size").and_then(Value::as_u64);
    let mut list: Vec<Annotation> = o.bin.annotations().to_vec();
    let existing = list
        .iter()
        .position(|a| a.address == address && size.is_none_or(|s| s == a.size || a.size == 0 || s == 0));
    let mut a = match existing {
        Some(i) => list.remove(i),
        None => Annotation {
            address,
            size: 0,
            name: String::new(),
            comment: String::new(),
            reviewed: false,
        },
    };
    if let Some(s) = size {
        a.size = s;
    }
    if let Some(n) = args.get("name").and_then(Value::as_str) {
        a.name = n.trim().to_string();
    }
    if let Some(c) = args.get("comment").and_then(Value::as_str) {
        a.comment = c.trim().to_string();
    }
    if let Some(r) = args.get("reviewed").and_then(Value::as_bool) {
        a.reviewed = r;
    }
    let summary = format!(
        "{} note at {address:#x}{}{}{}",
        if existing.is_some() { "Updated" } else { "Added" },
        if a.name.is_empty() {
            String::new()
        } else {
            format!(" named {}", a.name)
        },
        if a.comment.is_empty() {
            String::new()
        } else {
            format!(": {}", clip(&a.comment, 80))
        },
        if a.reviewed { " (reviewed)" } else { "" }
    );
    list.push(a);
    o.bin.set_annotations(list);
    Ok(format!(
        "{summary}. {} notes, {}.",
        o.bin.annotations().len(),
        save_notes(o)
    ))
}

fn remove_annotation(o: &mut Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let address = address_of(&o.bin, at)?;
    let before = o.bin.annotations().len();
    let list: Vec<Annotation> = o
        .bin
        .annotations()
        .iter()
        .filter(|a| a.address != address)
        .cloned()
        .collect();
    let removed = before - list.len();
    if removed == 0 {
        return Err(format!("no note starts at {address:#x}"));
    }
    o.bin.set_annotations(list);
    Ok(format!(
        "Removed {removed} note(s) at {address:#x}; {} left, {}.",
        o.bin.annotations().len(),
        save_notes(o)
    ))
}

fn list_annotations(o: &Open, args: &Value) -> String {
    let filter = string(args, "filter").map(str::to_lowercase);
    let notes: Vec<&Annotation> = o
        .bin
        .annotations()
        .iter()
        .filter(|a| {
            filter
                .as_ref()
                .is_none_or(|f| a.name.to_lowercase().contains(f) || a.comment.to_lowercase().contains(f))
        })
        .collect();
    if notes.is_empty() {
        return "No notes yet. Use annotate to name functions, comment addresses, or mark code reviewed.".into();
    }
    let mut out = format!(
        "{} notes{}:\n",
        notes.len(),
        o.notes
            .as_ref()
            .map(|p| format!(" ({})", p.display()))
            .unwrap_or_default()
    );
    for a in notes.iter().take(2000) {
        let _ = writeln!(
            out,
            "  {:#x}{} {}{}{}",
            a.address,
            if a.size > 0 {
                format!("+{:#x}", a.size)
            } else {
                String::new()
            },
            if a.reviewed { "[reviewed] " } else { "" },
            if a.name.is_empty() {
                String::new()
            } else {
                format!("{} ", a.name)
            },
            if a.comment.is_empty() {
                String::new()
            } else {
                format!("— {}", clip(&a.comment, 200))
            }
        );
    }
    out
}

// --- Cross-references and the call graph ----------------------------------------------

/// Builds the reference index if needed; says so when that took a while.
fn ensure_xrefs(o: &Open) -> Result<String, String> {
    if !o.bin.xrefs_supported() {
        return Err(format!(
            "references can't be found in {} code yet (x86, x86-64 and AArch64 are supported)",
            o.bin.summary().arch
        ));
    }
    if o.bin.xrefs_ready() {
        return Ok(String::new());
    }
    let t = Instant::now();
    o.bin.prepare_xrefs();
    let c = o.bin.xref_counts();
    Ok(format!(
        "(Indexed {} references in {:.1} s.)\n",
        count(c.total()),
        t.elapsed().as_secs_f64()
    ))
}

fn ref_counts(c: &binviz::RefCounts) -> String {
    let parts: Vec<String> = [
        (c.call, "call"),
        (c.jump, "tail call/jump"),
        (c.read, "read"),
        (c.write, "write"),
        (c.address, "address taken"),
        (c.pointer, "pointer in data"),
    ]
    .iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| {
        format!(
            "{} {what}{}",
            count(*n),
            if *n == 1 || what.ends_with("data") || what.ends_with("taken") {
                ""
            } else {
                "s"
            }
        )
    })
    .collect();
    if parts.is_empty() {
        "nothing".into()
    } else {
        parts.join(", ")
    }
}

fn node_kind(k: binviz::NodeKind) -> &'static str {
    match k {
        binviz::NodeKind::Function => "",
        binviz::NodeKind::Import => " [import]",
        binviz::NodeKind::Code => " [code]",
        binviz::NodeKind::Data => " [data]",
    }
}

/// The function containing an address, as (start, name), or an error that says what is there instead.
fn function_at(bin: &Binary, at: &str) -> Result<u64, String> {
    let address = address_of(bin, at)?;
    match bin.symbols().function_containing(address) {
        Some(f) => Ok(f.address),
        None => Err(format!(
            "{address:#x} is not inside a known function{}",
            bin.sections()
                .iter()
                .find(|s| s.loaded && address >= s.address && address < s.address + s.size)
                .map(|s| format!(" (it is in {})", s.name))
                .unwrap_or_default()
        )),
    }
}

fn function_info(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let start = function_at(&o.bin, at)?;
    let mut out = ensure_xrefs(o)?;
    let limit = int(args, "limit", 25, 500) as usize;
    let f = o.bin.function_summary(start, limit).ok_or("not in a function")?;
    let _ = writeln!(
        out,
        "{}  [{:#x}..{:#x}, {} bytes]",
        f.name,
        f.address,
        f.address + f.size,
        count(f.size)
    );
    let i = o.bin.inspect(Target::Address(f.address));
    if let Some(src) = &i.source {
        let _ = writeln!(out, "Source: {}:{}", src.path, src.line);
    }
    if let Some(a) = &i.annotation {
        let _ = writeln!(
            out,
            "Your note{}: {}",
            if a.reviewed { " (reviewed)" } else { "" },
            clip(&a.comment, 300)
        );
    }
    let _ = writeln!(out, "Referenced by: {}", ref_counts(&f.referenced_by));
    let _ = writeln!(
        out,
        "\nCalled by {} function{}:",
        count(f.caller_count),
        if f.caller_count == 1 { "" } else { "s" }
    );
    for c in &f.callers {
        let _ = writeln!(
            out,
            "  {:#x}  {}{}  ({}x, first at {:#x})",
            c.address,
            clip(&c.name, 120),
            node_kind(c.kind),
            c.calls,
            c.site
        );
    }
    if f.caller_count as usize > f.callers.len() {
        let _ = writeln!(out, "  … {} more (callers)", f.caller_count as usize - f.callers.len());
    }
    let _ = writeln!(out, "\nCalls {}:", count(f.callee_count));
    for c in &f.callees {
        let _ = writeln!(
            out,
            "  {:#x}  {}{}  ({}x, first at {:#x})",
            c.address,
            clip(&c.name, 120),
            node_kind(c.kind),
            c.calls,
            c.site
        );
    }
    if f.callee_count as usize > f.callees.len() {
        let _ = writeln!(out, "  … {} more (callees)", f.callee_count as usize - f.callees.len());
    }
    if !f.strings.is_empty() {
        let _ = writeln!(out, "\nStrings:");
        for s in &f.strings {
            let _ = writeln!(out, "  {:#x}  {:?}  (at {:#x})", s.address, clip(&s.text, 200), s.site);
        }
    }
    if !f.data.is_empty() {
        let _ = writeln!(out, "\nData:");
        for r in &f.data {
            let _ = writeln!(
                out,
                "  {:#x}  {:<8} {}  (at {:#x})",
                r.target,
                r.kind.as_str(),
                clip(r.to.as_deref().unwrap_or(""), 160),
                r.source
            );
        }
    }
    Ok(out)
}

fn xrefs(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let address = address_of(&o.bin, at)?;
    let mut out = ensure_xrefs(o)?;
    // A symbol's whole extent; otherwise the one address.
    let (lo, hi, what) = match o.bin.symbols().at(address) {
        Some(s) if s.size > 0 => (
            address,
            address + s.size,
            format!("{} ({} bytes)", s.display_name(), s.size),
        ),
        _ => (address, address + 1, format!("{address:#x}")),
    };
    let only = match string(args, "kind") {
        Some(k) => Some(serde_json::from_value::<binviz::RefKind>(json!(k)).map_err(|_| format!("unknown kind {k}"))?),
        None => None,
    };
    let limit = int(args, "limit", 100, 2000) as u32;
    let offset = int(args, "offset", 0, u32::MAX as u64) as u32;
    let page = o.bin.references_to(
        lo,
        hi,
        if only.is_some() { 0 } else { offset },
        if only.is_some() { u32::MAX } else { limit },
    );
    let _ = writeln!(out, "References to {what}: {}", ref_counts(&page.counts));
    let refs: Vec<&binviz::Reference> = page
        .refs
        .iter()
        .filter(|r| only.is_none_or(|k| r.kind == k))
        .skip(if only.is_some() { offset as usize } else { 0 })
        .take(limit as usize)
        .collect();
    for r in &refs {
        let target = if hi - lo > 1 && r.target != lo {
            format!("  → +{:#x}", r.target - lo)
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "  {:#x}  {:<8} from {}{target}",
            r.source,
            r.kind.as_str(),
            clip(r.from.as_deref().unwrap_or("?"), 140)
        );
    }
    let total = match only {
        Some(k) => page.refs.iter().filter(|r| r.kind == k).count(),
        None => page.total as usize,
    };
    if offset as usize + refs.len() < total {
        let _ = writeln!(
            out,
            "  … {} more (offset {})",
            total - offset as usize - refs.len(),
            offset as usize + refs.len()
        );
    }
    if page.total == 0 {
        out.push_str("  (none found: it may only be reached indirectly, through computed addresses or registers)\n");
    }
    Ok(out)
}

fn call_list(o: &Open, args: &Value, callers: bool) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let address = address_of(&o.bin, at)?;
    let mut out = ensure_xrefs(o)?;
    let limit = int(args, "limit", 100, 2000) as usize;
    let name = o
        .bin
        .symbols()
        .function_containing(address)
        .map(|f| f.display_name().into_owned())
        .unwrap_or_else(|| format!("{address:#x}"));
    let list = if callers {
        o.bin.callers(address)
    } else {
        o.bin.callees(address)
    };
    let sites: u32 = list.iter().map(|c| c.calls).sum();
    let _ = writeln!(
        out,
        "{name} {} {} function{} ({} call site{}):",
        if callers { "is called by" } else { "calls" },
        count(list.len() as u64),
        if list.len() == 1 { "" } else { "s" },
        count(sites),
        if sites == 1 { "" } else { "s" }
    );
    for c in list.iter().take(limit) {
        let _ = writeln!(
            out,
            "  {:#x}  {}{}  ({}x, first at {:#x})",
            c.address,
            clip(&c.name, 140),
            node_kind(c.kind),
            c.calls,
            c.site
        );
    }
    if list.len() > limit {
        let _ = writeln!(out, "  … {} more", list.len() - limit);
    }
    if callers {
        let start = o
            .bin
            .symbols()
            .function_containing(address)
            .map_or(address, |f| f.address);
        let c = o.bin.reference_counts(start, start + 1);
        if c.pointer + c.address > 0 {
            let _ = writeln!(
                out,
                "Also: {} — it may be called indirectly (see xrefs).",
                ref_counts(&binviz::RefCounts {
                    call: 0,
                    jump: 0,
                    read: 0,
                    write: 0,
                    ..c
                })
            );
        }
    }
    Ok(out)
}

fn call_graph(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let start = function_at(&o.bin, at)?;
    let mut out = ensure_xrefs(o)?;
    let up = int(args, "up", 1, 4) as u32;
    let down = int(args, "down", 2, 4) as u32;
    let fanout = int(args, "fanout", 8, 30) as usize;
    let g = o.bin.call_graph(start, up, down, fanout);
    let node = |a: u64| g.nodes.iter().find(|n| n.address == a);
    let label = |a: u64| {
        node(a).map_or_else(
            || format!("{a:#x}"),
            |n| format!("{} ({:#x}){}", clip(&n.name, 100), n.address, node_kind(n.kind)),
        )
    };
    let center = node(g.center).expect("the centre is a node");
    let _ = writeln!(
        out,
        "{}: {} callers, {} callees",
        label(g.center),
        center.callers.map_or("?".into(), count),
        center.callees.map_or("?".into(), count)
    );
    // Callers, as an upside-down tree.
    if up > 0 {
        let _ = writeln!(out, "\nCalled by:");
        let mut seen = std::collections::HashSet::from([g.center]);
        fn walk_up(
            g: &binviz::CallGraph,
            n: u64,
            depth: usize,
            seen: &mut std::collections::HashSet<u64>,
            out: &mut String,
            label: &dyn Fn(u64) -> String,
        ) {
            let mut parents: Vec<&binviz::GraphEdge> = g.edges.iter().filter(|e| e.to == n && e.from != n).collect();
            parents.sort_by_key(|e| std::cmp::Reverse(e.calls));
            for e in parents {
                let is_parent = g.nodes.iter().any(|x| x.address == e.from && x.depth < 0);
                if !is_parent {
                    continue;
                }
                let again = !seen.insert(e.from);
                let _ = writeln!(
                    out,
                    "{}← {} [{}x]{}",
                    "  ".repeat(depth + 1),
                    label(e.from),
                    e.calls,
                    if again { " (see above)" } else { "" }
                );
                if !again {
                    walk_up(g, e.from, depth + 1, seen, out, label);
                }
            }
        }
        walk_up(&g, g.center, 0, &mut seen, &mut out, &label);
    }
    if down > 0 {
        let _ = writeln!(out, "\nCalls:");
        let mut seen = std::collections::HashSet::from([g.center]);
        fn walk_down(
            g: &binviz::CallGraph,
            n: u64,
            depth: usize,
            seen: &mut std::collections::HashSet<u64>,
            out: &mut String,
            label: &dyn Fn(u64) -> String,
        ) {
            let mut children: Vec<&binviz::GraphEdge> = g.edges.iter().filter(|e| e.from == n && e.to != n).collect();
            children.sort_by_key(|e| std::cmp::Reverse(e.calls));
            for e in children {
                let is_child = g.nodes.iter().any(|x| x.address == e.to && x.depth > 0);
                if !is_child {
                    continue;
                }
                let again = !seen.insert(e.to);
                let _ = writeln!(
                    out,
                    "{}→ {} [{}x]{}",
                    "  ".repeat(depth + 1),
                    label(e.to),
                    e.calls,
                    if again { " (see above)" } else { "" }
                );
                if !again {
                    walk_down(g, e.to, depth + 1, seen, out, label);
                }
            }
        }
        walk_down(&g, g.center, 0, &mut seen, &mut out, &label);
    }
    if g.hidden > 0 {
        let _ = writeln!(
            out,
            "\n({} more neighbours not shown; raise fanout, or use callers/callees on a node.)",
            count(g.hidden)
        );
    }
    Ok(out)
}

fn call_path(o: &Open, args: &Value) -> Result<String, String> {
    let from = function_at(&o.bin, string(args, "from").ok_or("from is required")?)?;
    let to = function_at(&o.bin, string(args, "to").ok_or("to is required")?)?;
    let mut out = ensure_xrefs(o)?;
    let depth = int(args, "max_depth", 10, 20) as u32;
    match o.bin.call_path(from, to, depth) {
        Some(steps) => {
            let _ = writeln!(
                out,
                "{} call{}:",
                steps.len() - 1,
                if steps.len() == 2 { "" } else { "s" }
            );
            for (i, s) in steps.iter().enumerate() {
                let site = s.site.map(|a| format!("   (called at {a:#x})")).unwrap_or_default();
                let _ = writeln!(
                    out,
                    "{}{} {:#x} {}{site}",
                    "  ".repeat(i),
                    if i == 0 { " " } else { "→" },
                    s.address,
                    clip(&s.name, 140)
                );
            }
        }
        None => {
            let _ = writeln!(
                out,
                "No chain of direct calls within {depth} calls. The target may only be reached indirectly \
                 (virtual calls, callbacks, function pointers): try xrefs on it to find pointers to it."
            );
        }
    }
    Ok(out)
}
