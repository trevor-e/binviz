# binviz reference

The full description of every feature, view, command and design decision.
The [README](../README.md) is the short version.

binviz is a Rust library, with a WebAssembly-powered browser UI and a CLI, that
explains **ELF**, **Mach-O** and **PE/COFF** files down to individual header
fields, and maps machine code back to source through **DWARF** debug info.
It reads **game ROMs** too (NES, SNES, Game Boy, Game Boy Advance, Mega Drive,
Nintendo 64, PlayStation), with disassemblers for their CPUs.

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
  directions. Each DIE comes with the source behind it: where it is declared,
  where an inlined call was made, and which lines its code was generated from.
  Structures show their layout with holes and padding (like `pahole`); a
  unit's DIEs can be listed by tag; any `.debug_info` offset from
  `llvm-dwarfdump` output or an error message leads to its DIE; and at any
  address you get the variables in scope and where each one's value is.
  Separate debug files (`.dSYM` DWARF, `objcopy --only-keep-debug` output) can
  be attached, and so can a Windows binary's PDB: its modules, functions,
  globals and line records are read into DWARF, so everything above works
  for MSVC, clang-cl and Rust `-msvc` builds too. A Mach-O binary linked without dsymutil gets its DWARF from the
  object files its debug map names, moved to the binary's addresses as
  dsymutil would move it (functions reordered or dead-stripped included); the
  objects are found where they were built, next to the binary, anywhere in a
  folder opened whole, or in a folder you choose. Relocatable objects (`.o`)
  get synthetic addresses and relocated DWARF so they work too.
- **A checker for broken debug info.** Reads every unit, DIE, attribute and
  line program and lists what can't be read (unit headers, abbreviations,
  strings, range and location lists, line programs) or doesn't add up
  (references to no DIE, ranges that end before they start, files the line
  table doesn't define), each at its unit, DIE and section offset. Units that
  can't be read are reported rather than silently skipped.
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
- **Game ROMs, for reverse engineering.** NES (iNES, NES 2.0), SNES (LoROM,
  HiROM), Game Boy and Game Boy Color, Game Boy Advance, Mega Drive / Genesis
  (`.smd` dumps too), Nintendo 64 (`.z64`, `.v64`, `.n64`) and PlayStation
  executables (PS-X EXE) are recognized by their headers, which are decoded
  field by field with their checksums checked. Each bank gets an address of its
  own where the console's CPU sees it (bank 3's `$C000` is `0x3c000`); RAM and
  the hardware registers are regions, the registers named (`PPUCTRL`, `LCDC`,
  `INIDISP`, `DISPCNT`, `VDP_CTRL`, `VI_STATUS`, `GP1`...). The code is found by
  following it from the reset and interrupt vectors, each call and branch in
  turn, so a ROM's functions, call graph and cross-references (who writes
  `PPUCTRL`?) are there as for any binary. Disassemblers for the 6502, 65816
  (register widths followed through `rep` and `sep`), SM83, ARM7TDMI (ARM and
  Thumb), 68000 and MIPS (delay slots, `lui` pairs, `$gp`) name what the code
  touches, and jump tables are followed in the forms games' code uses (65816
  `jmp ($nnnn,x)`, the 6502's `pha`/`pha`/`rts` trick, 68000 offset and branch
  tables, ARM and Thumb switch tables). Addresses in banked ROMs read the way
  their debuggers write them, `03:C000` (on the SNES `$80:8000`), and go to
  and search take them too. PlayStation discs (`.bin` raw images or `.iso`)
  open to their files, the executable `SYSTEM.CNF` boots first; only the
  sectors needed are read. Files of no format binviz knows open as raw bytes.
- **Text and graphics in games.** Relative search finds text in a game's own
  encoding from a word it shows; table files (`.tbl`) read, search and dump the
  text, and the hex view reads it through the table. A tile viewer draws the
  graphics in the consoles' formats (1, 2, 4 and 8 bits per pixel, planar or
  linear) from any offset.
- **What emulators saw.** Play the game with FCEUX's or Mesen's code/data
  logger on and load its `.cdl`: the code the game ran is followed too (code
  reached only through jump tables and pointers), bytes it only read as data
  are never taken for code, each 65816 instruction decodes with the register
  widths it ran with and each ARM7 one as ARM or Thumb as it ran, and an NES
  game's switched banks go where they ran (MMC3's 8 KiB pages at `$A000`...).
  Label files come and go both ways: Mesen's `.mlb`, FCEUX's `.nl`, and the
  `.sym` files of RGBDS, WLA DX and no$gba become notes (banks placed), and
  your notes become label files for the emulator's debugger.
- **Function by function.** Two builds, two revisions of a game or a ROM and
  its patched copy are compared the way BinDiff does: functions matched by
  name, by identical bytes, by the same instructions (code that moved), through
  the call graph and, for ROMs, by address, even with no symbols at all; each
  pair identical, relocated (only addresses differ) or changed and how much,
  with its instructions lined up side by side; and the functions added and
  removed.
- **Patches.** Drop an IPS, UPS or BPS patch (a hack's, a translation's) on
  the open file: each run of bytes it changes is placed in its bank, function
  and region, with its bytes (and text, through the table file) before and
  after, and whether the patch was made for this version of the game is
  checked by CRC-32 (a patch made without the SNES copier header is applied
  past it). Or edit bytes in the hex view, hex digits or text: the edits save
  as an IPS, UPS or BPS patch, and the patched file opens with your notes.
- **Objective-C, recovered.** The class metadata a Mach-O image keeps even
  stripped is read back: classes, categories and protocols declared as their
  headers would (ivars with offsets, properties, methods with argument types,
  like class-dump), and every method's implementation named `-[Class selector]`,
  every selector reference `@selector(name)` and the metadata by the names
  clang gives it. For a selector: the methods implementing it, and the
  functions that send it (through `objc_msgSend` or an `objc_msgSend$` stub).
  Swift classes visible to Objective-C are listed as `Module.Class`.
- **Reverse-engineering coverage.** Stripped binaries get their functions back
  from `.pdata`, `.eh_frame` and `LC_FUNCTION_STARTS` (`sub_<address>`). A
  32-bit PE has none of these, so its code is followed instead, the way a
  disassembler maps it: from the entry point, exports, TLS callbacks and safe
  exception handlers, through every call, branch and jump table (MSVC's, kept
  in the code with a table of index bytes, too), knowing which calls never
  return (`ExitProcess`, and what only ends in it); then from the vtables,
  callbacks and other code addresses the image holds, its base relocations
  saying which words are addresses when it has them; then whatever else reads
  as a function. MSVC's `__finally` blocks stay in their functions, jump tables
  show as data in the disassembly, and each case's address says which table
  entry leads there. Name functions, comment instructions and mark code as
  reviewed; a coverage map shows what is named, recovered, reviewed or still
  unexplored, and lists the largest gaps with a guess at what they hold.
- **Folders of binaries.** Open a folder or a zip, and every binary in it is
  found by its header (Mach-O, ELF or PE, zips inside opened too), so an
  `.ipa`, an `.app` or `.xcarchive`, an APK or a build folder are all just
  folders. Each binary is paired with its separate debug file (a dSYM, an ELF
  `.debug` file, a PDB) by UUID or build ID, whatever the names, or by the
  debug link an ELF file names or the PDB a PE file names, so stripped
  binaries get their names and DWARF back.
  Debug files dropped later (the zip of dSYMs App Store Connect gives you, say)
  join what is open. The folder's size is broken down by content (asset
  catalogs, images, localizations, fonts…) with the largest and duplicated
  files, and the code of all its binaries is summed by owner: Swift modules,
  Objective-C classes, C++ namespaces, C prefixes.
- **What changed between builds.** Compare two binaries, or two folders or
  zips (two `.ipa` files, say): the files added, removed and grown, kinds of
  content, each binary, and inside the binaries their sections, owners and
  symbols. Paths line up even when the two builds' top folders are named
  differently.
- **Crash reports, symbolicated.** Drop or paste an Apple crash report
  (`.crash` or `.ips`), an Android tombstone or a stack trace: every frame gets
  its function, source line and inlined calls from the binaries that are open,
  each of the report's images found by UUID or build ID (open the app's folder
  with its dSYMs and they are all there). Images of another build are called
  out rather than symbolicated wrongly.

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
| Compare | What changed in size since an earlier build (choose it, or drop it on the view): files, kinds of content, binaries, and their sections, owners and symbols; for two binaries, their functions compared one by one, a pair's instructions side by side |
| Crash | A crash report symbolicated with what is open: each frame's function, source line and inlined calls, and the images the report needs; click a frame for its code |
| Folder | For a folder or zip: its binaries paired with their debug files, what its files are, the largest and duplicated ones, and the code of every binary summed by owner. Pick a binary (here or in the top bar) to explore it in the other views |
| Overview | Format facts, exact byte composition, a file map (by region, entropy or reverse-engineering coverage), and a diagram of how the file's bytes land in the address space |
| Layout | The file's parts. **Regions**: the region tree, expandable down to single fields and table entries. **Sections**: segments and sections as the loader sees them (a ROM's banks, RAM and registers). **Coverage**: how much of the code and data is mapped out, section by section, with the largest unexplored gaps |
| Hex | Every byte tinted by the structure that owns it; hover for the full path, minimap to navigate |
| Code | Functions and their disassembly with source lines (and source text, once loaded) interleaved; how many callers and callees each function has |
| Call graph | Callers and callees of the selected function, a few levels each way, with the complete lists below. Click a function to select it (the inspector shows it), double-click or **Centre here** to centre the graph on it; the functions centred on make a trail back. Find a path of calls from another function (`main`, an entry point) |
| Symbols | Symbols, imports, exports and strings; filter, sort, jump. Mach-O images with Objective-C get a Classes tab: each class, category and protocol as its header would declare it, with the functions sending each selector |
| DWARF | Units, the DIE tree or a unit's DIEs by tag, each DIE with its attributes (click a form for its bytes), declaration and call-site source, the lines its code came from and a structure's layout; line tables; Problems, a check of the whole DWARF |
| Sources | Every source file in the line tables, sized by the code and data it produced: where those bytes are in each section, its functions and variables, and its text, lines that produced code marked (click one for its addresses). **Units** shows the same per compilation unit: what each crate or `.cpp` adds |
| Text | Games' text (ROMs and raw files): relative search, a table file to edit, load and save, and the text it reads, searched or all of it |
| Tiles | Games' graphics: 8×8 tiles in the consoles' formats from any offset, as grays or hues |
| Patch | What a patch (dropped on the file) or your edits in the hex view change: each run of changed bytes placed in its bank, function and region, before and after, and the functions it changes, side by side; saved as IPS, UPS or BPS, or the patched file downloaded or opened |

The inspector on the right always shows everything known about the current
selection: its place in the file, what refers to it (callers, reads and
writes, pointers in data), and your notes about it. Views are on keys `1`–`8`
(`f` for Folder, `c` for Crash, `d` for Compare, `t` for Text, `g` for Tiles,
`p` for Patch). In the hex view, **Edit** types over bytes: hex digits in the
byte column, text in the text column (through the table file, when there is
one); `Ctrl+Z` takes an edit back.

Going somewhere (another view or tab, a link, a search result, a function
in the call graph) is a step in the browser's history: its Back and Forward
buttons (or `Alt+←/→`) walk it, even back to a file opened before. The URL
says where you are, so a sample's link opens right there:
`#sample=shapes-pe.exe&view=code&goto=total_area` (`goto` takes anything the
search box does; `tab` picks a view's tab).

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
(keyed by its SHA-256) and can be exported as JSON from **Layout → Coverage**.
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
| `info <file> [json]` | Summary, segments and sections (`json`: as JSON) |
| `layout <file> [depth]` | The region tree |
| `inspect <file> <address\|@offset>` | Everything about an address, or the byte at a file offset |
| `symbols <file> [filter]` · `strings <file> [filter]` | Symbols; strings in the data sections |
| `search <file> <query> [kind]` | The same search as the UI |
| `disasm <file> <addr\|symbol> [n]` | Disassembly with source lines |
| `func <file> <addr\|symbol>` | A function's callers, callees, strings and data |
| `refs <file> <addr\|symbol> [from]` | References to an address (`from`: the references a function or data makes) |
| `calls <file> <addr\|symbol> [up] [down]` · `calls <file> <from> to <to>` | The call graph around a function (`callers` or `callees` for just those); a shortest chain of calls |
| `coverage <file>` | Reverse-engineering coverage per section and the largest gaps |
| `objc <file> [name]` | Objective-C classes, categories and protocols; with a name, one declared as its header would, or a selector's implementations and senders |
| `dwarf <file> [check \| find \| die \| offset \| list \| at \| lines \| sources \| file]` | DWARF units, and: everything wrong with it; DIEs by name, a DIE, the DIE at a `.debug_info` offset, a unit's DIEs by tag; scopes and variables at an address; line tables, source files and their address ranges |
| `attribution <file> [unit] [id]` | Code and data per source file (or unit); with an id, that one's address ranges |
| `crash <file> <report>` | Symbolicate a crash report (Apple `.crash` or `.ips`, Android tombstone, stack trace) with a binary or a folder's binaries |
| `diff <old> <new>` · `diff <old> <new> functions [name]` | What changed in size between two builds (binaries, or folders or zips); which functions are which, and one function's code next to its match's |
| `patch <file> <patch> [out]` · `patch <old> <new> <out.ips>` | What an IPS, UPS or BPS patch changes, placed in banks and functions (`out`: the patched file); or the patch from one file to another |
| `relsearch <file> <word> [16] [tbl]` | Relative search: a word in the file's own text encoding; `tbl` prints the table it implies |
| `text <file> <table.tbl> [offset [length] \| text]` | Text read with a table file: all of it, at an offset, or where some text is |
| `labels <rom> <file \| format>` | An emulator's label file (`.mlb`, `.nl`, `.sym`) as notes, JSON for `--notes`; or with a format (`mlb`, `nl`, `sym`, `nocash`), the `--notes` as that label file |
| `check <file>` | Verify every byte is covered by the layout |

The commands before these (`at`, `xrefs`, `refs-from`, `callers`, `callees`,
`callgraph`, `callpath`, `die`, `lines`, `sources`, `file-lines`,
`dwarf-check`, `dwarf-list`, `dwarf-find`, `dwarf-offset`, `dwarf-at`,
`attributed`, `json`) still work.

`--debug <file>` attaches a separate debug file (or names the folder holding
the object files of a Mach-O debug map); `--member <n>` picks a slice of
a universal binary or an archive member; `--notes <file.json>` loads
annotations first; `--log <file.cdl>` follows a game ROM's code with an
emulator's code/data log.

A folder or a zip (an `.ipa`, an `.app`, a build) works in place of a file:
`info` lists every binary in it with its debug file, what the other files are,
and the code of all the binaries summed by owner; `search` searches them all;
`info json` describes the folder; any other command works on the first binary
(an app's own executable) or the one `--member` names, with its debug file
attached. A CD image opens the executable the disc boots (`--member` picks
another file).

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
| `open_binary` | Load a file (universal binaries pick arm64 unless told otherwise; `debug_file` attaches a dSYM's DWARF), or a folder or zip: every binary in it opens, paired with its debug file |
| `size_diff` | What changed in size between two builds on disk: two binaries, or two folders or zips |
| `symbolicate` | A crash report (Apple `.crash` or `.ips`, Android tombstone, stack trace) symbolicated with the open binaries, each image found by UUID or build ID |
| `folder_summary` | An opened folder: its binaries and debug files, what its size is made of, the largest and duplicated files; `analyze` sums the code of every binary by owner |
| `binary_summary` | Format, platform, entry point, build ID, segments, sections, DWARF |
| `size_report` | Why the binary is as big as it is: bytes by kind and section, the largest functions and data, and the owners of the code — Swift modules, Objective-C classes, C++ namespaces / Rust crates, C prefixes, compiler-generated helpers — and source files with DWARF |
| `search` | The UI's search: names, strings, addresses, byte patterns, "text", file:line; `binary: "all"` searches every binary of a folder |
| `inspect` | Everything about an address or file offset |
| `disassemble` | A function, with source lines and your comments |
| `function_info` | A function at a glance: callers, callees, the strings it uses, the globals it reads and writes |
| `callers` · `callees` · `call_graph` · `call_path` | Follow calls: who calls what, the tree around a function, how one function reaches another |
| `xrefs` | Every reference to an address, symbol or string: calls, reads, writes, address-taken, pointers in data |
| `objc` | Objective-C classes, categories and protocols; one declared as its header would; a selector's implementations and the functions sending it |
| `relative_search` · `table_text` | Games' text: find a word in the game's own encoding, then read, search and dump the text with a table file |
| `diff_functions` | Two builds compared function by function (stripped ones too): matches, what changed, what was added and removed, one function's code next to its match's |
| `patch` | What an IPS, UPS or BPS patch changes in the open file (checked by CRC-32, each change placed in its bank, function and region); or the patch that turns it into a modified copy |
| `code_log` · `labels` | A ROM's code followed with an emulator's code/data log; label files imported into the notes, or written from them |
| `dwarf_units` · `dwarf_search` · `dwarf_dies` · `dwarf_die` | Browse the DWARF: units, DIEs by name or tag, one DIE with its attributes, source, code lines and layout |
| `dwarf_at` | At an address: the source line, the inlined call stack, and the variables in scope with where each value lives |
| `dwarf_check` | Everything in the DWARF that can't be read or doesn't add up — start here with a customer's broken build |
| `list_symbols` · `list_strings` · `hexdump` | Browse tables and bytes |
| `coverage` | How much is mapped out, and the largest unexplored gaps |
| `annotate` · `remove_annotation` · `list_annotations` | Name functions, comment addresses, mark code reviewed |
| `next_functions` · `mark` · `similar_functions` · … | For a matching decompilation: see [Decompilation](#decompilation-mips-playstation-nintendo-64) |

Notes are saved next to the binary in `<file>.binviz-notes.json`, the format
the web UI imports and exports (Layout → Coverage → Import), so an agent can map
out a binary and you can look at the result in the UI, or the other way round.

Things to ask: *"Open ~/Downloads/MyApp and tell me why it's so big"*, *"Find
the code that parses deep links and name what you find"*, *"Which functions
use this error string, and how are they reached from main?"*, *"What haven't
we looked at yet?"*.

## Decompilation (MIPS: PlayStation, Nintendo 64)

What a matching decompilation needs from the binary side, wired into the
CLI and the MCP server:

| CLI | MCP | |
|---|---|---|
| `--psx-exe <exe>` | `open_binary` `psx_exe` | A PlayStation memory image (2 MiB of RAM dumped by an emulator; recognized by the kernel at its start) or an overlay opens with the boot executable's functions named, so calls into it read as in it. Functions are found by their prologues (`addiu $sp` with `$ra` saved) as well as by following the code |
| `--overlay-at <addr>` | `open_binary` `overlay_at` | Open a file as a code overlay loaded at an address. `Binary::psx_locate` finds where a file from the disc sits in a memory image |
| `--trace <file>` | `open_binary` `trace` | A trace of the code an emulator ran (any text with an address per line): each run of traced code the following hadn't reached is followed as a function, after everything reachable, so functions aren't split |
| `signature <file> <fn>` | `function_signature` | What a function's code says about its prototype: `$a0`–`$a3` read before written, stack arguments, whether `$v0` carries a result, frame size, saved registers, calls, GTE/FPU use, and the offsets loaded and stored off each base register (structure layout hints, named after the argument they came from) |
| — | `next_functions` · `mark` | The work queue (any architecture): what to write C for next, best first, and claiming it so parallel agents don't collide; each outcome recorded in the notes (see below) |
| — | `similar_functions` | The functions whose instructions are shaped most like a function's, with where each stands: a matched one's C is the worked example |
| `context <file> <fn> [n]` | `decomp_context` | The code with names resolved, the signature, callers and callees with theirs, strings, globals and notes, where decompiling it stands, and the matched functions shaped like it with their source files: one call per function |
| `match <file> <obj> [name]` | `match_function`, `match_object` | The compiler's object file (ELF, MIPS) scored against the original: instructions lined up by shape, relocation fields masked, each relocation's symbol checked against where the original points, every difference explained (registers allocated differently, stack frame or slot size, branch length, reordering, a nop missing from a delay slot) |
| `report <file> <json>` | `place_report` | objdiff's report placed on the binary's functions by virtual address or name; over MCP, recorded in the notes too (matched, best percent, what no longer matches) |
| `splat <file> <name> [dir] [splits…]` | `splat_export` | A splat YAML config (header, the code segment at its load address split into units, the bytes after the last function as data, the BSS size) and `symbol_addrs.txt` naming every function and known place |
| `splat <file> import <syms>` | `import_symbol_addrs` | A splat symbol file's names into the notes (splat's own `func_…`/`D_…` names left out) |
| `sdk <file> <libs…> [notes]` | `identify_sdk` | The Psy-Q SDK's functions in the binary, found by the signatures of its `.LIB`/`.OBJ` files (Sony's `LNK` object format, the linker's fields masked): each named, with an `sdk:` note and marked library code so a decompilation leaves it be (its callers don't wait on it), and the libraries the game was linked with |
| `locate <ram.bin> <file>` | (`Binary::psx_locate`) | Where a file from the disc sits in a memory image: which of the overlays sharing an address is the one loaded |
| `names <file> <json> [min%]` | `propose_names` | Names from another build of the game (a port with its source, a symbolized build): its functions with the strings they use and the functions they call, matched to functions here by shared strings, then through the calls; each with a confidence and the evidence |

Strings in a console's code area (a PlayStation executable's one section, a
ROM's banks) are found like those in data sections, so the strings a
function uses show for PlayStation games too.

The notes keep where each function stands: matched (and its source file),
nonmatching, tried and how close it came, claimed by an agent, set aside, or
library code. `next_functions` ranks the rest from what they call, for any
binary binviz disassembles: first the functions shaped like one already
matched (90 % of the same instructions, so its C is a template), then those
whose callees are all done, small ones that many callers wait on first, then
those still waiting; a function tried three times without matching waits
until something it calls is done. Functions are compared by a MinHash of
their instructions' shapes (operations and operand kinds, never addresses or
numbers), so `similar_functions` answers at once even for tens of thousands
of functions.

MIPS switch tables (the `sltiu` guard, `sll … 2`, `lui`/`addu`/`lw`, `jr`
idiom GCC and IDO write) are followed for PlayStation and Nintendo 64 code,
so the cases of a `switch` are part of their function.

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
  src/decomp.rs      a matching decompilation: where it stands, what to do next
  src/similar.rs     functions shaped alike (MinHash of instruction shapes, banded)
  src/discover/      function recovery from .pdata, .eh_frame, LC_FUNCTION_STARTS,
                     and by following x86 code (x86.rs) for 32-bit PE images
  src/strings.rs     strings in data sections
  src/xrefs.rs       cross-references and the call graph
  src/pointers.rs    pointers stored in data: chained fixups, dyld binds, ELF relocations, plain addresses
  src/objc.rs        Objective-C metadata: classes, categories, protocols, selectors, their names
  src/rom/           game ROMs: each console's header, memory map and registers, and
                     the code followed from the vectors (analysis.rs); emulators'
                     code/data logs (cdl.rs) and label files (labels.rs)
  src/cpu/           decoders for consoles' CPUs: 6502 and 65816, SM83, ARM7TDMI, 68000, MIPS
  src/tables.rs      games' text: relative search and table files
  src/patch.rs       IPS, UPS and BPS patches: applying, creating, where they change things
  src/stubs.rs       names for import stubs, PLT entries, GOT and IAT slots
  src/size.rs        where the bytes go: sections, symbols, owners (Swift, ObjC, C++, C)
  src/diff.rs        what changed in size between two builds
  src/fndiff.rs      functions matched across builds (BinDiff-like) and lined up
  src/disc.rs        CD images: ISO 9660 in raw or cooked sectors, what the disc boots
  src/crash.rs       crash reports: reading them, finding their images, symbolicating frames
  src/package.rs     folders of binaries: headers, debug files paired by build ID or name, contents by kind
  src/zip.rs         zip archives: the central directory, stored and deflated entries
  src/plist.rs       property lists, binary and XML
  src/dwarf/attribution.rs   code and globals per source file / unit
  src/dwarf/explore.rs       DIEs by tag, by offset and by name; variables in scope
  src/dwarf/check.rs         the DWARF checker
  src/dwarf/debugmap.rs      Mach-O debug maps: the objects' DWARF, linked to the binary
  src/dwarf/pdb.rs           PDBs: modules, procedures and line records, written as DWARF
crates/binviz-wasm   wasm-bindgen bindings (a Session object)
crates/binviz-cli    the command-line tool
crates/binviz-mcp    the MCP server for agents
web/                 TypeScript UI; the WASM runs in a Web Worker
tests/fixtures/      small ELF / Mach-O / PE test binaries and their sources, and
                     hand-assembled ROMs (src/roms.py writes them)
scripts/build-fixtures.sh   regenerates the fixtures with rust-lld (no SDKs needed)
```

Tests run against the fixtures and check, among other things, that every byte
of every fixture is explained and that every instruction of the test functions
maps back to its own source.

```bash
cargo test
```

## Limitations and ideas

- Swift names are demangled by the CLI and the MCP server through
  `swift-demangle`, when it is installed (with a Swift toolchain, or Xcode's
  through `xcrun`); the web UI shows them mangled. Their owners (Swift
  modules) are found either way.

- From a PDB, binviz reads modules, procedures, globals, public symbols and
  line records, not types or local variables (its type records aren't turned
  into DWARF types), so variables in scope and structure layouts need DWARF.
- Split DWARF (`.dwo`/`.dwp`) is detected but not followed.
- `.eh_frame`, dyld opcode streams and chained fixups are shown as regions but
  not decoded entry by entry (chained fixups are walked to find pointers).
- Disassembly covers x86, x86-64, AArch64 and ARM (A32); cross-references and
  the call graph x86, x86-64 and AArch64.
- Calls through registers are followed only when the register was just loaded
  from a pointer slot (`ldr x16, [got]; blr x16`, `call r14`); virtual calls are
  not resolved to their targets yet. Objective-C messages are followed by
  selector (who sends `hello`), not to the one method a send reaches at run time.
- ROMs: bank switching is modelled as the common mappers arrange it (NES: 16
  KiB banks switched at `$8000` with the last fixed at `$C000`, or 32 KiB
  banks); code a fixed bank calls in a switched window isn't followed, since
  which bank is there depends on the game. Only the first megabyte of a
  Nintendo 64 game (what the boot code copies) is placed; the rest of the
  cartridge is data at `0xB0000000`. Jumps through tables are followed in the
  common forms above; others (MIPS switch tables, computed jumps) only when a
  code/data log saw where they went.
  Master System, Game Gear and PC Engine ROMs aren't recognized yet (they open
  as raw bytes).
- CD images: the ISO 9660 file system on Mode 1 and Mode 2 Form 1 data
  sectors; XA audio and video files (Form 2) read as if they were data.
- Objective-C metadata is read from images outside the dyld shared cache;
  Swift's own metadata (beyond classes visible to Objective-C) is not.
