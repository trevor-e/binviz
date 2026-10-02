# Requests for binviz, from the FF9 matching work

Written 2026-09-30 from what the decompilation effort actually spent its time on. Evidence first, then the asks in
priority order. Everything here is about **removing N+1 patterns**: one process, one container call or one scoring round
trip *per variant* where one call for the whole batch would do.

## Evidence

* **Scoring**: scoring 300 objects with one `binviz match` process each took **21.4 s**; the same 300 through folder-mode
  `match <exe> <folder>` in one process took **0.285 s** (about 75x). Binviz re-reads and re-analyses the executable on
  every process start (~70 ms), which dominates for small objects.
* **Compiling**: each candidate costs a `docker exec` of cc1 + maspsx (a Python process) + GNU as: ~0.6 s. A hand-driven
  worker does about 1.7 variants per second. The farm that is being built compiles variants in parallel inside one container
  call, but binviz sees none of that: it can only score finished objects.
* **Searching**: about 750 functions are 60-99% identical; what is left in them is register allocation, scheduling and
  block layout. Workers solve these by trying many small C rewrites. Each worker wrote its own loop around
  compile-then-score because nothing shared existed (`build/w*/` has a dozen variants of the same search script). The
  percent plateaus, so a search has no gradient: a function stays at 96.4% across many different near-misses.
* **Bookkeeping around binviz** that Python had to do: a score cache keyed on source + headers + binary + merges, writing
  decomp state back into the notes, partial-credit totals, a compile-and-rank sweep over compiler flags (binviz's own `flags`
  command does that last one; it was not used, which was an oversight).

## Asks, in priority order

### 1. Machine-readable folder-mode `match`
`match <exe> <folder> --json` (and the same through MCP): one entry per **function** (not just "worst per unit"), with
`unit`, `name`, `address`, `percent`, `instructions_matched`, `original_len`, `rebuilt_len`, the diff categories with their
counts (`registers differ`, `reordered`, `extra`, `missing`, `instruction differs`, `branch offset differs`, `stack frame
differs`, ...), and `exact: true/false`. Folder mode already works in 0.3 s for 300 objects; today its text output only lists
the worst function per unit and the Python glue spawns a process per object just to read one percent.

### 2. A distance, not just a percent
For every function also emit `distance`: a weighted count of differing instructions, with register-only differences cheaper
than missing or extra instructions (suggested weights: register differs 1, reordered 2, immediate/offset differs 2,
instruction differs 3, extra/missing 4, branch layout 4, frame/slot size 6). A search compares distances; it cannot climb on a
percent that stays flat. Also useful: the number of *distinct* mismatch clusters, so "one swapped register pair in 40
places" and "40 independent differences" are different things.

### 3. Batch rank of variants (generalise `flags`)
`rank <exe> <variants...>` / MCP `rank_builds` for **many objects of one function** (or many functions): accept a folder or
list of objects, score all in one process, return them ranked by distance, with `--top N`. This is item 1 plus sorting and
a stable key per variant (file name), so a caller can pass 500 variants and read one ranking. The existing `flags` command
compiles one source with several flag sets through a user command (`{src}`, `{out}`, `{flags}`); extend it:
* `--jobs N` to run the compile command for several variants at once;
* an optional **batch compile command** that receives a directory of sources and must produce a directory of objects (so the
  caller's script can fan out inside a container with one `docker exec`: the 0.6 s startup paid once per batch);
* variants given as files (not only flag sets) so mutations of the source can be ranked, not just flag choices.

### 4. A content-hash cache for scoring
Cache each (object bytes, function, notes extent, binary hash) result so repeated scoring of unchanged objects is free
inside binviz: the Python side keeps its own cache keyed on source and headers, which is wrong in subtle ways (it did not
notice merge-table changes until it was fixed by hand). With object-hash keys the cache is correct by construction.

### 5. Recording results, from the CLI
`match <exe> <folder> --record [--meta compiler=...,flags=...]` writing `decomp` state (matched / nonmatching, percent,
source, compiler, flags) into the notes for every function it scored, as the MCP `record` option does today. The notes
journal makes concurrent sessions safe. Today the project edits the notes JSON from Python.

### 6. Smaller things seen along the way
* Report the compiler an epilogue implies in the per-function JSON (`lw $ra; addiu $sp; jr; nop` = GCC 2.7.2.x).
* A way to ask which function now contains an old fragment name after boundary joining (173 old names stopped
  resolving after the join work): `resolve <old name or address>`.
* `progress`: also count a merged range once, and give totals by area (boot executable vs overlays), by target of the notes.
* The new function joining treats a fall-through or a lone `jump` into a piece with no callers as one function; that looked
  right in the case checked (`sub_80030874`/`sub_800308c8`), but a piece that is itself a `jal` target elsewhere is a real
  entry point and should probably not be absorbed.

## What deliberately stays outside binviz

The *mutation operators* (statement reordering, variable splitting, register pins on a subset of locals, type changes,
dummy temporaries, duplicate arms, loop-shape rewrites) are C-source transformations tied to GCC 2.8.1's behaviour; they need
a C parser, which a binary-analysis tool should not grow. They live in `tools/solve.py` in the FF9 repo and call binviz for
the scoring. Items 1-4 are what that tool needs from binviz to be fast and to have a gradient.

## Status of the FF9-side prototype

`tools/farm-compile.py` and `tools/solve.py` (written by a worker, in progress) implement items 1-3 in Python on top of
folder-mode `match` and a container batch; their throughput numbers and the operators that paid off will be in
`docs/solve.md` and are the specification this request should be refined against.

## Feedback on c8fa8b4 (2026-09-30, tried on the FF9 boot executable)

Works as described: `match <exe> <folder> --json` scored 1175 objects in 2-4 s; `rank` ranks by distance (sub_8004bb7c:
d=44, sub_80051f54: d=2032); `progress` and the cache run. One regression against the build before it:

* **12 functions that scored 100% before now score low because their extent changed** (818 -> 808 exact among the objects
  checked; one function now scores higher). Example: `sub_8004bb7c` is `addi $a0,$a0,0x400` followed by a fall-through into
  `sub_8004bb80` (80 bytes), which has its own callers. The old binary read it as one 12-instruction function (our C for it
  matches all 12); the new one cuts it at 8004bb80 into 1 + 80 instructions, so the match shows "1 of 12". Others:
  sub_80051f54, sub_800531d0, sub_80051300, sub_80051084, sub_80018b40, sub_80030874, sub_8004bf20. This is the opposite
  side of the joining rule above: a piece that is called from elsewhere *and* is entered by fall-through from a function
  with a short prologue is how GCC lays out functions that share a tail. A note's `size` fixes it per function, but nothing
  says which ones need it; `match` could report "extent differs from the object's: the object is longer than the function
  and covers its neighbour" instead of "extra in the rebuild".

## Feedback on ab60342 (2026-10-01, same boot objects, notes as committed)

The 12 pinned extents hold, and `sub_8004bb7c` now reads 21 instructions instead of 1 (still not its 12, so our pin stays).
But the join rule absorbs **64 functions that exist as named notes entries**, and 26 of them were exact: they are the
fragments of the script interpreter (`sub_8003abd4`, `sub_8003ac50`, `sub_8003c374`, ... , `sub_8003cab4`; our notes name
each fragment because we score the interpreter per fragment) plus `sub_800512b8`, `sub_80051380`, `sub_80051610`. Folder-mode
`match --json` no longer has an entry for them: 966 exact functions with c8fa8b4, 940 with ab60342 on the same objects.

Asks: **a note that names a function (or gives it a `size`) is always a function boundary; joining must never remove a
boundary the notes put there.** The joiner is for unannotated pieces. The same holds for `resolve`: an annotated address
resolves to itself.

Not installed for the decompilation workers until that is fixed. The new features (`compare` of two runs, bulk `asm` export,
record from `flags`) look useful, especially bulk asm export: workers still run one `binviz asm` per function.

## Request: instruction-to-C-line mapping for a rebuilt object (2026-10-01, from the staged search)

A search over C rewrites should try the statements nearest the mismatching instructions first. `match <exe> <obj> <fn>`
lines the two functions up but says nothing about which line of the C each rebuilt instruction came from.

What exists today without binviz: an object built with `cc1 -g` (GCC 2.8.1 writes stabs: `.stabn 68,line,$LMn` labels, so
the object has `.stab` N_SLINE entries with the instruction offset of every line change). `tools/solve_aim.py` uses it:
compile with `-g -dg` (cpp WITHOUT `-P`, so the line markers keep the real file lines), read `objdump --stabs`, and look
up the rebuilt instruction index in the match rows. Two catches that cost work: (1) with `-g` maspsx lays out `nop`s
differently (the stab labels sit between instructions and stop its load-delay logic), so the `-g` object is not the real
one and its instructions have to be matched to the real object's by order, ignoring nops (`tools/farm-runner.py
debug_build`); (2) the stabs addresses are `.text` offsets, the FUN stab of a function comes after its first line stab,
so lines are attributed to functions by offset range, not by stab order.

Ask: `match <exe> <obj> <fn> --lines` (and in `--json`): per rebuilt instruction the source file and line from the
object's own `.stab`/`.debug_line`/`.loc` data when present, and per difference row the line(s) of the rebuilt side (for
"missing in the rebuild": the lines of the neighbouring rebuilt instructions). Then a caller can say "the mismatch is on
line 35" without compiling twice and aligning nops. Also useful there: the allocator dump (`cc1 -dg`: which hard register
each pseudo got) is how the search tells a pin with a second writer from a clean one; if binviz ever reads cc1's dumps,
"registers shared by a pinned variable and another value" would be a single answer.

## Consolidated asks after thirteen waves (2026-10-02)

State: 4705 of 5471 functions exact, 51.8% of the bytes; 765 partial functions left. What cost the most time or hid the most
bugs, in the order I would build it. Evidence is from `lanes/result-wave*.txt` and the tools in `tools/`.

1. **Never join away a boundary the notes name** (see the ab60342 section above). Still the blocker for using anything newer than c8fa8b4.
2. **Instruction to C-line mapping for a rebuilt object** (section above): `tools/solve_aim.py` does it from a `-g` compile aligned
   to the object; the staged search aims mutations with it. Native support in `match --json` (per mismatching instruction: the
   source line from the object's `.loc`) would remove that script and help every reader of a diff.
3. **Constants, widths and call arities, side by side.** Most real bugs this project found (about 80 behaviour bugs that the
   percent hid) were found by private scans over diffs: the multiset of immediates present on one side only, mnemonic counts
   (`lb`/`lbu`, `sb`/`sh`, `sra`/`srl`, `lwl`), stack-argument stores of original vs rebuilt, and a call's argument count against
   the callee's own signature (about 50 hits, 6 real in one pass). A `match --json` field per function for each, plus an
   `arity` check against `signature` of the callee, would replace several Python scripts (`build/w11p1/*.py`, `build/w9p4/`).
4. **`reordered` in folder-mode `match --json`** (it never appears there; scheduling-only differences have to be classified by a
   private multiset test; `tools/remaining-classify.py`). Also a stable `distance` per kind in the JSON.
5. **Unmasked relocation check.** The scorer masks relocations, so a wrong global or callee scores 100%: found only by linking the
   object at its real addresses and comparing unmasked values (`build/w9p3/audit.py`, `wasm/addrcheck.py`, `wasm/calleecheck.py`).
   A `match --strict-relocs` that compares symbol targets to the original's would catch these everywhere.
6. **Bulk `asm` export**: workers still call `binviz asm` once per function. One call for a list or a unit (as splat writes it)
   would remove the N+1; `m2c` the same (`binviz m2c` starts a Python process per function, 0.4 s of start-up for 0.25 s of work).
7. **Twin detection**: `tools/twins.py` groups functions by relocation-masked bytes and found 250+ exact transplants in one run
   (the biggest single win). `diff functions`, `names` and `store hits` are close; a `twins <file...>` command that returns
   groups, near-twins (>= 95% of masked words) and the relocation correspondence (the pairs of words that carry an address) would
   replace most of it.
8. **Progress**: `progress` that counts a merged range once, takes data ranges out of the denominator (`config/data-ranges.tsv`),
   reports by area and by function size band, and exports a treemap-ready JSON (see `docs/progress-treemap.html`: tiles = functions
   sized by bytes, coloured by state; in this project it is generated by `tools/treemap.py`). The measure that matters is exact
   bytes; function count runs far ahead of it because the large functions are the hard ones.
9. **A per-function `extent` warning**: "the object is longer than the function and covers its neighbour" or "the function runs
   past the unit image" instead of 'extra in the rebuild'. Several capped functions looked like register puzzles for hours
   (`sub_800de11c`, `sub_800bc600`, hoisted `lui/lw` heads that start 8 bytes early).
10. **Score cache and `rank` for variants** are used by the farm (`tools/farmlib.py`: 35-70 variants/s). Keep them.

## Status (2026-10-02, after the consolidated asks)

Built on main, in this order, and tried on the FF9 boot executable's 1489 objects (`build/pintest` in the FF9 repo):

1. **Notes are boundaries** (ask 1): `match`, `rank` and every other command that loads `--notes` re-read a ROM with the
   notes' functions as entry points (`Binary::with_function_boundaries`, `notes::function_boundaries`: a note that names
   a function, records its decompilation state, or says `function` with a size), and the run joining never absorbs an
   entry point (`rom/analysis.rs`, `join_runs`). The MCP server does the same when it loads notes. Result on the boot
   objects: 1308 functions scored instead of 1254, 2 unplaced instead of 66 (the 54 are the "boot functions the old
   finder missed" that had C already); 1109 exact against c8fa8b4's 1110. **33 functions in the script-interpreter region
   are shorter than c8fa8b4 read them** (sub_8003fbb4: 24 bytes, not 8928): c8fa8b4 had glued a six-instruction opcode
   handler that ends in `j` to the handler after it; the walk now stops at the jump, as it should. A note with `size`
   (config/merges.tsv) keeps the old extent where the C is written that way; `compare` of the two runs lists them.
2. **`reordered` in folder mode** (ask 4): a removed instruction that is added elsewhere in the function is `reordered` on
   both sides, wherever the two hunks are (before: only inside one hunk, so folder mode never showed it). 126 of the boot
   partials now carry the kind; the distance of a scheduling-only function drops (sub_8002920c 120 -> 94).
3. **`--strict-relocs`** (ask 5): a symbol the original has no name for is taken at the address its name carries
   (`D_800CB188`, `dword_800685fc`, `sub_…`); `%hi`/`%lo`/`jal` fields are then compared, never through the cache. On the
   boot objects 25 functions differ in a relocation, one of them scored exact (sub_8005c348 loads two globals into swapped
   registers; they feed an OR, so harmless). `tools/strict-audit.py` in the FF9 repo runs it over every unit.
4. **Side by side** (ask 3): every non-exact `match` result carries `audit`: immediates and load/store offsets on one side
   only (address pairs and relocations left out), the mnemonic counts that differ, stack stores on one side only, and calls
   to the same callee whose argument registers set before them differ (with the callee's own count when the original
   knows it). Text output prints it as "Side by side:"; JSON as `audit`.

Still open: the instruction-to-C-line mapping (ask 2), twins (ask 7), progress by area and size band (ask 8), the per-function
extent warning as a field rather than a difference text (ask 9; the text exists since ab60342).

Follow-ups (same day): a gap before a pinned function is never attached to it (an overlay image starting mid-function had moved
sub_800b7954's start to the image start); a relocation note stands only when the original's field occurs in none of the rebuild's
relocations (20 of 67 audit hits were a hoisted `lui`); the audit tracks address-holding registers instead of a four-instruction `lui`
window. The FF9 project switched its scorer to this build (`binviz-pin.exe`, 47 extent pins): 54 more functions scored, nothing lost.
