# Plan: binviz for agent-driven matching decompilation

Status: planning. Nothing below is built yet unless marked **have**.
Keep this file current: tick items as they land, and note the commit.

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
- Scoring tools to emulate: objdiff (https://github.com/encounter/objdiff), decomp.dev progress API

## How a matching decomp works (for whoever picks this up cold)

1. Identify the exact compiler and flags from the binary (Rich header, XDK version).
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
- **have** x86/x86-64 disassembly with targets, strings and globals named inline
- **have** xrefs, callers/callees, call graph, call path (`xrefs.rs`, `symbols.rs`)
- **have** notes: name functions, comment addresses, mark reviewed; JSON import/export
- **have** function-by-function diff of two builds with an instruction-hash similarity (`fndiff.rs`, MCP `diff_functions`)
- **have** MCP server exposing all of the above (`crates/binviz-mcp`)
- **have** Rich header located and its size accounted for (`layout/pe.rs`), but not decoded
- **have** `Format::Wasm` as a name only (`model.rs`); no wasm reader

## Phase 1: map a stripped 32-bit x86 binary

This unblocks everything else. A 32-bit PE has no `.pdata`, so today a
stripped x86 game yields almost no functions (`discover.rs` handles only
`.pdata`, `.eh_frame`, `LC_FUNCTION_STARTS`).

- [ ] **Recursive-descent function discovery for x86.** Follow calls and
      branches from the entry point, exports, TLS callbacks, vtable slots and
      pointers in data, the way `rom/` follows a ROM from its vectors. Then a
      prologue scan (`push ebp; mov ebp, esp`, `sub esp, N`, `push ebx/esi/edi`)
      for code nothing reaches. Where: `discover.rs`, reuse the ROM follower.
      Done when: a stripped MSVC 32-bit exe recovers >95 % of the functions
      its PDB names (test with a fixture built with and without the PDB).
- [ ] **x86 jump tables.** MSVC `jmp dword ptr [table + reg*4]`, with the
      optional byte index table, bounded by the preceding `cmp reg, N; ja`.
      Tables become data regions; targets become basic blocks of the function.
      Where: `cpu/` x86 follower.
- [ ] **Stack frame and calling convention.** Per function: `cdecl` /
      `stdcall` (`ret N`) / `fastcall` (ecx, edx read before write) /
      `thiscall` (ecx only); argument bytes from `ret N` or callers' `add esp, N`;
      local frame size; callee-saved registers; whether ebp is a frame pointer.
      Output in `function_info` and `inspect`. This is the prototype the agent writes.
- [ ] **Data typing.** Global extents from access widths and xrefs; float
      and double constants in `.rdata` (shown as numbers in disassembly);
      vtables (runs of code pointers in `.rdata`, RTTI `??_7` when present)
      and the class each implies; string tables.
- [ ] **Compiler identification.** Decode Rich header product IDs to
      compiler/linker versions and object counts (`layout/pe.rs`). Report it in
      `binary_summary`. Done when: a known MSVC 6/7/7.1/8 build names the right toolset.

## Phase 2: the matching loop

The agent's reward signal. Model on objdiff.

- [ ] **COFF `.obj` input.** Open a relocatable COFF object (the compiler's
      output), with its symbols and relocations, and give its functions
      synthetic addresses the way ELF/Mach-O `.o` already get them.
- [ ] **Byte match score with relocations masked.** For a target function
      in the original and a candidate in the `.obj`: compare instruction bytes
      with relocated operands masked, report matched/total bytes and a
      percent; also an instruction-level diff. Extend `fndiff.rs` alongside
      the existing similarity.
- [ ] **Explained diff.** Classify each mismatch: instructions reordered,
      different register allocation, stack slot offset differs, missing or
      extra inlined call, different immediate, different condition code.
      Agents converge faster on "ecx was a parameter" than on two asm columns.
- [ ] **MCP tools.** `match_function(target, obj_path, symbol?)` returning
      score + explained diff; `match_project(obj_dir)` scoring every function
      that has a candidate. Keep results cached per object mtime.
- [ ] **Project total.** Matched bytes over total code bytes, by section and
      by translation unit.

## Phase 3: borrow symbols from other builds

- [ ] **C header export from PDB/DWARF types.** Structs with explicit
      padding, enums, typedefs, unions, function prototypes, grouped by the
      compilation unit or header that declared them. Must round-trip: the
      header compiles and `sizeof`/offsets match the DWARF layout.
- [ ] **Symbol porting.** Match functions of a symbolized build to a
      stripped build (existing similarity in `fndiff.rs`, plus call-graph
      context and string references as tie-breakers) and write the names onto
      the stripped binary as notes, with a confidence. Review UI in Compare.

## Phase 4: project bookkeeping

- [ ] **Function status in notes.** Add `matched` (and `in_progress`) beside
      `named` and `reviewed`; coverage map and MCP `coverage` report them.
- [ ] **Translation-unit splitting.** Group address ranges into source files
      from PDB module info, from object ordering hints (Rich header counts,
      string pooling boundaries), or by hand; emit the link order file the
      build uses.
- [ ] **Progress export** in the format decomp.dev consumes.
- [ ] **Symbol list export** the build can consume (address, name, size,
      calling convention) and an import path back from the project's own
      symbol file.

## Phase 5: the target's container

- [ ] **XBE reader** (original Xbox). Header, certificate, section headers,
      library versions (tells the XDK, hence the compiler), kernel thunk table
      XOR-decoded (retail and debug keys), entry point decoded likewise. Map
      to the same `Section`/`Segment` model so everything above works. Only
      needed for an Xbox target; a PC target skips this.

## Phase 6: porting and verifying the result

- [ ] **WASM reader.** Sections, type/function/table/memory/global/export
      /import sections, the name section, Emscripten's DWARF and source maps.
      Then size diffs between builds, symbolicated browser stack traces, and
      the function table explained. Fill in `Format::Wasm`.
- [ ] **Behavioral comparison** (runtime; furthest from binviz's shape):
      run one function in the original (under an emulator or a harness) and in
      the port on the same asset and compare outputs. Consider last.

## Order and first target

Build 1, then 2, then 3 and 4 together; 5 only if committing to Xbox; 6 when
a port exists.

Do not start with Halo Xbox: matching it needs the Xbox XDK compiler, which
Microsoft never released publicly. Start with a 32-bit x86 PC game built by a
retail MSVC (1999-2005 titles are plentiful), ideally one with a symbolized
build somewhere (a leaked debug exe, a console version with a PDB, a later
re-release). Prove the loop on a small exe first: build a fixture with MSVC,
strip it, and drive the agent to re-match it end to end before touching a game.

## Progress log

- 2026-09-28: plan written; no implementation yet.
