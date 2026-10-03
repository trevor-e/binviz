# Reusable decompilation tooling for binviz

Status: implementation handoff, 2026-10-03. This spec records gaps observed while
porting FF9 disc 1. It proposes reusable capabilities; it does not authorize ABI
exceptions or establish game correctness. The FF9 goal continues separately.

## Outcome

A new game should provide original assets, load-unit descriptions, compiler
configuration, reconstructed source and game-specific fixtures. Binviz should
own the repeated work of joining identities, inspecting compiler contracts,
analyzing native register use, tracking evidence, explaining blockers and
coordinating batches. A project should not need another collection of scripts
that independently rediscover callers, resolve ownership or walk MIPS CFGs.

The immediate user problem is visibility: an agent runs a Python checker, but
the user cannot inspect the calls, assumptions or reason it accepted a change
inside binviz. Every feature below must produce inspectable structured evidence,
not just a successful exit code or an opaque percentage.

## Existing capabilities: extend these

| Capability | Current implementation | Remaining gap |
| --- | --- | --- |
| Binary callers, callees, xrefs and call paths | `xrefs.rs`, existing call graph UI/MCP | Join compiler-side facts and verified load-unit identities to these edges. |
| Imported C call-contract findings | `contracts.rs`, `contracts` CLI, browser Call contracts view; commits `58e698d`, `e77050b` | Producing facts, verifying identities, source/instruction navigation and MCP exposure. Import currently checks report consistency only. |
| Exact PS1 incoming-register analysis | `mipsaudit.rs`, `Binary::audit_ps1_register`, `register-use --batch`; commit `57a0f11` | MCP/UI access, paired caller/provider workflows, migration of remaining Python CFG walkers. |
| Function context, signatures, source scanning | `decomp.rs`, `signature.rs`, `csource.rs` | Native signature estimates and lexical C discovery are not compiler definition ABIs. Preserve that distinction. |
| Overlay discovery and loading | `blobs.rs`, existing overlay loader and disc readers | Authoritative member manifests, shared fragments, gaps and collision checks. Guessed blobs are not verified ownership. |
| Native matching and cached scores | `matching/`, `matching/cache.rs` | Join scores to the exact scored extent and proof revision; do not rebuild the matching engine. |
| Ranked work and parallel claims | `queue.rs`, MCP `next_functions`/`mark`/`decomp_progress` | Add contract/provider blockers and stage progress; retain existing claims and ranking. |

There is no proposal to replace these with a second graph, work queue or parser.
Clang remains the C/C++ compiler authority. Existing native execution and WASM
test tools can remain external adapters initially.

Paths in the capability table are relative to `crates/binviz/src/`. Unless
qualified, migration script names below are under `ff9-decomp/wasm/runtime/`.
Private proof collectors under `build/` are local examples, not required inputs
or reusable dependencies. Implementers should start with the tracked synthetic
tests and treat FF9 image tests as optional local validation.

“Owned caller” means a caller whose definition/provider belongs to a verified
load unit and source revision. It does not mean finding native call edges.
Binviz already finds those edges; the missing work is joining them to actual
compiler definitions, physical ownership and current evidence.

## Priorities and dependencies

| ID | Priority | Feature | First deliverable |
| --- | --- | --- | --- |
| BV-01 | P0 | Compiler facts and complete contract audit | One maintained producer plus a shared facts reader and full caller audit. |
| BV-02 | P0 | Physical units and authoritative function extents | Verified units, ownership and collision/gap report. |
| BV-03 | P0 | Evidence identities and stage lineage | Verified/stale/unverified status with dependency invalidation reasons. |
| BV-04 | P0 | Scoped ABI policies and paired register audits | Caller/site/provider checks using existing `mipsaudit`; no global truncation. |
| BV-05 | P0 | UI/MCP parity and live batch progress | Human and agent access to the same facts, witnesses and blockers. |
| BV-06 | P1 | Audited adapter generation | Reviewable direct-call transformations with full provenance. |
| BV-07 | P1 | Differential proof campaign orchestration | Reuse native/WASM runners through declarative campaigns and comparable results. |
| BV-08 | P1 | Incremental batch build integration | Stage-specific cache keys, bounded workers and cache explanations. |
| BV-09 | P2 | Scratch stack and closure evidence | Bounded footprint reports with explicit indirect/IRQ limits. |

BV-01/BV-02/BV-03 establish identities. BV-04 consumes them. BV-05 should land
incrementally with each backend feature. BV-06 requires BV-04; BV-07 and BV-08
consume the same evidence model. BV-09 should follow concrete scene needs.

P0 is infrastructure for current correctness and visibility, not a promise that
it will immediately make FF9 playable. P1 reduces repeated harness/build work.

## Shared records and identity rules

Use versioned records shared by library, CLI, WASM bindings and MCP. Suggested
names below are API proposals, not existing commands.

- `UnitId`: identifies a physical member and its address space/load context.
  Include asset identity, member offset/size, load address and overlay/bank
  context. Equal virtual addresses in different units remain different entities.
- `FunctionId`: unit plus exact entry and ownership role. Source filenames and
  display names are labels. Case folding, neighboring notes and a guessed next
  symbol cannot establish identity or extent.
- `CallSiteId`: caller identity plus original PC, call/delay-slot identity and
  compiler reference span when a correspondence is established. Native and C
  locations can remain unmatched; never pair calls solely by list order.
- `ArtifactRef`: role, content hash, location and producing recipe/tool version.
  Store raw-byte and normalized-text hashes separately when both are relevant.
- `EvidenceRef`: input identities, analysis version, explicit assumptions,
  result, witnesses and dependencies. An import starts unverified.
- `StageRecord`: parent artifacts, transformation/flags, output identities and
  stage result. Represent original, canonical source, prepared source, CPP,
  compiler facts, transformed C, object/LTO object and linked module separately.

Use SHA-256 for artifact/evidence identity. Existing matching score caches have
their own purpose and need not be replaced. Serialize addresses as documented
hex strings in new interchange schemas; preserve exact 64-bit values through
WASM/JavaScript. Identify unsupported architectures/ABI profiles explicitly.

Maintain independent axes: fact extraction success, identity validity, analysis
outcome, policy eligibility, compilation/link result, native matching score and
runtime milestone. A fresh report can still contain unresolved paths. An
internally consistent report can still be stale. A matching object does not
establish gameplay behavior.

## BV-01: compiler facts and complete contract audit

**Repeated logic:** `typed-abi.py`, `resident-call-abi.py`,
`resident-definition-prototypes.py`, `check-resident-owned-prototypes.py` and the
private all-caller report collector in `build/root-full-engine/report-contracts.py`.

Ship one compiler adapter with binviz. It may invoke Clang directly or remain a
small maintained sidecar; games should configure it rather than copy it.
The browser imports its output because a browser cannot invoke local Clang.
Do not implement a second C parser or treat lexical `csource.rs` results as ABI
evidence. Rust owns the common schema, joins, validation and contract analysis.

The adapter records:

- Actual definitions with bodies, linkage, raw/desugared types, parameters,
  variadic status and UTF-8 byte source spans. Prototypes are separate records.
- Direct calls with actual supplied/promoted types, argument spans, declaration
  identity and result consumption. Preserve calls through cast expressions.
- Address-taking/stored callback references separately from direct calls.
- Same-TU definitions, K&R definitions and original typedef/pointer spellings.
- Compiler binary/version, target triple/data layout, flags, prepared-input hash
  and relevant preprocessing dependencies.

Preserve type structure even when an initial ABI profile cannot lower it.
Initial lowering can support reviewed 32-bit scalar/pointer contracts. Integer
width/signedness, plain-char behavior and `long` width come from the target,
not FF9 assumptions. Aggregates, floating point and varargs remain explicit
unsupported or reviewed profiles. Separate native O32 wire contracts from WASM
C contracts, including hidden result pointers and packed records.

Audit every call in a batch, not only the first mismatch in each translation
unit. Report count mismatches, consumed void results, narrowing differences,
missing/ambiguous definitions, same-TU conflicts and unsupported contracts.
Give each finding stable identities and source spans. A compilation failure,
missing cache entry or unexamined caller is an explicit extraction gap.

**Acceptance:** retain the current report-reader tests; reject declarations as
definition evidence; cover K&R, typedef/pointer-to-pointer, signed-short results,
default promotions, casted calls, variadics, aggregate profiles, void casts,
conditions/comma expressions and address-only references. The existing owned
prototype regression must keep the real pointer declaration and callback
identity. Include legal multi-declarator function declarations such as
`extern void a(void), b(void), c(void);`: early AST declarators do not end at
the statement semicolon. Preserve declaration-group and per-symbol identities
instead of asserting that every individual range ends with `;`. This was
observed in the improved BOOT provider closure. Audit a synthetic caller with
two independent errors and return both.
FF9's guarded AEE68 rewrite must carry the correct stage identity instead of
silently disappearing as a cache miss.

## BV-02: physical units, extents and ownership

**Repeated logic:** `inventory-resident-units.py`, `emit-resident-spec.py`,
`file11-assembly.py`, `file12-assembly.py`, provider ownership scripts and native
extent extraction from disassembly/adjacent notes.

Accept a game-configured manifest, verify selected physical members, then join
them to loaded binaries and functions. Separate archive parsing adapters from
the generic verification/ownership engine. Read/hash selected regions once per
batch; do not repeatedly buffer a whole disc/archive for individual functions.

Record complete-member identity, exact function spans, primary entries, shared
fragments, aliases, data/unknown gaps and semantic exclusions. Retain inferred
boundaries with an inference label; require an explicit exact extent for a
proof. `analysisExtent` and `matchingExtent` are distinct fields.

Before generating/compiling a provider, detect duplicate physical ownership,
conflicting aliases, out-of-member spans, generated ASM/gap versus new C object
collisions and ambiguous same-address unit mappings. Valid shared fragments
remain represented without becoming independent callable definitions.

**Acceptance:** reject a function crossing a member boundary; keep two overlays
at `0x800a7000` distinct; preserve file11 CF074 as CEED4's fragment; preserve the
file12 B7098 exclusion; distinguish 1CA70 from an inferred 1C8B0 merge. B0FC0's
312-byte extent must not become 656 bytes because of a note gap. The 13,460-byte
B44C0 analysis must not receive full matching credit from a 136-byte scored head.
Provide machine-readable spans/call sites so no script parses formatted disasm.

## BV-03: evidence identity, provenance and invalidation

**Repeated logic:** per-helper hash checks, frozen manifests, promotion scripts,
`file11-boot-abi.py::verify_cache`, plus duplicated source/prepared/object gates.

Expose one evidence validator over supplied local artifacts. Track each stage's
parents and recipe; show exactly which input changed. Support unverified,
verified, stale, missing and unsupported states. Only verified evidence for the
current inputs may support a policy or edit. A supplied evidence label/hash is
not verification by itself.

Keep input and output digests distinct. Record reviewed before/candidate pairs,
native matching before/after and the proof run that supports the candidate.
Promotion tooling should create a deterministic review plan and preserve the
before artifact before applying any requested changes. Application remains an
explicit operation, never a side effect of importing a report.

**Acceptance:** changed original member, native slice, caller source, prepared
text, compiler/flags, object or recipe invalidates precisely dependent evidence.
Reuse the 59 drift refusals in `boot-pure-contracts.py`; include changed caller
and call count. LF/CRLF representations stay distinguishable. Output corruption
cannot be accepted because timestamps match. Failed/before proofs remain
available beside a successful candidate.

Track linked function imports and data-symbol allocation separately. An
`--allow-undefined` link can leave tentative/common data symbols unallocated
without exposing a missing function import. Show the definition, allocation
and alias lineage; a clean function-import list cannot establish a clean link.
Include a synthetic WASM common-array regression that refuses unresolved data
and accepts the properly allocated definition.

## BV-04: scoped policies and paired native audits

**Repeated logic:** `file11-battle-void.py`,
`file11-script-tail.py::native_nonuse`,
`file11-gpu-tail.py::native_gpu_nonuse`, `boot-pure-contracts.py`,
`file11-boot-tail.py`, `file11-unused-args.py`, `file11-bios-copy-tail.py`.

Extend existing `mipsaudit` rather than creating another CFG walker. A paired
audit examines an incoming word inside the exact callee and its survival after
each caller's original call/delay slot. Audit V0 independently. Preserve read,
overwrite, discarded-boundary and unresolved endpoints as different evidence.
Never turn a discarded return into a native kill or assume ABI call clobbers.

Policies identify exact caller, site, provider, true definition contract,
supplied word mapping, input revisions and supporting evidence. Distinguish
extra-word nonuse, recovered missing input, discarded result, return conversion,
varargs/aggregate lowering and guarded unsupported paths. The default never
pads missing words, truncates extra words or substitutes a return value.

Represent external callee summaries and complete indirect target sets as explicit
reviewed inputs with verifiable dependencies. A runtime-dependent guard is a
condition with a declared unsupported frontier; absence of observed reads on a
normal path cannot authorize its debug/IRQ path. Complex game/device behavior
belongs in the runtime adapter, not hard-coded FF9 addresses in the library.

Migrate old walkers by comparing frozen decisions/witnesses with the new core.
A more conservative unresolved result remains unresolved until separately
reviewed. Do not force equality by weakening the core.

**Acceptance:** retain existing core test groups and original 31718/548E8
audits; compare 212 battle captures and 41 native summary dependencies; exercise
the pure-BOOT 4 provider/5 post-call audits. Mutating a helper to read A2 refuses
the old nonuse policy. Distinguish GPU debug call 13818 with live A3 from queue
call 1386c with no reviewed trailing words. Unlisted caller, changed site/count,
incomplete indirect targets and surviving load-delay frontiers cannot inherit
an allowance. Conditional-link/unimplemented ISA cases remain explicit.

Support path-sensitive external effects: a callee may leave a word untouched
on one path, overwrite it on another and terminate on a strict panic path.
Neither unconditional `preserved` nor unconditional `killed` describes that
summary. Add explicit may-write/no-read and nonreturning outcomes with verified
closure dependencies. FF9 D8820's A2 modes and BIOS panic frontier are the
observed migration case; current CLI unresolved results remain valid until the
summary model and relevant GTE/BREAK semantics can establish stronger evidence.
Include 4AAF0's null-list preserve versus nonnull overwrite paths as a smaller
case; never relabel a discarded endpoint as an unconditional register kill.

## BV-05: visible workflow, MCP and progress

Extend the current Call contracts screen. Add unit/caller/callee/kind/identity
filters and source, original instruction and evidence links. Show both native
and compiler views of a call, the applied policy, guard/frontier and stage
lineage. Let users inspect an audit path instruction by instruction. Keep
imported findings usable independently of a selected binary; verification is
a separate state. Export the selected evidence with its input identities.

Expose equivalent read-only structured APIs through persistent MCP sessions:
unit/extent inventory, compiler facts import/audit, evidence verification,
register audit batches, caller/provider pair audits and blocker queries.
CLI, WASM and MCP must share core implementations and identifiers. Desktop
compiler execution is optional/configured; browser-only tools import facts.

Batch jobs emit stage events: input verification, preparation, fact extraction,
native audit, adapter generation, compile/link and validation. Include total,
processed/accepted/rejected/unexamined counts, cache hits/misses and reason,
elapsed time and cancellation state. Emit failures and changed frontiers as
they are discovered; do not wait silently for the final JSON. A cancelled or
failed job retains its partial coverage and never becomes a clean full report.

Extend `queue.rs` ranking/claims with blocker dependencies, e.g. which missing
provider or contract unlocks the most callers, and export disjoint batch work
packages. Keep first-failure caller counts separate from all observed finding
counts. Include every known finding for a caller in its work package, labeling
already-reviewed policies, so successive first failures do not trigger separate
discovery and full-replay cycles. Count matching starts/bytes, behavior fixtures and gameplay milestones
separately; there is no synthetic overall playable-game percentage.

**Acceptance:** the same request has equivalent core results via CLI/MCP/WASM;
one loaded unit serves a whole register batch. UI filters preserve overlay/case
identity and display stale/uncached cases. Navigation lands on the exact source
and original PC. The current real-report browser tests remain valid. A 1,060-function
replay shows stage progress, accurately attributes newly resolved callers to
changed inputs and leaves unrelated blockers intact.

## BV-06: reviewable adapter generation

**Repeated logic:** `resident-call-abi.py`, `resident-definition-prototypes.py`,
`direct-abi.py`, `typed-abi.py::call_effect`, reviewed GPU/script transformations
and parts of `wasm/runtime/fnptr.py`.

Generate an edit plan from verified compiler facts and eligible policies. Use
compiler-provided reference spans; change only intended direct calls and their
required declarations. Refuse ambiguous macro/source spans and overlapping
edits. Handle grouped declarations through a verified whole-statement edit or an
explicit unsupported-span diagnostic; do not corrupt neighboring declarators.
Preserve every supplied argument expression's evaluation exactly once,
including allowed ignored words. Preserve formal/result narrowing and signed
extension, pointer bits, same-TU typedef declarations and original callback
addresses. Explicitly represent native hidden-result/packed-record bridges and
reviewed varargs; reject unsupported representations.

Generate portable prepared C/adapters and a before/after diff by default. Keep
canonical matching source unchanged when the issue is target portability. A
real source bug should instead have its own separately validated candidate.
Compile/link checks are separate from native behavior proof. Runtime frontier
imports must have an explicit throwing binding; returning handlers must not
silently turn an unsupported path into success.

**Acceptance:** reuse 72 narrowing cases, pointer/same-TU regressions and the
3,414 guarded GPU campaign. Calls through direct casts are recognized; stored
addresses stay unchanged. Four side-effecting argument expressions execute
once. Short-to-int returns preserve native sign extension. Changed input and
unlisted direct sites refuse edits. Applying a plan twice is deterministic or
an explicit already-applied result. Synthetic wrong-arity traps are detected
in linked WASM without depending on symbol spelling alone.

## BV-07: reusable differential campaigns

**Repeated logic:** numerous `build-*-proof.py` / `check-*.mjs` pairs,
`wasm/test-asmgen.py::Mips` fixture setup and the existing difftest runners.

Start with an orchestrator and runner interfaces, not a new Rust emulator.
Accept declarative campaigns describing input artifacts/entry/ABI, initial
registers and memory writes, deterministic fixtures/seeds, real provider
closures, explicit controlled services/device ports and expected unsupported
boundaries. Load original assets once per worker and share a WASM memory across
compatible modules. Bound workers by memory as well as CPU capacity.

Compare full used return bits, selected RAM/scratch regions, ordered provider
arguments/device-port traces and saved-register/stack properties. Memory masks
must state their exact ranges and rationale; an ABI stack exclusion is not
permission to exclude arbitrary mismatching state. Distinguish real C/native
provider execution from hooks and controlled GTE/MMIO outputs. Independent
result specifications have separate results from differential comparisons.

Results include cases executed, instruction/site/path coverage, failures,
refused/unsupported cases, exclusions, provider profile, versions/hashes and
reproduction instructions. Keep a first failing case and before/candidate
comparison; do not report unexecuted paths as validated. Pluggable GTE/device
oracles should reuse recording semantics instead of extending a CPU ad hoc
for every proof. A fuller shared native executor can be a later project.

**Acceptance:** reproduce 3,344 pure-LTO comparisons,448 used-return comparisons,
349 text/sound comparisons and the preserved failing control-byte 44 text case.
Preserve exact signed-short/pointer word fidelity, RAM and call order. Unknown
instruction/provider/MMIO operations and bounded runaways yield explicit
failure/frontier results. A wrong returned word, changed RAM byte or extra
device access produces a concrete diff. Fixtures with user assets remain local;
tracked synthetic tests require no game image.

## BV-08: incremental batches and cache diagnostics

**Repeated logic:** `compile-cache.py`, resident/BOOT builders, preparation
drivers, AST/LTO caches and repeated subprocess/file enumeration wrappers.

Use a configured external build adapter and common stage/artifact/cache records.
Do not port the FF9 Docker build wholesale into binviz. Separate preprocessing,
fact extraction, native analysis, transformations, compilation and link keys.
Key each on actual semantic inputs, relevant dependencies, compiler/flags and
algorithm versions. Unrelated orchestration/doc changes must not rebuild code;
changing a relevant header or transformation must invalidate its dependents.

Batch byte reads, dependency digests, binary loads and compiler facts. Reuse
unchanged original/CPP/facts/LTO artifacts, but verify output integrity. Publish
cache entries atomically, deduplicate concurrent identical work, and prevent
parallel edits/promotions from silently changing inputs mid-job. Show miss and
invalidation reasons in BV-05. Existing compile and matching caches remain
adapters until replacing them demonstrably preserves their guarantees.

**Acceptance:** an unchanged run reuses output hashes; changing one leaf source
invalidates only relevant work plus links. Preserve the FF9 case where 3 BOOT
sources rebuild while 352 engine and 53 menu definitions reuse preparation/LTO.
Header shadowing, changed include search path/compiler flags and corrupt output
must refuse a stale hit. Measure cold/warm wall time, bytes read, subprocess
count and peak memory. Seven native requests currently took 0.4012s in one batch
versus 2.5315s separately; keep equivalent results without claiming that speedup
for the entire decompilation pipeline.

## BV-09: scratch stack and bounded closure evidence

**Repeated logic:** `file12-scratch.py`, `file11-scratch-stack.s`, scratch closure
proofs and hand-maintained native/WASM frame/provider inventories.

Report native SP transitions, actual WASM frame/stack-pointer behavior, bounded
call-depth/footprint, shared scratch aliases and restoration paths. Treat
saved native SP words as opaque native state; they are not automatically equal
to a WASM stack pointer. Record dynamic frames, indirect callees, recursion,
exception/unwind and IRQ/reentrancy uncertainties. Bounds are lower bounds
unless the relevant closure and contexts are complete. Guards must precede
the writes they protect.

**Acceptance:** preserve B7098's exclusion despite its isolated proof; distinguish
its native 80-byte frame from the measured compiled callee frame. Reproduce
bounded initialization/packet checks and mark unknown callees/IRQ paths. A
bounded pool/driver test must not automatically admit an entire scene.

## Where game-specific code stays

Keep archive formats, asset ownership configuration, function names, recovered
source, fixture values, format strings and expected game state in the game
project. Keep GPU/SPU/SIO/MMIO, BIOS semantics, audio synthesis and browser game
sessions in platform/game runtime packages. Binviz can inspect/import their
contracts and evidence; this spec does not make them analyzer internals.

Addresses such as GPU debug-level/byteflag locations are policy data. Device
implementations and fixtures may be reusable across PS1 games, but that is a
separate runtime/package concern. This avoids tying binviz to FF9 logic.

## First implementation session

1. Read this spec and the existing contracts/register-use docs and core modules.
2. Land versioned artifact/unit/compiler-fact records and consistency tests.
3. Add the maintained compiler adapter/import path and all-call audit with
   explicit extraction gaps. Preserve current imported-report compatibility.
4. Add input verification/stale explanations and source spans to the current UI.
5. Expose existing single/batch register analysis and contract queries in MCP.
6. Demonstrate a synthetic end-to-end caller plus an optional user-local FF9
   report, without modifying FF9 canonical source or promoting game policies.

Follow with paired policy audits and migration of one existing CFG collector.
Keep each slice usable and validated before expanding. Use ordinary API/CLI/MCP
names consistent with the repository rather than blindly adopting these IDs
as command names. Do not delete legacy scripts until equivalence/refusals are
accounted for. Deferred features remain explicit backlog items.

## Ongoing feature-gap reporting rule

This is a standing requirement from the user for ongoing decompilation work.

- Before writing a new analyzer/helper, check binviz's library, CLI and MCP for
  an existing capability. Prefer extending it over copying analysis logic.
- Flag a gap when repeated logic, a second CFG/ownership/type walker, parsing
  human-readable output, duplicated identity checks or invisible progress is
  discovered. Do not wait until a large batch of scripts has accumulated.
- Give a brief user-visible flag: what is repeated, why binviz currently cannot
  do it, the temporary workaround and the reusable feature it suggests.
- At the end of each coherent batch/process introspection, update this spec or
  its linked backlog with feature ID, evidence/script locations, implemented
  versus missing parts, game-specific inputs, priority and acceptance criteria.
- Continue useful game work with a small adapter when needed. A feature gap is
  not a reason to stop the authorized goal or automatically rewrite all tools.
  Keep implementation scope separate when the user assigns it to another session.

The deliverable is shared, observable capability. Merely renaming Python files
or translating the same per-game scripts into Rust does not satisfy this spec.
