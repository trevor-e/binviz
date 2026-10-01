# Playbook: binviz commands instead of scripts

For the CLAUDE.md of a decompilation project that uses binviz (written for the
FF9 repository; copy it there and fill in the two placeholders). An agent
working a matching decompilation keeps writing the same shell loops and Python
scripts around binviz. Each of them exists because the agent did not know the
batch form of the command. This file says what to run instead.

**Rules.** Before writing a loop over `binviz`, or a Python script that
compiles, scores, tallies or edits notes, check this table. If no command
covers the need, add a line to `docs/ff9-requests.md` describing it and work
around it once; do not build a second tool.

Placeholders: `$EXE` is the boot executable or the overlay opened with
`--psx-exe $BOOT --overlay-at $ADDR`; `$COMPILE` is the batch compile command
for this project's toolchain, run once for a folder of sources, writing
`<name>.o` for each `<name>.c`, with `{srcdir}` and `{outdir}` where binviz
fills the folders in (the `docker exec … cc1 … maspsx … as` line from
`tools/try.sh`, looped inside the container).

| Instead of | Run |
|---|---|
| `for f in …; do tools/try.sh $f.c $f; done` (a wave of new functions, one compile and one score each) | `binviz flags $EXE src/<unit> "$COMPILE" --record --meta compiler=…,flags=… --notes $NOTES`: every `.c` in the folder is a variant, compiled in one batch, scored in one process, each function's best outcome recorded |
| A Python loop that compiles permutations of one function and scores each (`mk.py`, `solve.py`'s inner loop) | Write the variants as files into one folder, then `binviz flags $EXE <folder> "$COMPILE" --function <fn> --top 5 --json`; ranked by `distance`, which a search can climb where the percent is flat. The mutation operators stay in Python; the compile-and-score loop does not |
| `binviz match` once per object, then a script that reads the percents | `binviz match $EXE build/ --json`: one process, one entry per function with `percent`, `exact`, `distance`, `clusters`, the kinds of difference |
| A script that diffs two runs' percents (`cmp.py`: up, down, same, exact) | `binviz scores before.json after.json` on two `--json` outputs |
| A per-worker score cache keyed on source and headers | Nothing: `match`, `rank` and `flags` cache by content hash (`$BINVIZ_CACHE`) |
| Python that edits the notes JSON to set matched / nonmatching / percent | `match … --record` or `flags … --record`; over MCP, `mark` and `match_project` with `record`. Never write the notes file by hand; every change goes through the journal |
| `for fn in …; do binviz asm $EXE $fn \| sed …; done` | `binviz asm $EXE --list lanes/wave.txt --out build/asm --bare` (the first word of each line names the function) |
| A twins or look-alike script (`twins.py`) | `similar_functions` (MCP) or `next_functions`, whose first group is the functions shaped like one already matched; `store hits` for functions another project matched |
| A hand-kept lanes file splitting the work | `worklist k/n` (CLI) or `next_functions` with `claim: true` (MCP): shards and claims, so workers don't collide |
| A merge table of fragments joined by hand, and renames | A note with the size and no name (`annotate` with `at` and `size`) pins a function's extent; `resolve <old name>` says which function holds it now |
| Guessing a function's extent for `match` | `match $EXE obj <fn> --range <start> <end>` |
| `progress.py` with partial credit and totals | `binviz progress $EXE`: counts by state, partial credit, totals per area, merged ranges counted once |
| Reading jump tables out of the archive by hand (`jt.py`) | `blobs` already takes the data after an overlay's code; re-extract the blob at the size `blobs` gives, and `asm` has the tables |
| A script that works out which compiler built a function | `match` says it from the epilogue (GCC 2.7.2 pops the frame before `jr $ra`) |

**When the command's output is text and you need data:** every batch command
has `--json`; use it rather than parsing the text.

**When a function won't match:** `match $EXE obj <fn>` lists each difference
with what to try; `context $EXE <fn>` gives the callers, callees, strings and
the matched functions shaped like it, with their source files, before you
write a line of C.
