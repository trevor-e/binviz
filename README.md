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
down to sections, owners and symbols. Functions are matched across builds by
name, bytes, instructions and the call graph, then lined up side by side. Drop
an Apple crash report, an Android tombstone or a stack trace and every frame
gets its function, source line and inlined calls from the binaries that are
open, each image found by build ID.

**WebAssembly and the Xbox.** A WebAssembly module opens like any binary: its
sections, functions, imports and exports, the name section, DWARF (a separate
`.debug.wasm` too) and source maps, and browsers' and Node's stack traces
symbolicated. An original Xbox executable (XBE) opens like the 32-bit PE it
holds, with its headers, certificate and kernel imports decoded.

**Folders of binaries.** Open a folder, a zip, an `.ipa`, an `.app` or a build
directory: every binary in it is found and paired with its debug file by build
ID. The folder's size is broken down by content, and the code of all its
binaries is summed by owner: Swift modules, Objective-C classes, C++
namespaces, C prefixes.

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
overlay opens with the boot executable's names, functions found by their
prologues, switch tables followed, and an emulator's trace of the code that
ran adds what pointers reach; the Psy-Q SDK's functions are named, with the
release that built them. For an x86 PC game: the compiler and linker from the
Rich header, calling conventions and stack frames with named slots, the C
runtime named from its `.lib`. Each function's code implies a prototype
(register and stack arguments, return value, frame, saved registers, the
structure offsets it walks), and one call gathers everything needed to write
its C, with worked examples: functions already matched that are shaped like
it, in this game or a sibling one built with the same compiler. m2c gets its
input for a first draft (`binviz asm`). The compiler's object file (MIPS ELF,
or COFF from MSVC or clang-cl) is scored against the original function by
function with the linker's fields masked, each difference explained
(registers, stack slots, branch lengths, reordering, delay-slot nops) with
what to try in the C, and builds with different flags ranked. A splat config
and symbol file start the project; objdiff's report places its progress on the
binary, and the notes export it again in that format for decomp.dev.
[samples/psx](samples/psx/README.md) walks the whole loop on a small program
built from source,
[samples/psx-advanced](samples/psx-advanced/README.md) adds an overlay, a memory
image, a trace and a jump table, and [samples/psx-vm](samples/psx-vm/README.md)
runs the whole loop through the MCP server with a script as the agent; the
[plan](docs/decomp-plan.md) says what is next.

**Built for agents.** The MCP server loads a binary once and answers every
question after that from memory, in milliseconds even for a 1 GB app: search,
inspect, disassemble, xrefs, call graphs, DWARF, diffs, crash symbolication,
and notes that the web UI can import. For a matching decompilation it keeps
the work queue too: which function to write C for next (callees first,
near-copies of matched functions before anything), claims so parallel agents
don't collide, and each function's outcome.

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
On Windows with the GNU toolchain, building `wasm-bindgen-cli` needs
`dlltool.exe` on `PATH` (it ships with MinGW-w64).

## The CLI

```bash
cargo run --release -p binviz-cli -- info path/to/binary
```

Run it with no arguments for the full command list: `info`, `layout`,
`inspect`, `check`, `symbols`, `strings`, `search`, `disasm`, `func`, `refs`,
`calls`, `coverage`, `globals`, `classes`, `objc`, `dwarf`, `header`,
`attribution`, `crash`, `diff`, `patch`, `relsearch`, `text` and `labels`, and
for a decompilation `signature`, `context`, `match`, `asm`, `m2c`, `flags`,
`report`, `progress`, `splat`, `sdk`, `locate`, `counterparts` and `names`.
`--debug` attaches a separate debug file, `--member` picks a slice of a
universal binary or a file in a folder, `--notes` loads annotations (and
`--types` the structures they name), and `--log` follows a ROM with an
emulator's code/data log. A folder, zip or CD image works in place of a file.

## The MCP server

```bash
cargo build --release -p binviz-mcp
```

```bash
claude mcp add binviz -- /path/to/binviz/target/release/binviz-mcp
```

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

## License

[MIT](LICENSE)
