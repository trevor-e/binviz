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
