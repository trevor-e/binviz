# Differential proof campaigns

Follow the [recommended workflow](decompilation-workspaces.md#recommended-workflow):
execute a campaign when new behavior needs evidence; use `campaign-compare` to
reinspect saved observations. This page describes runner configuration and scope.

```text
binviz campaign campaign.json --out proof.json
binviz campaign-compare proof.json.raw.json --out compared.json
python tools/proof_campaign.py campaign.json observations.json
```

Configure existing native and WASM runners. Binviz supplies orchestration and a
shared comparator; it does not add an emulator or platform/device semantics.
The CLI refuses failed/refused/unexamined cases and unverified execution evidence.
Import raw or compared reports in browser **Contracts**, or MCP `proof_campaign`
with `report` / `report_file`, optional `report_id` and `root`. Reinspection
recomputes results, rehashes current artifacts and retains all observations and
first-failure fixtures. Imported passed labels are ignored.

Configuration has schemaVersion 1, `artifacts` (ID → file path), `native` and
`wasm` runner specifications, `cases`, `comparison`, `workers`, `memoryMb` and
`timeoutSeconds`. A runner specification contains an argv `command`, declared
`memoryMb`, optional `environment`, `tools` and `profile`. Artifact placeholders
are `{artifact:ID}`. Cases own exact IDs, fixtures, arguments and seeds; game
values stay in their project. Every pair loads its frozen modules once, then
handles many cases. Worker count is bounded by CPU and declared pair memory.
This is a scheduling estimate, not measured RSS or a hard process-tree cap.

The persistent JSONL protocol is:

1. Binviz sends `{"op":"init","schemaVersion":1,"artifacts":{...},"profile":{...}}`.
   Reply with `ready: true`, actual `version`, `architecture` and provider profile.
2. Binviz sends `{"op":"case", ...case}`. Reply with the same `id` and an
   `execution` record.
3. `{"op":"stop"}` ends the worker. Timeouts, malformed IDs/records and unknown
   operations remain failures/frontiers. Ctrl-C preserves partial observations
   and unexamined counts. Input/module/tool drift refuses current evidence.

Execution records contain status/reason, actual instruction count, return word,
RAM observations, registers, ordered events, checkpoints, observed coverage and
optional local objects. Executed records require observed return bits and actual
execution evidence (instructions or a return event). Call events identify the
actual provider and `real` / `controlled` profile. PCs, words, addresses, extents
and byte strings use exact `0x` strings. A provider-entry observer must record
post-delay-slot values when that is its claimed timing; the fixture compares
delay-slot annotations as part of the ordered event record.

Comparison selects a 32-bit return mask, exact RAM regions/registers, event kinds,
required checkpoints and expected frontier reasons. Full-word comparison is
explicit (`0xffffffff`). Memory exclusions need exact range and rationale;
overlaps, unknown regions and missing checkpoints refuse. A native and WASM
unsupported case with the same declared expected reason is a frontier, with no
executed coverage. A requested path is not observed coverage.

For different stack addresses, declare narrowly bounded corresponding local
objects and exact event/argument pointer positions. Compare extent, lifetime,
initialized bytes and pairwise alias relationships after pointer normalization.
Uninitialized tail bytes are reported separately; initialization does not enlarge
an object. Pointer references outside compared events, bounds or lifetimes refuse.
No broad implicit stack/RAM mask is added.

The actual observations are a separate content-addressed artifact to avoid a
self-digest cycle. `observationsBound` means supplied bytes match the imported
records; artifact lineage must separately be current. Neither axis establishes
that a third-party runner's claimed behavior is authentic. Keep real runner
versions/configuration and reproduction fixtures inspectable.

Tracked tests build one Windows native DLL and four real WASM modules with Clang
and lld, then run ctypes versus Node WebAssembly. Seven baseline cases agree;
each wrong-return-word, wrong-RAM-byte and extra-device-event module produces
seven concrete failures. This is source-instrumented Windows-x64/WASM32 execution,
not PS1 instruction execution or FF9 migration. The synthetic runners declare
those architectures and use explicit controlled event profiles.

Build: `python tests/tools/build_campaign_fixture.py` in the configured compiler
environment. Run: `python tests/tools/test_campaign.py` on Windows with Node.
Portable comparator tests: `cargo test -p binviz --test campaign`.

Campaigns also accept `baseline` / `candidate` runner keys instead of `native` /
`wasm`, including two WASM modules. Each runner records its configured role,
actual architecture and runner kind. Comparisons may require exact participant
identities. Role names do not imply an architecture.

An `exception` execution supplies its kind, mapped operation, optional actual
guest PC and required exception checkpoint. Actual instructions or an observed
exception event establish execution; no return value is invented. Comparison
checks outcome, exception identity/checkpoint, RAM, registers and selected ordered
effects. Missing checkpoints refuse comparison. Distinct architecture PCs can
remain diagnostic while the reviewed operation maps the trap.

The additional zero-divisor fixture runs actual Node WASM traps with and without
a cursor write before the trap. A baseline/candidate pair agrees for the correct
module and detects the early write. Where the local FF9 CPU model is available,
the same test uses that existing independent interpreter to execute a native
BEQ/delay-slot/BREAK fixture; Binviz does not add another instruction walker.
WASM branch targets are inspectable through the existing disassembler; physical
instruction order alone does not establish trap order. The execution checkpoint
compares memory at the trap.
Ordinary void functions may omit `returnWord` only with independently selected
`comparison.voidProfiles` for both participants. Each `voidProfile` pins
`compilerFacts`, `definition`, `module`, actual `function` index, `buildEvidence`
and an identical `reviewArtifact`. Compiler evidence must identify a void body;
the linked function must have no results and producer ancestry must include its
exact prepared compiler buffer. An actual execution/return event is required.
RAM, register, event and checkpoint comparisons continue; no zero result is
invented. Integer getters still require their complete return word. Diagnostic
native V0 can be compared separately through the selected register set.

Configured `nativeProviders` can inject a complete COP2 backend into an existing
runner. Selection pins `artifact`, `sourceArtifact`, `compilerArtifact`,
`profileArtifact`, `authority` and all seven `operations`: read/write data,
read/write control, command, load and store. `tools/native_provider.py` provides
the ctypes callbacks for the existing GTE C ABI and requires an executor's
`set_cop2_provider` hook acknowledging all operations. Instruction decoding,
RAM and branch/load delays remain with that executor; source-text rewriting is
not used. Executors without the hook refuse until their own callbacks are extended.

The pinned profile contains the exact `selection`, input `sha256` map, finite
`commands` and positive `operationBudget`. Unknown commands and exhausted budgets
refuse. The initialized runner acknowledges the exact provider selection. Each
execution returns ordered `device` events and complete `providerState` data/control
banks, operation count, profile and authority. Select provider names through
`comparison.providerStates`; missing state refuses. Campaign import recomputes
backend/source/compiler pins and checks the trace, command set and operation
budget. Register states compare independently of the authority labels; a shared
software backend establishes a software model agreement, not hardware fidelity.
