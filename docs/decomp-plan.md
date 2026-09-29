# Plan: binviz for agent-driven matching decompilation

Status: planning. Nothing below is built yet unless marked **have** or ticked.
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
- Incremental re-implementation (C patched into the original XBE, a `kb.json` of declarations): https://github.com/halo-re/halo.
  Neither Halo repo describes the agent workflow itself; for that, see the next two.
- Agent-driven matching, first-hand: https://gambiconf.substack.com/p/can-llms-really-do-matching-decompilation
  (60 functions, 74 % matched, about half on the first try; one not matched by the third
  try rarely matches after; the model's worst habit is believing it matched without checking)
  and https://gambiconf.substack.com/p/starting-a-decompilation-project (Claude Code on a GBA game, 51 % of its code)
- Echo (2026), matching decompilation with the compiler in the loop: https://arxiv.org/abs/2609.18706
  (graded similarity feedback, rule-based rewrites before the model, compiler flags searched too;
  2.4x the exact matches of the best baseline)
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

- [x] **A1. Psy-Q SDK identification** (2026-09-29, `rom/psyq.rs`; CLI `sdk`, MCP `identify_sdk`; the SDK release is reported since 91a0212). Build byte signatures from a folder
      of Psy-Q `.LIB`/`.OBJ` files the user supplies (like `xsig` /
      `ghidra_psx_ldr`), match them against the executable and overlays, name
      the library functions (`GsSortObject4`, `CdRead`, `SpuSetKey`...), tag
      them `sdk` so coverage and the agent skip them, and report which SDK
      version matched, which fixes the GCC and ASPSX versions to use.
      Where: new `rom/psx_sdk.rs`; signature store in the notes file.
      Done when: the FF9 boot executable's SDK calls are all named and the
      version is reported.
- [x] **A2. Overlays and memory images** (2026-09-29; an archive's files are found in a memory image by their bytes since 91a0212, CLI `locate`; FF9's own archive format is still unread, which needs the game's disc). (a) Open an emulator RAM dump or
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
- [x] **A3. MIPS mapping quality** (2026-09-29: switch tables, signatures, struct hints; data classification in ee1041c: tables of code pointers and of strings, structures holding pointers, constants). Static switch tables (`sltiu`/`beq`
      guard, `sll 2`, `lui`/`addu`, `lw`, `jr` pattern, table in `.rodata`);
      `$gp`-relative small data named as symbols; function signature guess
      (`$a0-$a3` read before written, stack args above the frame, `$v0/$v1`
      return, leaf or not, frame size, saved registers); data classification
      (pointer tables, string tables, fixed-point constant tables); struct
      offset hints (offsets accessed off the same base register, grouped).
      Where: `cpu/mips.rs`, `rom/jumptable.rs`, `rom/analysis.rs`.
      Done when: function count on the boot exe is within a few percent of
      Ghidra's and every switch in a sample of 50 functions is followed.
- [x] **A4. Splat and symbol interchange** (2026-09-29, `splat.rs`; CLI `splat`, MCP `splat_export`/`import_symbol_addrs`). Export a splat YAML with segments
      derived from the map (SDK ranges as `bin`/`asm`, code as `c` units split
      at chosen boundaries, `data`/`rodata` extents) and `symbol_addrs.txt`
      from symbols plus notes; import `symbol_addrs.txt` and splat's
      `undefined_funcs_auto.txt` back. Where: `rom/labels.rs` (new formats).
      Done when: a project set up from the export builds with splat unchanged.
- [x] **A5. objdiff in, match score native** (2026-09-29, `matching.rs`; CLI `match`/`report`, MCP `match_function`/`match_object`/`place_report`; `place_report` records its verdicts as notes statuses, see the shared items). (a) Read objdiff's report JSON
      and mark each function `matched`/percent in notes and the coverage map,
      so binviz shows project progress by address. (b) Native score for a
      MIPS ELF `.o` against the original: bytes compared with relocations
      masked, per-function percent, and an explained diff (reordered
      instructions, different register, different immediate, different
      `$gp`/`lui` split, missing `nop` in a delay slot). Extend `fndiff.rs`.
      Done when: binviz's percent agrees with objdiff's on the same pair.
- [x] **A6. Agent bundle over MCP** (2026-09-29, `decomp.rs`; CLI `context`/`signature`, MCP `decomp_context`/`function_signature`; worked examples from matched look-alikes, see the shared items). `decomp_context(function)`: disassembly
      with pseudo-ops, the signature guess, callers and callees with their
      prototypes where known, strings and globals used with inferred types,
      struct offset hints, and two or three already-matched functions that
      look most similar as worked examples. `explain_mismatch(function, obj)`
      wrapping A5. `mark(function, status)`. One call per function instead of
      eight. Where: `crates/binviz-mcp/src/tools.rs`.
- [x] **A7. Names from the remaster** (2026-09-29, `names.rs`; CLI `names`, MCP `propose_names`; takes any build's functions with their strings and calls as JSON). Import a name list from the Unity
      port's C# (function names, enum and struct members) and propose matches
      to MIPS functions by shared string literals, call-graph shape and
      constant tables; the agent confirms. Where: `fndiff.rs` matching by
      strings, `rom/labels.rs` import.
- [x] **A8. Emulator traces** (2026-09-29, `rom/psx.rs` `with_psx_trace`; CLI `--trace`, MCP `open_binary` `trace`). Read an execution or coverage log from
      PCSX-Redux or DuckStation (check what each can emit) the way NES/SNES
      code/data logs are read, to find code reached only through pointers and
      to catch overlay load addresses. Where: `rom/cdl.rs`.

### Prove the loop first

Done (2026-09-29): `samples/psx/` builds a small PlayStation program (a
structure, a switch, strings, a global, a call chain, five arguments) as a
PS-X EXE with the MIPS code generator that ships with Rust's llvm-tools, so
no download is needed. Its README walks the loop: signature and context,
the compiler's own object matched at 100%, a deliberately wrong rebuild
explained (a missing stack argument, shifted slots, a variable that became
a constant). Next: the boot executable of FF9, then one overlay. The
compiler there is Psy-Q's GCC through maspsx, not LLVM.

## Track B: 32-bit x86 PC (later)

### Phase B1: map a stripped 32-bit x86 binary

A 32-bit PE has no `.pdata`; its code is followed instead
(`discover/x86.rs`). Fixture: `x86demo.exe` with its PDB, and
`x86demo-fixed.exe`, the same code linked `/FIXED` (no base relocations, as
games of the time were), built with clang for the MSVC ABI and lld-link, with
MSVC's idioms written in `x86demo-msvc.s` (no MSVC needed). It has every
calling convention, vtables, TLS callbacks and a SafeSEH table, for the items
below too.

- [x] **Recursive-descent function discovery for x86** (79a1903). Starts:
      entry point, exports, import thunks, TLS callbacks, SafeSEH handlers;
      then code addresses the image holds (relocated words, or aligned words
      of data without `.reloc`, and immediates); then gap starts that read as
      code. Calls that never return end a function (imports by name, and
      what only ends in them); MSVC's in-place `__finally` blocks stay in
      their parent; VS2019's conditional tail calls leave it. PE32 only:
      x86-64 PE keeps `.pdata`. Measured: every function of the fixture in
      both builds, no extras, PDB sizes exact; a generated 20 004-function
      program (1.4 MB) likewise, parsed in 0.16 s; real MSVC-built PE32s
      without PDBs (VS2008–2019) put 98.8–99.6 % of `.text` in functions.
      Still to measure: an exe MSVC itself built, against its PDB.
- [x] **x86 jump tables** (79a1903). `jmp [table + reg*4]` (or `jmp reg`
      after loading from one), with the optional byte index table, bounded by
      `cmp reg, N; ja` or `and reg, N`, else by where relocated entries stop.
      Tables are data: `dd`/`db` rows in disassembly, each entry a pointer to
      its case in the xrefs. x86-64's forms (absolute, relative to the image
      base as MSVC writes them, relative to the table as clang and GCC do) since
      bcd9b07.
- [x] **Stack frame and calling convention** (dcdc8f6). `cdecl` / `stdcall` (`ret N`) /
      `fastcall` / `thiscall`; argument bytes; frame size; saved registers.
- [x] **Data typing** (ee1041c; vtables and RTTI in 804a324). Global extents, float constants, vtables and RTTI,
      string tables.
- [x] **Compiler identification** (6cd40c1). Decode Rich header product IDs to
      compiler/linker versions (`layout/pe.rs`).

### Phase B1b: what a blind decompile of a real x86 game needed

One list: everything found decompiling all 1 239 functions of Quake 2's
`gamex86.dll` (id's 3.20 release, MSVC 6, no PDB) with agents that had only
binviz plus the headers, then graded against id's GPL source. Ticked items
landed in 9be1f10, 31a7118 and 78cae55. Open ones are roughly most useful
first; "unconfirmed" means an agent reported it and nobody has reproduced it.

Result: 927 of 945 game-code functions had the same logic, 13 the same
behaviour, 4 differed (2 real errors, in `Machinegun_Fire` and `monster_use`;
2 look like source-version differences: a `needpass` cvar and a `SOLID_BBOX`
check the GitHub source lacks). The other 294 functions are the statically
linked C runtime. Caveat: the agents knew Quake 2 from training and said so;
most of that result is recall. The binaries and source are in
`~/.local/share/binviz-validation/quake2/` (never commit them; its
`build-game.sh` rebuilds the source with clang); each part's grade is in
`decomp/out/compare_part_NN.md` there.

Fixed:

- [x] Float and double constants an instruction reads show their value
      (`fsub st, [0x2003e108] ; f32 -1.0`) in `disasm`, `context` and its
      Globals list, and a double is no longer read as a string ("333333").
      MSVC folds `x + 1` into `x - (-1)`, so the code cannot be read without
      it. Every agent hit this.
- [x] `binviz … | head` no longer panics on a broken pipe.
- [x] An export's extent no longer includes trailing `nop`/`int3` padding
      (`GetGameAPI` was 208 bytes, is 196).
- [x] `disasm` and `context` say when they stopped (default 4 000
      instructions, was a silent 400); `disasm` starts at a mid-function address.
- [x] `context` says how a function with no callers is referenced ("Also
      referenced by: 5 address taken"), and labels a call through a pointer
      as indirect instead of listing the pointer's address as a callee.

Missing features:

- [x] **Name the C runtime** (601556f, 7151cb6: signatures from the runtime's own
      `.lib` archives or objects, which the user supplies; none ship with binviz). About 300 of 1 239 functions are statically
      linked MSVC CRT and every one is `sub_XXXX`; agents identified them from
      code shapes and strings. Signatures (bytes with relocations masked, as
      A1 does for Psy-Q) for the MSVC 6 / 7 / 8 / 2010 CRTs and their `.lib`
      objects. Shares the matcher with A1.
- [x] **Map field offsets to struct fields** (2d5fecc). `[esi+0x21c]` has no name;
      agents built an offset table by compiling the headers with clang
      (`-fdump-record-layouts`). With headers or a PDB's types loaded, show
      `[esi+0x21c] ; edict_t.enemy` and type the register from the
      prototype. Needs B3's header/PDB type export first.
- [x] **Recognise an import table in data** (ee1041c: a table of function
      pointers filled at run time, where it is filled, each slot labelled). Quake 2 reaches the engine
      through a struct of function pointers (`gi`, at `0x20066ee0`, 4 bytes
      per slot), each call `call [0x20066f10]`. Detect a structure of
      pointers filled at one site (`GetGameAPI`'s `rep movsd` plus stores)
      and label each slot `gi+0x30`, or with its type from the headers.
- [x] **Name globals** (ee1041c; import slots in a015b27). `level`, `game`, `g_edicts`, cvar pointers and CRT
      state (`_nhandle`, `__pioinfo`) show only as `.data+0x25ee0`. Cluster
      by access pattern (a pointer read then `[ptr+0x14]` is a cvar's value;
      a constant stride is an array of structs) and let a note name them.
      Data tables are labelled inconsistently: some get a first-string
      annotation (`read -> "classname"`), most get nothing.
- [x] **Tables of code pointers and structs in `.data`** (ee1041c; vtables in
      804a324). A monster's
      `mmove_t` frame table, `_fptrap` (6 pointers, no callers), a vtable:
      these report as pointers, or as `write -> sub_20038960` when the target
      is a table, not code. Say "table of N code pointers", and show that
      `mov [esi+0x304], 0x2004xxxx` stores a pointer to a `.data` table.
      Likewise `Globals` lists `mov [esi+0x1c4], 0x2001f7e0` as an "address"
      without resolving it to a function.
- [x] **Stack frame and calling convention for x86** (dcdc8f6, the item
      above). MSVC merges `add esp, N` after several calls, so `[esp+N]`
      shifts between calls and is easy to misread: track the stack
      pointer's delta and show `arg1`, `arg2` against the entry frame. Then
      an x86 `signature` line in `context` (MIPS only today).
- [x] **A function in more than one range** (5e5a19f; folded functions read
      as one with both names). MSVC puts a function's tail
      before its entry (`strchr`'s shared `lea eax,[edx-1]; pop ebx; ret` at
      `0x200301d0`, three pieces of `getSystemCP` at `0x20037710`, x87 libm
      case stubs from `0x20036820`, alternate-entry stubs at `0x20031424`),
      and identical functions are folded (`gib_die` and `debris_die` at
      `0x2000bb20`). A function is one range, so these show as tiny functions
      with no callers. A function needs to own several ranges;
      `discover/x86.rs` has the jump analysis to find them.
- [x] **Say where a jump table starts** (bcd9b07: the table and each case
      marked in `disasm` and `context`). Tables are `dd` rows with their
      targets, but nothing marks the boundary after a `ret` (`; jump table,
      10 cases`), and `context` does not follow an indirect jump's targets.
- [x] **Short strings** (ee1041c). One- and two-character strings (`"a"`, `"m"`, the
      light styles) referenced from code show as bare `.data` addresses.
- [x] **Shorter `context`** (ab36979: 24 of each list and how many more). It prints up to 4 000 callers, callees and
      globals before any code; cap them ("and N more") and put the code
      first, or offer a brief form. `func` shows blank `strings:` and
      `data:` sections for tiny functions, which looks like truncation.
- [x] **Say which functions have no counterpart in a source or older build**
      (d409709: CLI `counterparts`, MCP `source_counterparts`)
      (`CheckNeedPass` and the extra `needpass` cvar are 3.20 changes the
      GitHub source lacks). `diff functions` does this for two binaries, not
      for a binary and source.

Unconfirmed reports (reproduce first; each checked on `gamedemo.dll`, the
fixture shaped like Quake 2's game DLL):

- [x] `Called by` mixes code callers with data references (`0x20067774 read
      .data`); split them. Perhaps the same thing as the "Also referenced by"
      line above, now that exists. Outcome: it was; callers are code only,
      data references are the "Also referenced by" line, and a callback lists
      where its address is taken (ab36979).
- [x] `Calls:` lists callbacks stored into struct fields (`0x2000fc00` in
      `0x2000f720`, the movetype functions under `0x200118e0`) as if called.
      Only the indirect-call case was seen and fixed. Outcome: reproduced,
      and fixed in ee1041c: a stored function address is a callback, not a
      call.
- [x] `context` output cut at ~400 lines without a marker in `0x2000d0e0`
      (`barrel_explode`); the marker exists in code, so check it was the
      `disasm` default and not a second cutoff. Outcome: it was the MCP
      tool's default of 400 instructions, which says where it stopped; the
      lists before the code are capped since ab36979, and the server marks
      its own 60 KB cut.
- [x] Very short functions (`player_pain`, a bare `ret`) appear only as data
      pointers, with no call sites: expected for callbacks; check the wording.
      Outcome: the context now calls them callbacks, says where their address
      is taken, and that a function reading no arguments may still be passed
      some (ab36979).

Tests still to run (none can run without the games and agents; not done):

- [ ] **A mutated Quake 2.** Agents reproduced the source almost verbatim,
      comments included. Change constants, swap conditions, add or drop
      calls, rename fields; compile with clang for PE32 (`build-game.sh`);
      run the same blind decompile; grade against the mutated source. A
      model working from memory reproduces the original and is wrong.
- [ ] The rest of the same release: `ref_soft.dll` (hand-written assembly),
      `ref_gl.dll`, `quake2.exe`. None of them run yet.
- [ ] An obscure open-source game with few copies online, ideally on another
      platform, and code written after the model's training cutoff.
- [ ] Compare each platform the same way once it has a real binary with known
      source: most of the list above is not x86-specific (float constants,
      truncation, tables, names, struct fields); what changes per platform
      is the instruction set, the compiler's idioms and the library linked in.

### Phase B2: the matching loop for COFF

- [x] **COFF `.obj` input** (0c20f50, add2594) with symbols and relocations, synthetic addresses.
- [x] **Byte match score with relocations masked** (0c20f50; what to try for
      each kind of difference, 2365e73) and an explained diff
      (shares the engine with A5).
- [x] **MCP tools** `match_function`, `match_project` (shares A6) (7151cb6).

### Phase B3: borrow symbols from other builds

- [x] **C header export from PDB/DWARF types** (29aa399, 72e0357, 34b0723,
      9aa31c4). Structs with explicit
      padding, enums, typedefs, unions, prototypes; must round-trip.
- [x] **Symbol porting** (d409709) from a symbolized build to a stripped one (shares A7).

### Phase B4: containers

- [x] **XBE reader** (be3b233, 61c0c0c) (original Xbox) only if an Xbox target is chosen; the
      XDK compiler is not public, so matching Xbox games is a sourcing problem
      before it is a tooling one.

## Shared: project bookkeeping and porting

- [x] **Function status in notes** (2026-09-29, 8e208bc). Each function's state rides
      on its note (`decomp`: todo, in-progress with who and since, matched
      with its source file, nonmatching, skipped, library; tries and best
      percent), in the notes file the web UI keeps too. MCP `mark` sets it,
      `place_report` records objdiff's verdicts (and sends back what stopped
      matching), `identify_sdk` marks library code; `coverage`,
      `binary_summary`, `inspect`, `function_info` and `disassemble` report
      it. The web UI's coverage map colours matched code since 6996685.
- [x] **Work queue and look-alikes** (2026-09-29, 8e208bc, `queue.rs`, `similar.rs`;
      MCP `next_functions`, `mark`, `similar_functions`). What to decompile
      next, best first: near-copies of a matched function (90 % of the same
      instruction shapes, 8 instructions or more), then functions whose
      callees are all done, cheapest for what they unlock
      (`(1 + 2·callers waiting only on it + log2(1 + callers)) / (1 + instructions/32)`),
      then those still waiting, fewest missing first; three tries without a
      match send one to the end until a callee is done after them. Claims
      (an hour) keep parallel agents apart. Functions are compared by a MinHash
      of their instruction shapes filed in LSH bands, the best candidates
      diffed exactly: `decomp_context` gets matched look-alikes as worked
      examples. 20 004 functions: 10–60 ms a ranking, 40 ms a claim-and-mark
      round over MCP.
- [x] **Tightening the loop further.** A programmatic first draft before the
      model (m2c for MIPS) and decomp-permuter in the background on
      near-misses, with rule-based rewrites for the common mismatch classes
      (Echo); compiler flags searched per unit; worked examples from sibling
      decomps built with the same compiler, so the first functions have some
      too; learned embeddings only where instruction shapes don't carry over
      (the remaster's C# in A7, searching agents' summaries in words).
      Landed: the rewrites to try for each kind of difference and the flags
      searched per unit (2365e73, CLI `flags`, MCP `rank_builds`); m2c's input
      written by `binviz asm` (MCP `export_asm`) and m2c run on it by `binviz
      m2c` (089fd72, cdeb134, 787e095): every function of the three PS1 samples
      assembles back to its words, and m2c writes the dispatcher's `switch`;
      worked examples from sibling decompilations (7d94e5f). Not done: running
      decomp-permuter, which needs the project's own compiler (`binviz asm`
      writes what its `import.py` takes), and learned embeddings.
- [x] **Progress export** (6996685: objdiff's report JSON) in the format decomp.dev consumes (or rely on
      objdiff's report and A5a).
- [x] **WASM reader** (187304c, 64858e7, 8cd2956, bdda68b). Sections, function table, imports/exports, the name
      section, Emscripten's DWARF and source maps, so the browser port's
      build can be opened, size-diffed and its stack traces symbolicated.
      Fill in `Format::Wasm`.
- [ ] **Behavioral comparison** (not started; runtime; furthest from binviz's shape):
      the same function on the same asset in the original (under an emulator)
      and in the port. Consider last.

## Progress log

- 2026-09-29 (Track B and the shared items): everything above that can be built and checked
  without the games landed. Phase B1 and B1b's x86 items: the Rich header (6cd40c1), stack
  frames and calling conventions (dcdc8f6), globals typed by use and named (ee1041c), import
  slots (a015b27), MSVC's RTTI and x86-64 leaf functions (804a324), functions in pieces
  (5e5a19f), switch cases and x86-64 jump tables (bcd9b07), short contexts (ab36979), fields
  named through typed pointers (2d5fecc); the four unconfirmed Quake 2 reports checked on
  `gamedemo.dll`. B2, COFF objects matched and library signatures from `.lib` archives (0c20f50,
  601556f, add2594, 7151cb6). B3, C headers from PDB or DWARF types (29aa399, 72e0357, 34b0723,
  9aa31c4) and names ported from a symbolized build (d409709). B4, the XBE reader (be3b233).
  The WASM reader (187304c, 64858e7, 8cd2956, bdda68b). Progress as objdiff's report and the
  matched colour (6996685), A1's SDK release and A2's archives found in memory (91a0212). The
  loop: what to try per mismatch and flags ranked per unit (2365e73), m2c's input and m2c run
  on it (089fd72, cdeb134, 787e095), worked examples from sibling decompilations (7d94e5f).
  Found on the way and fixed: a MIPS function's last delay slot was dropped as padding, trap
  codes weren't shown (so `break 7` came back `break 0`), and a loop counter read as an address.
  Not done, needing what isn't here: FF9's archive format and the FF9 checks, the Quake 2
  test runs, an MSVC-built exe measured against its PDB, decomp-permuter runs (the project's
  compiler); nor learned embeddings or behavioral comparison. `samples/loop.py` still takes
  psx-vm to 100%, in 47 rounds here (46 in the entry below).
- 2026-09-29 (Quake 2): decompiled `gamex86.dll` blind with agents and graded it against id's source;
  five fixes landed (float constants, broken pipe, padding, truncation, callbacks), the rest are
  Phase B1b above.
- 2026-09-28: plan written; no implementation yet.
- 2026-09-30 (later): `samples/psx-vm/` (48 functions in four objects, a 24-way jump table,
  pointer-reached syscalls, two overlays at one address, struct returns, a look-alike family)
  and `samples/loop.py`, which plays the agent over the MCP server: Map, Pick, Context,
  Compile, Compare with the queue tools, to 100% in 46 rounds with 10 simulated failures.
  Signatures now spot structures returned by value; the CLI gained `locate`.
- 2026-09-30: `samples/psx-advanced/` (two objects, a jump table, pointer-reached handlers, an
  overlay, a memory image, a trace, name candidates) runs every tool end to end and found four
  bugs, fixed: MIPS32 `movn`/`movz` stopping the follower, call clobbers applied before the
  delay slot (strings in slots lost), data notes followed as code into zeroed RAM, traces
  seeding only run starts.
- 2026-09-29: function status in notes, the work queue (`next_functions`,
  claims, `mark`) and look-alikes (`similar_functions`, worked examples in
  `decomp_context`) landed in 8e208bc; `place_report` records objdiff's verdicts and
  `identify_sdk` marks library code. Closes the matched-status and
  similar-function-examples notes above.
- 2026-09-29 (last): A1 Psy-Q signatures from LIB/OBJ files and A7 name proposals by shared
  strings and calls; strings are now found in console code areas (PS-X EXE had none before).
  Track A's eight items are all in; what remains are the notes above: SDK version detection,
  FF9's archive format, data classification, a `matched` note status, similar-function examples.
- 2026-09-29 (later): A4 splat export/import, A5 match scoring with explained diffs and objdiff
  report placement, A6 decomp context bundle, A8 trace input, all wired into the CLI and the
  MCP server (`--psx-exe`, `--overlay-at`, `--trace`; `signature`, `context`, `match`, `report`,
  `splat`). Left in Track A: A1 Psy-Q signatures, A7 names from the remaster, and the
  data-classification and matched-status items noted above.
- 2026-09-29: A3 switch tables (`rom/jumptable.rs`), function signatures and struct hints
  (`signature.rs`), A2 memory images, overlays and `locate` (`rom/psx.rs`) landed; library only,
  CLI and MCP wiring to follow once the other session's edits to those files are pushed.
- 2026-09-28: first target chosen: Final Fantasy IX (PS1). Track A added from
  the decomp.wiki PS1 page (Psy-Q GCC via maspsx, splat, objdiff, Ghidra with
  ghidra_psx_ldr). Track B (x86) kept for later.
- 2026-09-28: Track B, Phase B1's first two items landed in 79a1903 (x86
  function discovery by following the code, x86 jump tables), with the
  `x86demo` fixtures. Follow-ups seen on real MSVC code: a `__finally` block
  its parent doesn't branch over still reads as its own small function; the
  load config's CFG table (`/guard:cf` builds) would list address-taken
  functions exactly; x86-64 PE could run the follower seeded from `.pdata` to
  find leaf functions, which have no entry there.
