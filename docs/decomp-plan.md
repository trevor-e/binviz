# Plan: binviz for agent-driven matching decompilation

Status: planning. Nothing below is built yet unless marked **have**.
Keep this file current: tick items as they land, and note the commit.

Chosen first target: **Final Fantasy IX (PlayStation 1)**, Track A below.
Track B (32-bit x86 PC) is kept for later.

## Why

In 2026 a Halo: Combat Evolved decompilation went from nothing to 99.5 %
byte-matching in about two months (repo created 26 July 2026). The work was
done by LLM agents writing candidate C for each function and iterating until
the compiled bytes matched the original, with names and types borrowed from
a later release that shipped a PDB. A browser port followed within days by
recompiling the SDL port with Emscripten.

The loop those agents run needs exactly what binviz is built for: a loaded
binary answering questions in milliseconds, function diffs, PDB/DWARF types,
xrefs and notes, all reachable over MCP. This plan lists what is missing to
run such a project with binviz as the agent's eyes, and in what order.

References:
- Decomp: https://github.com/punpckhdq/halo (fork with ports: https://github.com/bnunu/halo-1)
- Ports: https://github.com/cybersecurity/halo-ce-universal (SDL3 + OpenGL; assets from the user's disc image)
- Agent workflow description: https://github.com/halo-re/halo
- Symbol source (Anniversary PDB corpus): https://github.com/surreptitiousresearch/halocea
- Ecosystem: https://decomp.wiki (tips per platform), https://decomp.dev (progress), https://decomp.me (scratches)
- Scoring tool to emulate or feed: objdiff (https://github.com/encounter/objdiff)

## How a matching decomp works (for whoever picks this up cold)

1. Identify the exact compiler and flags from the binary.
2. Map the binary: every function's bounds, signature, the globals and strings it touches.
3. Borrow names and types from any symbolized build of the same engine.
4. Per function: write C, compile to an object, compare bytes with relocations
   masked, iterate until 100 %. This is the agent loop.
5. Link everything in the original order; the rebuilt binary is byte-identical.
6. Then stop matching on purpose: swap the platform layer (SDL, OpenGL/WebGL,
   Web Audio) and recompile for new targets, including wasm32 via Emscripten.

Assets (maps, sounds, textures) are never in the exe; the port reads them
from the user's own copy of the game.

## Phase 0: what binviz already has

- **have** PE/COFF, ELF, Mach-O readers; PDB read into DWARF (`crates/binviz/src/dwarf`)
- **have** PS-X EXE reader with the PS1 memory map, I/O registers and DMA channels (`rom/psx.rs`);
  PS1 discs (`.bin`/`.iso`) opened to their files, `SYSTEM.CNF` boot executable first (`disc.rs`)
- **have** MIPS disassembler with delay slots, `lui`/`addiu` pairs and `$gp` (`cpu/mips.rs`);
  code followed from the entry point (`rom/analysis.rs`); MIPS switch tables only when a code/data log saw them
- **have** x86/x86-64 disassembly with targets, strings and globals named inline
- **have** xrefs, callers/callees, call graph, call path (`xrefs.rs`, `symbols.rs`)
- **have** notes: name functions, comment addresses, mark reviewed; JSON import/export;
  label files in and out (`rom/labels.rs`)
- **have** function-by-function diff of two builds with an instruction-hash similarity (`fndiff.rs`, MCP `diff_functions`)
- **have** relocatable ELF/Mach-O objects opened with synthetic addresses and relocated DWARF
- **have** MCP server exposing all of the above (`crates/binviz-mcp`)
- **have** Rich header located and its size accounted for (`layout/pe.rs`), but not decoded
- **have** `Format::Wasm` as a name only (`model.rs`); no wasm reader

## Track A: Final Fantasy IX (PS1)

### The standard PS1 pipeline binviz plugs into

- **Compiler**: Psy-Q GCC 2.x, reproduced through maspsx (https://github.com/mkst/maspsx),
  which massages GCC's assembly so GNU `as` yields the bytes ASPSX would.
  decomp.me packages the Psy-Q compiler versions.
- **Splitting**: splat (https://github.com/ethteck/splat) turns the executable
  into per-file `asm`/`c`/`data`/`rodata`/`bin` segments from a YAML config
  plus `symbol_addrs.txt`.
- **Scoring**: objdiff compares the target `.o` with the freshly built `.o`
  per unit and writes the report decomp.dev shows.
- **Mapping**: Ghidra with `ghidra_psx_ldr` (memory regions, Psy-Q library
  signatures). This is the slot binviz takes over, plus the agent's context.

binviz does not replace the compiler, splat or objdiff. It maps the binary,
feeds splat and the agent, and reads objdiff's verdicts back.

### Prerequisites (not tooling)

- Your own dumps of the FF9 discs (US: SLUS-01251, four discs). Never fetch
  ROMs. Open the `.bin` in binviz; it boots `SLUS_012.51`.
- FF9 keeps almost everything outside the boot executable, in overlays loaded
  from the disc's big archive file per field, battle and menu. Mapping the
  overlays is most of the work (A2).
- The 2016 Unity remaster's C# (decompiled with ILSpy) carries original names
  and logic. It is the "symbolized relative" for A7. Own that copy too.

### Items, in order

- [ ] **A1. Psy-Q SDK identification.** Build byte signatures from a folder
      of Psy-Q `.LIB`/`.OBJ` files the user supplies (like `xsig` /
      `ghidra_psx_ldr`), match them against the executable and overlays, name
      the library functions (`GsSortObject4`, `CdRead`, `SpuSetKey`...), tag
      them `sdk` so coverage and the agent skip them, and report which SDK
      version matched, which fixes the GCC and ASPSX versions to use.
      Where: new `rom/psx_sdk.rs`; signature store in the notes file.
      Done when: the FF9 boot executable's SDK calls are all named and the
      version is reported.
- [x] **A2. Overlays and memory images** (2026-09-29; FF9 archive parsing still open). (a) Open an emulator RAM dump or
      savestate (2 MiB) as a PS1 memory image at `0x80000000`, with the boot
      executable's symbols laid over it, so whatever overlay was loaded at the
      time is analysable in place. (b) Find overlay blobs in the disc's
      archive, work out their load addresses (from the loader's tables, or by
      matching bytes against a RAM dump), and open each as its own image with
      the boot executable's symbols shared. (c) Symbol scopes per overlay so
      two overlays at the same address do not collide.
      Where: `rom/psx.rs`, `disc.rs`, `symbols.rs`.
      Done when: a field overlay and a battle overlay both open with SDK and
      boot-exe calls resolved.
- [x] **A3. MIPS mapping quality** (2026-09-29: switch tables, signatures, struct hints; data classification still open). Static switch tables (`sltiu`/`beq`
      guard, `sll 2`, `lui`/`addu`, `lw`, `jr` pattern, table in `.rodata`);
      `$gp`-relative small data named as symbols; function signature guess
      (`$a0-$a3` read before written, stack args above the frame, `$v0/$v1`
      return, leaf or not, frame size, saved registers); data classification
      (pointer tables, string tables, fixed-point constant tables); struct
      offset hints (offsets accessed off the same base register, grouped).
      Where: `cpu/mips.rs`, `rom/jumptable.rs`, `rom/analysis.rs`.
      Done when: function count on the boot exe is within a few percent of
      Ghidra's and every switch in a sample of 50 functions is followed.
- [ ] **A4. Splat and symbol interchange.** Export a splat YAML with segments
      derived from the map (SDK ranges as `bin`/`asm`, code as `c` units split
      at chosen boundaries, `data`/`rodata` extents) and `symbol_addrs.txt`
      from symbols plus notes; import `symbol_addrs.txt` and splat's
      `undefined_funcs_auto.txt` back. Where: `rom/labels.rs` (new formats).
      Done when: a project set up from the export builds with splat unchanged.
- [ ] **A5. objdiff in, match score native.** (a) Read objdiff's report JSON
      and mark each function `matched`/percent in notes and the coverage map,
      so binviz shows project progress by address. (b) Native score for a
      MIPS ELF `.o` against the original: bytes compared with relocations
      masked, per-function percent, and an explained diff (reordered
      instructions, different register, different immediate, different
      `$gp`/`lui` split, missing `nop` in a delay slot). Extend `fndiff.rs`.
      Done when: binviz's percent agrees with objdiff's on the same pair.
- [ ] **A6. Agent bundle over MCP.** `decomp_context(function)`: disassembly
      with pseudo-ops, the signature guess, callers and callees with their
      prototypes where known, strings and globals used with inferred types,
      struct offset hints, and two or three already-matched functions that
      look most similar as worked examples. `explain_mismatch(function, obj)`
      wrapping A5. `mark(function, status)`. One call per function instead of
      eight. Where: `crates/binviz-mcp/src/tools.rs`.
- [ ] **A7. Names from the remaster.** Import a name list from the Unity
      port's C# (function names, enum and struct members) and propose matches
      to MIPS functions by shared string literals, call-graph shape and
      constant tables; the agent confirms. Where: `fndiff.rs` matching by
      strings, `rom/labels.rs` import.
- [ ] **A8. Emulator traces.** Read an execution or coverage log from
      PCSX-Redux or DuckStation (check what each can emit) the way NES/SNES
      code/data logs are read, to find code reached only through pointers and
      to catch overlay load addresses. Where: `rom/cdl.rs`.

### Prove the loop first

Before FF9: take a small Psy-Q sample program (or any open-source PS1
homebrew built with the same GCC), build it, strip it, and drive the agent to
re-match it end to end with A3, A5 and A6. Then the boot executable of FF9,
then one overlay.

## Track B: 32-bit x86 PC (later)

### Phase B1: map a stripped 32-bit x86 binary

A 32-bit PE has no `.pdata`, so today a stripped x86 game yields almost no
functions (`discover.rs` handles only `.pdata`, `.eh_frame`,
`LC_FUNCTION_STARTS`).

- [ ] **Recursive-descent function discovery for x86.** Follow calls and
      branches from the entry point, exports, TLS callbacks, vtable slots and
      pointers in data, the way `rom/` follows a ROM from its vectors. Then a
      prologue scan for code nothing reaches. Where: `discover.rs`.
      Done when: a stripped MSVC 32-bit exe recovers >95 % of the functions
      its PDB names.
- [ ] **x86 jump tables.** MSVC `jmp dword ptr [table + reg*4]`, with the
      optional byte index table, bounded by the preceding `cmp reg, N; ja`.
- [ ] **Stack frame and calling convention.** `cdecl` / `stdcall` (`ret N`) /
      `fastcall` / `thiscall`; argument bytes; frame size; saved registers.
- [ ] **Data typing.** Global extents, float constants, vtables and RTTI,
      string tables.
- [ ] **Compiler identification.** Decode Rich header product IDs to
      compiler/linker versions (`layout/pe.rs`).

### Phase B2: the matching loop for COFF

- [ ] **COFF `.obj` input** with symbols and relocations, synthetic addresses.
- [ ] **Byte match score with relocations masked** and an explained diff
      (shares the engine with A5).
- [ ] **MCP tools** `match_function`, `match_project` (shares A6).

### Phase B3: borrow symbols from other builds

- [ ] **C header export from PDB/DWARF types.** Structs with explicit
      padding, enums, typedefs, unions, prototypes; must round-trip.
- [ ] **Symbol porting** from a symbolized build to a stripped one (shares A7).

### Phase B4: containers

- [ ] **XBE reader** (original Xbox) only if an Xbox target is chosen; the
      XDK compiler is not public, so matching Xbox games is a sourcing problem
      before it is a tooling one.

## Shared: project bookkeeping and porting

- [ ] **Function status in notes.** `matched` and `in_progress` beside
      `named` and `reviewed`; coverage map and MCP `coverage` report them.
- [ ] **Progress export** in the format decomp.dev consumes (or rely on
      objdiff's report and A5a).
- [ ] **WASM reader.** Sections, function table, imports/exports, the name
      section, Emscripten's DWARF and source maps, so the browser port's
      build can be opened, size-diffed and its stack traces symbolicated.
      Fill in `Format::Wasm`.
- [ ] **Behavioral comparison** (runtime; furthest from binviz's shape):
      the same function on the same asset in the original (under an emulator)
      and in the port. Consider last.

## Progress log

- 2026-09-28: plan written; no implementation yet.
- 2026-09-29: A3 switch tables (`rom/jumptable.rs`), function signatures and struct hints
  (`signature.rs`), A2 memory images, overlays and `locate` (`rom/psx.rs`) landed; library only,
  CLI and MCP wiring to follow once the other session's edits to those files are pushed.
- 2026-09-28: first target chosen: Final Fantasy IX (PS1). Track A added from
  the decomp.wiki PS1 page (Psy-Q GCC via maspsx, splat, objdiff, Ghidra with
  ghidra_psx_ldr). Track B (x86) kept for later.
