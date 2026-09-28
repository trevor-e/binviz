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
  branch targets resolved to symbols and source lines interleaved.
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
| Code | Functions and their disassembly with source lines (and source text, once loaded) interleaved |
| Symbols | Symbols, imports, exports and strings; filter, sort, jump |
| Sections | Segments and sections |
| DWARF | Units, the DIE tree, attributes with clickable references, line tables |
| Sources | Every source file in the line tables; lines that produced code are marked, click one to see its addresses |
| Map | Code and data per source file or compilation unit, drawn onto each section; reverse-engineering coverage with the largest unexplored gaps |

The inspector on the right always shows everything known about the current
selection, including your notes about it. Views are on keys `1`–`9`;
`Alt+←/→` walks the history.

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
| `check <file>` | Verify every byte is covered by the layout |

`--debug <file>` attaches a separate debug file; `--member <n>` picks a slice of
a universal binary or an archive member; `--notes <file.json>` loads
annotations first.

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
```

All model types implement `serde::Serialize`. Universal binaries and archives
are opened with `binviz::Container`.

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
  src/dwarf/attribution.rs   code and globals per source file / unit
crates/binviz-wasm   wasm-bindgen bindings (a Session object)
crates/binviz-cli    the command-line tool
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
  not decoded entry by entry.
- Disassembly covers x86, x86-64, AArch64 and ARM (A32).
