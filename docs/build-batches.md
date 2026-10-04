# Incremental build batches

Follow the [recommended workflow](decompilation-workspaces.md#recommended-workflow):
preflight current identities, prepare reviewed edits, then build the affected stages
once and reuse this report. This page describes configuration and cache behavior.

```text
binviz build-batch config.json --out build-report.json
python tools/build_batch.py config.json build-report.json
```

Commands run as argv arrays with no shell. Stages use the common artifact/stage
model; the maintained adapter orchestrates configured tools, without a game
builder or second C parser. `--python` selects the interpreter for the CLI.

Minimal configuration:

```json
{
  "schemaVersion": 1,
  "workers": 2,
  "memoryMb": 512,
  "cache": ".binviz-stage-cache",
  "output": "binviz-candidates",
  "stages": [{
    "id": "candidate-compile",
    "phase": "compile",
    "unit": "caller-tu",
    "inputs": {"candidate-source": "candidate.c"},
    "outputs": {"candidate-object": "candidate.o"},
    "command": ["clang", "--target=wasm32", "-c", "{input:candidate-source}", "-o", "{output:candidate-object}"],
    "includeDirectories": [],
    "tools": [],
    "memoryMb": 128,
    "timeoutSeconds": 120
  }]
}
```

Phases: prepare, extract, analyze, transform, compile, link, validate. A dependent
input is `stage:STAGE_ID:ARTIFACT_ID`, with the same artifact ID as the upstream
output, and the producer listed in `dependsOn`. `{include:N}` selects a frozen
include directory; pass it through the compiler's normal include flag. Bind every
source, transformation, configuration, linker script, runtime library and extra
tool that affects output. Undeclared inputs cannot be made safe by a cache key.

Keys include actual input bytes, executable/tool digests, adapter version/hash,
platform, argv, semantic environment, output paths and include-search directory
indexes. Indexes bind names and contents, so a newly shadowing header invalidates
preparation. `environment` supplies explicit values; `environmentKeys` includes
additional inherited semantic variables. Values are hashed rather than printed.
Include roots should be precise and exclude the batch's cache/output directories.
Directory symlinks need explicit snapshots for cross-platform reinspection.

Each stage runs with frozen inputs, stable paths and bounded timeout. Workers and
declared stage memory are bounded; sampled direct-process RSS also refuses a job
that exceeds the memory budget. RSS excludes grandchildren and parent process
memory and is sampled, not a complete operating-system memory cap. Missing
samples are null. Atomic cache publication and key locks deduplicate concurrent
identical stages. Output digests are rechecked on reuse; corrupt entries rebuild.
Input drift at the end of a run refuses promotion even if old frozen outputs are
valid cache entries. Ctrl-C leaves explicit partial/unexamined results.

Stage reports retain the actual process exit code, stdout and stderr when cleanup
also fails. `cleanupWarnings` and JSONL `cleanup-warning` events identify remaining
work/lock paths without replacing the primary rejection or missing-output reason.
Owned locks are released independently. A cleanup warning does not invalidate
already published, digest-verified outputs or make a rejected stage successful;
failed subprocesses still publish no cache entry. The next run rechecks retained
work before executing in the same namespace.

JSONL events on stderr show phase, processed count, cache key/reason and time.
The report records wall time, subprocesses, bytes read, hits/misses, declared
scheduled memory and observed direct-process RSS. Warm tests reuse all three
synthetic stages with zero subprocesses; changing one leaf reruns that leaf and
its link, while an unrelated source stays cached. Corrupt leaf output rebuilds it;
if reproduced bytes are identical, the link remains cached. These are synthetic
measurements, not a claim about FF9 throughput or Clang-facts-specific cache reuse.

Import the report through browser **Contracts → Import build stages**, MCP
`workspace_import.build_report_file`, or CLI `workspace --build-report`. Existing
artifact digests and stage records cannot be silently replaced; baseline and
candidate transitions use different IDs and an explicit promotion plan.

Tests: `python tests/tools/test_build_batch.py`.

For readability comparisons, set `compilerVisibleNamespace` and map baseline
and candidate artifact IDs to identical `inputSlots` and `inputNames`. Stages
share a locked stable compiler-visible directory while retaining distinct stage,
artifact and cache identities. Complete object equality remains mandatory;
path normalization does not waive object differences. The fourth batch regression
checks the actual compiler-visible filename and directory in both namespaces.

Use `inputLayout` to map input artifact IDs to repository-relative staged paths,
`includeLayout` to map include-directory indices to complete staged scan roots,
and `executionRoot` for the producer's working directory. Paths are relative to
the stage workspace, reject absolute/parent traversal, and enter the recipe key.
Overlapping file/directory inputs must have identical bytes. Full directory
indexes capture scanners such as a fixed-register map generator, including files
the compiler does not include. A dependency superset is valid.

For Docker commands, `mountMappings` maps `{mount:N}` to a bind mount. Declare
`source: "execution-root", readOnly: true` for frozen inputs and
`source: "outputs", readOnly: false` for a separate owned output tree. A writable
output mount can be nested at `/work/build` beneath read-only `/work`; unrelated
overlaps refuse. This supports a producer deriving paths from its repository root.
Windows bind staging remains the default; Linux-volume archive staging is an
unmeasured optimization hypothesis, not an established speed gain.

For short Windows paths, `cache`/`output` may be objects containing an explicit
ancestor `workspace` and owned `.binviz-*` `directory` basename. Ownership is
recorded for this configuration, symlinks and other owners refuse, and cleanup
remains bounded to that directory. This does not use `../` cache paths or change
machine-wide long-path settings. Setup/path-limit failures are reported separately.

Optional `preflight: {"workspaceFile":"workspace.json","executable":"binviz"}`
invokes the shared identity-only workspace preflight before any producer. All
mismatches are retained in a refused batch report with zero producer subprocesses.
Verified preflight inputs are frozen and checked again for drift during the run.

Declare `measurementsOutput` as an output artifact ID to capture a producer's JSON
counters even on a nonzero exit. `stagingSeconds`, `producerSeconds`, exit status,
stdout/stderr and available producer measurements survive rejected/refused stages.
Actual subprocess counts exclude failed staging. Warm cache reuse launches no
producer; it does not prove an optimization reduced cold compiler/linker work.
