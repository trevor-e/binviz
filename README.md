# binviz

See what every byte and every address of a binary is.

binviz is a Rust library with a browser UI (WebAssembly), a CLI and an MCP
server for agents. It explains ELF, Mach-O, PE/COFF, original Xbox (XBE) and
WebAssembly files down to individual header fields, maps machine code back to
source through DWARF, PDB debug info and source maps, and reads game ROMs for
the NES, SNES, Game Boy, GBA, Mega Drive, Nintendo 64 and PlayStation.
Everything runs locally; in the browser the file never leaves your machine. It
handles gigabyte binaries with millions of symbols.

- **Every byte accounted for**: headers, tables, sections, DWARF, padding, and what nothing claims.
- **Every address explained**: region, section, symbol, source line, inlined calls, instruction.
- **Disassembly, cross-references and call graph** for x86, x86-64, AArch64, ARM and WebAssembly, plus consoles' CPUs.
- **DWARF and PDB, readable**, with a checker for broken debug info, and C headers written from their types.
- **Compare builds**: size diffs and function-by-function diffs, even with no symbols.
- **Crash reports symbolicated** against the binaries you have open, browsers' WebAssembly stack traces too.
- **Game ROMs** with banks, hardware registers, emulator logs, text, tiles and patches.
- **An MCP server** so an LLM agent can ask all of the above.
- **Decompilation support** for PlayStation, Nintendo 64 and x86 PC games: memory images and overlays, guessed prototypes and stack frames, rebuilt code scored against the original with each difference explained and what to try, m2c's first draft, splat and objdiff interchange.

The [reference](docs/reference.md) describes everything in full.

## Top features

**Every byte and every address.** The file is a tree of regions: headers and
their fields, program and section headers, load commands, symbol and string
tables, relocations, imports and exports, resources, code signatures, DWARF
units and DIEs. Hover any byte in the hex view for its full path. Give an
address or file offset and get the region, segment, section, symbol plus
offset, source file, line and column, the inlined call stack and the decoded
instruction. One search box takes addresses, offsets, symbols, imports,
sections, `file:line`, DWARF names, strings and byte patterns with wildcards.

**Debug info you can read.** Compilation units, DIE trees with rendered types,
location lists, line tables and source-to-address mapping both ways.
Structures show their layout with holes and padding. At any address you get
the variables in scope and where each one lives. A checker lists everything in
the DWARF that cannot be read or does not add up, at its unit, DIE and section
offset. Separate debug files attach: dSYMs, `.debug` files, Mach-O debug maps
(the objects' DWARF moved to the binary's addresses), and PDBs, which are read
into DWARF, types included, so MSVC, clang-cl and Rust `-msvc` builds get the
same treatment. From the types, a C header: structures with their padding,
unions, enums and prototypes, and the member at any offset named.

**Code, references and calls.** Disassembly with branch targets resolved to
symbols, the strings and globals each instruction uses named inline, and
source lines interleaved. Every reference to an address: calls, tail calls,
reads, writes, address-taken, and pointers stored in data (vtables,
Objective-C metadata, callbacks), found however the loader relocates them.
Callers and callees, the graph around a function, and a path of calls from
one function to another. Stripped binaries get their functions back from
unwind tables, and 32-bit PE images, which have none, by following their code
(jump tables and calls that never return included); name them, comment
instructions and mark code reviewed, and a coverage map shows what is still
unexplored. The data the code uses is typed by how it uses it (floats with
their values, tables of functions filled at run time, structures reached
through pointers, jump tables) and named as disassemblers do; MSVC's C++ RTTI
gives classes, bases and vtables back, and a prototype or type on a note names
the fields reached through it. Mach-O images get their
Objective-C classes, categories, protocols and selectors back.

**Builds compared and crashes explained.** Compare two binaries, or two
folders or zips (two `.ipa` files, say): what was added, removed and grew,
down to sections, owners and symbols. The browser's **Progress** view exports
byte-weighted decompilation treemaps as PNG/SVG; `binviz progress <file> --svg progress.svg`
generates the same image from the CLI ([usage](docs/progress-images.md)). Functions are matched across builds by
name, bytes, instructions and the call graph, then lined up side by side. Drop
an Apple crash report, an Android tombstone or a stack trace and every frame
gets its function, source line and inlined calls from the binaries that are
open, each image found by build ID.

**WebAssembly and the Xbox.** A WebAssembly module opens like any binary: its
sections, functions, imports and exports, the name section, DWARF (a separate
`.debug.wasm` too) and source maps, and browsers' and Node's stack traces
symbolicated. An original Xbox executable (XBE) opens like the 32-bit PE it
holds, with its headers, certificate and kernel imports decoded.

**Folders of binaries.** Open a folder, a zip, an `.ipa`, an `.app`, a web
build or a build directory: every binary in it is found and paired with its
debug file, by build ID or by the name the binary gives it. The folder's size
is broken down by content, and the code of all its binaries is summed by
owner: Swift modules, Objective-C classes, C++ namespaces, C prefixes.

**Game ROMs.** Headers decoded field by field with checksums checked, each
bank at the address the console's CPU sees, RAM and hardware registers named.
Code is followed from the reset and interrupt vectors, so a ROM gets
functions, a call graph and cross-references like any binary. Disassemblers
for the 6502, 65816, SM83, ARM7TDMI, 68000 and MIPS. Emulator code/data logs
and label files (FCEUX, Mesen, RGBDS, WLA DX, no$gba) come and go both ways.
Text is found in the game's own encoding and read through table files, tiles
are drawn in the consoles' formats, and IPS, UPS and BPS patches are placed
in banks and functions or created from your edits. PlayStation discs open to
their files.

**Matching decompilation.** For a PlayStation or Nintendo 64 game being
decompiled: a PlayStation memory image (RAM as an emulator dumped it) or an
overlay opens with the boot executable's names (its notes' too), functions
found by their prologues and laid out the way GCC does (a head hoisted above
the frame setup, a switch's cases past a table that isn't in the image, a
frameless leaf nothing calls, all joined to their function), switch tables
followed, and an emulator's trace of the code that ran adds what pointers
reach; the Psy-Q SDK's functions are named, with the release that built
them. For an x86 PC game: the compiler and linker from the
Rich header, calling conventions and stack frames with named slots, the C
runtime named from its `.lib`. Each function's code implies a prototype
(register and stack arguments, return value, frame, saved registers, the
structure offsets it walks), and one call gathers everything needed to write
its C, with worked examples: functions already matched that are shaped like
it, in this game or a sibling one built with the same compiler. m2c gets its
input for a first draft (`binviz asm`). The compiler's object file (MIPS ELF,
or COFF from MSVC or clang-cl) is scored against the original function by
function (or against any range of code you name) with the linker's fields
masked, each difference explained (registers, stack slots, branch lengths,
reordering, delay-slot nops) with what to try in the C, the compiler the
original's epilogue gives away, and builds with different flags ranked.
Huge functions (a script interpreter's) are lined up on anchors, so they
score by what they share rather than giving up. A splat config
and symbol file start the project; objdiff's report places its progress on the
binary, and the notes export it again in that format for decomp.dev.
[samples/psx](samples/psx/README.md) walks the whole loop on a small program
built from source,
[samples/psx-advanced](samples/psx-advanced/README.md) adds an overlay, a memory
image, a trace and a jump table, and [samples/psx-vm](samples/psx-vm/README.md)
runs the whole loop through the MCP server with a script as the agent; the
[plan](docs/decomp-plan.md) says what is next. The
[reusable tooling spec](docs/decompilation-tooling-spec.md) prioritizes gaps
found during FF9 work, with migration targets and acceptance criteria.

**Built for agents.** The MCP server loads a binary once and answers every
question after that from memory, in milliseconds even for a 1 GB app: search,
inspect, disassemble, xrefs, call graphs, DWARF, diffs, crash symbolication,
and notes that the web UI can import. For a matching decompilation it keeps
the work queue too: which function to write C for next (callees first,
near-copies of matched functions before anything), claims so parallel agents
don't collide, and each function's outcome, every change appended to a
journal beside the notes before the notes file is rewritten, so agents
writing at once lose nothing.

**Agents that map binaries with you.** An agent (through the MCP server)
works through a worklist of the functions to name next (those whose callees
all have names first, then the most called); its notes land in the same notes
file the web UI follows as it changes, marked as the agent's until you confirm
them, and the names you give go back to it. Several agents can split one
binary between them.

**Homebrew with its source.** A game built with the cc65 tools (ca65, cc65)
comes with the debug file ld65 writes (`ld65 --dbgfile game.dbg`): load it (or
drop it on the ROM) and the game gets its source lines, its `.proc`s (C
functions by their C names), labels named within their scopes
(`player::update`) and RAM variables, read into DWARF like any debug info.
Each place is found by its offset in the file, so banked games work too. It is
also an answer key: `score` (CLI) and `compare_names` (MCP) set the names in
notes, yours or an agent's from mapping the ROM blind, against the real ones
(the same, close, different, missed); an unstripped build of a binary serves
the same way.

**From a line of text to the code that prints it.** Click anywhere in a string
(ASCII, or a game's text read through its table file) and the whole string
lights up, with everything that refers to it. For 6502 and 65816 games that
includes how their code builds pointers: an address loaded as two bytes
(`lda #<text` … `lda #>text`), a table of words read into a pointer, or the low
and high bytes kept in two tables. Tables of pointers the code reads are named
(`ptrs_c120`, `ptrs_lo_c120`), and they lead to the code that reads them. A
ROM's other words holding the string's address count too, as the code in their
bank would read them.

## Running the web UI

Prerequisites: Rust (stable) with the `wasm32-unknown-unknown` target,
`wasm-bindgen-cli` matching the crate version (0.2.129), and Node.js 20+.

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

`npm run dev` builds the WASM package and starts Vite. After changing Rust
code, run `npm run wasm`. `npm run build` produces a static site in `web/dist`.
For faster local iteration, run `npm run wasm:debug`, then `npx vite`; keep Vite
running for TypeScript/CSS edits and rerun `npm run wasm:debug` after Rust edits.
Run `npm run build` for the production output when the batch is finished. Web
builds do not rebuild the native CLI or MCP executables.
On Windows with the GNU toolchain, building `wasm-bindgen-cli` needs
`dlltool.exe` on `PATH` (it ships with MinGW-w64).

## The CLI

```bash
cargo build -p binviz-cli
./target/debug/binviz info path/to/binary
```

Use debug while developing Binviz itself; on Windows its filename is
`target/debug/binviz.exe`. Rebuild after Rust changes, then reuse the executable.
For routine inspection and decompilation work, prefer the optimized native build:
`cargo build --release -p binviz-cli` and use `target/release/binviz` (`.exe` on
Windows). Debug and release are separate outputs: updating one leaves the other
unchanged. Release builds use full optimization and link-time optimization and
take longer. See the [recommended decompilation workflow](docs/decompilation-workspaces.md#recommended-workflow).

Run it with no arguments for the full command list: `info`, `layout`,
`inspect`, `check`, `symbols`, `strings`, `search`, `disasm`, `func`, `refs`,
`calls`, `coverage`, `globals`, `classes`, `objc`, `dwarf`, `header`,
`attribution`, `crash`, `diff`, `patch`, `relsearch`, `text` and `labels`, and
for a decompilation `signature`, `context`, `match`, `asm`, `m2c`, `flags`,
`report`, `progress`, `contracts`, `register-use`, `splat`, `sdk`, `locate`, `counterparts`, `names`, `files`,
`extract`, `blobs`, `libraries` and `fieldrefs`.
`--debug` attaches a separate debug file, `--member` picks a slice of a
universal binary or a file in a folder, `--notes` loads annotations (and
`--types` the structures they name), and `--log` follows a ROM with an
emulator's code/data log. A folder, zip or CD image works in place of a file.

`contracts <report.json>` makes imported compiler call-contract findings readable:
caller declarations, actual supplied arguments, definition signatures and reported
source identities. Filter with `--caller` or `--callee`; `--json` preserves the
adapter evidence. Importing a report does not verify native behavior or grant ABI
exceptions. See [call-contract inspection](docs/call-contracts.md).

The browser's **Call contracts** view imports the same reports with exact
caller/callee filters and expandable source, type and identity evidence.
The maintained [Clang facts adapter](docs/compiler-facts.md) produces versioned
observations for full-call auditing. `evidence <report.json>` verifies actual
artifact bytes and explains stale stage dependencies. The browser offers source
spans, identity filters and native register paths; persistent MCP sessions expose
the same contract and register audits.

`register-use <file> <address> <bytes> <entry> <register>` audits an incoming
PS1 MIPS register word over an exact extent, with read/end/frontier instruction
paths. Unknown calls and surviving returns remain unresolved by default.
`--batch requests.json` loads the binary once for multiple IDs, registers and
explicit per-request policies; `--json` preserves all witnesses and assumptions.
See [register-use examples and policy schema](docs/register-use.md).

## The MCP server

```bash
cargo build -p binviz-mcp
```

```bash
claude mcp add binviz -- /path/to/binviz/target/debug/binviz-mcp
```

On Windows use the absolute path to `binviz-mcp.exe`. Restart the MCP server
after rebuilding so the client uses the new code. For an optimized installation,
build with `--release` and configure the client to use `target/release/binviz-mcp`.
The configured path must match the profile you rebuilt.

Any MCP client works (JSON-RPC over stdio). Things to ask: *"Open
~/Downloads/MyApp and tell me why it's so big"*, *"Which functions use this
error string, and how are they reached from main?"*, *"What haven't we looked
at yet?"*. The [reference](docs/reference.md#agents-mcp-server) lists every
tool.

## The library

```rust
use binviz::{Binary, Target};

let bin = Binary::parse(std::fs::read("a.out")?)?;
let entry = bin.summary().entry.unwrap();
let info = bin.inspect(Target::Address(entry));
for step in &info.path {
    println!("{} = {}", step.name, step.value.as_deref().unwrap_or(""));
}
```

All model types implement `serde::Serialize`. It is built on the
[gimli-rs](https://github.com/gimli-rs) crates: `object`, `gimli` and
`addr2line`.

## Performance

| File | Open (browser) | Memory (browser) | Search | Coverage |
|---|---|---|---|---|
| 1.0 GB iOS-style Mach-O, 2.6M functions and symbols, 5M strings | 1.4 s | 1.5 GB WASM, 11 MB JS | 60–130 ms | 250 ms |
| 333 MB `msedge.dll`, 1M functions recovered from `.pdata` | 0.5 s | 440 MB WASM | 10–75 ms | 180 ms |

The file is streamed into WebAssembly memory once; symbols and strings are
compact records pointing into it; searches use SIMD; the cross-reference
index is built on first use (2 s for 100 MB of x86-64 code) and answers in
about a millisecond after that. See the [reference](docs/reference.md#performance).

## Repository

```
crates/binviz        the library (layout, DWARF, disassembly, xrefs, ROMs, diffs, crashes)
crates/binviz-wasm   wasm-bindgen bindings for the web UI
crates/binviz-cli    the command-line tool
crates/binviz-mcp    the MCP server for agents
web/                 TypeScript UI; the WASM runs in a Web Worker
tests/fixtures/      small ELF, Mach-O, PE, XBE and WebAssembly binaries, hand-assembled ROMs
docs/                the full reference and plans
```

```bash
cargo test
```

## Limitations

- PDBs contribute modules, procedures, globals, types and line records, not
  local variables; those need DWARF.
- Split DWARF (`.dwo`/`.dwp`) is detected but not followed.
- Cross-references and the call graph cover x86, x86-64, AArch64,
  WebAssembly and the consoles' CPUs; virtual calls are not resolved to their
  targets.
- Swift names are demangled only by the CLI and MCP server, through
  `swift-demangle` when it is installed.
- Master System, Game Gear and PC Engine ROMs are not recognized yet.

The [reference](docs/reference.md#limitations-and-ideas) has the full list.

## Decompilation tooling

[Decompilation workspaces](docs/decompilation-workspaces.md) join physical units,
compiler call facts, actual linked providers, paired native audits and exact scoped
policies. Inspect caller packages in Contracts, CLI `workspace`, or persistent MCP.
Review typed adapter edits, storage/access extents and before/candidate promotion
plans. [Build batches](docs/build-batches.md) reuse content-addressed stage outputs;
[proof campaigns](docs/proof-campaigns.md) compare configured native/WASM runners.
Scoped matching publication, readability acceptance, selected service and memory
initialization closures, finite callback/stack models and exception comparisons
are available through the same workspace, CLI and MCP APIs. Unknown proof paths
remain explicit refusals. Start with the [recommended workflow and command guide](docs/decompilation-workspaces.md#recommended-workflow):
run identity preflight first, reuse native tools and caches, and execute campaigns
only when new behavioral evidence is needed. The [feature overlap review](docs/feature-overlap-review.md)
records consolidation candidates and the acceptance scopes to preserve.

[Progress images](docs/progress-images.md) export the built-in function treemap as
PNG or SVG. Open **Progress** (`R`) or run `binviz progress game.exe --svg progress.svg`.

## License

[MIT](LICENSE)
