//! The tools an agent can call, and how their results read.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use binviz::{Annotation, Binary, Container, HitKind, SymbolQuery, Target};
use serde_json::{Value, json};

use crate::notes;

pub const INSTRUCTIONS: &str = "binviz explains ELF, Mach-O, PE and XBE binaries down to every byte, maps code back to source through DWARF, and keeps binaries loaded between calls, so exploring a large file stays fast. \
Start with open_binary (a path; universal binaries pick arm64 unless you pass member). A folder or a zip (an .ipa, an .xcarchive, an .app, a build folder; zips inside it too) opens every binary inside at once — Mach-O, ELF or PE: an app, its frameworks and extensions, libraries — each under its own id, paired with its debug file (a dSYM, an ELF .debug file, a PDB) by UUID, build ID or the name the binary records; folder_summary then shows the whole folder: sizes by kind of content, each binary and its debug file, the largest and duplicate files, and (analyze: true) code owners across all binaries. Then: binary_summary for the overview, size_report to see where the bytes go (sections, largest functions, and owners: Swift modules, Objective-C classes, C++ namespaces, C prefixes), search for anything (names, strings, addresses, byte patterns like `48 8b ?? 05`, \"exact text\", file.c:42), inspect to learn what is at an address or file offset, disassemble a function, list_symbols / list_strings to page through tables, hexdump for raw bytes. \
Addresses: 0x401000 (or 401000), a symbol, name+0x10, @0x200 for a file offset, and for banked ROMs bank:address (03:C000; $80:8000 on the SNES). To follow code: function_info gives a function's callers, callees, strings and data at a glance; callers / callees list call sites; call_graph draws the neighbourhood; call_path finds a chain of calls from one function to another; xrefs lists every reference to an address (calls, reads, writes, address-taken, pointers stored in data — e.g. who uses a string or a global). The reference index is built on first use (about a second per 100 MB of code). Calls through import stubs, PLT entries and GOT/IAT slots show the imported function's name. \
For DWARF: dwarf_units lists compilation units; dwarf_search finds DIEs by name; dwarf_dies lists a unit's DIEs by tag (functions, variables, types, DW_TAG_...); dwarf_die shows one DIE with all its attributes, where it is declared (with the source line when the file exists here), the lines its code came from, a struct's layout with padding, and its children; dwarf_at gives the inlined call stack, scopes and variables (with where each lives) at an address; dwarf_check lists everything in the DWARF that can't be read or doesn't add up — use it first on a customer's binary whose debug info seems wrong. DIEs are named by .debug_info offset (0x1a2b, as llvm-dwarfdump prints them), by unit:offset (3:0x44), or by name. \
For Objective-C (Mach-O apps and frameworks): objc lists the classes, categories and protocols; given a name it declares one as its header would (ivars, properties, methods with their types and implementations), or for a selector lists the methods implementing it and the functions that send it. The metadata also names a stripped binary's methods (-[Class selector]), its metadata and its selector references (@selector(name)), so those names work everywhere. \
For an original Xbox executable (XBE: a game's default.xbe): it opens like a 32-bit PE; binary_summary gives the title, whether it is a retail or debug build (by the keys its entry point is encoded with) and which XDK built it (from the library versions: XAPILIB 1.0.5849 is XDK 5849); calls into the kernel read as calls through its thunk table's slots, named after the export each ordinal is (call [__imp_KeBugCheck]), and the code is followed from the entry point and TLS callbacks, so disassemble, function_info, xrefs, call_graph, coverage and decomp_context work as for any binary. \
For game ROMs and console executables (NES, SNES, Game Boy and Game Boy Color, Game Boy Advance, Mega Drive / Genesis, Nintendo 64, PlayStation PS-X EXE): open_binary recognizes them by their headers (files of no known format open as raw bytes); banks get addresses of their own (bank 3's $C000 is 0x3c000), the hardware registers are named (PPUCTRL, LCDC, INIDISP, DISPCNT, VDP_CTRL, VI_STATUS, GP1), and the code is found by following it from the reset and interrupt vectors, so disassemble, function_info, xrefs (who writes PPUCTRL?) and call_graph work as for any binary. For their text: relative_search finds a word in the game's own encoding, and table_text reads, searches and dumps text with a table file. With emulators: code_log follows the code with an FCEUX or Mesen code/data log (what the game ran when played: code behind jump tables, where an NES game's banks were mapped), and labels imports a Mesen, FCEUX, RGBDS, WLA DX or no$gba label file into the notes, or writes the notes as one for the emulator's debugger. \
To see what grew between two builds: size_diff compares two binaries or two folders or zips (.ipa files, say) without opening them. \
For a crash: symbolicate takes an Apple .crash or .ips, an Android tombstone or a stack trace, and turns every frame into its function, source line and inlined calls with the open binaries (each image found by UUID or build ID) — open the app's folder or zip with its dSYMs first. \
To map a binary out: annotate names functions, comments addresses and marks code reviewed (names show up in disassembly and search); coverage shows how much is named, recovered, reviewed or still unexplored, with the largest unexplored gaps. Notes persist in <binary>.binviz-notes.json, which the binviz web UI can import. \
For a matching decompilation (C that compiles back to the same bytes), the loop is: next_functions says what to do next, best first — functions shaped like one already matched (its C is a template), then those whose callees are all done, cheapest for what they unlock — and claim: true takes one, so parallel agents don't collide; decomp_context gives everything for writing it in one call (code, prototype guess, callers and callees with theirs, strings, globals, and the matched functions shaped like it with their source files); match_function scores the compiled object against the original and explains each difference; mark records the outcome (matched with its source file, nonmatching, attempted with its percent, skipped, library), which re-ranks the rest, and place_report records a whole objdiff report at once. After three tries without a match, move on: the function comes back once something it calls is done. identify_sdk marks the SDK's functions as library code, which callers don't wait on. \
Addresses can be written 0x401000 (hex, also without 0x), a symbol name, name+0x10, or @0x200 for a file offset.";

const MAX_OUTPUT: usize = 60_000;

pub(crate) struct Open {
    pub id: String,
    pub path: PathBuf,
    pub label: String,
    pub bin: Binary,
    pub notes: Option<PathBuf>,
    /// The folder or zip it came from (an index into `Server::packages`).
    pub package: Option<usize>,
    /// Its debug file in the folder (file index, path), attached on first use.
    pub pending_debug: Option<(u32, String)>,
    /// What happened when the debug file was attached.
    pub debug_note: Option<String>,
    /// Whether the DWARF its debug map names has been looked for (Mach-O built without dsymutil).
    pub debug_map_tried: bool,
}

#[derive(Default)]
pub struct Server {
    pub(crate) open: Vec<Open>,
    pub(crate) current: Option<usize>,
    pub(crate) packages: Vec<crate::folders::OpenPackage>,
}

// --- Tool definitions --------------------------------------------------------

fn binary_param() -> Value {
    json!({ "type": "string", "description": "Which open binary (the id open_binary returned). Defaults to the most recently opened." })
}

fn tool(name: &str, title: &str, description: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    let mut props = props;
    if !matches!(
        name,
        "open_binary" | "list_binaries" | "symbolicate" | "size_diff" | "diff_functions"
    ) {
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
            "Loads an ELF, Mach-O, PE or XBE file (also universal/fat binaries and .a archives) and keeps it in memory for the other tools, or a folder or zip of binaries (an .ipa, an .xcarchive, a build folder; zips inside open too): every binary inside opens under its own id, and its debug file there (a dSYM, a .debug file or a PDB, paired by UUID, build ID or the name the binary records) attaches on first use. Loads notes from <path>.binviz-notes.json (<folder>.<id>.binviz-notes.json) if present. Returns ids and a summary.",
            json!({
                "path": { "type": "string", "description": "Path to the binary, or to a folder or zip of binaries." },
                "member": { "type": "string", "description": "For universal binaries or archives: the slice/member index or architecture (e.g. arm64, x86_64). Universal binaries default to arm64." },
                "debug_file": { "type": "string", "description": "Separate debug info to attach: a .dSYM's DWARF file (…/Contents/Resources/DWARF/<name>), an ELF .debug file, a PE's PDB (read as DWARF), or an unstripped copy. For a Mach-O binary linked without dsymutil, the folder holding the object files its debug map names (found by themselves when they are where they were built, or next to the binary)." },
                "notes_file": { "type": "string", "description": "Where to keep notes; defaults to <path>.binviz-notes.json." },
                "psx_exe": { "type": "string", "description": "PlayStation: the game's boot executable (PS-X EXE), whose functions are named in the file being opened when it is a memory image (2 MiB of RAM dumped by an emulator) or an overlay." },
                "overlay_at": { "type": "string", "description": "PlayStation: open the file as a code overlay loaded at this address (0x80100000)." },
                "trace": { "type": "string", "description": "PlayStation: a trace of the code an emulator ran (any text with an address per line: a CPU trace, a list of PCs); code it saw run that following the code didn't reach is followed too." },
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
            "size_diff",
            "Compare the sizes of two builds",
            "What changed in size between two builds on disk: two binaries (kinds of bytes, sections, owners — Swift modules, Objective-C classes, C++ namespaces, C prefixes — and the symbols that came, went or changed size), or two folders or zips such as two .ipa files (files added, removed and changed, kinds of content, each binary, and owners across all binaries). Paths inside the two folders are lined up even when their top folders differ.",
            json!({
                "old": { "type": "string", "description": "The earlier build: a binary, or a folder or zip." },
                "new": { "type": "string", "description": "The later build, of the same kind." },
                "top": { "type": "integer", "description": "Entries per list (default 30, max 500)." },
            }),
            &["old", "new"],
            true,
        ),
        tool(
            "diff_functions",
            "Compare two builds function by function",
            "Which functions of two builds on disk are which, the way BinDiff does it: matched by name, by identical bytes, by the same instructions (code that moved), through the call graph, and for ROMs by address; each pair identical, relocated (only addresses differ) or changed with how similar; and the functions added and removed. Works on stripped builds, and on ROM revisions or a patched copy of a game. With function, that function's instructions next to its match's.",
            json!({
                "old": { "type": "string", "description": "The earlier build (a binary or ROM)." },
                "new": { "type": "string", "description": "The later build." },
                "function": { "type": "string", "description": "A function of the earlier build (name or address): its code lined up with its match's." },
                "top": { "type": "integer", "description": "Entries per list (default 40, max 1000)." },
            }),
            &["old", "new"],
            true,
        ),
        tool(
            "symbolicate",
            "Symbolicate a crash report",
            "Turns a crash report (an Apple .crash or .ips, an Android tombstone, or a stack trace that gives images and offsets) into the function, source line and inlined calls of every frame, with the open binaries: each of the report's images is found by UUID or build ID, and binaries of another build are called out. Open the app first — its folder or zip with the dSYMs gives every binary at once.",
            json!({
                "report": { "type": "string", "description": "The crash report's text." },
                "report_file": { "type": "string", "description": "Or the path of a file holding it." },
            }),
            &[],
            true,
        ),
        tool(
            "folder_summary",
            "Summarize a folder of binaries",
            "For an opened folder or zip: the app it holds (bundle id, version, minimum OS, from its Info.plist), every binary with its kind, format, architectures and debug file, sizes by kind of content (binaries, asset catalogs, images, localization…), the largest files, duplicate files and the bytes they waste. With analyze: true, also the code and data of every binary broken down by owner (Swift modules, Objective-C classes, C++ namespaces, C prefixes) and summed across binaries, with debug files attached so stripped binaries have names.",
            json!({
                "package": { "type": "string", "description": "Which folder or zip (its id or file name). Defaults to the current binary's." },
                "analyze": { "type": "boolean" },
                "top": { "type": "integer", "description": "Entries per list (default 25, max 500)." },
            }),
            &[],
            true,
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
            "One search over addresses, file offsets (@0x…), symbols (raw and demangled), imports/exports, sections, source files and file:line, DWARF names, notes, strings, byte patterns with ?? wildcards (48 8b ?? 05) and \"exact text\". Best matches first, grouped by kind. With binary: \"all\", searches every binary of the folder (or every open binary).",
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
            "objc",
            "Objective-C classes and selectors",
            "The Objective-C metadata of a Mach-O image, which stripping keeps: without name, every class (with its superclass), category and protocol; with the name of one of them, its declaration as a header would have it (like class-dump: ivars with offsets, properties, methods with argument types and the address of each implementation); with a selector (hello, tableView:cellForRowAtIndexPath:), the methods implementing it and the functions that send it (through objc_msgSend or an objc_msgSend$ stub). Swift classes visible to Objective-C are included.",
            json!({
                "name": { "type": "string", "description": "A class (NSObject, MyApp.ViewController), a category (NSObject (Extras)), a protocol, or a selector." },
                "filter": { "type": "string", "description": "Without name: only classes, categories and protocols whose name contains this (case-insensitive)." },
                "limit": { "type": "integer", "description": "Without name: how many to list (default 300, max 5000)." },
            }),
            &[],
            true,
        ),
        tool(
            "relative_search",
            "Find text in an unknown encoding",
            "Relative search, the classic ROM hacking tool: old games store text in encodings of their own (A = $80, say, the alphabet in order but starting anywhere). Given a word the game shows (SWORD, CONTINUE), finds it by the spacing of its letters in every encoding where it occurs, with the text around each hit read that way, and gives the table (A-Z) each encoding implies, to use with table_text. Works on any open file's bytes.",
            json!({
                "word": { "type": "string", "description": "A word of three letters or more, in one case, without spaces or punctuation: text the game displays." },
                "width": { "type": "integer", "description": "Bytes per character: 1 (default) or 2 (little-endian)." },
            }),
            &["word"],
            true,
        ),
        tool(
            "code_log",
            "Follow a ROM's code with an emulator's log",
            "Reads the open game ROM again with a code/data log, which FCEUX's or Mesen's code/data logger writes while the game is played (.cdl): the code the game ran is followed too (code reached only through jump tables and pointers), bytes it only read as data are never taken for code, the 65816's register widths and ARM or Thumb are as they were when each instruction ran, and an NES game's switched banks are placed where they ran (MMC3's 8 KiB pages). Notes are kept. NES (FCEUX, Mesen), SNES, Game Boy and Game Boy Advance (Mesen 2).",
            json!({ "path": { "type": "string", "description": "The .cdl file." } }),
            &["path"],
            false,
        ),
        tool(
            "patch",
            "What a patch changes",
            "Applies an IPS, UPS or BPS patch (a ROM hack's, a translation's) to the open file and says what it changes: its format and checks (whether it was made for this file, by CRC-32), and each run of changed bytes placed in its bank or section, address, function and region; `out` writes the patched file (to open_binary next). Or, with `target` (a modified copy of the open file) and `out`, writes the patch that turns the open file into it (.ips, .ups or .bps by out's extension).",
            json!({
                "apply": { "type": "string", "description": "The patch file to apply." },
                "target": { "type": "string", "description": "Or a modified copy of the open file, to make the patch for." },
                "out": { "type": "string", "description": "Where to write the patched file (with apply), or the patch (with target: .ips, .ups or .bps)." },
                "limit": { "type": "integer", "description": "Changes to list (default 200, max 5000)." },
            }),
            &[],
            false,
        ),
        tool(
            "labels",
            "Emulator label files",
            "Imports an emulator's label file into the notes (Mesen's .mlb, FCEUX's .nl, a .sym from RGBDS, WLA DX or no$gba: names and comments placed at binviz's addresses, banks included), or writes the notes as one for the emulator's debugger: Mesen .mlb, FCEUX .nl (one file per bank, next to the ROM), .sym (RGBDS for the Game Boy, WLA DX for the SNES), no$gba .sym.",
            json!({
                "import": { "type": "string", "description": "The label file to import." },
                "export": { "type": "string", "enum": ["mlb", "nl", "sym", "nocash"], "description": "Or the format to write the notes in." },
                "to": { "type": "string", "description": "Where to write (default: next to the ROM, named after it as the emulator expects)." },
            }),
            &[],
            false,
        ),
        tool(
            "table_text",
            "Read text with a table file",
            "Reads the open file's text through a table file (.tbl: one `hex=text` line per entry, like 80=A or 8A20=the ; `/FF=<end>` for what ends a string; `*FE` for a line break), as relative_search suggests or a ROM hacking community publishes: every string (the default), the text at an offset, or where some text is.",
            json!({
                "table": { "type": "string", "description": "The table's lines." },
                "table_file": { "type": "string", "description": "Or the path of a .tbl file." },
                "at": { "type": "string", "description": "Read at this file offset (0x-prefixed hex, or decimal)." },
                "length": { "type": "integer", "description": "Bytes to read at `at` (default 256)." },
                "find": { "type": "string", "description": "Where this text is (with [XX] for a raw byte)." },
                "min": { "type": "integer", "description": "When dumping: entries a string needs, at least (default 4)." },
                "limit": { "type": "integer", "description": "At most this many strings or places (default 300, max 5000)." },
            }),
            &[],
            true,
        ),
        tool(
            "dwarf_units",
            "DWARF compilation units",
            "The compilation units in the DWARF: source file, language, producer (compiler and flags), DWARF version, size, and how much code each covers.",
            json!({
                "filter": { "type": "string", "description": "Only units whose name, producer or language contains this." },
                "offset": { "type": "integer" },
                "limit": { "type": "integer", "description": "Default 100, max 5000." },
            }),
            &[],
            true,
        ),
        tool(
            "dwarf_search",
            "Find DIEs by name",
            "Named DIEs by (qualified) name: functions with code, global variables, types, members, enumerators, best matches first. With everything: true, every DIE with a matching name, locals and parameters included (slower).",
            json!({
                "query": { "type": "string" },
                "everything": { "type": "boolean" },
                "limit": { "type": "integer", "description": "Default 50, max 1000." },
            }),
            &["query"],
            true,
        ),
        tool(
            "dwarf_dies",
            "List a unit's DIEs",
            "A compilation unit's DIEs, flattened, filtered by tag and name, each with its enclosing scopes. Without tags, also counts the unit's DIEs by tag.",
            json!({
                "unit": { "type": "integer" },
                "tags": { "type": "string", "description": "Comma-separated tags (DW_TAG_member or member) or kinds: functions, variables, types, scopes." },
                "name": { "type": "string", "description": "Only DIEs whose name contains this." },
                "offset": { "type": "integer" },
                "limit": { "type": "integer", "description": "Default 100, max 2000." },
            }),
            &["unit"],
            true,
        ),
        tool(
            "dwarf_die",
            "Show a DIE",
            "One DIE in full: its parents, every attribute (name, form, decoded value), type, declaration (with the source line when the file exists on this machine), inlined call site, the source lines its code came from, a structure's layout with holes and padding, and its children.",
            json!({
                "die": { "type": "string", "description": "A .debug_info offset (0x1a2b or <0x1a2b>), unit:offset (3:0x44), or a name to search for." },
                "children": { "type": "integer", "description": "How many children to list (default 50, max 1000)." },
            }),
            &["die"],
            true,
        ),
        tool(
            "dwarf_at",
            "DWARF at an address",
            "What the debug info says about an address: the source line, the inlined call stack, and the scopes around it (function, inlined calls, blocks) with the parameters and variables they declare and where each one's value lives at that address (register, frame offset, constant, or optimized out).",
            json!({ "at": address("An address in code") }),
            &["at"],
            true,
        ),
        tool(
            "dwarf_check",
            "Check the DWARF",
            "Reads every unit, DIE, attribute and line program and lists what can't be read (bad headers, abbreviations, strings, range and location lists, line programs) or doesn't add up (references to no DIE, ranges ending before they start, files the line table doesn't define), with the unit, DIE and section offset of each.",
            json!({ "limit": { "type": "integer", "description": "Problems to list (default 100, max 2000)." } }),
            &[],
            true,
        ),
        tool(
            "decomp_context",
            "Everything needed to decompile a function",
            "One call with what writing a function's C needs: its code with names resolved, what its code says about its prototype (arguments in registers and on the stack, whether it returns, frame, saved registers, the structures it walks), its callers and callees with their prototypes, the strings and globals it touches, and the notes on it. MIPS (PlayStation, Nintendo 64) for the prototype; the rest for any binary.",
            json!({
                "at": address("A function or address inside it"),
                "limit": { "type": "integer", "description": "Instructions, and entries per list (default 400, max 5000)." },
            }),
            &["at"],
            true,
        ),
        tool(
            "function_signature",
            "A function's prototype, from its code",
            "What a MIPS function's code says about how it is called: which of $a0-$a3 it reads before writing, arguments taken from the stack, whether $v0 carries a result, its frame size and saved registers, whether it calls anything, and the offsets it loads and stores off each base register (structure layout hints).",
            json!({ "at": address("A function or address inside it") }),
            &["at"],
            true,
        ),
        tool(
            "match_function",
            "Score a rebuilt function against the original",
            "Compares a function in the compiler's object file (ELF, MIPS) with the original's: instructions lined up, the fields the linker fills in masked (call targets, address halves), each relocation checked against where the original points, and every difference explained (registers allocated differently, a stack frame or slot of another size, a branch of another length, reordered instructions, a nop missing from a delay slot). Returns the percent matched and the lined-up code.",
            json!({
                "object": { "type": "string", "description": "Path to the compiled object file (.o)." },
                "symbol": { "type": "string", "description": "The function's name in the object file." },
                "at": address("The original's function; defaults to the one named like the symbol"),
            }),
            &["object", "symbol"],
            true,
        ),
        tool(
            "match_object",
            "Score every function of an object file",
            "Every function of the compiler's object file (ELF, MIPS) that the original names, scored against it, worst first, with the kinds of difference in each. The project's progress in one call.",
            json!({
                "object": { "type": "string", "description": "Path to the compiled object file (.o)." },
                "limit": { "type": "integer", "description": "Functions to list (default 50, max 1000)." },
            }),
            &["object"],
            true,
        ),
        tool(
            "place_report",
            "Place an objdiff report on the binary",
            "Reads objdiff's report JSON (what decomp.dev shows) and places its per-function verdicts on this binary's functions, by the virtual address the report records or by name: the totals, each function's match percent, and the entries no function was found for. Records them in the notes too (unless record is false), as mark would: functions at 100% matched, the others' best percent kept, and those that matched before and no longer do sent back to be done, so next_functions works from what actually compiles.",
            json!({
                "report": { "type": "string", "description": "Path to the report JSON (objdiff-cli report generate)." },
                "below": { "type": "number", "description": "List only functions matched below this percent (default 100: the unfinished ones)." },
                "limit": { "type": "integer", "description": "Functions to list (default 100, max 5000)." },
                "record": { "type": "boolean", "description": "Record the verdicts in the notes (default true)." },
            }),
            &["report"],
            false,
        ),
        tool(
            "splat_export",
            "Write a splat config and symbol file",
            "For a PlayStation executable: a splat YAML config (header, the code segment at its load address split into asm units at the given addresses, the bytes after the last function as data, the BSS size) and symbol_addrs.txt naming every function and known place, the starting point of a decompilation project. Written to a folder, or returned.",
            json!({
                "name": { "type": "string", "description": "The project's name (the basename splat uses)." },
                "out_dir": { "type": "string", "description": "Folder to write <name>.yaml and symbol_addrs.txt to; omitted, both are returned." },
                "splits": { "type": "array", "items": { "type": "string" }, "description": "Addresses where new units start." },
            }),
            &["name"],
            false,
        ),
        tool(
            "import_symbol_addrs",
            "Read a splat symbol file into the notes",
            "Names from a splat symbol file (symbol_addrs.txt, undefined_funcs_auto.txt, undefined_syms_auto.txt) become notes here, so the project's names show in disassembly and search. splat's own made-up names (func_80010000, D_8001ABCD) are left out.",
            json!({ "path": { "type": "string", "description": "Path to the symbol file." } }),
            &["path"],
            false,
        ),
        tool(
            "identify_sdk",
            "Find the Psy-Q SDK's functions",
            "PlayStation: matches the binary's functions against the signatures of Sony's SDK libraries (.LIB/.OBJ files from a Psy-Q installation, given as files or a folder), naming the SDK's code (GsSortObject4, CdRead, SpuSetKey…) and saying which libraries the game was linked with. With apply, the matches become notes with an `sdk:` comment, so a decompilation can leave them be.",
            json!({
                "paths": { "type": "array", "items": { "type": "string" }, "description": "Library and object files, or folders of them." },
                "apply": { "type": "boolean", "description": "Name the matched functions in the notes (default false: report only)." },
                "limit": { "type": "integer", "description": "Matches to list (default 200, max 5000)." },
            }),
            &["paths"],
            false,
        ),
        tool(
            "propose_names",
            "Propose names from another build",
            "Names from another build of the game (a port with its source, a symbolized build): a JSON list of its functions, each with the string literals it uses and the functions it calls ([{\"name\": \"InitField\", \"strings\": [\"field.bin\"], \"calls\": [\"LoadFile\"]}, …]). Functions here using the same strings are proposed as the same functions, then their neighbours through the calls, each with a confidence and the evidence. With apply, proposals at or above min_confidence become notes.",
            json!({
                "path": { "type": "string", "description": "Path to the candidates JSON." },
                "apply": { "type": "boolean", "description": "Name the functions in the notes (default false: report only)." },
                "min_confidence": { "type": "number", "description": "0 to 1; proposals below it aren't applied (default 0.6)." },
            }),
            &["path"],
            false,
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
            "How much of the code and data is mapped out: bytes that are reviewed, annotated, named (symbols/DWARF), format structure, recovered (functions from unwind info / function starts or found by following the code, strings), padding, or unexplored — per section — plus the largest unexplored gaps with a guess at what they hold.",
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
        tool(
            "next_functions",
            "Pick functions to decompile",
            "For a matching decompilation: the functions to write C for next, best first, each with why. Functions shaped like one already done come first (its C is a template: see similar_functions), then ready ones (everything they call is matched, nonmatching or library code), cheapest for what they unlock first (small functions that many callers wait on), then those still waiting on callees, and last those tried three times without matching (until something they call gets done). Leaves out what is done, set aside, or claimed by another agent in the last hour. claim: true claims the first one for you, so parallel agents don't take the same function. Also says how far the decompilation has come. Record each outcome with mark.",
            json!({
                "count": { "type": "integer", "description": "How many to list (default 10, max 200)." },
                "within": { "type": "string", "description": "Only functions starting in this range of addresses: lo..hi (a source file's, say)." },
                "claim": { "type": "boolean", "description": "Claim the first function listed (in progress, for agent)." },
                "agent": { "type": "string", "description": "Your name, for claims (default \"agent\")." },
                "include": { "type": "string", "enum": ["claimed", "skipped", "all"], "description": "Also list what others have claimed, what was set aside, or both." },
            }),
            &[],
            false,
        ),
        tool(
            "mark",
            "Record where decompiling a function stands",
            "Records a function's state in the notes: in-progress (you are working on it: claims it), attempted (a try that didn't match: counted, the best percent kept, handed back), matched (its C compiles to the same bytes; pass source, the file the C is in), nonmatching (equivalent C that doesn't match byte for byte), skipped (set aside for later), library (SDK or runtime code, not decompiled), or todo (cleared). next_functions ranks by these, and done functions become worked examples for the ones shaped like them. Says which callers a match makes ready.",
            json!({
                "at": address("The function (any address in it)"),
                "state": { "type": "string", "enum": ["matched", "nonmatching", "attempted", "in-progress", "skipped", "library", "todo"] },
                "percent": { "type": "number", "description": "How much of it matched (0-100), as objdiff or your compare says." },
                "source": { "type": "string", "description": "The source file its C is in." },
                "agent": { "type": "string", "description": "Your name, for in-progress (default \"agent\")." },
            }),
            &["at", "state"],
            false,
        ),
        tool(
            "similar_functions",
            "Functions shaped like one",
            "The functions whose instructions are most like a function's (the same operations on the same kinds of operands, whatever addresses and numbers they hold), most alike first, with where each stands in the decompilation. A matched one's C is the best worked example for writing this one; a near-copy of a function just matched likely takes the same C.",
            json!({
                "at": address("The function (any address in it)"),
                "count": { "type": "integer", "description": "How many (default 8, max 50)." },
                "done_only": { "type": "boolean", "description": "Only functions already matched or nonmatching." },
            }),
            &["at"],
            true,
        ),
    ]
}

// --- Formatting ------------------------------------------------------------------

pub(crate) fn human(n: u64) -> String {
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

pub(crate) fn count(n: impl Into<u64>) -> String {
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

pub(crate) fn pct(part: u64, whole: u64) -> String {
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

pub(crate) fn clip(s: &str, max: usize) -> String {
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

pub(crate) fn int(args: &Value, key: &str, default: u64, max: u64) -> u64 {
    args.get(key).and_then(Value::as_u64).unwrap_or(default).min(max)
}

pub(crate) fn string<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
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
    // A ROM's bank:address (03:C000).
    if let Some(a) = bin.rom_address(t) {
        return Ok(Loc::Address(a));
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

pub(crate) fn address_of(bin: &Binary, text: &str) -> Result<u64, String> {
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
            "folder_summary" => self.folder_summary(args).map(finish),
            "symbolicate" => self.symbolicate(args).map(finish),
            "size_diff" => self.size_diff(args).map(finish),
            "diff_functions" => self.diff_functions(args).map(finish),
            "search" if string(args, "binary") == Some("all") => self.search_all(args).map(finish),
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
                    "next_functions" => crate::queue::next_functions(o, args)?,
                    "mark" => crate::queue::mark(o, args)?,
                    "similar_functions" => crate::queue::similar_functions(o, args)?,
                    "function_info" => function_info(o, args)?,
                    "xrefs" => xrefs(o, args)?,
                    "callers" => call_list(o, args, true)?,
                    "callees" => call_list(o, args, false)?,
                    "call_graph" => call_graph(o, args)?,
                    "call_path" => call_path(o, args)?,
                    "objc" => objc(o, args)?,
                    "relative_search" => relative_search(o, args)?,
                    "table_text" => table_text(o, args)?,
                    "code_log" => code_log(o, args)?,
                    "patch" => patch(o, args)?,
                    "labels" => labels(o, args)?,
                    "dwarf_units" => dwarf_units(o, args)?,
                    "dwarf_search" => dwarf_search(o, args)?,
                    "dwarf_dies" => dwarf_dies(o, args)?,
                    "dwarf_die" => dwarf_die(o, args)?,
                    "dwarf_at" => dwarf_at(o, args)?,
                    "dwarf_check" => dwarf_check(o, args)?,
                    "decomp_context" => decomp_context(o, args)?,
                    "function_signature" => function_signature(o, args)?,
                    "match_function" => match_function(o, args)?,
                    "match_object" => match_object(o, args)?,
                    "place_report" => place_report(o, args)?,
                    "splat_export" => splat_export(o, args)?,
                    "import_symbol_addrs" => import_symbol_addrs(o, args)?,
                    "identify_sdk" => identify_sdk(o, args)?,
                    "propose_names" => propose_names(o, args)?,
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
        self.attach_pending(i);
        Ok(&mut self.open[i])
    }

    fn ids(&self) -> String {
        self.open.iter().map(|o| o.id.as_str()).collect::<Vec<_>>().join(", ")
    }

    fn open_binary(&mut self, args: &Value) -> Result<String, String> {
        let path = PathBuf::from(string(args, "path").ok_or("path is required")?);
        if string(args, "member").is_none() && binviz::package::is_package_path(&path) {
            return self.open_package(&path, args).map(finish);
        }
        let started = Instant::now();
        let data = binviz::read_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file_name = path
            .file_name()
            .map_or_else(|| "binary".into(), |n| n.to_string_lossy().into_owned());
        let mut note = String::new();
        let psx_exe = match string(args, "psx_exe") {
            Some(p) => {
                let bytes = binviz::read_file(Path::new(p)).map_err(|e| format!("{p}: {e}"))?;
                Some(Binary::parse(bytes).map_err(|e| format!("{p}: {e}"))?)
            }
            None => None,
        };
        let (bin, label) = if let Some(at) = string(args, "overlay_at") {
            let at = parse_number(at).ok_or_else(|| format!("overlay_at: not an address: {at}"))?;
            (
                Binary::parse_psx_overlay(data, at, psx_exe.as_ref()).map_err(|e| e.to_string())?,
                file_name.clone(),
            )
        } else if let Some(exe) = &psx_exe {
            (
                Binary::parse_psx_memory(data, Some(exe)).map_err(|e| e.to_string())?,
                file_name.clone(),
            )
        } else if Container::is_container(&data) {
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
        let bin = match string(args, "trace") {
            Some(t) => {
                let text = std::fs::read_to_string(t).map_err(|e| format!("{t}: {e}"))?;
                let (traced, s) = bin.with_psx_trace(&text).map_err(|e| format!("{t}: {e}"))?;
                let _ = writeln!(
                    note,
                    "Trace: {} lines, {} addresses, {} in this image, {} new runs of code followed.",
                    s.lines, s.addresses, s.placed, s.new_runs
                );
                traced
            }
            None => bin,
        };
        let mut open = Open {
            id: String::new(),
            path: path.clone(),
            label,
            bin,
            notes: None,
            package: None,
            pending_debug: None,
            debug_note: None,
            debug_map_tried: true,
        };
        let mut folders = Vec::new();
        if let Some(debug) = string(args, "debug_file") {
            if Path::new(debug).is_dir() {
                folders.push(PathBuf::from(debug));
            } else {
                let data = binviz::read_file(debug).map_err(|e| format!("{debug}: {e}"))?;
                open.bin
                    .attach_debug_file(debug, data)
                    .map_err(|e| format!("{debug}: {e}"))?;
            }
        }
        // Built without dsymutil, a Mach-O binary's DWARF is in the objects its debug map names.
        if open.bin.debug_info().is_none() && !open.bin.debug_map().is_empty() {
            if let Some(dir) = path.parent() {
                folders.push(dir.to_path_buf());
            }
            let _ = writeln!(
                note,
                "{}",
                debug_map_note(open.bin.attach_debug_map_from_disk(&folders))
            );
        }
        // Swift names read better through `swift-demangle`, where it is installed.
        open.bin.demangle_swift_with_tool();
        // Notes: <path>.binviz-notes.json unless told otherwise.
        let notes_path = string(args, "notes_file")
            .map(PathBuf::from)
            .unwrap_or_else(|| sidecar(&path, open.label.split(" [").nth(1).map(|s| s.trim_end_matches(']'))));
        match load_notes(&mut open, &notes_path) {
            Some(loaded) => note.push_str(&loaded),
            None => {
                let _ = writeln!(note, "Notes will be saved to {}", notes_path.display());
            }
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
            let package = o
                .package
                .and_then(|p| self.packages.get(p))
                .map(|p| format!(" [in {}]", p.id))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "{} `{}`{package} {} — {} {} {}, {} symbols{}{}",
                if Some(i) == self.current { "*" } else { " " },
                o.id,
                o.label,
                s.format_name,
                s.arch,
                human(s.file_size),
                count(s.symbol_count),
                if s.has_dwarf { ", DWARF" } else { "" },
                if o.pending_debug.is_some() {
                    ", debug file attaches on first use"
                } else {
                    ""
                }
            );
        }
        out
    }

    fn close_binary(&mut self, args: &Value) -> Result<String, String> {
        let id = self.get(args)?.id.clone();
        let i = self.open.iter().position(|o| o.id == id).expect("found above");
        if let Some(p) = self.open[i].package {
            for slot in &mut self.packages[p].ids {
                if slot.as_deref() == Some(id.as_str()) {
                    *slot = None;
                }
            }
        }
        self.open.remove(i);
        self.current = if self.open.is_empty() {
            None
        } else {
            Some(self.open.len() - 1)
        };
        Ok(format!("Closed `{id}`."))
    }
}

/// Loads notes into a binary; what it loaded, or `None` if there are none yet.
pub(crate) fn load_notes(open: &mut Open, notes_path: &Path) -> Option<String> {
    if !notes_path.exists() {
        return None;
    }
    Some(match notes::load(notes_path) {
        Ok((list, fingerprint)) => {
            let n = list.len();
            open.bin.set_annotations(list);
            let mut note = format!("Loaded {n} notes from {}", notes_path.display());
            if fingerprint.is_some_and(|f| f != open.bin.summary().fingerprint) {
                note.push_str(" (they were saved for a different build of this file: check they still line up)");
            }
            note.push('\n');
            note
        }
        Err(e) => format!("Could not read notes: {e}\n"),
    })
}

/// `<path>.binviz-notes.json`, or `<path>.<arch>.binviz-notes.json` for a slice.
pub(crate) fn sidecar(path: &Path, member: Option<&str>) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    if let Some(m) = member {
        name.push(format!(".{m}"));
    }
    name.push(".binviz-notes.json");
    path.with_file_name(name)
}

// --- Tools -------------------------------------------------------------------------

fn relative_search(o: &Open, args: &Value) -> Result<String, String> {
    let word = string(args, "word").ok_or("word is required")?;
    let width = int(args, "width", 1, 2) as u32;
    let found = binviz::tables::relative_search(o.bin.data(), word, width, 30)?;
    let mut out = found.to_text();
    if let Some(e) = found.encodings.first() {
        let _ = writeln!(
            out,
            "\nThe table the first implies (add the other characters you see, then pass it to table_text):\n{}",
            binviz::tables::Table::from_alphabet(e.first, e.letter, found.width).to_tbl()
        );
    }
    Ok(out)
}

fn table_text(o: &Open, args: &Value) -> Result<String, String> {
    let text = match (string(args, "table"), string(args, "table_file")) {
        (Some(t), _) => t.to_string(),
        (None, Some(path)) => std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?,
        (None, None) => return Err("table (its lines) or table_file (a .tbl path) is required".into()),
    };
    let table = binviz::tables::Table::parse(&text)?;
    let data = o.bin.data();
    let limit = int(args, "limit", 300, 5000) as usize;
    let show = |s: &str| s.replace('\n', "⏎");
    let mut out = String::new();
    if let Some(at) = string(args, "at") {
        let offset = parse_number(at).ok_or_else(|| format!("{at:?} is not an offset"))? as usize;
        let len = int(args, "length", 256, 1 << 20) as usize;
        let bytes = data
            .get(offset..(offset + len).min(data.len()))
            .ok_or_else(|| format!("{offset:#x} is past the end of the file"))?;
        let _ = writeln!(out, "{}", table.decode(bytes, false).text);
    } else if let Some(find) = string(args, "find") {
        let hits = table.find(data, find, limit)?;
        let _ = writeln!(out, "{} places:", hits.len());
        for at in hits {
            let bytes = &data[at as usize..(at as usize + 96).min(data.len())];
            let _ = writeln!(out, "  {at:#08x}  {}", show(&table.decode(bytes, true).text));
        }
    } else {
        let min = int(args, "min", 4, 1000) as usize;
        let found = table.strings(data, min, limit);
        let _ = writeln!(out, "{} strings of {min} entries or more:", found.len());
        for s in found {
            let _ = writeln!(out, "  {:#08x}  {}", s.offset, show(&s.text));
        }
    }
    Ok(out)
}

/// `0x1f`, `1f` (hex) or `31`.
fn parse_number(s: &str) -> Option<u64> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

fn objc(o: &Open, args: &Value) -> Result<String, String> {
    let b = &o.bin;
    let info = b.objc();
    if info.is_empty() {
        return Ok(format!("{}: no Objective-C metadata", o.label));
    }
    if let Some(name) = string(args, "name") {
        if let Some(i) = info.interface(name) {
            return Ok(i.to_text());
        }
        if let Some(uses) = b.objc_selector(name) {
            return Ok(uses.to_text());
        }
        let similar: Vec<String> = info
            .entries()
            .into_iter()
            .filter(|e| e.name.to_lowercase().contains(&name.to_lowercase()))
            .take(20)
            .map(|e| e.name)
            .collect();
        return Err(if similar.is_empty() {
            format!("no class, category, protocol or selector named {name}")
        } else {
            format!("no {name}; similar: {}", similar.join(", "))
        });
    }
    let limit = int(args, "limit", 300, 5000) as usize;
    let text = info.to_text(usize::MAX);
    let Some(filter) = string(args, "filter").map(str::to_lowercase) else {
        return Ok(clip_lines(&text, limit + 1));
    };
    // The counts, then the matching lines under their headings.
    let mut out = String::new();
    let mut heading = "";
    let mut shown = 0;
    for (i, line) in text.lines().enumerate() {
        if i == 0 {
            let _ = writeln!(out, "{line}");
        } else if !line.starts_with(' ') {
            heading = line;
        } else if line.to_lowercase().contains(&filter) && shown < limit {
            if !heading.is_empty() {
                let _ = writeln!(out, "{heading}");
                heading = "";
            }
            let _ = writeln!(out, "{line}");
            shown += 1;
        }
    }
    if shown == 0 {
        let _ = writeln!(out, "nothing matches {filter:?}");
    }
    Ok(out)
}

/// The first `n` lines of `text`, and how many more there are.
fn clip_lines(text: &str, n: usize) -> String {
    let total = text.lines().count();
    let mut out: String = text.lines().take(n).map(|l| format!("{l}\n")).collect();
    if total > n {
        let _ = writeln!(out, "… {} more lines (use filter or limit)", total - n);
    }
    out
}

/// How linking a debug map's DWARF went, for a note.
pub(crate) fn debug_map_note(result: binviz::Result<binviz::dwarf::debugmap::DebugMapReport>) -> String {
    match result {
        Ok(r) => {
            let mut text = r.to_text();
            for m in r.missing.iter().take(5) {
                text.push_str(&format!("\n  not found: {m}"));
            }
            for f in r.failed.iter().take(5) {
                text.push_str(&format!("\n  not used: {f}"));
            }
            text
        }
        Err(e) => format!("debug map: {e} (pass the objects' folder as debug_file)"),
    }
}

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
    let objc = b.objc();
    if !objc.is_empty() {
        let _ = writeln!(out, "Objective-C: {} (see objc)", objc.counts().to_text());
    }
    if !b.imports().is_empty() || !b.exports().is_empty() {
        let _ = writeln!(
            out,
            "Imports: {} · Exports: {}",
            count(b.imports().len() as u64),
            count(b.exports().len() as u64)
        );
    }
    if let Some(n) = &o.debug_note {
        let _ = writeln!(out, "{n}");
    }
    let notes = b.annotations();
    if notes.iter().any(|a| a.is_note()) {
        let _ = writeln!(
            out,
            "Notes: {} ({} reviewed)",
            notes.iter().filter(|a| a.is_note()).count(),
            notes.iter().filter(|a| a.reviewed).count()
        );
    }
    if notes.iter().any(|a| a.decomp.is_some()) {
        let p = b.decomp_progress();
        let _ = writeln!(
            out,
            "Decompilation: {} of {} functions matched, {} nonmatching, {} library (next_functions picks what to do)",
            count(p.matched),
            count(p.functions),
            count(p.nonmatching),
            count(p.library)
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
                match s.format {
                    binviz::Format::MachO => " (for Mach-O it usually lives in a .dSYM: pass its DWARF file as debug_file; without one, in the object files the debug map names: pass their folder)".to_string(),
                    binviz::Format::Pe => pdb_hint(s),
                    _ => String::new(),
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

pub(crate) fn search(o: &Open, args: &Value) -> Result<String, String> {
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
    if let Some(a) = i.annotation.as_ref().filter(|a| a.is_note()) {
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
    if let Some(d) = i
        .address
        .and_then(|a| o.bin.symbols().function_containing(a))
        .and_then(|f| o.bin.decomp_at(f.address))
    {
        let _ = writeln!(out, "Decompilation: {}", crate::queue::state_text(d));
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
    if let Some(dec) = o.bin.decomp_at(d.start) {
        let _ = writeln!(out, "; decompilation: {}", crate::queue::state_text(dec));
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
    if o.bin.annotations().iter().any(|a| a.decomp.is_some()) {
        let p = o.bin.decomp_progress();
        let _ = writeln!(
            out,
            "Decompilation: {} of {} functions matched ({} of {} of code, library code aside), {} nonmatching, {} library, {} in progress, {} set aside (next_functions picks what to do)",
            count(p.matched),
            count(p.functions),
            human(p.matched_bytes),
            human(p.bytes.saturating_sub(p.library_bytes)),
            count(p.nonmatching),
            count(p.library),
            count(p.in_progress),
            count(p.skipped)
        );
    }
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

fn code_log(o: &mut Open, args: &Value) -> Result<String, String> {
    let path = string(args, "path").ok_or("path is required")?;
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let notes = o.bin.annotations().to_vec();
    let (mut bin, s) = o.bin.with_code_log(&bytes).map_err(|e| format!("{path}: {e}"))?;
    bin.set_annotations(notes);
    o.bin = bin;
    let mut out = format!(
        "Read with a {} code/data log: {} bytes of code and {} of data seen, of {} ({} both); {} subroutine starts and {} jump targets marked.",
        s.format.name(),
        count(s.code),
        count(s.data),
        count(s.bytes),
        count(s.both),
        count(s.entries),
        count(s.jumps)
    );
    if s.pages_placed > 0 {
        out.push_str(&format!(" {} PRG pages placed where they ran.", s.pages_placed));
    }
    if s.crc_matches == Some(false) {
        out.push_str(" The log was made for another version of the ROM (its CRC-32 differs): treat it with care.");
    }
    let found = o
        .bin
        .summary()
        .properties
        .iter()
        .find(|p| p.key == "Code found")
        .map(|p| p.value.clone());
    if let Some(f) = found {
        out.push_str(&format!("\nCode found: {f}."));
    }
    Ok(out)
}

fn patch(o: &mut Open, args: &Value) -> Result<String, String> {
    use binviz::patch::{ChangeKind, PatchFormat};
    let data = o.bin.data();
    if let Some(target) = string(args, "target") {
        let out = string(args, "out").ok_or("out is required with target (where to write the patch)")?;
        let format = Path::new(out)
            .extension()
            .and_then(|e| PatchFormat::from_name(&e.to_string_lossy()))
            .ok_or("name the patch .ips, .ups or .bps")?;
        let modified = std::fs::read(target).map_err(|e| format!("{target}: {e}"))?;
        let bytes = binviz::patch::create(format, data, &modified).map_err(|e| e.to_string())?;
        std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
        return Ok(format!(
            "Wrote {out}: a {} patch of {} bytes, {} runs of changes.",
            format.name(),
            bytes.len(),
            binviz::patch::changes(data, &modified).len()
        ));
    }
    let path = string(args, "apply").ok_or("apply (a patch file) or target (a modified file) is required")?;
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let applied = binviz::patch::apply(&bytes, data).map_err(|e| format!("{path}: {e}"))?;
    let i = &applied.info;
    let mut out = format!(
        "{} patch: {} records; makes {} bytes. ",
        i.format.name(),
        count(i.records),
        count(i.target_size)
    );
    out.push_str(match (applied.source_matches, applied.target_matches) {
        (Some(true), Some(true)) => "Made for this file, and the result is as promised.",
        (Some(false), _) => "Made for ANOTHER file (the CRC-32 differs): the result is likely wrong.",
        (Some(true), Some(false)) => "The result isn't what the patch promises.",
        _ => "IPS patches carry no checksums.",
    });
    if let Some(m) = &i.metadata {
        out.push_str(&format!("\nMetadata: {m}"));
    }
    for w in &applied.warnings {
        out.push_str(&format!("\nWarning: {w}"));
    }
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(200).min(5000) as usize;
    let placed = binviz::patch::place(&o.bin, &applied.changes[..applied.changes.len().min(limit)]);
    out.push_str(&format!(
        "\n{} {}, {} {} differ{}:",
        count(applied.changes.len() as u64),
        if applied.changes.len() == 1 {
            "run of changes"
        } else {
            "runs of changes"
        },
        count(applied.differ),
        if applied.differ == 1 { "byte" } else { "bytes" },
        if applied.changes.len() > limit {
            format!(" (the first {limit})")
        } else {
            String::new()
        }
    ));
    for p in &placed {
        let c = &p.change;
        let kind = match c.kind {
            ChangeKind::Changed => "",
            ChangeKind::Added => " added",
            ChangeKind::Removed => " removed",
        };
        out.push_str(&format!(
            "\n  {:#x} ({} bytes{kind}) {}{}{}{}",
            c.offset,
            c.len,
            p.section.as_deref().unwrap_or("-"),
            p.address.map(|a| format!(" {a:#x}")).unwrap_or_default(),
            p.function.as_ref().map(|f| format!(" in {f}")).unwrap_or_default(),
            if p.region.is_empty() {
                String::new()
            } else {
                format!(" [{}]", p.region.join(" > "))
            }
        ));
    }
    if let Some(dest) = string(args, "out") {
        std::fs::write(dest, &applied.output).map_err(|e| format!("{dest}: {e}"))?;
        out.push_str(&format!(
            "\nWrote the patched file to {dest}: open_binary it to explore it."
        ));
    }
    Ok(out)
}

fn labels(o: &mut Open, args: &Value) -> Result<String, String> {
    use binviz::rom::labels::LabelFormat;
    if let Some(path) = string(args, "import") {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let read = o.bin.read_labels(path, &text).map_err(|e| e.to_string())?;
        let mut list = o.bin.annotations().to_vec();
        let (mut added, mut updated) = (0, 0);
        for l in read.labels {
            match list.iter_mut().find(|a| a.address == l.address && a.size == l.size) {
                Some(a) => {
                    if !l.name.is_empty() {
                        a.name = l.name;
                    }
                    if !l.comment.is_empty() {
                        a.comment = l.comment;
                    }
                    updated += 1;
                }
                None => {
                    list.push(l);
                    added += 1;
                }
            }
        }
        o.bin.set_annotations(list);
        let mut out = format!(
            "From a {} file: {added} notes added, {updated} updated",
            read.format.name()
        );
        if read.skipped > 0 {
            out.push_str(&format!("; {} lines skipped (places binviz can't place: a switched window's address without its bank, memory it doesn't model)", read.skipped));
        }
        if read.directives > 0 {
            out.push_str(&format!(
                "; {} directives (.arm, .thumb, data markers) not imported",
                read.directives
            ));
        }
        out.push_str(&format!(". Notes {}.", save_notes(o)));
        return Ok(out);
    }
    let format = string(args, "export").ok_or("import (a label file) or export (a format) is required")?;
    let format = LabelFormat::from_name(format)
        .ok_or_else(|| format!("unknown label format {format}: mlb, nl, sym or nocash"))?;
    let files = o.bin.write_labels(format).map_err(|e| e.to_string())?;
    let rom = o.path.to_string_lossy().to_string();
    let mut written = Vec::new();
    for f in &files {
        let target = match string(args, "to") {
            Some(to) if files.len() == 1 => PathBuf::from(to),
            Some(to) => PathBuf::from(to).join(format!(
                "{}.{}",
                o.path
                    .file_name()
                    .map_or("rom".into(), |n| n.to_string_lossy().to_string()),
                f.suffix
            )),
            // FCEUX's name lists take the ROM's whole name (game.nes.0.nl); the others replace its extension.
            None if format == LabelFormat::Nl => PathBuf::from(format!("{rom}.{}", f.suffix)),
            None => o.path.with_extension(&f.suffix),
        };
        std::fs::write(&target, &f.text).map_err(|e| format!("{}: {e}", target.display()))?;
        written.push(target.display().to_string());
    }
    Ok(format!(
        "Wrote {} notes as {}: {}",
        o.bin
            .annotations()
            .iter()
            .filter(|a| !a.name.is_empty() || !a.comment.is_empty())
            .count(),
        format.name(),
        written.join(", ")
    ))
}

pub(crate) fn save_notes(o: &Open) -> String {
    match &o.notes {
        Some(path) => match notes::save(path, &o.label, &o.bin.summary().fingerprint, o.bin.annotations()) {
            Ok(()) => format!("saved to {}", path.display()),
            Err(e) => format!("NOT saved ({e}); kept in memory for this session"),
        },
        None => "kept in memory".into(),
    }
}

fn decomp_context(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let start = function_at(&o.bin, at)?;
    let mut out = ensure_xrefs(o)?;
    let limit = int(args, "limit", 400, 5000) as usize;
    let c = o.bin.decomp_context(start, limit).ok_or("not in a function")?;
    out.push_str(&c.describe());
    Ok(out)
}

fn function_signature(o: &Open, args: &Value) -> Result<String, String> {
    let at = string(args, "at").ok_or("at is required")?;
    let start = function_at(&o.bin, at)?;
    let s = o
        .bin
        .function_signature(start)
        .ok_or("not in a function, or not MIPS code (signatures are for MIPS so far)")?;
    Ok(s.describe())
}

fn match_function(o: &Open, args: &Value) -> Result<String, String> {
    let object = string(args, "object").ok_or("object is required")?;
    let symbol = string(args, "symbol").ok_or("symbol is required")?;
    let bytes = std::fs::read(object).map_err(|e| format!("{object}: {e}"))?;
    let funcs = binviz::matching::object_functions(&bytes).map_err(|e| e.to_string())?;
    let f = funcs.iter().find(|f| f.name == symbol).ok_or_else(|| {
        format!(
            "no function {symbol} in {object}; it has: {}",
            funcs.iter().map(|f| f.name.as_str()).take(40).collect::<Vec<_>>().join(", ")
        )
    })?;
    let at = string(args, "at").unwrap_or(symbol);
    let start = function_at(&o.bin, at)?;
    let m = o.bin.match_function(start, f).ok_or("not in a function")?;
    Ok(m.to_text())
}

fn match_object(o: &Open, args: &Value) -> Result<String, String> {
    let object = string(args, "object").ok_or("object is required")?;
    let bytes = std::fs::read(object).map_err(|e| format!("{object}: {e}"))?;
    let results = o.bin.match_object(&bytes).map_err(|e| e.to_string())?;
    if results.is_empty() {
        return Err("no function of the object has a name the binary knows (name them with annotate or import_symbol_addrs first)".into());
    }
    let limit = int(args, "limit", 50, 1000) as usize;
    let exact = results.iter().filter(|m| m.percent >= 100.0).count();
    let mut out = format!(
        "{} functions compared, {exact} match exactly, {:.1}% of instructions overall.\n\n",
        results.len(),
        results.iter().map(|m| m.matched_instructions as f64).sum::<f64>() * 100.0
            / results
                .iter()
                .map(|m| m.original_instructions.max(m.rebuilt_instructions) as f64)
                .sum::<f64>()
                .max(1.0)
    );
    for m in results.iter().take(limit) {
        let kinds: Vec<String> = m.differences.iter().map(|(k, n)| format!("{n} × {k}")).collect();
        let _ = writeln!(
            out,
            "  {:#x} {:>6.1}%  {}  {}",
            m.address,
            m.percent,
            m.name,
            if kinds.is_empty() { String::new() } else { format!("({})", kinds.join(", ")) }
        );
    }
    if results.len() > limit {
        let _ = writeln!(out, "  … {} more", results.len() - limit);
    }
    out.push_str("\nmatch_function shows one function's instructions lined up with the original's.");
    Ok(out)
}

fn place_report(o: &mut Open, args: &Value) -> Result<String, String> {
    let path = string(args, "report").ok_or("report is required")?;
    let json = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let report = binviz::matching::ObjdiffReport::parse(&json).map_err(|e| e.to_string())?;
    let p = o.bin.place_report(&report);
    let below = args.get("below").and_then(Value::as_f64).unwrap_or(100.0) as f32;
    let limit = int(args, "limit", 100, 5000) as usize;
    let mut out = format!(
        "{:.2}% matched: {} of {} bytes of code, {} of {} functions; {} placed on this binary, {} not found.\n",
        p.fuzzy_match_percent,
        p.matched_code,
        p.total_code,
        p.matched_functions,
        p.total_functions,
        p.functions.len(),
        p.unplaced.len()
    );
    let listed: Vec<_> = p.functions.iter().filter(|f| f.percent < below).collect();
    let _ = writeln!(out, "\n{} functions below {below}%:", listed.len());
    for f in listed.iter().take(limit) {
        let _ = writeln!(out, "  {:#x} {:>6.1}%  {}  ({})", f.address, f.percent, f.name, f.unit);
    }
    if listed.len() > limit {
        let _ = writeln!(out, "  … {} more", listed.len() - limit);
    }
    if !p.unplaced.is_empty() {
        let _ = writeln!(out, "\nNot found here: {}", p.unplaced.iter().take(30).cloned().collect::<Vec<_>>().join(", "));
    }
    if args.get("record").and_then(Value::as_bool).unwrap_or(true) {
        let (matched, lost) = crate::queue::record_report(o, &p.functions);
        let _ = write!(out, "\nRecorded in the notes: {} newly matched", count(matched));
        if !lost.is_empty() {
            let names: Vec<String> = lost
                .iter()
                .take(20)
                .map(|&a| {
                    o.bin
                        .symbols()
                        .at(a)
                        .map_or_else(|| format!("{a:#x}"), |s| s.display_name().into_owned())
                })
                .collect();
            let _ = write!(out, "; {} no longer match: {}", lost.len(), names.join(", "));
        }
        let _ = writeln!(out, ". {}.", save_notes(o));
    }
    Ok(out)
}

fn splat_export(o: &Open, args: &Value) -> Result<String, String> {
    let name = string(args, "name").ok_or("name is required")?;
    let splits: Vec<u64> = args
        .get("splits")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().and_then(parse_number)).collect())
        .unwrap_or_default();
    let e = o.bin.splat_export(name, &splits).map_err(|e| e.to_string())?;
    match string(args, "out_dir") {
        Some(dir) => {
            let dir = Path::new(dir);
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let yaml = dir.join(format!("{name}.yaml"));
            std::fs::write(&yaml, &e.config).map_err(|e| e.to_string())?;
            std::fs::write(dir.join("symbol_addrs.txt"), &e.symbol_addrs).map_err(|e| e.to_string())?;
            Ok(format!(
                "Wrote {} and symbol_addrs.txt: {} functions, {} data symbols.",
                yaml.display(),
                e.functions,
                e.data_symbols
            ))
        }
        None => Ok(format!(
            "{}\n--- symbol_addrs.txt ({} functions, {} data symbols) ---\n{}",
            e.config, e.functions, e.data_symbols, e.symbol_addrs
        )),
    }
}

fn import_symbol_addrs(o: &mut Open, args: &Value) -> Result<String, String> {
    let path = string(args, "path").ok_or("path is required")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let names = binviz::splat::parse_symbol_addrs(&text);
    let mut list: Vec<Annotation> = o.bin.annotations().to_vec();
    let mut added = 0;
    let mut updated = 0;
    for n in names {
        match list.iter_mut().find(|a| a.address == n.address) {
            Some(a) => {
                if a.name != n.name {
                    a.name = n.name;
                    updated += 1;
                }
                if a.size == 0 {
                    a.size = n.size;
                }
            }
            None => {
                list.push(n);
                added += 1;
            }
        }
    }
    o.bin.set_annotations(list);
    Ok(format!(
        "{added} names added, {updated} changed; {} notes, {}.",
        o.bin.annotations().len(),
        save_notes(o)
    ))
}

/// Adds `notes` to the binary's, names replacing names at the same address.
fn merge_notes(o: &mut Open, notes: Vec<Annotation>) -> (usize, usize) {
    let mut list: Vec<Annotation> = o.bin.annotations().to_vec();
    let (mut added, mut updated) = (0, 0);
    for n in notes {
        match list.iter_mut().find(|a| a.address == n.address) {
            Some(a) => {
                if a.name != n.name || a.comment != n.comment {
                    a.name = n.name;
                    if !n.comment.is_empty() {
                        a.comment = n.comment;
                    }
                    if n.size > 0 {
                        a.size = n.size;
                    }
                    updated += 1;
                }
            }
            None => {
                list.push(n);
                added += 1;
            }
        }
    }
    o.bin.set_annotations(list);
    (added, updated)
}

fn identify_sdk(o: &mut Open, args: &Value) -> Result<String, String> {
    let paths = args
        .get("paths")
        .and_then(Value::as_array)
        .ok_or("paths is required")?
        .iter()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut found: Vec<PathBuf> = std::fs::read_dir(&p)
                .map_err(|e| format!("{}: {e}", p.display()))?
                .flatten()
                .map(|e| e.path())
                .filter(|f| {
                    f.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("lib") || e.eq_ignore_ascii_case("obj"))
                })
                .collect();
            found.sort();
            files.extend(found);
        } else {
            files.push(p);
        }
    }
    if files.is_empty() {
        return Err("no .LIB or .OBJ files found".into());
    }
    let mut sigs = binviz::rom::psyq::SignatureSet::default();
    let mut loaded = String::new();
    for f in &files {
        let bytes = std::fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?;
        match sigs.add_file(&f.to_string_lossy(), &bytes) {
            Ok(n) => {
                let _ = writeln!(loaded, "  {}: {n} signatures", f.display());
            }
            Err(e) => {
                let _ = writeln!(loaded, "  {}: {e}", f.display());
            }
        }
    }
    let r = o.bin.identify_sdk(&sigs);
    let limit = int(args, "limit", 200, 5000) as usize;
    let mut out = format!("Libraries read:\n{loaded}\n");
    let mut text = r.to_text();
    if r.matches.len() > limit {
        let keep: usize = text.lines().take(1 + r.libraries.len() + limit).map(|l| l.len() + 1).sum();
        text.truncate(keep);
        let _ = writeln!(text, "… {} more", r.matches.len() - limit);
    }
    out.push_str(&text);
    if args.get("apply").and_then(Value::as_bool).unwrap_or(false) {
        let (added, updated) = merge_notes(o, r.annotations());
        let _ = writeln!(out, "\n{added} names added, {updated} changed; {}.", save_notes(o));
    }
    Ok(out)
}

fn propose_names(o: &mut Open, args: &Value) -> Result<String, String> {
    let path = string(args, "path").ok_or("path is required")?;
    let json = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let candidates = binviz::names::parse_candidates(&json).map_err(|e| e.to_string())?;
    let p = o.bin.propose_names(&candidates);
    let mut out = p.to_text();
    if args.get("apply").and_then(Value::as_bool).unwrap_or(false) {
        let min = args.get("min_confidence").and_then(Value::as_f64).unwrap_or(0.6) as f32;
        let notes = p.annotations(min);
        let n = notes.len();
        let (added, updated) = merge_notes(o, notes);
        let _ = writeln!(out, "\n{n} proposals at or above {min:.2} applied: {added} names added, {updated} changed; {}.", save_notes(o));
    }
    Ok(out)
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
            kind: None, decomp: None,
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
        if let Some(d) = &a.decomp {
            out.pop();
            let _ = writeln!(out, " [{}]", crate::queue::state_text(d));
        }
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
    if let Some(a) = i.annotation.as_ref().filter(|a| a.is_note()) {
        let _ = writeln!(
            out,
            "Your note{}: {}",
            if a.reviewed { " (reviewed)" } else { "" },
            clip(&a.comment, 300)
        );
    }
    if let Some(d) = o.bin.decomp_at(f.address) {
        let _ = writeln!(out, "Decompilation: {}", crate::queue::state_text(d));
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

// --- DWARF ---------------------------------------------------------------------------

fn debug_of(o: &Open) -> Result<&binviz::DebugInfo, String> {
    o.bin.debug_info().ok_or_else(|| {
        let s = o.bin.summary();
        format!(
            "{} has no DWARF debug info{}",
            o.label,
            match s.format {
                binviz::Format::MachO => "; for Mach-O it usually lives in a .dSYM: open_binary with debug_file pointing at …/Contents/Resources/DWARF/<name>, or for a build without one, at the folder of the object files its debug map names".to_string(),
                binviz::Format::Pe => pdb_hint(s),
                _ => String::new(),
            }
        )
    })
}

/// Where a PE's debug info is: the PDB it names, to pass as debug_file.
fn pdb_hint(s: &binviz::Summary) -> String {
    match &s.debug_link {
        Some(pdb) => format!(" (it is in a PDB, {pdb}: open_binary with debug_file pointing at it)"),
        None => " (MSVC builds keep it in a PDB, which this image doesn't name)".to_string(),
    }
}

fn die_line(d: &binviz::dwarf::DieSummary) -> String {
    format!(
        "<{:#x}> {} {}{}{}",
        d.section_offset,
        d.tag.trim_start_matches("DW_TAG_"),
        d.scope.as_deref().map(|s| format!("{s}::")).unwrap_or_default(),
        d.name.as_deref().unwrap_or(""),
        d.detail
            .as_deref()
            .map(|x| format!("  ({})", clip(x, 120)))
            .unwrap_or_default()
    )
}

/// The text of a source line, if the file exists on this machine.
fn source_text(path: &str, line: u32) -> Option<String> {
    if line == 0 {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().nth(line as usize - 1).map(|l| l.trim().to_string())
}

fn dwarf_units(o: &Open, args: &Value) -> Result<String, String> {
    let d = debug_of(o)?;
    let filter = string(args, "filter").map(str::to_lowercase);
    let units: Vec<&binviz::dwarf::UnitInfo> = d
        .units()
        .iter()
        .filter(|u| {
            filter.as_ref().is_none_or(|f| {
                [&u.name, &u.producer, &u.language]
                    .iter()
                    .any(|v| v.as_deref().is_some_and(|v| v.to_lowercase().contains(f)))
            })
        })
        .collect();
    let offset = int(args, "offset", 0, u32::MAX as u64) as usize;
    let limit = int(args, "limit", 100, 5000) as usize;
    let summary = d.summary();
    let mut out = format!(
        "{} units ({}), DWARF {:?}, from {}{}:\n",
        count(d.units().len() as u64),
        if filter.is_some() {
            format!("{} match", units.len())
        } else {
            "all".into()
        },
        summary.versions,
        summary.source,
        if d.load_problems().is_empty() {
            String::new()
        } else {
            format!(
                "; {} unit(s) could not be read (see dwarf_check)",
                d.load_problems().len()
            )
        }
    );
    for u in units.iter().skip(offset).take(limit) {
        let _ = writeln!(
            out,
            "  [{}] {} — {} v{}, {}{}{}",
            u.index,
            u.name.as_deref().unwrap_or("(no name)"),
            u.language.as_deref().unwrap_or(&u.kind),
            u.version,
            human(u.size),
            if u.code_size > 0 {
                format!(", {} of code", human(u.code_size))
            } else {
                String::new()
            },
            u.producer
                .as_deref()
                .map(|p| format!("; {}", clip(p, 100)))
                .unwrap_or_default()
        );
    }
    if units.len() > offset + limit {
        let _ = writeln!(
            out,
            "  … {} more (offset {})",
            units.len() - offset - limit,
            offset + limit
        );
    }
    Ok(out)
}

fn dwarf_search(o: &Open, args: &Value) -> Result<String, String> {
    let d = debug_of(o)?;
    let query = string(args, "query").ok_or("query is required")?;
    let limit = int(args, "limit", 50, 1000) as usize;
    let everything = args.get("everything").and_then(Value::as_bool).unwrap_or(false);
    let hits = if everything {
        d.search_all(query, limit)
    } else {
        d.search(query, limit)
    };
    if hits.is_empty() {
        return Ok(format!(
            "No DIE named like {query:?}{}",
            if everything {
                "."
            } else {
                "; try everything: true to include locals and parameters."
            }
        ));
    }
    let mut out = format!("{} DIE(s):\n", hits.len());
    for h in &hits {
        let _ = writeln!(out, "  [{}] {}", h.unit, die_line(h));
    }
    Ok(out)
}

fn dwarf_dies(o: &Open, args: &Value) -> Result<String, String> {
    let d = debug_of(o)?;
    let unit = args.get("unit").and_then(Value::as_u64).ok_or("unit is required")? as u32;
    if unit as usize >= d.units().len() {
        return Err(format!("no unit {unit}; there are {}", d.units().len()));
    }
    let tags = string(args, "tags").unwrap_or("");
    let name = string(args, "name").unwrap_or("");
    let offset = int(args, "offset", 0, u32::MAX as u64) as u32;
    let limit = int(args, "limit", 100, 2000) as u32;
    let page = d.list_dies(unit, tags, name, offset, limit);
    let u = &d.units()[unit as usize];
    let mut out = format!(
        "Unit {unit} ({}): {} DIE(s){}{}\n",
        u.name.as_deref().unwrap_or("?"),
        count(page.total),
        if tags.is_empty() {
            String::new()
        } else {
            format!(" tagged {tags}")
        },
        if name.is_empty() {
            String::new()
        } else {
            format!(" named like {name:?}")
        }
    );
    for x in &page.dies {
        let _ = writeln!(out, "  {}", die_line(x));
    }
    if page.total > offset + page.dies.len() as u32 {
        let _ = writeln!(
            out,
            "  … {} more (offset {})",
            page.total - offset - page.dies.len() as u32,
            offset + page.dies.len() as u32
        );
    }
    if tags.is_empty() && name.is_empty() {
        let _ = writeln!(out, "\nBy tag:");
        for t in d.tag_counts(unit).iter().take(40) {
            let _ = writeln!(out, "  {:>8}  {}", count(t.count), t.tag);
        }
    }
    Ok(out)
}

/// `0x1a2b` / `<0x1a2b>` (.debug_info offset), `3:0x44` (unit:offset), or a name.
fn find_die(d: &binviz::DebugInfo, text: &str) -> Result<(u32, u64), String> {
    let t = text.trim().trim_start_matches('<').trim_end_matches('>');
    if let Some((u, off)) = t.split_once(':')
        && let (Ok(u), Some(off)) = (u.trim().parse::<u32>(), binviz::search::parse_number(off.trim()))
    {
        return d
            .die(u, off)
            .map(|_| (u, off))
            .ok_or_else(|| format!("no DIE at unit {u} offset {off:#x}"));
    }
    if t.starts_with("0x") || t.starts_with("0X") {
        let off = binviz::search::parse_number(t).ok_or_else(|| format!("not an offset: {t}"))?;
        return d
            .die_at_offset(off)
            .ok_or_else(|| format!("no unit in .debug_info covers {off:#x}"));
    }
    let hits = d.search(t, 1);
    let hit = hits.first().ok_or_else(|| format!("no DIE named like {t:?}"))?;
    Ok((hit.unit, hit.offset))
}

fn dwarf_die(o: &Open, args: &Value) -> Result<String, String> {
    let d = debug_of(o)?;
    let (unit, offset) = find_die(d, string(args, "die").ok_or("die is required")?)?;
    let det = d.die(unit, offset).ok_or("that DIE can't be read")?;
    let mut out = String::new();
    let u = &d.units()[unit as usize];
    let _ = writeln!(
        out,
        "{} {}  <{:#x}> (unit {unit} offset {offset:#x}, {} bytes at {} {:#x}..{:#x})",
        det.die.tag,
        det.die.name.as_deref().unwrap_or(""),
        det.die.section_offset,
        det.byte_end - det.byte_start,
        det.section,
        det.byte_start,
        det.byte_end
    );
    let _ = writeln!(out, "In unit {unit}: {}", u.name.as_deref().unwrap_or("?"));
    if !det.parents.is_empty() {
        let path: Vec<String> = det
            .parents
            .iter()
            .map(|p| {
                format!(
                    "{} {}",
                    p.tag.trim_start_matches("DW_TAG_"),
                    p.name.as_deref().unwrap_or("")
                )
            })
            .collect();
        let _ = writeln!(out, "Inside: {}", path.join(" › "));
    }
    if let Some(t) = &det.type_name {
        let _ = writeln!(out, "Type: {t}");
    }
    if let Some(s) = det.byte_size {
        let _ = writeln!(out, "Size: {s} bytes");
    }
    if let Some(l) = &det.decl {
        let text = source_text(&l.path, l.line)
            .map(|t| format!("\n    {}", clip(&t, 200)))
            .unwrap_or_default();
        let _ = writeln!(out, "Declared at {}:{}{text}", l.path, l.line);
    }
    if let Some(l) = &det.call_site {
        let text = source_text(&l.path, l.line)
            .map(|t| format!("\n    {}", clip(&t, 200)))
            .unwrap_or_default();
        let _ = writeln!(out, "Inlined at {}:{}:{}{text}", l.path, l.line, l.column);
    }
    if !det.ranges.is_empty() {
        let r: Vec<String> = det
            .ranges
            .iter()
            .take(8)
            .map(|[a, b]| format!("{a:#x}..{b:#x}"))
            .collect();
        let _ = writeln!(
            out,
            "Code: {}{}",
            r.join(", "),
            if det.ranges.len() > 8 {
                format!(" … {} more", det.ranges.len() - 8)
            } else {
                String::new()
            }
        );
    }
    if !det.code_lines.is_empty() {
        let _ = writeln!(out, "\nSource lines of its code ({}):", det.code_lines.len());
        for l in det.code_lines.iter().take(60) {
            let file = l.path.rsplit(['/', '\\']).next().unwrap_or(&l.path);
            let _ = writeln!(
                out,
                "  {file}:{:<5} {:>6} bytes from {:#x}{}",
                l.line,
                l.bytes,
                l.first,
                source_text(&l.path, l.line)
                    .map(|t| format!("   {}", clip(&t, 100)))
                    .unwrap_or_default()
            );
        }
    }
    if !det.layout.is_empty() {
        let _ = writeln!(out, "\nLayout:");
        for m in &det.layout {
            let at = match (m.bit_offset, m.bit_size) {
                (Some(b), Some(w)) => format!("{:#x}:{}", b / 8, b % 8) + &format!(" ({w} bits)"),
                _ => m.offset.map_or("-".into(), |o| format!("{o:#x}")),
            };
            let _ = writeln!(
                out,
                "  {at:>12} {:>6}  {:<8} {} {}{}{}",
                m.size.map_or(String::new(), |s| s.to_string()),
                m.kind,
                m.type_name,
                m.name.as_deref().unwrap_or(""),
                if m.artificial { " (compiler-generated)" } else { "" },
                if m.hole > 0 {
                    format!("   ← {} byte hole before", m.hole)
                } else {
                    String::new()
                }
            );
        }
        if let Some(t) = det.tail_padding.filter(|&t| t > 0) {
            let _ = writeln!(out, "  {t} byte(s) of padding at the end");
        }
    }
    let _ = writeln!(out, "\nAttributes:");
    for a in &det.attributes {
        let _ = writeln!(
            out,
            "  {:<28} {:<22} {}",
            a.name,
            a.form,
            clip(&a.value.replace('\n', "\n      "), 400)
        );
    }
    let max = int(args, "children", 50, 1000) as usize;
    if det.child_count > 0 {
        let kids = d.die_children(unit, Some(offset));
        let _ = writeln!(out, "\nChildren ({}):", det.child_count);
        for k in kids.iter().take(max) {
            let _ = writeln!(out, "  {}", die_line(k));
        }
        if kids.len() > max {
            let _ = writeln!(out, "  … {} more", kids.len() - max);
        }
    }
    Ok(out)
}

fn dwarf_at(o: &Open, args: &Value) -> Result<String, String> {
    let d = debug_of(o)?;
    let address = address_of(&o.bin, string(args, "at").ok_or("at is required")?)?;
    let mut out = String::new();
    match d.location(address) {
        Some(l) => {
            let text = source_text(&l.path, l.line)
                .map(|t| format!("\n    {}", clip(&t, 200)))
                .unwrap_or_default();
            let _ = writeln!(out, "{address:#x} is {}:{}:{}{text}", l.path, l.line, l.column);
        }
        None => {
            let _ = writeln!(out, "{address:#x}: no line table row covers it");
        }
    }
    let frames = d.frames(address);
    if frames.len() > 1 {
        let _ = writeln!(out, "\nInlined call stack (innermost first):");
        for f in &frames {
            let _ = writeln!(
                out,
                "  {} at {}:{}",
                f.demangled.as_deref().or(f.function.as_deref()).unwrap_or("??"),
                f.file.as_deref().unwrap_or("?"),
                f.line.unwrap_or(0)
            );
        }
    }
    match d.scope_at(address) {
        None => {
            let _ = writeln!(out, "\nNo DWARF function covers this address.");
        }
        Some(scope) => {
            let _ = writeln!(out, "\nScopes, outermost first, and what they declare:");
            for (i, sc) in scope.scopes.iter().enumerate() {
                let _ = writeln!(out, "{}{}", "  ".repeat(i + 1), die_line(sc));
                for v in scope.variables.iter().filter(|v| v.scope == i as u32) {
                    let _ = writeln!(
                        out,
                        "{}  {} {}: {} — {}",
                        "  ".repeat(i + 1),
                        v.kind,
                        v.name,
                        v.type_name.as_deref().unwrap_or("?"),
                        v.location
                    );
                }
            }
        }
    }
    Ok(out)
}

fn dwarf_check(o: &Open, args: &Value) -> Result<String, String> {
    let d = debug_of(o)?;
    let started = Instant::now();
    let c = d.check();
    let limit = int(args, "limit", 100, 2000) as usize;
    let mut out = format!(
        "Checked {} units, {} DIEs and {} line table rows in {:.1} s: {} error(s), {} warning(s).\n",
        count(c.units),
        count(c.dies),
        count(c.line_rows),
        started.elapsed().as_secs_f64(),
        count(c.errors),
        count(c.warnings)
    );
    if c.problems.is_empty() {
        out.push_str("No problems: everything reads and every reference resolves.\n");
        return Ok(out);
    }
    for p in c.problems.iter().take(limit) {
        let place = match (p.unit, p.die) {
            (Some(u), Some(die)) => format!(
                "unit {u} {}<{:#x}>",
                p.tag
                    .as_deref()
                    .map(|t| format!("{} ", t.trim_start_matches("DW_TAG_")))
                    .unwrap_or_default(),
                p.offset.unwrap_or(die)
            ),
            (Some(u), None) => format!("unit {u}, {} {:#x}", p.section, p.offset.unwrap_or(0)),
            _ => format!("{} {:#x}", p.section, p.offset.unwrap_or(0)),
        };
        let _ = writeln!(
            out,
            "  {} [{}] {place}: {}",
            if p.severity == binviz::dwarf::Severity::Error {
                "ERROR  "
            } else {
                "warning"
            },
            p.area,
            p.message
        );
    }
    if c.problems.len() > limit || c.truncated {
        let _ = writeln!(out, "  … more (raise limit)");
    }
    Ok(out)
}
