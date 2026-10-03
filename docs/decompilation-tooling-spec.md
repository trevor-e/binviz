# Reusable decompilation tooling for binviz

Status: implementation handoff and living backlog, 2026-10-03. This spec records gaps observed while
porting FF9 disc 1. It proposes reusable capabilities; it does not authorize ABI
exceptions or establish game correctness. The FF9 goal continues separately.

## Handoff summary

Use [the concise implementation handoff](decompilation-tooling-handoff.md) for
delivery order and a prompt for another session; this document supplies the
detailed records, observed cases and acceptance requirements.

Start with the existing implementations and the implementation checkpoint below.
Some compiler-fact/evidence/UI/MCP work is already present in the working tree;
do not build competing versions. Availability of a backend is separate from
demonstrated migration of the FF9 collectors. Reproduce the synthetic example
with matching producer/consumer versions before using it to replace a helper.

The next useful slice is **verified ownership plus a complete caller work
package**. For one selected caller, show every compiler finding, the actual
linked provider, original call instructions, current policy scope, evidence
status and build outcome in one inspectable view. Expose the same record to
agents through MCP. Then add scoped policy consumption and incremental batch
execution; these remove repeated discovery and full-replay cycles.

| Work currently repeated in game scripts | Binviz destination | What the user should see |
| --- | --- | --- |
| Find which recovered callers belong to a load unit; resolve duplicate names/addresses | BV-02 verified ownership, extending native xrefs | Physical member, extent, aliases, conflicts and why a provider owns the call. |
| Extract real definitions and inspect every C call | BV-01 maintained Clang adapter and shared contract audit | Supplied arguments, definition ABI, source spans and every finding/gap. |
| Check extra words, return use and exact caller/site eligibility | BV-04 paired audits and scoped policy consumer | Instruction witnesses, surviving bits, eligible/refused sites and explicit frontiers. |
| Recheck source/native/prepared/object hashes and frozen proof inputs | BV-03 shared stage/evidence validator | Current/stale/missing inputs and the exact change invalidating a decision. |
| Generate direct-call bridges and preserve callback identities | BV-06 edit planner | Reviewable before/after edits, argument evaluation and required evidence. |
| Build native-versus-WASM proof recipes and compare traces | BV-07 configured campaigns | Real versus controlled providers, case diffs, exclusions and reproduction recipe. |
| Repeat preparation, compile, link and full caller scans | BV-08 stage cache and bounded batches | Reused work, invalidation reasons, progress and actual linked providers. |
| Rank the next blockers and coordinate disjoint agent work | BV-05 existing queue extensions | Unresolved caller packages, dependencies and claims; applied policies stay inspectable. |

Python itself is not the problem. A maintained Clang adapter is reasonable;
per-game copies of type/ownership/CFG logic are the duplication to remove.
Game configuration, recovered source and expected fixture values remain in
the game project. Shared analyses, identities and explanations belong in Binviz.

For the caller-discovery concern specifically, native callers already come from
Binviz xrefs. The missing feature is a verified join from those edges to the
selected C definition, physical load unit and current build. Do not port an
"owned callers" Python collector verbatim. Replace its duplicated joins with
BV-02/BV-01 records and show the result through the BV-05 caller inspector.
The inspector should identify which steps ran in the shared core, compiler
adapter or game runner, with inputs, structured results and reproduction links.
This makes a temporary orchestration script inspectable while it is migrated.

Implementation handoff prompt:

> Extend Binviz using this spec. Inspect the current implementation checkpoint
> and existing contracts, compiler-facts, evidence, register-use and queue APIs
> first. Deliver verified ownership and one complete, visible caller work package
> through the shared core, CLI, UI and MCP; then implement exact caller/site policy
> consumption. Use tracked synthetic inputs for acceptance. Preserve unresolved
> outcomes and all negative scope/staleness checks. Demonstrate equivalence with
> one frozen legacy collector before retiring it. Keep archive/device semantics
> configurable and do not modify FF9 canonical source to obtain a tool pass.

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

## User workflow: make the analysis visible

The first useful workflow is **Inspect caller**. From a function, graph edge or
compiler finding, open one work package containing:

1. The selected physical unit, original extent, canonical source and prepared
   source, with verified/stale/missing identity indicators.
2. Every known direct-call finding for this caller, including already-applied
   policies and extraction gaps; callback/address references have their own list.
3. For each call, supplied C arguments beside the actual definition contract,
   the original call and delay-slot instructions, and any unmapped correspondence.
4. Register-audit witnesses: which instruction reads or overwrites each word,
   which paths remain unresolved, and why a policy is eligible or refused.
5. The current preparation/compile/link outcome and the evidence supporting the
   next proposed action. Clicking a reason opens its source, instruction or input.

The second workflow is **Audit batch**. Select units or callers, preview the
inputs and analyses, then run one job with progress, cached work and concrete
failures visible. Export the same structured work packages for agents. Inspection
and audit do not silently edit source or grant policies; a generated edit plan
is a separate, reviewable artifact.

Suggested operation boundaries below are requirements, not prescribed command
names. A browser can import compiler/build outputs and run supported local
analysis; a desktop adapter can produce those outputs.

| Operation | Inputs | Inspectable output |
| --- | --- | --- |
| Verify unit inventory | Member manifest and supplied original bytes | Ownership, exact/inferred spans, aliases, collisions and missing providers. |
| Import/extract compiler facts | Prepared inputs, target/compiler recipe | Definitions, calls, source spans and explicit extraction gaps. |
| Audit caller contracts | Unit identities and compiler facts | All findings per caller, actual provider selection and unresolved joins. |
| Audit register batch | Exact native spans, registers and explicit summaries | Ordered witnesses, path outcomes and summary dependencies. |
| Explain a policy | Caller/site/provider identities and current artifacts | Eligibility, refusal reasons, original observations and applied stage. |
| Plan adapters | Eligible policies and compiler spans | Exact edits, before/after artifacts and preserved expression evaluation. |
| Run proof/build batch | Configured runners and artifact dependencies | Case diffs, stage events, cache reasons and actual compile/link outcomes. |
| Query blockers | Findings, policy ledger and current build records | Outstanding work packages ranked by verified blocked callers. |

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

Classify the actual selected provider as original reconstructed C, retained
assembly, compiler runtime/libcall, external host, generated bridge or missing.
Record its definition contract and selection reason across the linked module.
Distinguish an available source/object from the definition actually linked:
exclusion or substitution may select a different provider. Generated host C
definitions require the same compiler-fact inspection as recovered C; argument
counts in a resolver map are descriptive metadata, not definition evidence.
A familiar library name does not prove a typed provider: file12's `abs(int)`
calls currently encounter a generated twenty-word host bridge, while original
`printf` and `sprintf` boundaries require separate varargs/prefix work. Add
synthetic libcall-versus-host collisions and inspect the final linked signature;
absence of a source file or compiler-facts record is an explicit provider gap.

Do not merge definition maps with last-writer-wins semantics. A linked resident
definition can share a symbol/address with a BOOT overlay/HLE surrogate while
the actual direct reference resolves to the resident definition. Record all
candidates and the link/namespace decision before choosing its contract. The
file12 B7A14 case exposed this: its genuine `int(int)` contract was overwritten
by a twenty-word BOOT surrogate in a private provider inventory, creating false
caller blockers. Acceptance must join the selected resident provider, retain
the shadowed surrogate as such, refuse ambiguous selection and independently
verify callback/runtime registration. A caller inspector must explain the
winner; simply reversing map-update order is not sufficient ownership evidence.

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
Compare every native range, not just entry names. A new provider can contain a
previously compiled entry with a different name; flag its canonical source,
objects, resolver registrations and coverage records before promotion. Record
the reviewed retirement or alias decision and preserve the previous evidence.

**Acceptance:** reject a function crossing a member boundary; keep two overlays
at `0x800a7000` distinct; preserve file11 CF074 as CEED4's fragment; preserve the
file12 B7098 exclusion; distinguish 1CA70 from an inferred 1C8B0 merge. B0FC0's
312-byte extent must not become 656 bytes because of a note gap. The 13,460-byte
B44C0 analysis must not receive full matching credit from a 136-byte scored head.
Detect BOOT 4BFB0's 312-byte extent enclosing the old 4BFB8 304-byte source:
the latter omits two instructions that initialize incoming V0/V1 and its portable
C uses uninitialized locals despite an exact native score. Require an ownership
decision before retaining either resolver entry. Replacing that fragment must
not credit two functions or double-count the 304 shared bytes.
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

Enforce caller/site restrictions in the shared policy consumer and edit planner,
not only in a game adapter's filtering code. A policy carrying an
`allowedCallers` field must not become a global callee allowance when merged
with another collection. Report an equivalent existing allowance separately
from conflicting policy claims; neither may silently broaden the scope.
The five-caller BIOS-copy contract is the observed case: the collector records
its caller list, but the current generic rewriter requires the builder to filter
it manually. Acceptance includes a sixth caller and a second unlisted call site
that remain refused even when another caller has a verified policy.

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

Keep physical register reads distinct from observable use of incoming bits.
Add an explicitly validated bit/lane-sensitive mode for partial writes and
unaligned merge loads; keep the current conservative word audit available.
DB6B8's paired LWL/LWR paths are the observed case: an instruction reads old
register fragments, but a later merge may replace them before a provider, store
or return observes them. Its error/formatter paths remain separate frontiers.
Require architecture-correct load-delay/merge behavior and exact paths; never
pair loads merely because they are adjacent. Acceptance includes a complete
replacement pair, an intervening observable store, a pair leaving old bits,
aliasing memory and a branch separating the operations. Report which incoming
bits reach each observable boundary instead of inventing a whole-word kill.

Support separately checked bounded-loop facts where a cycle alone prevents a
post-call conclusion. Record induction initialization/update/bound, exits,
possible writes/aliases and the incoming bits at each iteration. D7094's second
post-call audit is the observed candidate: a slot counter progresses from zero
to eleven, with the next active iteration overwriting A0. Until the core verifies
that fact, retain the current unresolved cycle; finite fixture success alone is
not a general loop proof. Acceptance includes an altered step/bound, a skipped
update and an aliased counter write that invalidate the proposed bound.

Add a **used-return closure certificate** for return-contract corrections.
Join the original caller's consumed V0 bits, the provider's path-sensitive
return behavior, the installed indirect callback identity/contract and the
actual compiled provider/resolver wrappers. A byte store in one caller does
not establish a byte-return provider ABI. Show where narrowing occurs and
whether another caller or wrapper observes the full word. Unknown callback
targets and incomplete paths remain explicit frontiers.

Concrete acceptance: FF9 BBF30 stores V0 as a byte after calling BOOT21698;
21698 returns the installed AB810 callback's full slot word unchanged. Slots
256 and 264 must reach a full-word caller or resolver as 256 and 264, and reach
the original byte store as 0 and 8. The old void provider and byte declaration,
and all three corrected sources, independently score 100% native matching;
matching alone must not satisfy the certificate. Mutating the callback slot,
return cast or generated wrapper invalidates it. Keep callback identity and
actual linked definition checks separate from declaration-only facts.

## BV-05: visible workflow, MCP and progress

Extend the current Call contracts screen. Add unit/caller/callee/kind/identity
filters and source, original instruction and evidence links. Show both native
and compiler views of a call, the applied policy, guard/frontier and stage
lineage. Let users inspect an audit path instruction by instruction. Keep
imported findings usable independently of a selected binary; verification is
a separate state. Export the selected evidence with its input identities.

Join observed contract findings to the current policy ledger and actual build
outcome. Distinguish unresolved, verified-and-applied, guarded, stale/ineligible,
excluded and unknown; include caller/site scope and the successful or rejected
stage identity. A raw mismatch remains inspectable after a policy applies, but
must not rank as an outstanding blocker solely because its fan-out is high.
Acceptance: FF9 1D898's eleven and 548E8's seven observations are already
handled, while the actual 57 rejected caller units identify the next work.
Report raw observations and strict rejected callers as separate measures.

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

For an indirect table, retain each target's actual definition ABI and the
original guest callback identity. Distinguish a proved finite target set from
an incomplete set and a runtime unknown. A mixed table of four-word handlers
and zero-word getters must not receive a fabricated uniform C prototype.
FF9 EF094's dispatcher is the migration case: its real four input words must
reach consuming handlers while getter results remain discarded. Unknown/null
targets require the configured explicit frontier, not successful substitution.

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

When native and WASM stacks occupy different addresses, declare narrowly
bounded corresponding local objects and compare their identity relationships,
initialized contents, offsets and lifetime. Preserve alias relationships and
provider observations; a raw pointer mismatch is not automatically a behavior
failure, and a broad stack/RAM mask is not an adequate replacement. Reproduce
EF094's caller loops with two initialized local bytes, live count mutations and
448 actual histories using this explicit correspondence.

Results include cases executed, instruction/site/path coverage, failures,
refused/unsupported cases, exclusions, provider profile, versions/hashes and
reproduction instructions. Keep a first failing case and before/candidate
comparison; do not report unexecuted paths as validated. Pluggable GTE/device
oracles should reuse recording semantics instead of extending a CPU ad hoc
for every proof. A fuller shared native executor can be a later project.

Runner interfaces must expose ordered instruction, load, store, call and return
events, plus memory/register checkpoints at exact guest PCs. Current scene-word
proofs specialize `Mips.run` using `inspect.getsource` and inject an entry hook
to observe three byte loads and the subsequent cursor store. Replace that
repeated specialization with a supported event API; preserve architectural
load-delay and branch-delay order. Observers must not change execution or
silently suppress unknown operations. Keep game-specific event selection and
expected decoded arguments in the campaign configuration.

**Acceptance:** reproduce 3,344 pure-LTO comparisons,448 used-return comparisons,
349 text/sound comparisons and the preserved failing control-byte 44 text case.
Preserve exact signed-short/pointer word fidelity, RAM and call order. Unknown
instruction/provider/MMIO operations and bounded runaways yield explicit
failure/frontier results. A wrong returned word, changed RAM byte or extra
device access produces a concrete diff. Fixtures with user assets remain local;
tracked synthetic tests require no game image.

Also reproduce the 1,060 D650C decoded-word histories in
`wasm/runtime/build-file11-scene-words.py`: aliasing command/state bytes, ordered
loads before cursor storage, live callback mutations and both opcode branches.
Compare traced C and independent plain C on the same histories, report them as
two configurations rather than 2,120 distinct fixtures, and retain the 1,057
old-source missing-word refusals plus three unaffected wait paths. A trace
observer failure or missing checkpoint must be an explicit campaign failure.

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

Have the configured linker adapter emit a checked memory/layout certificate
from actual object and module symbols: guest RAM placement, alias definitions,
shared memory/table identities, data end, heap base, reserved stack and required
imports. Reuse current `wasm/mkmod.py` helpers rather than rebuilding a linker
inside binviz. Proof recipes repeatedly generate these aliases and link twice
to establish guest RAM placement; turn that work into a configured shared stage.
Refuse shifted RAM, unresolved aliases, overlapping reserved regions and wrong
stack sizes. Retain common-symbol allocation checks from BV-03 and compare
identical-input module hashes. Device/BIOS semantics remain runner configuration.

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

Expose object extent and access extent separately: native stack ranges and saved
register slots, compiler object size/alignment/lifetime, initialized byte ranges,
and every established provider copy/read/write range. Do not infer C storage
safety from a matching score or an ABI audit. Distinguish uninitialized bytes
inside a valid object from accesses outside that object. Unknown dynamic access
ranges remain explicit; this slice need not solve general C memory safety.

Concrete acceptance: EE37C's native local has 16 bytes before the next saved
register, initializes 14, and passes the local to an actual 16-byte copier.
The baseline C object's LLVM lifetime is 14 bytes; a private candidate reserves
16 without inventing tail values and retains native 100% matching. Show the
baseline overread, the candidate extent correction and the two unspecified
bytes independently. Both matching versions and a passing register policy
must not conceal this distinction. Separately show EEED4's two-byte C buffer
versus its actual four-byte 2ECA0 writer and EEFA8's corresponding two-byte
buffer versus four-byte 2F344 writer. Their native locals have four bytes before
the next saved-register region. Record the initialized bytes and ordered writes
of the real providers; a controlled hook that writes only two bytes cannot
establish the actual access extent. An opening-window proof that never invokes
these callbacks does not cover refresh. Preserve the finite downstream-use review
as incomplete, rather than declaring the unspecified tail globally unobservable.

## Where game-specific code stays

Keep archive formats, asset ownership configuration, function names, recovered
source, fixture values, format strings and expected game state in the game
project. Keep GPU/SPU/SIO/MMIO, BIOS semantics, audio synthesis and browser game
sessions in platform/game runtime packages. Binviz can inspect/import their
contracts and evidence; this spec does not make them analyzer internals.

Addresses such as GPU debug-level/byteflag locations are policy data. Device
implementations and fixtures may be reusable across PS1 games, but that is a
separate runtime/package concern. This avoids tying binviz to FF9 logic.

## Next implementation session

1. Read this spec, the implementation checkpoint and the existing
   contracts/compiler-facts/register-use docs and core modules. Establish which
   changes are built and usable before planning additional work.
2. Reproduce the current synthetic compiler-facts/evidence example with aligned
   schemas across producer, CLI, UI and MCP. Record any gaps instead of claiming
   that game collectors have already migrated.
3. Extend BV-02 records into a verified ownership inventory, including aliases,
   shared fragments, exclusions and collisions. Join actual selected providers
   to compiler findings; available objects alone do not establish selection.
4. Deliver the complete caller package described above, using the existing
   contracts view and queue. Keep raw observations, applied policies and current
   build blockers distinct and navigate to their actual source/instructions.
5. Add the BV-04 shared policy consumer with exact caller/site/provider checks;
   demonstrate that an unlisted caller and changed artifact remain refused.
6. Demonstrate the synthetic end-to-end workflow plus one optional user-local
   frozen FF9 collector comparison. Record migration evidence before removing
   the old path. Leave canonical game source and policy promotion to game work.

Follow with adapter generation, campaigns and incremental build stages.
Keep each slice usable and validated before expanding. Use ordinary API/CLI/MCP
names consistent with the repository rather than blindly adopting these IDs
as command names. Do not delete legacy scripts until equivalence/refusals are
accounted for. Deferred features remain explicit backlog items.

## Migration and delivery checklist

For each slice, the implementation session should record the affected feature
IDs, existing entry points extended, old helpers replaced, configuration still
owned by the game, and an independently repeatable demonstration. Run the old
and new paths on identical frozen inputs; compare decisions, witnesses and
refusals, including intentionally stale/unsupported cases. Differences need an
explanation, not a relaxed check to obtain agreement.

The first end-to-end demonstration needs no FF9 image: a synthetic two-caller
fixture with an overlay collision, an actual compiler-definition mismatch, a
scoped register policy and one changed artifact. Show the same IDs/results in
CLI, UI and MCP; show the changed artifact invalidating only its dependents.
Keep optional local FF9 campaigns separate from the distributable tests.

Measure the work being replaced: analysis wall time, original bytes loaded,
compiler invocations, cache reuse, peak memory, and the number of per-game
analysis helpers needed. Compare cold and warm runs on identical inputs. Faster
native scoring alone does not establish a faster behavioral decompilation.

A slice is ready to migrate when users can inspect its inputs and reasons,
agents can consume the same structured results, failures remain explicit, and
the game can remove duplicated analysis while retaining its configuration and
fixtures. Library code without UI/MCP access leaves the visibility requirement
unfinished. Retire each old helper only after this demonstration and equivalence
review; preserve historical evidence and reproduction recipes.

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

Use a consistent gap record: date, feature ID, observed task/example, existing
entry point, missing shared capability, temporary adapter, game-specific inputs,
priority, acceptance/refusal cases and migration status. Link the actual helper
or evidence package. Report a gap when discovered, then consolidate it at the
batch checkpoint; the user should not have to request a tooling review again.
Distinguish proposed, implemented, locally demonstrated and migrated. Record
measured savings after equivalence, rather than promising a decompilation speedup.

The deliverable is shared, observable capability. Merely renaming Python files
or translating the same per-game scripts into Rust does not satisfy this spec.

## Implementation checkpoint: 2026-10-03

The first implementation slice is available; see [compiler facts and evidence
lineage](compiler-facts.md) for configuration, schemas, commands and limits.

| ID | Implemented in this slice | Remaining acceptance work |
| --- | --- | --- |
| BV-01 | Maintained `tools/compiler_facts.py` Clang C producer, versioned shared reader, all-direct-call audit, multiple findings, raw/desugared types, body/prototype distinction, exact prepared spans, grouped declarator identities, separate address references and extraction gaps. | Broader structured type lowering, C++ support, canonical/macro source correspondence, provider ownership joins and game collector migration. |
| BV-02 | Shared physical unit/function and separate analysis/matching span records; exact hex addresses and member-boundary consistency tests. | Verified archive manifests, complete ownership inventory, alias/shared-fragment/exclusion and collision reports, authoritative native/compiler joins. |
| BV-03 | SHA-256 artifacts, config/compiler/adapter/recipe identities, prepared/facts stages, raw versus normalized hashes, record-to-artifact binding, missing/stale reasons and DAG invalidation in library/CLI/WASM/MCP. | Object/LTO/link and data-allocation lineage, before/candidate promotion plans, runtime proof records, include-shadow/environment checks and FF9 drift regressions. |
| BV-05 | Binary-independent contract screen, unit/kind/identity filters, local byte verification, prepared source excerpts, evidence export, native register witness paths; persistent `compiler_facts`, `call_contracts`, `verify_evidence`, `register_use` and `register_use_batch` MCP tools. Producer emits live JSONL stage/count/gap/cancellation events. | Full native/compiler correspondence, paired audits/policy display, batch lifecycle events for every build/proof stage, cancellation of native jobs and queue blocker/work-package integration. |

At this original slice checkpoint, BV-04 paired scoped policies, BV-06 edit
generation, BV-07 campaigns, BV-08 incremental builds and BV-09 stack/closure
evidence remained backlog work. See the working-tree reconnaissance below for
subsequent implementation leads. This slice does not approve native allowances
or modify any FF9 canonical source.
The current MIPS core and strict per-request policies are reused; unresolved
results and discarded endpoints retain their existing meaning.

Tracked synthetic fixtures live in `tests/fixtures/contracts/`; adapter
regressions live in `tests/tools/test_compiler_facts.py`. The example joins eleven
calls and reports six independent findings from one caller. Core tests cover
modified records, ambiguous providers, unsupported profiles, grouped declarations,
dependency invalidation, corrupt outputs, cycles and physical member boundaries.
MCP session tests cover import/query retention without a binary and single/batch
register reports on one loaded synthetic overlay. Game-image acceptance campaigns
listed above are still optional local migration work, not claimed completed.

### Subsequent working-tree reconnaissance (2026-10-03)

The checkout now contains additional, in-progress implementation. This is a
source inspection checkpoint, not a test result or a migration claim. The next
implementation session must examine these paths before building another version.
They may be owned by a concurrent session; coordinate edits accordingly.

| Requirements | Existing implementation lead | What still needs to be demonstrated |
| --- | --- | --- |
| BV-02 ownership | `crates/binviz/src/inventory.rs`; MCP `unit_inventory` | Current-byte verification, competing physical identities and exact extents surfaced in the complete caller workflow. |
| BV-04/05 caller packages and scoped decisions | `crates/binviz/src/workspace.rs`; MCP `workspace_import`, `caller_package`, `contract_blockers`, `paired_register_audits`, `explain_policy` | End-to-end eligibility/refusal parity, actual provider correspondence and equivalence with a frozen game collector. |
| BV-06 edit plans | `crates/binviz/src/adapters.rs`; MCP `plan_adapters` | Linked-output validation, preserved evaluation and game-adapter migration. |
| BV-07 campaign comparisons | `crates/binviz/src/campaign.rs`; MCP `proof_campaign` | Supported runner integration and architectural event timing. Inspection of supplied observations does not itself execute a campaign. |
| BV-03/08 link/build evidence | `crates/binviz/src/linkevidence.rs`; `Workspace::merge_build_batch` | Validated promotion plans, precise cache dependency boundaries and recovery from interruption/concurrent drift. |

Do not label a feature migrated because an API or file exists. Update this
checkpoint with an aligned producer/consumer revision, demonstration recipe,
positive and negative results, and the actual legacy helper retired.

### Object callback work-package gap (2026-10-03)

BV-04/BV-06, priority P0: the local evidence in
`ff9-decomp/docs/file11-object-callback.md` and
`build/agent-vm/file11-object-callback/production-inventory.json` joins the
BBF30/4A8A4 callers, 21698 provider, installed AB810 callback, C declarations
and generated zero-return resolver wrappers by a game-specific recipe. Existing
compiler facts and native register audits supply parts of that evidence; a shared
used-return certificate and linked-wrapper validation remain missing. Temporary
adapter: the existing MIPS/WASM runners and typed AST/direct-call preparation,
without a new type or CFG walker. Game-owned inputs: exact asset extents,
callback installation and resource fixtures. Acceptance is the full-word versus
byte-store example in BV-04, plus drift/unknown-target refusals and CLI/UI/MCP
visibility. The private package reports 347 comparisons, explicit unknown/voice
frontiers and six 100% matching rows; it does not establish general callback
closure or migration to shared tooling.

### Resident geometry work-package gaps (2026-10-03)

BV-04/BV-09, priority P1: FF9's private
`build/agent-resident-geometry/{register-requests.json,register-audits.json,status-review.json}`
reuses six shared register-use audits rather than adding a CFG walker. The
BACB4 -> C0ACC -> BEDBC chain exposes two missing composition capabilities:

- BEDBC's incoming A2 is overwritten before its normal downstream calls, while
  early status returns preserve it without reading it. The existing default
  audit retains surviving-return frontiers; explicit discarded-return analysis
  is dead with no reads/frontiers. Neither result supports a universally killed
  or preserved callee summary. Add a reviewed no-consumption/may-kill contract
  that conservatively retains the word across normal continuation and preserves
  exact provider extent/hash and separate killed/discarded endpoint lineage.
- C0ACC's native S0 loop initializes at zero, increments once per iteration and
  compares unsigned against32. The shared first-read audit reports a live cycle
  instead of inferring this bound. Add explicit bounded-loop evidence or a
  native/compiler correspondence proof that makes termination assumptions
  inspectable and keeps unproved cycles unresolved.

Temporary game adapter exports only the independently proven B0060 -> C0C7C
A1 overwrite allowance. BACB4's extra-word allowance remains review-required;
198 finite actual native/C fixtures and200 mutation checks are separate evidence,
not a substitute for either missing generic proof. Acceptance: synthetic mixed
kill/preserve and fixed-bound-loop fixtures, unchanged unknown/callback/return
frontiers, refusal of fabricated kill summaries, CLI/UI/MCP parity and exact
identity invalidation. No generic tooling implementation was changed here.

### Storage correction and evidence promotion gap (2026-10-03)

BV-03/BV-08/BV-09, priority P1: the EEED4/EEFA8 list-buffer correction changes
two-byte C objects to four bytes for the actual SDK writers. The source can
retain 100% native matching while the old object extent remains unsafe; retain
that distinction in the object/access report. The opening-window evidence also
pins the accepted EEED4 C, preprocessed C and object identities. Changing the
source therefore requires an explicit transition through those stages before
the new policy can be consumed, while preserving the historical two-byte proof
input and its limited coverage.

Temporary adapter: `ff9-decomp/wasm/runtime/build-file11-list-buffer-proof.py`
and the local `build/agent-vm/file11-list-buffer-transition/` package reproduce
baseline/candidate artifacts using the actual compiler flags, namespace and
transformation stages. They do not add a new analyzer. Shared stage records
exist, but a reproducible before/candidate promotion plan remains missing.

Add a promotion plan that identifies current inputs, candidate outputs, affected
consumers and exact replacements. Validate the baseline against accepted
artifacts, then independently reproduce candidate outputs before publication.
Keep candidate evidence in a separate stage namespace until its prerequisites
are verified; never bypass an identity gate or silently seed artifacts to make
a policy pass. Present source, object-extent and proof-scope changes together.

Acceptance: a synthetic two-to-four-byte storage correction propagates through
CPP/object/policy dependencies; an unrelated source remains cached. Changed
flags, namespace, transformations or baseline outputs refuse the plan. Preserve
the old artifact and distinguish a proof that registers a callback from a proof
that invokes its actual writer. Promotion interruption or concurrent input drift
must leave an explicit recoverable outcome. Game-owned inputs remain the SDK
closure, fixture values and selected compiler/build configuration. Status:
observed and locally reproduced, not migrated to shared tooling.

### Explicit extent versus inferred extent conflict (2026-10-03)

BV-02, priority P0: the C94B8/C9600 review found Binviz's inferred BOOT4BF20
extent is 144 bytes and includes the next unnamed routine, while the game's
tracked symbol/merge rows explicitly bound the routine at 84 bytes. Native
register analysis must consume the verified 84-byte extent; a guessed next
symbol must not silently supersede it. Existing register-use accepts an explicit
span, so the temporary adapter supplies that span and pins its manifest rows
and original bytes. No new extent finder is needed.

Expose both extent candidates, their provenance and the selection/conflict in
the function/caller inspector. Acceptance: a synthetic routine followed by an
unnamed adjacent routine retains the explicit member extent; missing or changed
manifest rows invalidate dependent audits. Unverified inferred spans remain
usable for exploration with their status visible, and cannot establish a
verified policy. Status: observed, shared explicit-span audit reused, ownership
join and inspector migration pending.

### Discovery-stamp transition and raw-report reuse (2026-10-03)

BV-03/BV-08, priority P0 for the promotion interface: adding reviewed file-12
preparation to the common resident builder changed its discovery recipe key.
Eleven file-11 discovery stamp hashes changed across the GPU, queue and formation
policy inputs. Their canonical/prepared source, preprocessed C, raw object and
dependency outputs were unchanged. The old policy correctly refused the new
stamp; the game session needed a one-off transition collector and three reproofs.

Evidence: `ff9-decomp/build/root-resident-integration/discovery-stage-transition.json`
records exact old/new stamps, recipe identities, policy hashes and unchanged
semantic artifacts. Its `before/` directory preserves the verified baseline
stamp bytes. The temporary adapter is
`build/root-resident-integration/reseal-discovery-stages.py`; existing compiler
and proof runners remain the authorities. This adapter is a local example, not
a reusable dependency or a generic approval mechanism.

Required shared behavior:

- Show separately which recipe/metadata identity changed and which semantic
  inputs/outputs changed. An algorithm revision remains a real dependency.
- Produce a reviewable baseline/candidate promotion plan with exact affected
  consumers. Verify baseline bytes and reproduce candidate stages before applying
  replacements. Equal outputs alone do not authorize a new policy.
- Keep old evidence accessible; publish replacements atomically and preserve
  exact caller/site scope. A failed reproof must leave the policy unpromoted.
- Cache raw caller observations separately from policy decisions. Report reuse
  must validate preprocessed C, selected actual definition ABIs, extraction
  dependencies and producer versions. A policy-only change can rerun eligibility
  without rediscovering identical raw calls; a changed provider invalidates the
  affected joins even when a caller's source stays the same.

Acceptance: a synthetic common discovery-recipe revision changes stamp identity
while reproducing identical CPP/object outputs. Show the transition and exact
reproof dependencies without silently relaxing old gates. A one-byte source,
provider, header, object or recipe change must prevent reuse where relevant.
Preserve an untouched unit's validated raw report; interrupted/concurrent
promotion must report recoverable state. Status: locally reproduced transition;
shared promotion and game collector migration remain to be demonstrated.

### Provider-entry events after delay slots (2026-10-03)

BV-07, priority P1: the BBD3C unit-initialization review needed to observe the
actual unit pointer delivered to C09EC. The native call sets a meaningful
argument in its branch delay slot. The local runner's `target_kind` observation
sampled registers before executing that slot and therefore reported an incorrect
provider-entry value. This was an observation-timing bug, not evidence of a
native/C behavior difference.

Evidence: `ff9-decomp/build/file11-unit-init-tracked-review/` and
`ff9-decomp/docs/file11-unit-init.md`. Temporary adapter: the existing MIPS
executor's provider-entry observer, corrected to sample after the delay slot;
no new instruction decoder or CFG walker. Game-owned inputs are the unit pointer,
actual provider closure and initialization fixtures.

The runner event contract must distinguish call instruction, delay-slot
execution and provider entry. Define register/memory state at each event,
including pending load-delay state, and identify the originating call PC and
target. A campaign must request the relevant phase explicitly. The shared
comparison records should reject unsupported/missing phase information instead
of treating pre-slot arguments as provider-entry arguments. Existing trace
records with a `delay_slot` marker need this runner guarantee; a marker alone
does not prove that the sample was taken at the correct point.

Acceptance: tracked synthetic direct and indirect calls set distinct A0 values
in their delay slots. The pre-slot event retains the old value and provider entry
observes the new value exactly once. Ordered stores and architectural load-delay
behavior remain unchanged with observation enabled. A deliberately early sample
fails the comparison with a concrete timing/argument diff. Unknown targets or
missing entry checkpoints remain explicit frontiers. Status: local observer
corrected; supported shared event API and campaign migration pending.
