# binviz

See what every byte and every address of a binary is.

binviz is a Rust library, with a WebAssembly-powered browser UI and a CLI, that
explains **ELF**, **Mach-O** and **PE/COFF** files down to individual header
fields, and maps machine code back to source through **DWARF** debug info.

- **Every byte accounted for.** The file is described as a tree of regions:
  headers and their fields, program/section headers, load commands, sections,
  symbol and string tables, relocations, import/export tables, resources,
  code signatures, DWARF units and DIEs… Large tables are decoded on demand, so
  hovering any byte of `.symtab` names the symbol entry *and* the field. Bytes
  nothing claims are labelled as padding or flagged as unclaimed.
- **Every address explained.** Given a file offset or a virtual address you get
  the region path, segment, section, symbol + offset, source file:line:column,
  the inlined call stack, and the decoded instruction.
- **DWARF, readable.** Compilation units, DIE trees with rendered types
  (`const vector<geo::Shape*> &`), location lists (`DW_OP_reg23 (xmm6)`),
  demangled linkage names, line tables, and source ↔ address mapping in both
  directions. Separate debug files (`.dSYM` DWARF, `objcopy --only-keep-debug`
  output) can be attached. Relocatable objects (`.o`) get synthetic addresses and
  relocated DWARF so they work too.
- **Disassembly** for x86/x86-64 (iced-x86) and AArch64/ARM (yaxpeax-arm), with
  branch targets resolved to symbols, the strings and globals an instruction
  uses named inline (AArch64 `adrp` pairs included), and source lines
  interleaved.
- **Call graph and cross-references.** Who calls a function, what it calls, a
  path of calls from one function to another, and every reference to an
  address: calls, tail calls, reads, writes, address-taken, and pointers stored
  in data (vtables, Objective-C metadata, callbacks). Import stubs, PLT entries
  and GOT/IAT slots are named after what they import (`_objc_msgSend`,
  `printf@plt`, `__imp_CreateFileW`), so calls into libraries read as such.
  Pointers are found however the loader relocates them: Mach-O chained fixups,
  ELF `RELATIVE`/RELR relocations, or plain addresses.
- **One search box for everything**: addresses, file offsets, symbols (raw and
  demangled), imports/exports, sections, source files and `file:line`, DWARF
  functions/types/variables, strings, and byte patterns with wildcards.
- **Where each source file lives.** Code (from the line tables) and globals
  (from DWARF) attributed to source files or compilation units, drawn onto every
  section at scale.
- **Reverse-engineering coverage.** Stripped binaries get their functions back
  from `.pdata`, `.eh_frame` and `LC_FUNCTION_STARTS` (`sub_<address>`). Name
  functions, comment instructions and mark code as reviewed; a coverage map
  shows what is named, recovered, reviewed or still unexplored, and lists the
  largest gaps with a guess at what they hold.

Everything runs locally; in the browser the file never leaves your machine.

It is built on the [gimli-rs](https://github.com/gimli-rs) crates: [`object`]
parses the containers, [`gimli`] reads DWARF, and [`addr2line`] resolves
inlined frames.

[`object`]: https://crates.io/crates/object
[`gimli`]: https://crates.io/crates/gimli
[`addr2line`]: https://crates.io/crates/addr2line

## The web UI

| View | What it shows |
|---|---|
| Overview | Format facts, exact byte composition, a file map (by region or entropy), and a diagram of how the file's bytes land in the address space |
| Layout | The region tree, expandable down to single fields and table entries |
| Hex | Every byte tinted by the structure that owns it; hover for the full path, minimap to navigate |
| Code | Functions and their disassembly with source lines (and source text, once loaded) interleaved; how many callers and callees each function has |
| Call graph | Callers and callees of the selected function, a few levels each way, with the complete lists below; find a path of calls from another function (`main`, an entry point) |
| Symbols | Symbols, imports, exports and strings; filter, sort, jump |
| Sections | Segments and sections |
| DWARF | Units, the DIE tree, attributes with clickable references, line tables |
| Sources | Every source file in the line tables; lines that produced code are marked, click one to see its addresses |
| Map | Code and data per source file or compilation unit, drawn onto each section; reverse-engineering coverage with the largest unexplored gaps |

The inspector on the right always shows everything known about the current
selection: its place in the file, what refers to it (callers, reads and
writes, pointers in data), and your notes about it. Views are on keys `1`–`9`
and `0`; `Alt+←/→` walks the history.

### Search

The search box (`Ctrl+K` or `/`) searches everything at once and groups the
results; `↑↓` and `Enter` jump to one, and each group has a **Show all**.

| Query | Finds |
|---|---|
| `0x401000`, `401000`, ``00007ff6`12345678`` | that address (and file offset, or RVA in a PE), plus names containing it |
| `@0x200` | a file offset |
| `main+0x10` | an address relative to a symbol |
| `48 8b ?? 05` or `488b??05` | a byte pattern, `??` matching any byte |
| `"GetProcAddress"` | exact ASCII or UTF-16 text anywhere in the file |
| `shapes.cpp:42` | the code generated for a source line |
| anything else | symbols, imports, exports, sections, source files, DWARF names, your notes and strings containing it; exact, prefix and `::`-segment matches rank first |

### Notes and coverage

Press `N` (or **Add note…** in the inspector) to name a function, comment an
instruction, or mark a range as reviewed. Names become symbols everywhere:
disassembly, branch targets, search. Notes are saved in the browser per file
(keyed by its SHA-256) and can be exported as JSON from **Map → Coverage**.
**Import…** there accepts a binviz export or a symbol list from another tool:
a CSV with an address column (a Ghidra symbol table export, for example),
`nm` output, or plain `address name` lines. RVAs are rebased onto the image.

Coverage gives every byte of the code and data sections the strongest status
that applies: *reviewed*, *annotated*, *named* (symbols and debug info),
*format structure* (tables binviz decodes), *recovered* (functions from unwind
tables and function-start lists, and strings), *padding*, or *unexplored*.

Drop a folder onto the page (or use **Sources**) to load source code; files are
matched to the paths recorded in DWARF by their trailing path components.

### Running it

Prerequisites: Rust (stable) with the `wasm32-unknown-unknown` target, the
`wasm-bindgen` CLI **matching the `wasm-bindgen` crate version** (0.2.129), and
Node.js 20+.

```bash
rustup target add wasm32-unknown-unknown
```

```bash
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

Then, from `web/`:

```bash
npm install
```

```bash
npm run dev
```

`npm run dev` builds the WASM package into `web/src/pkg` and starts Vite.
After changing Rust code, run `npm run wasm` and the page reloads. `npm run
build` produces a static site in `web/dist` that can be hosted anywhere.

> **Windows with the GNU toolchain:** building `wasm-bindgen-cli` needs
> `dlltool.exe` on `PATH` (it ships with MinGW-w64).

## The CLI

```bash
cargo run --release -p binviz-cli -- info path/to/binary
```

| Command | |
|---|---|
| `info <file>` | Summary, segments and sections |
| `layout <file> [depth]` | The region tree |
| `at <file> <offset>` | What the byte at a file offset is |
| `inspect <file> <addr\|symbol>` | Everything about an address |
| `disasm <file> <addr\|symbol> [n]` | Disassembly with source lines |
| `symbols <file> [filter]` | Symbols |
| `dwarf <file>` · `die <file> <unit> [offset]` · `lines <file> <unit>` | DWARF units, DIEs, line tables |
| `sources <file>` · `file-lines <file> <id>` | Source files and their address ranges |
| `search <file> <query> [kind]` | The same search as the UI |
| `strings <file> [filter]` | Strings in the data sections |
| `attribution <file> [unit]` · `attributed <file> <id> [unit]` | Code and data per source file (or unit), and one file's ranges |
| `coverage <file>` | Reverse-engineering coverage per section and the largest gaps |
| `xrefs <file> <addr\|symbol>` · `refs-from <file> <addr\|symbol>` | References to an address, and the references a function makes |
| `callers` · `callees` · `func <file> <addr\|symbol>` | Call sites in and out of a function; its callers, callees, strings and data |
| `callgraph <file> <addr\|symbol> [up] [down]` · `callpath <file> <from> <to>` | The call graph around a function; a shortest chain of calls |
| `check <file>` | Verify every byte is covered by the layout |

`--debug <file>` attaches a separate debug file; `--member <n>` picks a slice of
a universal binary or an archive member; `--notes <file.json>` loads
annotations first.

## Agents (MCP server)

`binviz-mcp` lets an LLM agent work with a binary the way you would in the UI.
It is a [Model Context Protocol](https://modelcontextprotocol.io) server: the
agent opens a binary once, it stays loaded, and every question after that is
answered from memory, in milliseconds even for a 1 GB app.

```bash
cargo build --release -p binviz-mcp
```

```bash
claude mcp add binviz -- /path/to/binviz/target/release/binviz-mcp
```

Any MCP client works the same way (the server speaks JSON-RPC over stdio).

| Tool | |
|---|---|
| `open_binary` | Load a file (universal binaries pick arm64 unless told otherwise; `debug_file` attaches a dSYM's DWARF) |
| `binary_summary` | Format, platform, entry point, build ID, segments, sections, DWARF |
| `size_report` | Why the binary is as big as it is: bytes by kind and section, the largest functions and data, and the owners of the code — Swift modules, Objective-C classes, C++ namespaces / Rust crates, C prefixes — and source files with DWARF |
| `search` | The UI's search: names, strings, addresses, byte patterns, "text", file:line |
| `inspect` | Everything about an address or file offset |
| `disassemble` | A function, with source lines and your comments |
| `function_info` | A function at a glance: callers, callees, the strings it uses, the globals it reads and writes |
| `callers` · `callees` · `call_graph` · `call_path` | Follow calls: who calls what, the tree around a function, how one function reaches another |
| `xrefs` | Every reference to an address, symbol or string: calls, reads, writes, address-taken, pointers in data |
| `list_symbols` · `list_strings` · `hexdump` | Browse tables and bytes |
| `coverage` | How much is mapped out, and the largest unexplored gaps |
| `annotate` · `remove_annotation` · `list_annotations` | Name functions, comment addresses, mark code reviewed |

Notes are saved next to the binary in `<file>.binviz-notes.json`, the format
the web UI imports and exports (Map → Coverage → Import), so an agent can map
out a binary and you can look at the result in the UI, or the other way round.

Things to ask: *"Open ~/Downloads/MyApp and tell me why it's so big"*, *"Find
the code that parses deep links and name what you find"*, *"Which functions
use this error string, and how are they reached from main?"*, *"What haven't
we looked at yet?"*.

## The library

```rust
use binviz::{Binary, Target};

let bin = Binary::parse(std::fs::read("a.out")?)?;
let entry = bin.summary().entry.unwrap();

// What is at the entry point?
let info = bin.inspect(Target::Address(entry));
for step in &info.path {
    println!("{} = {}", step.name, step.value.as_deref().unwrap_or(""));
}
if let Some(src) = &info.source {
    println!("{}:{}", src.path, src.line);
}
for frame in &info.frames {
    println!("{} (inlined: {})", frame.demangled.as_deref().unwrap_or("?"), frame.inlined);
}

// Source → addresses.
if let Some(debug) = bin.debug_info() {
    for file in debug.source_files() {
        for range in debug.file_lines(file.id) {
            println!("{}:{} → {:#x}..{:#x}", file.name, range.line, range.start, range.end);
        }
    }
}
```

```rust
// Search, attribution and coverage.
let hits = bin.search("area", 10, None);
let files = bin.attribution(binviz::AttributionMode::File);
let coverage = bin.coverage(20);
println!("{} bytes unexplored in {} gaps", coverage.totals.unexplored, coverage.gap_count);

// Calls and references (the index is built on first use).
let main = bin.symbols().by_name("main").unwrap().address;
for caller in bin.callers(main) {
    println!("{} calls main {} times", caller.name, caller.calls);
}
let graph = bin.call_graph(main, 1, 2, 8);
let refs = bin.references_to(main, main + 1, 0, 100);
```

All model types implement `serde::Serialize`. Universal binaries and archives
are opened with `binviz::Container`.

## Performance

binviz is built for large binaries (a gigabyte or more, millions of
functions). Measured in the browser (WebAssembly, Edge) and natively:

| File | Open (browser) | Memory (browser) | Search | Coverage |
|---|---|---|---|---|
| 1.0 GB iOS-style Mach-O, 2.6M functions and symbols, 5M strings | 1.4 s | 1.5 GB WASM, 11 MB JS | 60–130 ms | 250 ms |
| 333 MB `msedge.dll`, 1M functions recovered from `.pdata` | 0.5 s | 440 MB WASM | 10–75 ms | 180 ms |

The cross-reference index is built the first time references are asked for
(right away for files under 32 MB): 2.0 s in the browser (0.9 s natively) for
the 316 MB `rustc_driver.dll`, whose ~100 MB of x86-64 code yields 3M
references in 23 MB. After that, callers, callees, call graphs and reference
lists take about a millisecond.

How it stays fast:

- The file is streamed into WebAssembly memory once, in chunks; the page never
  holds a copy (the hex view reads what it shows straight from the `File`).
- Symbols are 32-byte records with names in one shared arena; names are
  demangled on display, or all at once only when a search or sort needs them.
  Your names are merged on top, so editing notes never rebuilds the table.
- Strings are 16-byte records pointing into the file; searches scan names,
  strings and bytes with SIMD (`memchr`, WebAssembly SIMD enabled in
  `.cargo/config.toml`), anchored on the query's rarest byte.
- Lists (functions, symbols, strings, table entries) are paged from the
  worker, and variable-size tables get an entry index built on first use.
- Search indexes are built in the background when the search box is focused.
- References are packed 8 bytes each into one sorted list per kind, so "who
  refers to X" is a binary search; what a function refers to is found by
  decoding just that function again. AArch64 code is scanned as raw
  instruction words, without full disassembly.

To measure a file yourself:

```bash
cargo run --release -p binviz --example bench -- path/to/binary
```

`scripts/gen-big-macho.py` generates a synthetic 1 GB iOS-style Mach-O
(`--strip` for an App Store-like build) for testing at scale.

## Layout of this repository

```
crates/binviz        the library
  src/binary.rs      parsing into the model (sections, segments, symbols, imports, exports)
  src/layout/        the byte-level region tree: ELF, Mach-O, PE/COFF builders,
                     struct field specs, on-demand decoders, DWARF section decoders
  src/dwarf/         units, DIEs, types, expressions, line tables (gimli + addr2line)
  src/disasm.rs      iced-x86 and yaxpeax-arm
  src/inspect.rs     the "what is here?" query, separate debug files, annotations
  src/search.rs      the search box: query forms and ranking
  src/coverage.rs    reverse-engineering coverage and gap hints
  src/discover.rs    function recovery from .pdata, .eh_frame, LC_FUNCTION_STARTS
  src/strings.rs     strings in data sections
  src/xrefs.rs       cross-references and the call graph
  src/pointers.rs    pointers stored in data: chained fixups, ELF relocations, plain addresses
  src/stubs.rs       names for import stubs, PLT entries, GOT and IAT slots
  src/size.rs        where the bytes go: sections, symbols, owners (Swift, ObjC, C++, C)
  src/dwarf/attribution.rs   code and globals per source file / unit
crates/binviz-wasm   wasm-bindgen bindings (a Session object)
crates/binviz-cli    the command-line tool
crates/binviz-mcp    the MCP server for agents
web/                 TypeScript UI; the WASM runs in a Web Worker
tests/fixtures/      small ELF / Mach-O / PE test binaries and their sources
scripts/build-fixtures.sh   regenerates the fixtures with rust-lld (no SDKs needed)
```

Tests run against the fixtures and check, among other things, that every byte
of every fixture is explained and that every instruction of the test functions
maps back to its own source.

```bash
cargo test
```

## Limitations and ideas

- MSVC debug info lives in PDB files, which are not read yet (the PDB path and
  GUID are shown). Binaries from MinGW or Rust's `-gnu` toolchain carry DWARF.
- Split DWARF (`.dwo`/`.dwp`) and the Mach-O debug map (DWARF left in `.o`
  files) are detected but not followed; use a dSYM.
- `.eh_frame`, dyld opcode streams and chained fixups are shown as regions but
  not decoded entry by entry (chained fixups are walked to find pointers).
- Disassembly covers x86, x86-64, AArch64 and ARM (A32); cross-references and
  the call graph x86, x86-64 and AArch64.
- Calls through registers are followed only when the register was just loaded
  from a pointer slot (`ldr x16, [got]; blr x16`, `call r14`); virtual calls and
  `objc_msgSend` selectors are not resolved to their targets yet.
