# binviz reference

The full description of every feature, view, command and design decision.
The [README](../README.md) is the short version.

binviz is a Rust library, with a WebAssembly-powered browser UI and a CLI, that
explains **ELF**, **Mach-O**, **PE/COFF** and **WebAssembly** files down to
individual header fields, and maps machine code back to source through
**DWARF** debug info. It reads **game ROMs** too (NES, SNES, Game Boy, Game Boy
Advance, Mega Drive, Nintendo 64, PlayStation), with disassemblers for their
CPUs, and original Xbox executables (**XBE**).

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
  globals, line records and type records are read into DWARF (the types in a
  unit of their own, which the modules' functions and variables refer to), so
  everything above works for MSVC, clang-cl and Rust `-msvc` builds too,
  structure layouts and function signatures included. A Mach-O binary linked without dsymutil gets its DWARF from the
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
- **C headers from debug info**, DWARF or PDB: every structure, union, enum
  and typedef (or those named, with what they need) in an order C accepts,
  forward declarations for what is only pointed at, and prototypes of the
  external functions with their calling conventions. The layout doesn't depend
  on a compiler's rules: every gap is a padding member (`char _pad_1c[4];`),
  bit fields sit in storage units Microsoft's and System V's rules place
  alike, members a PDB flattened out of an anonymous union are a union again,
  and `#pragma pack` appears only where natural alignment can't give the
  layout. Each structure is followed by `_Static_assert`s on its size and
  member offsets, so compiling the header (`clang -fsyntax-only -m32 -x c`)
  proves it. C++ classes become structures with their bases embedded first and
  a `void **__vftable`; qualified names are flattened (`geo::Rect` is
  `geo__Rect`), the original in a comment. The same model names the member at
  an offset of a structure (`[esi+0x21c]` in an `edict_t`: `enemy`,
  `s.origin[1]`).
- **Fields named in the code.** Where the code reaches through a pointer whose
  type is known, `disasm` and `context` name the member:
  `mov [esi+0xc], eax  <edict_t.health>`, `fld [ecx+0x14]  <cvar_t.value>`,
  `fstp [0x2000306c]  <level.time>`. The stack walk follows each argument into
  the registers that hold it, and each pointer loaded from a field or a global
  on to what is read through it (`level.sight_client->health`); the types
  come from the function's parameters and the globals' types in the debug
  info (a PDB's too), or, for a stripped binary, from notes: a function's
  prototype (`"type": "void SP_monster_soldier(edict_t *self)"`) or a global's
  type (`"level_locals_t"`, `"cvar_t *"`), with the structures from a types
  file (`--types`, MCP `types_file`): the program's headers compiled with
  `clang -g -fno-eliminate-unused-debug-types -c -x c game.h` for the
  binary's target, or a PDB.
- **Disassembly** for x86/x86-64 (iced-x86), AArch64/ARM (yaxpeax-arm) and
  WebAssembly (binviz's own decoder), with branch targets resolved to symbols,
  the strings and globals an instruction uses named inline (AArch64 `adrp`
  pairs included), and source lines interleaved.
- **Call graph and cross-references.** Who calls a function, what it calls, a
  path of calls from one function to another, and every reference to an
  address: calls, tail calls, reads, writes, address-taken, and pointers stored
  in data (vtables, Objective-C metadata, callbacks). Import stubs, PLT entries
  and GOT/IAT slots are named after what they import (`_objc_msgSend`,
  `printf@plt`, `__imp_CreateFileW`, an XBE's `__imp_KeBugCheck`), so calls
  into libraries read as such, in 32-bit code too (`call dword ptr [0x4021b4]`).
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
- **Original Xbox executables.** An XBE (a game's `default.xbe`) opens like a
  32-bit PE. Its headers are decoded field by field: the image header (the
  entry point and the kernel thunk table's address shown encoded and decoded,
  with the key that decodes them), the certificate (title ID and name,
  alternate title IDs, media, regions, ratings, version, keys), the section
  headers with their names and shared page counters, the library versions,
  the debug file names and the logo; so are the kernel thunk table and the
  TLS directory inside the sections. Which keys decode the entry point and the
  thunk table's address into the image says whether it is a retail or a debug
  (development kit) build, and XAPILIB's library version says which XDK built
  it (`XDK 5849`), next to every library linked (`D3D8 1.0.5849 (QFE 1)`).
  Each section is placed at its address, its permissions and kind from its
  flags. The kernel is imported by ordinal: each thunk slot becomes an import
  from `xboxkrnl.exe` and a `__imp_` symbol named after the export (the
  ordinals of the open-source Xbox toolchain and emulator, nxdk and
  Cxbx-Reloaded, for exports 1 to 366; others read `xboxkrnl.exe #N`). The
  code is followed as a 32-bit PE's is, from the entry point and TLS
  callbacks, knowing which kernel calls never return (`HalReturnToFirmware`,
  `KeBugCheck`, `KeBugCheckEx`, `PsTerminateSystemThread`), so disassembly,
  references, the call graph, coverage and the decompilation context work on
  it. A folder of a game's files lists its XBEs like other binaries.
- **WebAssembly, for a browser port.** A module (Emscripten's, wasm-ld's,
  rustc's, or a relocatable `.o`) is laid out section by section down to each
  entry: types, imports, functions, tables, memories, globals, exports, the
  start function, element and data segments, each function body's locals and
  code, and the custom sections (the name section's subsections, `producers`,
  `target_features`, `build_id`, `sourceMappingURL`, `external_debug_info`, an
  object's `linking` and `reloc.*`, and `.debug_*`, decoded as DWARF is
  anywhere else). Its code has no address space of its own, so a module's
  addresses are its file offsets, the numbers browsers and Node print
  (`wasm-function[12]:0x1a2b`); a function is at its body's locals, where its
  DWARF says it starts once moved. Linear memory is placed at `0x80000000`
  (memory address `0x400` is `0x80000400`): each data segment is a section
  there, named as the linker named it (`.rodata`, `.data`), and the
  zero-filled memory above them is `.bss` up to the last variable known or,
  with none known, `bss and stack` up to where the stack pointer starts.
  Functions are named by the name section, an object's symbol table, DWARF or
  their exports, else `func[N]` (the index stack traces give); imports sit at
  their import entries, globals at theirs, and variables come from DWARF, the
  symbol table or exported globals. DWARF 4 and 5 are read with their
  addresses moved: code offsets counted from the code section's contents
  become module offsets, memory addresses move up to `0x80000000`, and what
  the linker dropped (its tombstones) stays out. A source map (`emcc
  -gsource-map`) attaches as a debug file and becomes a line table, and so
  does the module with the DWARF (`emcc -gseparate-dwarf`'s `.debug.wasm`, or
  an unstripped build of the same code, which names functions too); the CLI
  and the MCP server attach the one a module names when it is beside it. The
  disassembler reads the MVP, sign extension, the `0xFC` group (saturating
  truncation, bulk memory, tables), reference types, tail calls, exception
  handling (both encodings), typed function references, SIMD and relaxed SIMD,
  and atomics: branches go to where their block ends or their loop starts,
  calls name their functions, a `call_indirect` says how many functions of its
  type its table holds, and loads and stores off a constant address name the
  variable. Calls, `ref.func`, globals read and written, memory at constant
  addresses, the function tables' slots and pointers between data are
  cross-references, so callers, the call graph and coverage work as they do
  for native code, and two builds compare by size and function by function.
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
- **C++ classes, recovered.** A Windows binary built by MSVC (or clang-cl)
  with RTTI keeps, stripped, a type descriptor for each polymorphic class, a
  class hierarchy descriptor listing its bases and where their sub-objects
  sit, and before each vtable a complete object locator. binviz reads them
  back and names each structure as MSVC does (`const Label::`vftable'{for
  `Named'}`, `shapes::Square::`RTTI Class Hierarchy Descriptor'`), so a
  stripped binary reads as its PDB would; each class lists its bases and its
  vtables with the virtual function in every slot, and a function's context
  says which vtable slots hold it.
- **Reverse-engineering coverage.** Stripped binaries get their functions back
  from `.pdata`, `.eh_frame` and `LC_FUNCTION_STARTS` (`sub_<address>`). A
  32-bit PE (or an XBE) has none of these, so its code is followed instead, the way a
  disassembler maps it: from the entry point, exports, TLS callbacks and safe
  exception handlers, through every call, branch and jump table (MSVC's, kept
  in the code with a table of index bytes, too), knowing which calls never
  return (`ExitProcess`, and what only ends in it); then from the vtables,
  callbacks and other code addresses the image holds, its base relocations
  saying which words are addresses when it has them; then whatever else reads
  as a function. An x86-64 PE lists its functions in `.pdata`, all but the
  leaf functions, which need no unwind data: its code is followed from those
  to find them. MSVC's `__finally` blocks stay in their functions, jump tables
  show as data in the disassembly, and each case's address says which table
  entry leads there. `disasm` and `context` mark each switch: at its jump,
  the cases and where the rest go; at each case's code, the values that lead
  there (`cases 1, 4:`, `default:`), counted from what the code took off the
  value before checking it; at a table kept in the code, what it is. 64-bit
  code's tables are read too — MSVC's offsets from the image base
  (`__ImageBase`), clang's and GCC's offsets from the table, addresses — and
  in an ELF or Mach-O binary, whose code nothing follows, each switch is read
  from the code before its jump. Code the optimizer moved away from its function's entry
  (a piece reached only by a jump from it, with no frame of its own) stays
  that function's: the disassembly and the context show it after the entry's
  code, and the stack is followed through it. A function whose last
  instruction runs on into the next function is another way into that one
  (an alternate entry, as MSVC's `_CIsqrt` is to `sqrt`), and its context
  says so; functions the linker folded into one show all their names. Name functions, comment instructions and mark code as
  reviewed; a coverage map shows what is named, recovered, reviewed or still
  unexplored, and lists the largest gaps with a guess at what they hold.
- **Data typed by its use.** Every address in data that the code reads,
  writes, calls through or takes, or that a pointer in data points at, is a
  global, and how the code uses it says what it is: read as a `float` or a
  `double` (with its value, when nothing writes it), a byte, a word…; indexed
  (an array of elements that size); a pointer read and then reached through
  (a pointer to a structure, with the offsets used: a cvar's value at `0x14`);
  pointers stored in it (a table of functions — callbacks, a vtable — of
  strings, of other data; records with a pointer in the same place in each, a
  monster's frame table; a structure holding pointers); called through, its
  slots filled at run time (a game DLL's table of engine functions, with the
  `rep movsd` that fills it); a `switch`'s jump table. Where nothing names
  it, it is named the way disassemblers do — `flt_4020a0`, `funcs_402000`,
  `stru_20003024`, `fptrs_2000309c+0x30` — in the disassembly, the context,
  cross-references and inspection; a note naming it wins. In 32-bit x86
  code the disassembly names the absolute addresses instructions use and
  the addresses they push or store (`push offset string`, `mov [esi+0x24],
  offset callback`), and strings a letter or two long show once the code
  takes their address.
- **Which compiler built it.** A PE file's Rich header, where Microsoft's
  linker records the tools that made each object it linked, is decoded: each
  entry names its tool (C or C++ compiler, with link-time code generation or
  profile-guided optimization; assembler; linker; resource converter…), the
  tool's version and build, and the Visual C++ release it shipped with, down
  to the service pack or update where the build number says. The summary
  names the compiler that made most of the code, and the linker — what a
  matching decompilation has to rebuild it with. The header's checksum is
  checked, so one edited after linking shows.
- **Folders of binaries.** Open a folder or a zip, and every binary in it is
  found by its header (Mach-O, ELF, PE or XBE, zips inside opened too), so an
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
  out rather than symbolicated wrongly. A browser's or Node's stack trace
  through WebAssembly works the same way: Chrome's and Node's
  (`at f (https://…/app.wasm:wasm-function[12]:0x1a2b)`, `wasm://wasm/…` for
  a module compiled from bytes), Firefox's (`f@…:wasm-function[12]:0x1a2b`)
  and Safari's (`wasm-function[12]@[wasm code]`, the function alone), the
  JavaScript frames between them kept as they are.

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
| `globals <file> [filter]` | The data the code uses, typed by its use and named where nothing names it (see below) |
| `objc <file> [name]` | Objective-C classes, categories and protocols; with a name, one declared as its header would, or a selector's implementations and senders |
| `classes <file> [filter]` | C++ classes from an MSVC binary's RTTI: bases with their offsets, vtables with their virtual functions |
| `dwarf <file> [check \| find \| die \| offset \| list \| at \| lines \| sources \| file]` | DWARF units, and: everything wrong with it; DIEs by name, a DIE, the DIE at a `.debug_info` offset, a unit's DIEs by tag; scopes and variables at an address; line tables, source files and their address ranges |
| `header <file> [name...]` | The debug info's types and external functions as a C header that checks its own layout (a PDB's with `--debug`); with names, those and the types they need |
| `attribution <file> [unit] [id]` | Code and data per source file (or unit); with an id, that one's address ranges |
| `crash <file> <report>` | Symbolicate a crash report (Apple `.crash` or `.ips`, Android tombstone, stack trace, a browser's or Node's through WebAssembly) with a binary or a folder's binaries |
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
the object files of a Mach-O debug map; for a WebAssembly module, a source map
or the module with its DWARF, which are attached without it when the module
names one that is beside it); `--member <n>` picks a slice of
a universal binary or an archive member; `--notes <file.json>` loads
annotations first (each `{"address", "size", "name", "comment", "reviewed",
"kind", "type"}`, all but the address optional); `--types <file>` takes the
structures notes' types name from another file's debug info; `--log
<file.cdl>` follows a game ROM's code with an emulator's code/data log.

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
| `open_binary` | Load a file (universal binaries pick arm64 unless told otherwise; `debug_file` attaches a dSYM's DWARF, a PDB, or a WebAssembly module's source map or DWARF module), or a folder or zip: every binary in it opens, paired with its debug file |
| `size_diff` | What changed in size between two builds on disk: two binaries, or two folders or zips |
| `symbolicate` | A crash report (Apple `.crash` or `.ips`, Android tombstone, stack trace, a browser's or Node's through WebAssembly) symbolicated with the open binaries, each image found by UUID or build ID |
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
| `cpp_classes` | C++ classes from an MSVC-built binary's RTTI, even stripped: bases and where their sub-objects sit, vtables and their virtual functions |
| `relative_search` · `table_text` | Games' text: find a word in the game's own encoding, then read, search and dump the text with a table file |
| `diff_functions` | Two builds compared function by function (stripped ones too): matches, what changed, what was added and removed, one function's code next to its match's |
| `patch` | What an IPS, UPS or BPS patch changes in the open file (checked by CRC-32, each change placed in its bank, function and region); or the patch that turns it into a modified copy |
| `code_log` · `labels` | A ROM's code followed with an emulator's code/data log; label files imported into the notes, or written from them |
| `dwarf_units` · `dwarf_search` · `dwarf_dies` · `dwarf_die` | Browse the DWARF: units, DIEs by name or tag, one DIE with its attributes, source, code lines and layout |
| `dwarf_at` | At an address: the source line, the inlined call stack, and the variables in scope with where each value lives |
| `dwarf_check` | Everything in the DWARF that can't be read or doesn't add up — start here with a customer's broken build |
| `c_header` · `struct_field` | The types (all, or those named with what they need) and function prototypes as a C header whose layout checks itself when compiled, from DWARF or a PDB; which member of a structure is at an offset (`edict_t.enemy`, `entity.matrix[1][2]`) |
| `list_symbols` · `list_strings` · `hexdump` | Browse tables and bytes |
| `list_globals` | The data the code uses, typed by its use: floats and doubles with their values, integers by width, pointers to structures with the offsets reached through them, tables of functions, strings or pointers, records holding pointers, arrays, jump tables, and tables of function pointers filled at run time and called through (an engine's import table, with where it is filled) |
| `coverage` | How much is mapped out, and the largest unexplored gaps |
| `annotate` · `remove_annotation` · `list_annotations` | Name functions, comment addresses, mark code reviewed, give a function its prototype or data its type (which names the fields reached through them) |
| `next_functions` · `mark` · `similar_functions` · `match_project` · … | For a matching decompilation: see [Decompilation](#decompilation-playstation-nintendo-64-x86-pc) |

Notes are saved next to the binary in `<file>.binviz-notes.json`, the format
the web UI imports and exports (Layout → Coverage → Import), so an agent can map
out a binary and you can look at the result in the UI, or the other way round.

Things to ask: *"Open ~/Downloads/MyApp and tell me why it's so big"*, *"Find
the code that parses deep links and name what you find"*, *"Which functions
use this error string, and how are they reached from main?"*, *"What haven't
we looked at yet?"*.

## Decompilation (PlayStation, Nintendo 64, x86 PC)

What a matching decompilation needs from the binary side, wired into the
CLI and the MCP server:

| CLI | MCP | |
|---|---|---|
| `--psx-exe <exe>` | `open_binary` `psx_exe` | A PlayStation memory image (2 MiB of RAM dumped by an emulator; recognized by the kernel at its start) or an overlay opens with the boot executable's functions named, so calls into it read as in it. Functions are found by their prologues (`addiu $sp` with `$ra` saved) as well as by following the code |
| `--overlay-at <addr>` | `open_binary` `overlay_at` | Open a file as a code overlay loaded at an address. `Binary::psx_locate` finds where a file from the disc sits in a memory image |
| `--trace <file>` | `open_binary` `trace` | A trace of the code an emulator ran (any text with an address per line): each run of traced code the following hadn't reached is followed as a function, after everything reachable, so functions aren't split |
| `signature <file> <fn>` | `function_signature` | What a function's code says about its prototype: `$a0`–`$a3` read before written, stack arguments, whether `$v0` carries a result, frame size, saved registers, calls, GTE/FPU use, and the offsets loaded and stored off each base register (structure layout hints, named after the argument they came from). For x86 and x86-64 too: the stack pointer is followed from the entry through pushes, calls (what each callee pops: `ret N` in its code, the arguments of a Windows import, the stack adding up at the returns for a call through a pointer), merged `add esp, N`, `ebp` frames and `and esp, -N` alignment, so the calling convention (cdecl, stdcall, thiscall, fastcall; Microsoft x64, System V), the arguments read (a `float` or `double` where the code reads one, an argument on the x87 stack as `double st0`), the frame, the saved registers and a result in `eax` or on the FPU stack come out, and every stack slot in `disasm` and `context` is named against the entry frame (`arg2`, `local_10`, `saved esi`) however the pushes before calls move `esp` |
| — | `next_functions` · `mark` | The work queue (any architecture): what to write C for next, best first, and claiming it so parallel agents don't collide; each outcome recorded in the notes (see below) |
| — | `similar_functions` | The functions whose instructions are shaped most like a function's, with where each stands: a matched one's C is the worked example |
| `context <file> <fn> [n]` | `decomp_context` | The code with names resolved (switches' cases and pieces marked), the signature, callers and callees with theirs, strings, globals and notes (the first 24 of each list, with how many more), where a callback's address is taken, where decompiling it stands, and the matched functions shaped like it with their source files: one call per function |
| `match <file> <obj> [name]` | `match_function`, `match_object` | The compiler's object file (MIPS ELF; x86 or x86-64 COFF from MSVC or clang-cl, or ELF) scored against the original: instructions lined up by shape, relocation fields masked, each relocation's symbol checked against where the original points, every difference explained (registers allocated differently, stack frame or slot size, branch length, reordering, a nop missing from a delay slot; for x86, see below). Functions are found by the object's names as they are, undecorated (`_foo`, `_foo@8`, `@foo@8`) or demangled (`?scaled@Shape@@QBEHH@Z` is `Shape::scaled`), so a PDB's names and the user's notes work |
| `match <file> <folder>` | `match_project` | Every object of a build folder (and its subfolders; or a list, over MCP) matched against the original: the totals (functions compared and matching exactly, bytes of code in exact matches, the percent weighted by size), each object (unit) worst first with its worst functions, and the functions not matching yet with their kinds of difference; over MCP, `record` puts the outcomes in the notes as `place_report` does, each unit the source file of its matches |
| `report <file> <json>` | `place_report` | objdiff's report placed on the binary's functions by virtual address or name; over MCP, recorded in the notes too (matched, best percent, what no longer matches) |
| `splat <file> <name> [dir] [splits…]` | `splat_export` | A splat YAML config (header, the code segment at its load address split into units, the bytes after the last function as data, the BSS size) and `symbol_addrs.txt` naming every function and known place |
| `splat <file> import <syms>` | `import_symbol_addrs` | A splat symbol file's names into the notes (splat's own `func_…`/`D_…` names left out) |
| `sdk <file> <libs…> [notes]` | `identify_sdk` | Library code in the binary, found by the signatures of the libraries' functions (their code with the linker's fields masked): a PlayStation game's Psy-Q SDK (its `.LIB`/`.OBJ` files, Sony's `LNK` object format), or a Windows program's statically linked C runtime (MSVC's `.lib` archives of COFF objects, `.obj` files; ELF `.a`/`.o` too). Each function found is named, with an `sdk:` note, and marked library code so a decompilation leaves it be (its callers don't wait on it); the libraries it was linked with are counted. Import library members and objects with no machine code (compiled with `/GL` or `-flto`) are skipped, and said so. Signatures shorter than 16 bytes aren't used (they match by chance); an x86 function takes a signature's name only when it is as long as the signature, padding aside |
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

For x86 and x86-64, instructions are lined up by their shape (the operation,
the kinds of its operands, the registers in them and the size of what it
reads), not their numbers. The bytes a relocation covers are masked on both
sides, and its symbol, when the binary knows the name, is checked against
where the original points (`call target differs: calls _diff in the rebuild;
the original calls clamp`, `global differs: …`). A jump inside the function
is compared by where it lands, so code of another length before it counts
once; MSVC's jump tables in the code after a function (x64's holding
offsets from the image base) are compared entry by entry, by the case each
leads to. The kinds of difference, counted per
function: `registers differ`, `stack slot offset differs` (`[esp+0x4]` for
`[esp+0x8]`), `stack frame size differs` (`sub esp, N`), `arguments popped
differ` (`add esp, N` after a call, `ret N`), `immediate differs`, `offset
differs` (another structure field), a register or memory where the other
has a constant (a variable became a constant), `memory vs register`,
`operand size differs`, `signedness differs` (`movzx`/`movsx`, `jl`/`jb`),
`condition inverted` and `condition differs`, `short vs near jump`, `branch
target differs`, `call target differs`, `global differs`, `jump table
differs`, `encoding differs`, `reordered`, `alignment padding differs`,
missing and extra instructions.

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

```rust
// Types as C, and the member at an offset (a PDB's too, once attached).
let debug = bin.debug_info().unwrap();
let header = debug.c_header(&["edict_t"]); // or &[] for everything
println!("{}", header.text);
if let Some(field) = debug.struct_field("edict_t", 0x21c) {
    println!("{} ({})", field.label("edict_t"), field.type_name); // edict_t.enemy (struct edict_s *)
}
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
  src/layout/        the byte-level region tree: ELF, Mach-O, PE/COFF, XBE builders,
                     struct field specs, on-demand decoders, DWARF section decoders
  src/xbe.rs         original Xbox executables: headers, keys, sections, the kernel's
                     exports by ordinal, the code followed as in a 32-bit PE
  src/dwarf/         units, DIEs, types, expressions, line tables (gimli + addr2line)
  src/disasm.rs      iced-x86 and yaxpeax-arm
  src/inspect.rs     the "what is here?" query, separate debug files, annotations
  src/search.rs      the search box: query forms and ranking
  src/coverage.rs    reverse-engineering coverage and gap hints
  src/decomp.rs      a matching decompilation: where it stands, what to do next
  src/matching/      a decompilation's objects matched against the original and each
                     difference explained (MIPS; x86 and x86-64 in x86.rs); objdiff's reports
  src/sigs.rs        library code found by its bytes: signatures from Psy-Q and COFF libraries
  src/similar.rs     functions shaped alike (MinHash of instruction shapes, banded)
  src/discover/      function recovery from .pdata, .eh_frame, LC_FUNCTION_STARTS,
                     and by following x86 code (x86.rs) for 32-bit PE images and XBEs
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
  src/dwarf/pdb.rs           PDBs: modules, procedures, line records and types, written as DWARF
  src/dwarf/ctypes.rs        the types as C: merged across units, named, laid out explicitly;
                             the member at an offset
  src/dwarf/header.rs        C headers of them, in dependency order, with layout asserts
  src/wasm/          WebAssembly: reading modules (read.rs), their layout (layout.rs),
                     the bytecode (code.rs) and what it refers to (analysis.rs), DWARF
                     moved to binviz's addresses (dwarf.rs), source maps (sourcemap.rs)
crates/binviz-wasm   wasm-bindgen bindings (a Session object)
crates/binviz-cli    the command-line tool
crates/binviz-mcp    the MCP server for agents
web/                 TypeScript UI; the WASM runs in a Web Worker
tests/fixtures/      small ELF / Mach-O / PE / WebAssembly test binaries and their
                     sources, and hand-assembled ROMs and an XBE (src/roms.py, src/xbe.py
                     write them)
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

- From a PDB, binviz reads modules, procedures (with their parameters),
  globals, public symbols, line records and types, not local variables or
  their locations, so variables in scope need DWARF. A class's methods and
  its vtable's shape aren't read (the vtable pointer is).
- C headers: a virtual base is left as padding (only the most derived class
  says where it is), Rust enums with data are their bytes, and a structure of
  no size (a Rust zero-sized type) is declared but not defined. A PDB has no
  typedefs in its type records, so members name the types themselves; for a C
  program, every structure gets a typedef of its own name, since a PDB can't
  tell `typedef struct {…} T` from `struct T`. A `long` and a `long double`
  keep the binary's sizes, which not every compiler for its CPU shares (64-bit
  Windows's `long` is 4 bytes, MSVC's `long double` 8); the header says when
  it needs them.
- Split DWARF (`.dwo`/`.dwp`) is detected but not followed.
- `.eh_frame`, dyld opcode streams and chained fixups are shown as regions but
  not decoded entry by entry (chained fixups are walked to find pointers).
- Disassembly covers x86, x86-64, AArch64, ARM (A32) and WebAssembly;
  cross-references and the call graph x86, x86-64, AArch64 and WebAssembly.
- Matching x86 objects: relocations other than absolute and relative ones
  (through the GOT, section-relative) are masked but not checked (offsets
  from the image base, MSVC x64's, are checked),
  and an instruction a linker rewrote (a `mov` from the GOT relaxed into a
  `lea`) reads as a difference.
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
- XBEs: an Xbox disc image (XDVDFS) doesn't open to its files yet; open the
  extracted `default.xbe`. The XDK's library functions are not named yet
  (nothing matches them by signature, as the Psy-Q SDK's are); the section
  digests and the header signature are shown, not checked; the non-kernel
  import directory (development builds only) is laid out as recalled, not
  confirmed, and its imports aren't read; the header and certificate fields
  some later XDKs add past 0x178 and 0x1D0 carry Cxbx-Reloaded's names,
  unconfirmed. Kernel exports past ordinal 366 (development kits' `MmDbg…`)
  are left unnamed.
- Objective-C metadata is read from images outside the dyld shared cache;
  Swift's own metadata (beyond classes visible to Objective-C) is not.
- WebAssembly: only memory 0, and only a 32-bit one, is placed at
  `0x80000000`. A passive data segment is placed where the start function
  copies it (`__wasm_init_memory`, in a build with threads); one copied
  elsewhere, or placed at a global's value (a side module's
  `__memory_base`), stays at its bytes in the file. The GC proposal's
  instructions (`0xFB`) aren't decoded: a body stops decoding there, the
  rest shown as bytes. A `call_indirect` isn't followed to the functions it
  may reach (the disassembly says how many there are). Without DWARF or a
  symbol table, where the zero-filled variables end and the stack begins
  isn't known (`bss and stack`), and the heap above is no section. A source
  map gives lines, not inlined calls or variables. Modules in a folder or
  zip aren't recognized as binaries yet: open the `.wasm`.
