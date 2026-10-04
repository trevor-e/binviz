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

For the next implementation session, prioritize **migration demonstrations**
over duplicating the implementations now listed at the end of this document.
The highest-value demonstrations are: replace one owned-caller collector with
the shared caller package; reuse compiler facts after a policy-only change;
recompute only the changed caller after a source edit; and replace one custom
proof harness with a configured campaign. Record the helpers actually retired,
preserved refusals and measured cold/warm cost for each demonstration.

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

Measured follow-up: after the BBD3C source correction, exactly one of 1,060 raw
caller CPP inputs changed. Regenerating the legacy cached all-call report took
133.7 seconds and produced 129 findings across 72 callees; AEE68 remained an
explicit raw-cache gap. Before this correction it held 130 findings/73 callees.
This is a concrete BV-08 baseline for incremental report acceptance, not a
measured speedup. A subsequent policy-only integration should reuse these raw
facts after validating their input/definition/producer identities.

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

### Selected HLE providers, scheduling and DATA bindings (2026-10-03)

BV-02/BV-03/BV-04, priority P0 for the provider inspector: the maintained
compiler-facts adapter and shared workspace now expose FF9's four apparently
missing providers as intentionally excluded native definitions with differently
named HLE endpoints. Local evidence is
`ff9-decomp/build/agent-resident-providers-private/{review.md,workspace-report.json}`.
The imported workspace retains unbound calls and separate hoisted-prologue
fragments. These providers are MoveImage, DrawOTag, ClearOTagR and CD busy.

Show the native definition, selected linked HLE endpoint, alias/universal wrapper,
actual direct-call ABI and platform-service profile together. In the CD-busy
example, 1,024 finite original/C comparisons pass for the native busy-byte leaf
and one caller's pending/skip paths. Yet the existing host endpoint also advances
completion scheduling. Replacing it with the native leaf can leave two waiting
callers stuck. A passing leaf campaign must not silently approve that replacement.
Acceptance must include ordinary pending-to-complete waits and ordered scheduler
effects, with device/retail timing scope explicit. A separate hidden printf
contract remains a blocker even after the busy provider is supplied.

The menu caller campaign in
`ff9-decomp/build/agent-vm/file11-menu-next/proposed/docs/file11-menu-next.md`
supplies a concrete DATA-binding refusal case: an omitted `g_state` guest-RAM
alias linked without an error but produced pointer 291 instead of native
`0x80140123`. The corrected proof explicitly binds the source symbol at guest
`0x8006794c`. Extend the existing layout/ownership evidence to expose unmapped
extern DATA/common allocations, actual module addresses and required guest
bindings before declaring a closure usable. A synthetic omitted alias must
remain unresolved or fail layout validation despite a successful link. Preserve
unknown mappings; do not guess them from a symbol name.

Status: shared facts/workspace inspection used locally; provider adaptation,
scheduler equivalence and DATA-binding certification still require migration.

Required follow-on slices for the existing implementation:

- **Selected-provider chain:** extend `workspace::CallBinding` and
  `Workspace::analyze`, plus `linkevidence::inspect`, to expose physical native
  ownership, exclusion/substitution, the selected wrapper, the actual import
  ABI and the service obligations in one structured chain. Import identity
  includes module, field and signature. The same `hle.ff9_cd_busy` field can
  legitimately have zero-input direct and six-input legacy imports, with a
  separate twenty-word dispatcher. Name-only binding must refuse ambiguity.
  Preserve every independent blocker, including printf, after resolving CD.
  Surface this through the existing caller package and policy/blocker APIs.
- **Comparison roles and authority:** extend `campaign::compare`,
  `campaign::audit_report` and the configured runner adapter. Their current
  `native`/`wasm` side names cannot describe a baseline/candidate comparison of
  two WASM configurations accurately. Record role, execution architecture,
  genuine/controlled boundaries and the obligations established. A same-host
  scheduler comparison must not become native device-timing evidence. Retain
  pending-to-idle waits, ordered callbacks/copies, partial-read cancellation,
  full-word results, raw-leaf nonprogress and premature-zero negative cases.
- **Shared-memory initialization:** extend `linkevidence::ModuleReport`,
  `LayoutCertificate::verify` and campaign initialization records beyond DATA
  address checks. A private module's BSS initialization overwrote live BOOT
  dispatcher RAM despite correct aliases. Show active initialization writes,
  ordering, imported-memory maxima and cross-module stack reservations.
  Reject destructive initialization, incompatible maxima and overlapping
  callback stacks. Fixture save/restore must declare its exact phase and range;
  passing such a fixture does not establish production initialization safety.

The frozen local CD package at
`ff9-decomp/build/agent-resident-providers-private/cd-boundary/review.md`
records 143 runtime checks, 42 baseline/direct scheduler pairs and 72 scope/drift
refusals. It is a concrete migration target, not a public game integration.

### Scoped matching-record publication (2026-10-03)

BV-03/BV-08, priority P1: `ff9-decomp/tools/score.py --record sub_800bbd3c`
scores the requested source but republishes cached note rows from other sources.
Two unrelated nonmatching percentages and timestamps changed in this batch;
root restored those incidental note changes and preserved them for audit.
The corrected BBD3C remained a 100% exact match. Aggregate cached score rows
were not used as a verified unique-function coverage percentage.

Expose a publication plan containing the requested scope, actually validated
source/extent/producer identities and exact proposed note changes. Cached rows
outside that scope must require their own current validation/publication plan.
Acceptance: recording one selected routine updates only its verified record;
unrelated stale scores retain their prior notes and show their stale status.
Distinguish per-source rows, unique physical function starts and matched byte
coverage. This extends existing matching/evidence tooling; it needs no new scorer.

### Shared tooling continuation checkpoint (2026-10-03)

BV-01/02/03/04/05: shared workspace/inventory now joins physical functions,
compiler definitions, actual selected WASM object bodies/signatures and linked
call targets to complete caller packages. Exact call/slot correspondence artifacts
and policy review artifacts are content-bound; scopes remain per caller/site.
Paired audits preserve killed versus discarded endpoints. Applied findings need
current edit/compile/link lineage. Inventory retains separate matching/analysis
extents, collisions, fragment ownership and unknown/data gaps. Queue context joins
require actual loaded bytes and mapping; existing claims/completion filters remain.
CLI `workspace`, browser Contracts and MCP expose the same IDs and reasons.

BV-03/06: deterministic scalar extra-word bridge plans retain exact source spans,
before/after bytes, argument evaluation and callback references. Unsupported result
conversion and closure cases refuse. Separate candidate publication is atomic and
idempotent. Before/candidate promotion plans require current baseline/candidate
producer stages, pinned recipes, prerequisites and the exact affected consumer
closure. Historical inputs survive; publication/policy promotion is not automatic.

BV-07/08: configured persistent runner pairs and shared full-word/RAM/ordered-event
comparison are implemented, with narrow local-object correspondence, initialization,
alias and checkpoint checks. The synthetic Windows-x64/WASM32 campaign runs actual
compiled code; it is not a PS1 runtime proof. Build stages share frozen inputs and
content keys, atomic/deduplicated cache publication, include-name/content snapshots,
bounded scheduling and sampled direct-process RSS. Partial/cancelled/drifting jobs
retain refusals. Compiler-facts producer-specific typed extraction caching is also
implemented: cold/warm tests measure two/zero extraction subprocesses, with
leaf-only invalidation, output-corruption recovery, header-shadow and environment
refusals. Target discovery and preprocessing remain fresh on every run.

BV-04/09: optional observable-bit audit extends the existing MIPS state/CFG for
copies, masks, shifts, byte stores, delayed loads and locally constant bounded
counters. Default policy consumption remains conservative. Reviewed storage records
show object size/reservation/initialized bytes versus exact-site provider access;
actual native SP observations and WASM stack-pointer operations are visible.
Reviewed description identity does not prove access completeness. Complete callback
used-return closure, mixed may-write callee certificates, external loop bounds,
IRQ/recursive stack upper bounds and automatic compiler allocation/lifetime
extraction remain open explicit frontiers.

Reproduction and acceptance tests are documented in
[workspaces](decompilation-workspaces.md), [build batches](build-batches.md),
[proof campaigns](proof-campaigns.md) and [progress images](progress-images.md).
No legacy FF9 helper is retired, and no game-source change or matching credit is
claimed from these synthetic demonstrations. Compare frozen game collectors before
migration; retain the concrete gap records above as acceptance targets.

### Exception order and portable C transformations (2026-10-03)

BV-06/BV-07, priority P1: the item/menu follow-on campaign found a real
native-versus-WASM exception-order difference. A zero divisor reaches a native
MIPS BREAK before the random cursor advances. The compiled C remainder traps
after LLVM has moved the cursor store; byte `0x8007b720` changes from `0x30` to
`0x31` before the WASM trap. The runner must compare the state at the exception,
not merely classify both executions as failed.

The temporary game-owned candidate adds an explicit WASM zero-divisor trap
before the remainder. Its reported 315 histories include 275 complete paths,
39 strict frontiers and one exception path. This is preliminary private worker
evidence; root review, frozen promotion identities and shared migration remain
pending. Locate the campaign under `ff9-decomp/build/agent-vm/`; do not treat this
record as source-promotion approval or a universal modulo rewrite.

Extend campaign results with an exception outcome: architecture/runner kind,
guest PC or mapped source operation, ordered effects before the exception and
the final observable memory/register checkpoint. An unknown instruction, missing
provider or execution budget exhaustion is a separate frontier. Edit plans must
identify native undefined-C behavior, the proposed target-specific correction,
its exact stage and the evidence needed before applying it.

Acceptance: tracked synthetic inputs make the baseline native and WASM runs
both trap but disagree on a visible store. The campaign reports that store as
a failure. A reviewed candidate preserves the pre-exception state and ordinary
nonzero paths; missing exception checkpoints remain unresolved. Preserve the
failing baseline, compiler flags and candidate lineage. Current configured
campaigns are an implementation starting point, not evidence that this native
instruction/exception case is already supported.

### Variadic and shared-tail caller explanations (2026-10-03)

BV-01/BV-02/BV-04/BV-06, priority P1: the private formatter campaign contains
109 C call sites but only 78 distinct native JAL sites, because several recovered
callers share native tails. The original formatter's argument-home prefix is
also required: its complete span is 2,180 bytes, including a 12-byte prefix
omitted by an interior entry label. Caller counts, physical call coverage and
matching/analysis extents must remain separate in the inspector.

One recovered caller omitted two meaningful inputs to a variadic call. Native
instructions preserve the incoming index and produce a second word before the
call; correcting the C arguments is a source candidate, not permission to pad
every variadic call. Another printf endpoint is only a native BIOS veneer with
an absent ROM body. Its software-console contract is explicit platform evidence,
not an original-ROM behavioral proof. Local evidence lives in
`ff9-decomp/build/file12-format-proof/` and `build/file12-format-package/`.

Show actual fixed/promoted argument types, variadic ABI profile, argument-home
requirements, shared physical call identities, provider selection and result
use. Known scalar varargs profiles may produce reviewed bridge plans; unsupported
format/type combinations and absent provider semantics remain explicit.

Acceptance: a synthetic pair of C callers shares one native tail without
double-counting physical coverage. An interior formatter entry refuses a proof
requiring its missing prefix. A missing meaningful variadic argument remains a
source defect; an explicit reviewed candidate preserves supplied values and
evaluation. A veneer-only provider displays its software-service scope and
does not receive ROM coverage credit. The private FF9 package has not replaced
any shared Binviz collector or been integrated into the public game builder.

### Matching-preserving readability batches (2026-10-03)

BV-03/08, priority P2: compose existing build-batch, matching and promotion
interfaces into a configured readability-candidate acceptance workflow. The FF9
pilot and follow-up cleaned 6 and 26 canonical C files respectively, preserving
complete before/after native object bytes and original-code matching results.
The temporary turn-local adapters and reports are under
`binviz/target/ff9-cleanup-pilot/` and `binviz/target/ff9-cleanup-batch2/` (ignored).
They reuse FF9's compiler farm and `binviz match --json --strict-relocs`; the
reviewed rename choices remain game-owned. No general C refactoring engine or
second scorer was added.

Acceptance: freeze source/header/tool/recipe inputs, compile baseline and candidate
with identical compiler-visible input paths, compare complete object bytes,
retain original-code scores, and refuse source/header drift before publication.
The current FF9 runner embeds worker PIDs in compiler input filenames: otherwise
identical objects can differ only in `.strtab` when workers change. The temporary
batch uses one persistent worker; the reusable interface should support stable
input paths without relaxing byte equality. Candidate preparation errors remain
failures, and source symbol identities, types, layouts and expression order are
preserved. Source edits invalidate downstream source-bound evidence; object
equality does not establish a new linked-image, WASM or gameplay proof.

Function-name acceptance must also retain a unit-scoped mapping between descriptive
C identifiers and native symbol identities. FF9's first naming batch maps 27
options-menu functions through an alias header and changes 41 affected sources;
all objects and original-code scores remain unchanged, including one pre-existing
partial caller. WASM preprocessing retains identical tokens. Temporary evidence
is in `binviz/target/ff9-function-names/`; the tracked mapping is
`ff9-decomp/docs/function-names.json`. Validate canonical identities after
preprocessing rather than rejecting a descriptive definition in raw source.
Scope overlay aliases by unit, retain source paths, verify all affected callers,
and preserve partial baseline scores without granting new matching credit.

Function-name scaling acceptance (BV-03/BV-08, P2): FF9 now has 711 reviewed
behavioral aliases in docs/function-names-catalog.json, including all 52 recovered
options-menu C definitions. Reports under docs/function-names-*.json bind each
historical before/after batch; current catalog hashes bind named definitions.
Later caller naming can change sources already present in earlier reports, so
historical batch hashes must not be presented as current source validation.
Temporary adapters reuse the existing FF9 compiler farm and strict-relocation
scorer: binviz/target/ff9-names-{wide,extra,options-complete,library,gameplay}/.
Every affected native object and original-code score stayed unchanged; WASM
preprocessed tokens also stayed identical. No new linked/gameplay proof is claimed.

Preserve macro namespaces when renaming C references. sub_800614ac uses canonical
#define/#undef names to hide an incompatible header declaration. A naive rename
removed the semantic alias instead and changed one native object; the complete
object comparison rejected it before installation. Acceptance covers this case,
unit-scoped overlapping overlay addresses, definitions/declarations/all callers,
and existing partial scores. Inspect canonical identities after preprocessing.
Audio-name evidence additionally follows the original executable's hashed command
table through dispatch normalization to handler effects; do not infer behavior
from opcode numbers alone. A catalog/report importer is bookkeeping until it
independently validates the source/header/tool/recipe inputs and observations.

Overlay naming acceptance (BV-03/BV-08, P2): FF9 adds 86 battle-results names
and 169 save/load and memory-card names, reaching 966 reviewed aliases. Reports:
ff9-decomp/docs/function-names-{results,save}.json. Thin adapters reuse the native
compiler farm, strict-relocation scorer and token checks under
binviz/target/ff9-names-{results,save}/. All 255 complete native objects are
byte-identical; original scores (including 15 existing partials) and WASM
preprocessed tokens remain unchanged. No linked/gameplay proof is claimed.

Acceptance for a reusable naming campaign: identity keys must include the unit
because overlay addresses overlap; apply each overlay header only to its own
sources. Refresh the current catalog from historical batch reports without
relabeling old snapshot hashes as current evidence. Behavioral review must follow
actual callees when source comments disagree: save read 801f8478 reaches BIOS
B0:34 through fa5a4/fa5d0/fa644, while write 801f7e68 reaches B0:35 through
fa4b8/fa4e4/fa53c. Open fa3d8 reaches B0:32 rather than formatting; format fa754
reaches the recovered 66310 directory/broken-sector/MC-header writer. Bind these
reviewed callee sources as evidence. Keep uncertain fields and text IDs descriptive
without invented domain meanings. Existing campaign importers remain bookkeeping
until their source/header/compiler/recipe checks independently validate observations.

Menu and shared-card naming acceptance (BV-03/BV-08, P2): FF9 adds 332
reviewed names across shop (88), items (41), abilities/equipment (66), card
collection (41), name entry/party selection (40), shared card/file helpers (35),
and status (21). Catalog total: 1298. All 387 affected native source comparisons
are byte-identical, original strict-relocation scores retain 318 exact and 69
existing partial observations, and WASM preprocessed tokens are identical.
Reports: ff9-decomp/docs/function-names-{shop,items,equipment,cards,selection,
card-runtime,status}.json. Thin scratch adapters reuse the established farm,
scorer and token checks under binviz/target/ff9-names-<purpose>/.

Behavioral review acceptance: do not promote historical comments over the code.
Item-menu 0f50 swaps inventory id/count records rather than party members; ability
menu efe2c toggles an enabled command and its budget rather than permanent learning
progress. Card result counter names are supported by the win/loss/draw branches
in ovl_0ba800/800afbe4. BIOS file first-entry wrapper 65ec0 ends in B0:42 despite
its older open annotation. A matching native object verifies identifier-only
change, not the correctness of a proposed behavioral name.

Keep alternate relocated party-selection addresses unresolved as identities until
ownership is reviewed: ovl_126800 definitions around 801fa7a8..801fb6fc coexist
with recovered callees around 801f37a8..801f4704. This batch names exact existing
definition symbols without rewriting alternate references. Any reusable campaign
must represent that relationship explicitly rather than equating addresses by
proximity or prose. New caller naming invalidates earlier live source hashes;
current catalog hashes and historical batch observations retain separate roles.
The all-functions naming campaign remains active; these batches do not establish
that every remaining function has been reviewed. No linked/gameplay proof claimed.

Tetra Master naming acceptance (BV-03/BV-08, P2): FF9 adds 127 reviewed
module-A match, battle, AI, deck, reward and rendering names; the catalog now
contains 1425 names. Evidence: ff9-decomp/docs/function-names-tetra-core.json
and binviz/target/ff9-names-tetra-core/. All 127 complete native objects are
identical, all strict-relocation scores unchanged (105 exact, 22 existing
partials), and all installed WASM preprocessed token streams identical.
No new original matching coverage, linked-image or gameplay proof is claimed.

Semantic review checks bodies and established callees rather than copying old
comments: abc74 returns the hand to the collection, ab338 removes a selected
collection occurrence, a986c uploads sprite data, and af100 returns retained
player-owned cards regardless of whether they have been placed. Empty a932c
and return-second-argument stubs abebc/b3480 remain canonical.

Ownership fixture: include/ovl-lane-02.h documents three overlays concatenated
in ovl_0ba800, each actually loading at 800a7000. Module-B/C definition addresses
use blob offsets, while their calls retain actual load addresses. This batch
maps only module A; do not infer a B/C semantic callee from a coincident A
address or rename alternate identities without a reviewed module binding.
Existing shared readability acceptance and frozen compiler namespaces were
inspected; this batch retains the historical native scorer/farm/token adapter.
No shared-workspace acceptance or scorer migration is claimed by these reports.
Use this real identity/comment fixture for configured shared acceptance; a
report import alone cannot prove its source, header, compiler or recipe inputs.

Title/bonus and hardware naming acceptance (BV-03/BV-08, P2): FF9 adds 179
reviewed names (126 title/movie/Blackjack/story definitions, 53 resident GPU/SPU/
TIM/BIOS helpers). Current catalog:1604 names. Evidence: docs/function-names-
{title-bonus,hardware-images}.json and binviz/target/ff9-names-{title-bonus,
hardware-images}/. Across235 before/after source comparisons, every complete
native object stays identical; all original strict scores stay unchanged
(193 exact,42 existing partials), as do all installed WASM preprocessed tokens.
No new linked/module/gameplay or original matching coverage is claimed.

The concatenated-overlay fixture now includes reviewed B/C primary definitions.
Their actual-load-address sub_/func_/cb_ references remain canonical: a blob
address and a load address are distinct linkage symbols, not interchangeable
spellings. No address/dispatch repair is included. Module-A empty a932c and
identity state stubs abebc/b3480 stay canonical; title transition callbacks
b5344/b5378/b53ac await a more precise scene-role review. Scope remains unit-bound.

Semantic acceptance follows bodies and established callees: title four-handle
operations are BIOS/card events, not audio channels; Blackjack baa28 splits a
hand and bab34 renders hand cards. TIM1d960/1d9cc read/write CLUT and image
rectangle origins rather than dimensions. SPU transfer branch directions must
be checked against PIO fallback, CHCR values and existing library identities,
not inherited prose. Keep signed GPU field extraction and unusual allocation
conflict predicates intact. Existing shared readability/frozen namespace tools
are available; these legacy native-scorer/farm checks do not constitute shared
workspace acceptance or authorize a historical scorer migration.

Controller/CD naming acceptance (BV-03/BV-08, P2): FF9 adds116 reviewed
names:68 controller/card C definitions plus8 BIOS assembly aliases, and38 CD
C definitions plus2 BIOS assembly aliases. Current catalog:1720 names. Evidence:
docs/function-names-{pad-driver,cd-driver}.json; snapshots and compile observations
are in binviz/target/ff9-names-{pad-driver,cd-driver}/. Every complete native
object stays identical across153 source comparisons; strict original scores
stay unchanged (105 exact,48 pre-existing partials), as do all installed WASM
preprocessed tokens. No new linked/module/gameplay or matching credit is claimed.

Semantic fixtures follow recovered bodies, not inherited prose: controller
functions19798/197d8/19948/19aa8/19b78 configure actuators, alignment, main mode,
state and mode-info despite old libcd labels. CD236a4 exchanges the debug level
used by printf thresholds, despite the old CdDataCallback guess. CD213e0/21500
read sectors with blocking/asynchronous DMA completion; naming does not adopt
the old assertion of opposite transfer directions. Existing zero-argument
forwarding and other ABI discrepancies are retained for separate integration
work. BIOS assembly aliases retain literal table/number pairs and raw labels.

Existing descriptive CdSearchFile/CD_* definitions and the one-argument CdRead
retry coordinator are kept intact and excluded from this new-name count.
Unknown command0x4b, kernel patchers, opaque state setters and unclear enqueue
entry roles remain deferred. Shared readability acceptance and frozen namespace
features are already available; these maintained legacy farm/scorer adapters
provide concrete compatibility fixtures, not shared workspace acceptance or a
historical scorer migration. Preserve canonical unit/linkage identity and
source-bound report hashes when future batches or shared adoption touch them.

Shared UI/fade naming acceptance (BV-03/BV-08, P2): FF9 adds126 reviewed
C definitions (72 options/UI/text/key-item helpers and54 fade/transition
handlers). Current catalog:1846 names. Evidence: docs/function-names-
{ui-support,fade-transitions}.json and binviz/target/ff9-names-
{ui-complete,fade-transitions}/. Across395 source comparisons, every complete
native object stays identical and original strict scores stay unchanged
(328 exact,67 existing partials). All installed WASM preprocessed tokens agree.
No new linked/module/gameplay or original matching coverage is claimed.

Semantic fixtures bind names to actual bodies and consumers: key-item ownership
at work+774 is confirmed by the item-menu collector; high bits stay secondary
flags with no guessed story role. Play-time formatting retains the601-hour
threshold,599-hour clamp, hundreds glyph and separator blink. Ordered OT reads,
signed/unsigned rectangle access, extended glyph width and unaligned copies
remain intact. UI2c4b0 overrides/restores pad input signatures/control state,
not SPU control suggested by old prose. Repeat getters retain20-byte slots.

Fade colors/modes retain exact helper arguments, dual-phase/eased hook tables,
read-and-clear easing state and callback order. Unknown additive/subtractive
interpretations are not baked into names; mode0/mode1 denote the existing mode
byte. Screen32798 loads changed party portraits (group1 entry30), followed by
party_upload_portraits at32a44, rather than intro code asserted by old prose.
Read-done D_ spellings, zero-argument forwarding, true function heads and all
matching hacks stay stable. Empty stubs, unknown transition/widget bytes and
unreviewed rotation assembly/wrappers remain deferred.

Existing shared readability acceptance, configured frozen namespaces and compiler
facts are available. These maintained legacy farm/scorer observations are game
compatibility fixtures, not shared workspace acceptance or scorer migration.
Future migration must rebind exact current source/header identities and keep
unit ownership, compiler flags, raw assembly and unrelated work stable.

Battle effect naming acceptance (BV-03/BV-08, P2): FF9 adds76 reviewed C
function names in ovl_065800, bringing the catalog to1922. Evidence is in
docs/function-names-battle-effects.json and binviz/target/ff9-names-battle-effects/.
All103 complete native baseline/candidate objects are identical, with unchanged
strict original scores (100 exact,3 existing partials;34484 matched code bytes).
All103 installed WASM preprocessed token comparisons agree. This establishes
identifier preservation, not new original matching, module or gameplay coverage.

Semantic review follows complete particle/emitter bodies:48-byte particles,
108-byte emitters, per-frame emission, child spawning, lifespan reclamation,
owner-linked movement and combined update/render behavior. Free-slot search
updates the high-water mark without reserving the slot. Arena rounding, wrap,
zeroing and oversized-request behavior remain unchanged. Native signatures,
position-pointer forwarding, raw assembly and matching hacks stay intact.

Mesh names follow alternating quad/triangle vertex bytes and per-polygon vertex
expansion rather than old stat-cost prose or deduplication assumptions. The
surprising out+2*n result and count clamp remain. Actual frame-copy loop draws
ten32-pixel strips; old eight-strip prose is not naming authority. Unit helpers
retain transform masks, half-turn rotation,16.16 positions,4.12 scales, target
height adjustment, model extent ambiguity and filtered fade case uncertainty.
Unidentified unit flags and command roles are recorded as deferred.

Aliases apply only to this legacy unit, not other pieces of group0/file11 or
shared boot, regardless of misleading resident-code comments. Existing shared
readability acceptance and configured frozen/compiler inputs remain available.
This reuses maintained legacy farm/scorer compatibility fixtures, not shared
workspace acceptance or scorer migration. Rebind exact source/header identities
before consuming these renamed sources in downstream proofs.

Battle command/UI naming acceptance (BV-03/BV-08, P2): FF9 adds76 new
reviewed C definitions and corrects one existing descriptive name. Current
catalog:1998 unique unit/symbol names; ovl_065800 has152. Evidence is in
docs/function-names-battle-commands-ui.json and binviz/target/ff9-names-battle-
commands/. All111 complete native objects agree and original strict scores stay
unchanged (107 exact,4 existing partials;22252 matched code bytes). All111
installed WASM preprocessed token comparisons agree. No linked/module/gameplay
or new original matching coverage is claimed.

Semantic correction: e5f24 starts a selected CAMERA SCRIPT rather than playing
a sound. Actual c2f9c resets camera state then c316c binds camera subscripts,
keys and mode selection. Those exact inspected callee hashes are recorded.
Consequently e9168/e9290/e94c4 follow camera-script permission/selection. The
old report and unused alias remain historical; the catalog exposes one current
corrected name, never duplicate coverage. Catalog refresh permits a correction
only when its explicit previousName and canonical identity agree.

Actual AKAO calls separately establish six positional sound slots, voice masks,
oldest-age replacement, sound release, tagged-pointer/unit projection and pan.
Effect command names follow emitter chains and action/move/turn/draw state
machines without guessing event labels. Scene color command sets/holds a level;
it does not interpolate. Shared cf588 level/opcode dispatch labels stay deferred.

Battle UI names follow lifecycle, VSync callback, active requests, menu modes,
selectable-unit queue, ability bindings, unit timers and frame submission. Pad
repeat save/restore retains2x23 bytes at24-byte strides and input signatures,
not sound channels suggested by old comments. SO resources keep magic/strides;
GPU helpers keep packet order,24-bit OT links and transfer failure handling.
All matching tricks, ABI inconsistencies, raw assembly and callback spellings
remain unchanged. Aliases are confined to ovl_065800, not other file11 pieces.

Shared readability acceptance already requires configured frozen namespaces and
compiler inputs. These reused legacy farm/scorer results are compatibility
fixtures, not shared workspace acceptance or a scorer migration. Regenerate
source-bound downstream evidence against current catalog/header identities.

Battle selection naming acceptance (BV-03/BV-08, P2): FF9 adds105 new
reviewed C definitions. Current catalog:2103 unique unit/symbol names;
ovl_065800 has257. Evidence: docs/function-names-battle-selection.json and
binviz/target/ff9-names-battle-selection/. All149 complete native objects agree
and pinned original strict-relocation scores remain unchanged (137 exact,
12 existing partials;23440 matched of29360 code bytes). All149 installed WASM
preprocessed token comparisons agree. No linked/module/gameplay or new original
matching coverage is claimed. Current2103 source identities,25 alias headers
and153 owned committed paths were independently rechecked.

Reservation names follow queue-success increments and owned-minus-reserved
availability, preserving byte wrapping and unchecked indices. Inventory lists
keep neutral primary/secondary labels for ID>=0xe0 versus ID<0x58/flag4.
Command-window open/close, row dispatch, lateral animation and strip packets
follow recovered bodies. Help names follow mode-specific text and popup drawing,
not old cursor-beep comments. GPU masks, linkage and matching tricks stay intact.

Target names follow eligibility/selection flags, sides, visibility, single versus
multiple targets, paired actions and marker packets. Actual widget registration
proves f1a8c is OPEN despite its old close-callback comment. f07f0 disables pad
repeat, not sound. Party-panel entry animation, blink and counter names preserve
primary/secondary stat labels pending stronger field semantics; secondary color
uses p24[1]. Unknown animation-table transition meanings remain deferred.

Aliases remain exclusive to ovl_065800. These compatibility farm/scorer proofs
reuse the frozen workflow; they are not shared workspace acceptance or a scorer
migration. Shared readability acceptance still needs configured frozen namespace,
compiler inputs and actual outputs. Source-bound downstream observations require
regeneration against the current catalog/header identities.

Battle display naming acceptance (BV-03/BV-08, P2): FF9 adds83 new
reviewed C definitions; catalog2186 unique unit/symbol names, ovl_065800340.
Evidence: docs/function-names-battle-display.json and binviz/target/ff9-names-
battle-display/. All97 complete native objects agree, pinned strict scores stay
unchanged (91 exact,6 existing partials;17940 matched code bytes), and97 installed
WASM preprocessed token comparisons agree. No linked/module/gameplay or new
original matching coverage is claimed. Catalog/source/header and owned commit
identities are independently rechecked; unrelated CLAUDE content remains intact.

Full original group0/file11 identity d8053882...5381e binds ten callback/animation
tables. Actual mode-help table confirms eedf0 dispatches ability/item help.
List-kind dispatch confirms edf34/edf90/edfec open ability/primary/secondary item
lists. Original animation done pointers bind4140/4180 to party-info opening and
closing completion. Those five previously deferred definitions now have evidence.
Record table offsets/lengths/digests and inspected source identities, not assets.

New names cover action availability, stat costs, remembered menu cursors,
direction input, target-list captions and helpers, party-info stat ratios/status
icons/gauges, VRAM strip capture/queuing, message priority/rectangles/lifetime
and menu draw/animation drivers. Preserve null-before-check behavior, repeated
reservation calls, signed fields, ABI differences and all matching tricks.
Primary/secondary stats, party-class masks and combined-button hold keep neutral
labels where stronger gameplay meanings remain unconfirmed. Old comments that
call COPY RECT packets sprites, gauges badges, or close routines open are not
semantic authority. Aliases remain confined to ovl_065800.

These farm/scorer results remain legacy compatibility evidence. Shared readability
acceptance needs configured frozen namespaces/compiler inputs/actual outputs;
this is not shared workspace acceptance or a scorer migration. Source-bound
downstream evidence must be refreshed against the current catalog/header IDs.

Battle helper naming acceptance (BV-03/BV-08, P2): FF9 adds37 names,
33 reviewed C definitions and four native assembly entry aliases. Catalog2223
unique unit/symbol names; ovl_065800377 named, four deferred after review of all
41 remaining primary definitions. Evidence: docs/function-names-battle-helpers.json
and binviz/target/ff9-names-battle-helpers/. All43 affected complete native objects
agree, pinned strict scores stay unchanged (42 exact,1 existing partial;
14848 matched code bytes), and43 installed WASM preprocessed token comparisons
agree. No linked/module/gameplay proof or new original matching credit claimed.

The original group0/file11 identity d8053882...5381e binds the first16 observed
opcode handler entries at800f8d10; adjacent data is not treated as handlers or
proof of the full dispatch extent. Concrete callees and consumers bind motion,
render-level fade, frame VSync interval, status clearing/model masks, animation
tracks, action dispatch and feedback-ring behavior. Source identities are recorded.

Particle aliases retain raw entry labels, shared assembly interiors and GTE math.
Position blending retains actual argument order; rendering level stays neutral
without an RGB/brightness claim. Model extent is not guessed height/radius;
nonparty is not inferred enemy category. Packed tag and resource-buffer helpers
retain neutral labels. Empty hooks follow concrete frame/init call sites and
explicitly say noop. Four flags/record-field helpers remain deferred. All types,
ABI discrepancies, unchecked accesses, arithmetic and matching tricks stay intact.

Catalog/source/header and owned commit identities are independently rechecked.
Unrelated CLAUDE and working changes remain intact. Legacy farm/scorer evidence
is compatibility evidence, not shared workspace acceptance or a scorer migration.
Shared acceptance requires configured frozen namespaces/compiler inputs/outputs;
source-bound downstream evidence must be refreshed against current catalog IDs.

Battle feedback/render naming acceptance (BV-03/BV-08, P2): FF9 adds71
names in ovl_04e800:62 C/dual-C definitions and nine native assembly call aliases.
Catalog2294 unique unit/symbol names. Evidence: docs/function-names-battle-feedback-
render.json and binviz/target/ff9-names-battle-feedback/. All83 affected complete
native objects agree, pinned strict scores remain unchanged (62 exact,21 existing
partials;16000 matched code bytes), and83 installed WASM preprocessed token
comparisons agree. No linked/module/gameplay or new matching credit claimed.

Original full group0/file11 d8053882...5381e binds the three feedback handler
pointers (numeric0/1 -> cc818, sprite2 -> cc9c8) and four BCD divisors. Semantic
inputs bind primary definitions, inspected callers/callees and raw assembly.
Names cover feedback ring/request processing, numeric/sprite/status drawing,
message formatting and information pages, effect script initialization/rendering,
primitive masks/depth shifts, interpolation, direction/angle helpers, scratchpad
randomness and gradient strip/GPU packet generation. Historical comments are
not authority: message pages are not sound/cursor routines; cf808 returns angles
with a zero third component rather than a radius; packed local Z offsets are
not vector pointers. Keep stat/status labels neutral where meanings are unknown.

cf5f8 retains its unusual IR inputs dx, original ax, dy and unused dz, explicitly
named a legacy distance estimate. Matrix/light scratch reads, raw division,
integer/unsigned arithmetic, packet sizes/links, clipping, callback declarations,
native &id+1 versus WASM varargs and matching barriers remain unchanged. Native
assembly aliases leave all raw labels/instructions stable. cf074 is reviewed as
the interior continuation of ceed4, not an additional independently callable
function, and adds no naming coverage. Unit scopes remain separate.

Independent audit verifies all current catalog source/header IDs and87 owned
committed paths; unrelated CLAUDE content and working changes remain intact.
Legacy farm/scorer fixtures are compatibility evidence, not shared workspace
acceptance or scorer migration. Shared acceptance still requires configured
frozen namespaces/compiler inputs/outputs; downstream source-bound evidence
must refresh against current identities. Continue remaining animation/model,
resource and geometry consumers before inferring gameplay-specific enums.

Battle animation/script naming acceptance (BV-03/BV-08, P2): FF9 adds85
reviewed C definitions in ovl_04e800, now156 named there and2379 catalog-wide.
Evidence: docs/function-names-battle-animation-script.json and binviz/target/
ff9-names-battle-animation/. All88 affected complete native objects agree,
pinned strict scores stay unchanged (83 exact,5 existing partials;11940 matched
code bytes), and88 installed WASM preprocessed token comparisons agree. No
linked/module/gameplay or new original matching credit claimed.

Original full group0/file11 d8053882...5381e binds48 observed script handler slots
atf72e0, including nulls/two unnamed earlier entries, and four19-halfword timing
rows atf73a0. Each newly named wrapper binds to its actual native slot and full
reviewed executor. Source/header identities bind observed definitions, callees
and consumers. Names cover script slot/arena lifecycle, animation binding/frame
counts/frame holds, unit visibility and model parts, party membership/tag timing,
unit sound, movement/turn handlers and their sizing/initialization wrappers.

Actual bc21c establishes flag68 bit21 as animation-frame hold and status3c mask
02001103 as a separate frame-advance gate. Movement follows current/reference
position getters and local-Z transforms, not old timed-rotation prose. Keep
numeric animation23/25/29/30 meanings neutral. Preserve actor16 writes, native
operand/state sizes, byte-packed globals, leading-frame waits, exact counter
equality, signed/byte truncation, extra arguments and mismatched declarations.
d908c still passes the counter pointer as banked-animation context. No scheduler
slot overflow, zero division, animation clamp or interpolation-order repair.

Party-byte75/mapped effect selection remains deferred until complete downstream
renderer/loader review; do not guess weapon or character enum meaning. All alias
edits stay confined to this unit. Independent audit verifies current catalog
source/header IDs, both original data slices and92 owned committed paths while
unrelated CLAUDE and worktree changes stay intact. Legacy farm/scorer proof is
compatibility evidence, not shared workspace acceptance or scorer migration.
Shared acceptance still needs configured frozen namespaces/compiler inputs/
actual outputs. Refresh downstream source-bound evidence after naming changes.

Battle resource/model naming acceptance (BV-03/BV-08, P2): FF9 adds74
reviewed C definitions in ovl_04e800, now230 there and2453 catalog-wide.
Evidence: docs/function-names-battle-resources-models.json and binviz/target/
ff9-names-battle-resources/. All81 affected complete native objects agree,
pinned strict scores stay unchanged (70 exact,11 existing partials;14280 matched
of20912 code bytes), and81 installed WASM preprocessed token comparisons agree.
FF9 commit324a0497c. No linked/module/gameplay or new original matching credit.

Original full group0/file11 d8053882...5381e supplies bounded byte/halfword remap
tables atf727c/f72a0,85 selected-script IDs atf7460 and48 handler slots atf72e0;
d84c4 binds slot0x5c. The actual interpreter establishes three-byte effect
commands, one saved nested-script return cursor and waits; the actual streaming
loader establishes CD sectors, alternating records, VRAM uploads, audio-driver
block loads and draw-sync/cache-flush handshake. The previously deferred party
byte75 now has a concrete script-selection consumer; its gameplay enum remains
neutral. Preserve all counter, return, unchecked indexing and ABI behavior.

New strong model naming evidence:27 original diagnostic function labels in the
bounded a7d00..a81a8 span agree with complete recovered model bodies. They identify
solid/Gouraud/textured/summon registration, sliced/morphed/bone-matrix draw,
summon animation/frame, mesh visibility, texture animation, primitive ABR/RGB,
offset and slice. Store original addresses, bounded slice hashes and observed
labels, not game assets. Low-halfword bone translation copies, strict frame
count comparison, record layout, matching pins and raw assembly stay intact.

Reusable evidence collection gap (BV-03/BV-08, P2): scoped naming catalog support
should bind a diagnostic-label observation to exact original binary identity,
member/load offset, bounded string digest, owning unit and current recovered
definition/consumer IDs. Reject wrong-unit/address collisions, changed binary,
changed string or unbound source; accept descriptive names only with reviewed
behavior. Temporary game adapter uses explicit addresses and existing shared
batch/catalog tools, without a private AST/CFG walker or tool rewrite.

Independent audit verifies all2453 current catalog source/header identities,
five original data slices and85 committed owned paths, preserving foreign
CLAUDE and worktree edits. Legacy farm/scorer compatibility evidence remains
separate from genuine shared workspace acceptance; the latter still requires
configured frozen namespaces/compiler inputs/actual outputs. Refresh downstream
source-bound evidence against current catalog and header IDs.

Battle formation/action naming acceptance (BV-03/BV-08, P2): FF9 adds45
reviewed names (44 C definitions,1 assembly alias) in ovl_04e800, now275 there
and2498 catalog-wide. Evidence: docs/function-names-battle-formation-actions.json
and binviz/target/ff9-names-battle-formation-actions/. All52 affected complete
native objects agree, pinned strict scores stay unchanged (38 exact,14 existing
partials;9336 matched of19680 code bytes), and52 installed WASM preprocessing
comparisons agree. FF9 commit6e9345216. No linked/module/gameplay or new matching
credit. Independent audit verifies2498 current source/header IDs,4 original data
slices and56 owned committed paths, preserving unrelated worktree changes.

Names cover weighted encounter formation, enemy model/animation records, party
placement, unit-action resource/slot lifecycle, action callbacks, position/yaw/
scale tweens, mesh visibility, target selection/meanXZ, message/effect/camera
commands, packet colors/semitransparency, mesh counts, actor resource records,
sliced projection, sprite sheets/strips/quads and fixed-point easing.

Concrete semantic refusals: actual ca5ac walks past4 occupied slots instead of
reusing the last slot as old prose claims; actual f5858 supplies34 observed
enter/run pairs and the next pair is data though opcode34 passes the guard.
Preserve both unchecked boundaries. Original file11 d8053882...5381e binds the
observed pairs, following data words and four formation-angle halfwords.
Actual f4f04/f4f44 consumers establish cb700 MESSAGE output, not sound; actual
c2ffc->c2f9c->c316c establishes cb884 CAMERA setup, not actor movement. Setter55
establishes uniform model scale. The a81a8 diagnostic label suggests vertex
splitting but the complete dd0dc->dd10c body returns model byte3 mesh count;
behavior takes precedence over diagnostic naming as well as old comments.

Preserve16-bit accumulation/narrowing before meanXZ division/color saturation,
fixed16.16 position words, short/byte boundaries, zero division, exact counters,
packed operands, masks, ABI discrepancies, raw assembly and matching tricks.
The textured quad decrementsUV once for>=256 rather than true clamping. Sprite
packet allocation/depth guards and page-change ordering remain unchanged.

Shared readability/callback naming evidence should expose observed table extent
and accepted opcode range separately, and retain explicit contradictory-label
observations with source/consumer IDs; an original function label alone cannot
override the actual recovered body. This extends the existing scoped-label
evidence backlog, without adding a private walker or tooling rewrite. Legacy
farm/scorer compatibility remains separate from shared workspace acceptance,
which still requires configured frozen namespaces/compiler inputs/actual output.
Refresh downstream source-bound evidence after naming.

Battle strip/model rendering naming acceptance (BV-03/BV-08, P2): FF9
adds20 reviewed names (10 C definitions,10 callable assembly aliases), bringing
the catalog to2518. ovl_04e800 has295 named callable functions; the remaining
primary-looking file cf074 is an interior continuation of the already-named
ceed4 polygon walker. All296 primary source files in this unit are accounted
for without counting the continuation twice. FF9 commit ff5772911; report
docs/function-names-battle-strip-model-render.json and compatibility scratch
binviz/target/ff9-names-battle-strip-model-render/.

All17 affected complete native objects agree; pinned strict scores remain
unchanged (7 exact,10 existing partials;2748 matched of18740 code bytes). All17
installed WASM preprocessing comparisons agree. Ten original assembly sources
stay byte-identical. Independent audit verifies2518 current source identities,
32 alias headers,21 committed owned paths and4 bounded original code slices.
No linked/module/gameplay proof or new original matching credit.

Names cover transformed sprite sheets/tiles/quads, keyed textured/Gouraud mesh
strips, layered screen effects, bone-driven sprite figures, inherited-register
color fading, packet emitters, depth-offset/sliced model packet construction
and morphed vertex projection. d4a08 performs DPCS color fade, contrary to old
linear-vector prose. d597c is a callable triangle linker, contrary to its old
fragment comment: two original d59d0 jal instructions plus its jr ra establish
the boundary. Bounded original file11 code and complete caller/source hashes
bind this observation. cf074 instead shares the ceed4 frame and return path.

Preserve allocation/count/depth order, frame clamp/modulo, signed remainder
quirks, inherited registers, fixed-point narrowing, UV/palette ordering, raw
assembly, MAC0 tests, unchecked divisions, existing uninitialized paths and
extra projected vertices. No backface label is inferred from a MAC0 test after
RTPT without actual NCLIP. Behavioral names take precedence over stale comments.

Shared readability inventory should distinguish callable inherited-register
helpers from split interior continuations using explicit caller/control-flow
evidence and observed original ranges. This extends the existing naming-evidence
backlog without a private walker or tooling rewrite. Reused legacy farm/scorer
checks are compatibility fixtures; shared workspace acceptance still requires
configured frozen namespaces/compiler inputs/actual outputs. Refresh downstream
source-bound evidence after names change. Preserve unrelated session changes.

Battle camera/model naming acceptance (BV-03/BV-08, P2): FF9 adds29
reviewed names (28 C definitions,1 callable assembly alias) in ovl_03e000.
Current catalog2547;33 alias headers. Unit inventory146 primary sources,29
named and117 remaining for combat/status/reward review. FF9 commit7a11562b1;
docs/function-names-battle-camera-model.json and compatibility scratch
binviz/target/ff9-names-battle-camera-model/.

All32 affected complete native objects agree; pinned strict scores unchanged
(22 exact,10 existing partials;7536 matched of22968 code bytes). All32 installed
WASM preprocessing comparisons agree. Original assembly source stays untouched.
Independent audit verifies2547 current definition identities,33 headers,36
owned committed paths,7 inspected semantic inputs and4 original data/code
slices. Unrelated work preserved. No new original matching credit, linked image,
compiled module or gameplay proof; old behavioral-test comments are historical.

Names cover camera initialization/actor binding, script selection/interpreter,
key decode/timing/evaluation, anchor lookup/midpoints, random shake, sine bob,
rolled look-at matrices, stored transitions, model relocation/UV offsets,
visibility/scaling, part projection/packet emission and animated bone matrices.
Complete camera consumers contradict old result/color-fade/water-ripple prose.
The look-at roll uses eye->pad, not eye->vz. Original f5758 preset sample is
bounded evidence, not proof of safe table extent or valid class indices.

Preserve signed16 narrowing before midpoint shift, unsigned bob shift, wrapped
angle equality rules, uninitialized interpolation weights, polynomial factors6,
script command count/timing/flags and repeated unadvanced scene-end pointer.
Retain flags|=2 then flags=0 in model initialization, one-time relocation/UV
mutation, shared-index scratch marking, low-half translation, groups of three
with possible extra/preloaded vertices, low-bone do-while behavior and next-parent
reads. The two packet emitters keep distinct tag writing, hidden-object return,
texture flags and depth arithmetic. Bone-builder code spans clipped overlay
boundaries; aliases remain confined to its canonical03e000 naming unit.

Shared naming evidence should retain complete consumer chains and conflicting
old labels, observed code extents and unresolved same-address overlay callees.
No role is inferred for ea004 from a different unit's source. This extends the
existing semantic-evidence backlog without a private walker or tooling rewrite.
Legacy farm/scorer checks remain compatibility fixtures; shared workspace
acceptance requires configured frozen namespaces/compiler inputs/actual outputs.
Refresh downstream source-bound evidence after names change.

Battle combat/status naming acceptance (BV-03/BV-08, P2): FF9 adds76
reviewed C names in ovl_03e000, commit9bec11145. Current catalog2623,
34 alias headers. Unit inventory146 primary sources,105 named,41 remaining.
Evidence: docs/function-names-battle-combat-status.json and compatibility
scratch binviz/target/ff9-names-battle-combat-status/.

All89 affected complete native objects agree, pinned strict-relocation scores
unchanged:75 exact,14 existing partials,19308 matched of33272 code bytes.
All89 installed WASM preprocessing comparisons agree. Independent audit checks
2623 current definitions,34 headers,93 committed paths,12 semantic inputs and
7 original bounded jump-pointer/code slices. No new original matching credit,
linked image, compiled module or gameplay proof. Unrelated work preserved.

Names cover action results, stealing, hit/evade/critical rolls, physical/magic
formulas, elemental affinity, HP/MP damage and healing, partner-cast checks,
command-entry flags, status masks/timers/model colors, encounter-start rolls,
reward drops, party results and unit-list selection. Complete callees contradict
old hit-sound, status-hit-roll, magic-stat, hue and preemptive-roll prose.
Resident4ab7c dispatches actor VM commands; b4040 cancels queued commands.
Damage and healing paths preserve all result flags, integer narrowing, signed
arithmetic, repeated-byte RNG, queue/animation side effects and ABI mismatches.

Status application accepts masks and selects their highest bit for side effects;
removal uses an exact-mask switch. Timers expire when negative, not at zero.
The encounter forced-mode flag path is opposite its old prose. Reward item0 can
write into adjacent card storage after a rare drop fills the last item slot;
retain this behavior and whole-word card eligibility. Preserve unchecked list
insertion, zero-enemy list linkage, candidate-array capacity and differing side
selector conventions. No status/ability label inferred solely from numeric masks.

Deferred c19f8 calls dispatcher kind0x38, but the02e800 dispatcher only recovers
a scored head with stand-in cases; no complete action purpose follows from it.
c0a3c names the observed spirit-scaled counter initialization without claiming
an unconfirmed period consumer. Shared naming evidence should retain conflicting
old labels, complete consumers, bounded original tables and explicit partial
reconstruction boundaries. This extends existing semantic-evidence backlog;
no private walker or tooling rewrite. Legacy farm/scorer checks remain
compatibility fixtures, not shared workspace acceptance, which requires frozen
namespaces/compiler inputs and actual outputs. Refresh source-bound evidence.

Battle unit lifecycle naming acceptance (BV-03/BV-08, P2): FF9 adds40
reviewed C names in canonical ovl_03e000, commit04c6031d7. Catalog2663;
35 alias headers. All146 primary sources in this unit reviewed,145 named;
c19f8 deferred because its dispatcher case bodies are stand-ins.
Evidence: docs/function-names-battle-unit-lifecycle.json and compatibility
scratch binviz/target/ff9-names-battle-unit-lifecycle/.

All51 affected complete native objects agree and pinned strict-relocation
scores are unchanged:39 exact,12 existing partials. All51 installed WASM
preprocessing comparisons agree. Current catalog/source/header hashes and55
owned committed paths checked,17 semantic inputs and12 bounded original
jump-pointer/color/code slices verified. No new original matching credit,
linked image, compiled module, gameplay or shared workspace acceptance.

Names cover enemy/model initialization, equipment element affinities, drawing,
hit/death animation, model-part visibility, periodic HP/MP effects, tick speed,
unit-field/battle-state script getters/setters, packet color/status bob,
grayscale CLUT preparation, music transitions, hit/death/positional sound,
ground-marker dimensions and action-effect script selection/context.

Actual ad90c/AKAO/e976c consumers contradict generic effect labels for sound
helpers and establish stereo pan. d62a0 starts an effect script, so c2598/c28f4
are not merely displayed-message helpers. Ground-marker matrix consumers
ac37c/ad4e4 establish c23c8 marker dimensions rather than geometry scale.
Color walkers distinguish packet RGB mutation from CLUT grayscale uploads.
Old setter field ids, visual death/petrify test and CLUT row-offset prose are
incorrect; preserve observed operations and types rather than adopting them.

Keep all original expressions, masks, signed shifts, narrow stores, fixed
strides, compiler flags, allocation pins/barriers/gotos and ABI mismatches.
In particular bfaf4 retains its no-argument bff20 declaration/call despite the
callee requiring a unit; naming acceptance does not repair ABI. Retain pending
reaction/death fallthrough, frame-byte wrap, ignored command failures, unchecked
modulo/array/pointer paths, target count average and stale partner pointer.
Interior split heads are within complete primary functions, not extra names.
Unit namespaces stay distinct across file11 pieces. Shared naming semantic
evidence backlog now includes conflicts between old labels, sound/effect/marker
consumers and explicit incomplete-dispatch deferrals. Reuse existing tooling;
legacy farm/scorer fixtures do not replace configured frozen compiler/output
workspace acceptance. Refresh source-bound downstream evidence after renames.

Battle queue/sound naming acceptance (BV-03/BV-08, P2): FF9 adds59
reviewed C names in canonical ovl_02e800, commitd83e8f03e. Catalog2722,
36 alias headers; unit106 primary sources,59 named,47 remaining.
Evidence: docs/function-names-battle-queue-sound.json and compatibility
scratch binviz/target/ff9-names-battle-queue-sound/.

All68 affected complete native objects agree, pinned strict scores unchanged:
58 exact,10 existing partials. All68 installed WASM preprocessing comparisons
agree. Independent audit verifies2722 current source identities,36 headers,
72 owned committed paths,14 semantic inputs and6 bounded original jump/code
slices. No new original matching credit, linked image, compiled module or
gameplay; unrelated adoption/runtime work preserved.

Names cover actor/model resource slots and tables, handle release, ground and
target marker rendering, sound/music command engine and streamed bank loading,
queue initialization/insertion/cancellation/execution/completion, reactions,
cover, source status attributes, target redirection, MP payment and item
reservation release. Source and runtime identities retain original symbols.

Complete consumers contradict voice/effect/channel labels: item commands
release inventory reservations; sound-bank paths call AKAO; command records
are queue sentinels and marker records are GPU packets. Resource release uses
inclusive indices (4/3 handles rather than old3/2 comments), dynamic slot
counter initializes0, stream chunks rebase amounts before feeding. MP payment
is mutating, cost mask4 is bit2, and command preparation pays gil separately.
Random refusal x<64 is64/256 rather than one-in64. Preserve these observations
and differing owner checks, sentinel membership, kind priorities and masks.

Preserve all bodies, flags/types, ABI mismatches, matching compiler settings,
allocation pins/barriers, raw GTE code, partial baselines and existing traps.
Retain allocation before ground-marker center-depth exit, a missing center
in one depth maximum, angle step4f1, unchecked candidates/indices/pointers,
negative-cost/narrow overflow, double RNG refill, signed movement division,
ignored command errors and reservation made before reaction eligibility.
The action dispatcher b44c0 remains a scored head with stand-in case bodies;
complete surrounding command names do not assert its reconstruction.

Shared semantic-evidence backlog includes complete callee/consumer chains,
conflicting old comments, mutating query helpers and partial dispatcher
boundaries. No private walker/tooling rewrite. Legacy farm/scorer fixtures
remain compatibility evidence; shared workspace acceptance requires frozen
namespaces/compiler inputs and actual outputs. Refresh downstream evidence.

Battle screen/phase naming acceptance (BV-03/BV-08, P2): FF9 commit5311a6549
adds33 reviewed C definitions, reaching2755 unique unit/symbol names. Unit
ovl_02e800 now has92/106 primary definitions named;14 remain for purpose review,
including loading state machines and the partial action dispatcher. Evidence:
docs/function-names-battle-screen-phase.json and binviz/target/ff9-names-battle-
screen-phase/. All33 complete native objects and installed WASM preprocessed
tokens agree; original strict scores stay unchanged (29 exact,4 existing
partials). No new original matching credit, linked image/module or gameplay
proof is claimed.

Complete bodies and30 semantic inputs establish screen lifecycle, common
archive resources, scene buffer allocation, actor records, disc-wait/pause,
ordering tables/frame submission, battle phase transitions and render tails.
Four original code ranges bind the complete pause epilogue, zero-return hook,
phase driver and outcome-transition body. Catalog/header/current-source and
isolated committed-path checks pass across2755 names and37 alias headers.

Stale prose incorrectly calls graphics initialization an audio start, global
exit flags a direct button mask, scene intro an aftermath phase and the broad
outcome-transition routine victory-only. Names follow actual bodies and callees.
The frame-render tail passes primitive cursors; marker projection uses mask2
(bit1). Release cleanup repeats sweeps of the same fixed table. The actor sprite
record wrapper takes palette/texture positions, not a display rectangle. Keep
all ABI mismatches, signed/narrow counters, pointer/flag asymmetries, packet
order, raw assembly, register pins and compiler matching hacks unchanged.

Legacy farm/scorer compatibility evidence is not shared workspace acceptance
or scorer migration. Shared acceptance needs configured frozen namespaces and
compiler inputs. Regenerate source-bound downstream evidence using current
catalog/header identities. Preserve unrelated runtime-adoption work.

Battle load-sequence naming acceptance (BV-03/BV-08, P2): FF9 commite34004516
adds13 reviewed C definitions, reaching2768 unique unit/symbol names. Unit
ovl_02e800 now has105/106 primary definitions named; b44c0 remains deferred as
an action-dispatch HEAD with109 stand-in cases. Evidence:
docs/function-names-battle-load-sequence.json and binviz/target/ff9-names-battle-
load-sequence/. All16 complete native objects and installed WASM preprocessed
tokens agree; original strict scores stay unchanged (12 exact,4 existing
partials;6232/9472 matched code bytes). No new original matching credit,
linked image/module or gameplay proof is claimed.

Complete bodies and20 semantic inputs establish sound-bank, scene texture,
party weapon/model and single-slot reload stages plus archive-sector selection,
texture-table patching and actor rebinding. Six original full code ranges bind
the complex drivers. Catalog/current-source/header and isolated committed-path
checks pass across2768 names and38 alias headers. Deferred source identity is
pinned, not counted as semantic reconstruction or naming coverage.

Old prose treats load-control flags as buttons/cancel and lowest-sector resource
selection as player dialog choices. Actual AKAO, scatter-read, image upload,
equipment/model/animation consumers contradict that. Preserve all existing
behavior, including unguarded reverse scans, dedup out-1 probe, missing-candidate
outsel unchanged, fixed capacities, asymmetric actor frame selection and
unchecked current-command pointer. aabc4 uses first selected size n rather than
n2 for the second scatter segment; its second selection guides buffer placement.
The default-flag wrappers each STEP a machine rather than block until completion.

Legacy farm/scorer compatibility evidence is not shared workspace acceptance
or scorer migration. Shared acceptance needs configured frozen namespaces and
compiler inputs. Refresh downstream source-bound evidence against current
catalog/header identities. Preserve unrelated runtime-adoption work.

Battle hit/script-helper naming acceptance (BV-03/BV-08, P2): FF9 commite624dc9ba
adds77 reviewed C definitions, reaching2845 unique unit/symbol names. Unit
ovl_02d800 now has77/151 primary definitions named;74 remain, including five
reviewed but uncertain roles. Evidence: docs/function-names-battle-hit-script-
helpers.json and binviz/target/ff9-names-battle-hit-script-helpers/. All77
complete native objects and installed WASM preprocessed tokens agree; original
strict scores stay unchanged (76 exact,one existing partial;7436/7780 matched
code bytes). No new matching credit, linked image/module or gameplay proof.

Complete82 primary bodies,19 semantic inputs and three shared-header identities
establish command slots, hit/damage calculations, unit lookup, weapon anchors,
model colors and action-script commands. Four original code ranges plus35
native enter/run handler pairs are pinned without provider/index-safety claims.
Catalog/current-source/header and isolated committed-path checks pass across
2845 names and39 alias headers. Five deferred source identities remain explicit.

Shared field labels are not semantic authority: bit0 is SET for party units,
CLEARED for enemies;20-byte effect slots are COMMAND entries with six party and
three enemy slots. Keep fields/layouts and conditions unchanged. Entity1e is a
model id used by enemy construction, not a script id; entity62 is a sound id.
The percentage calculation uses target MAX HP, not current HP. Retain signed
costs/modifiers, chance narrowing before clamping, modulo/zero-duration hazards,
null-target unlink behavior, unguarded packet traversal and hidden-part cursor
asymmetry. Unconfirmed status masks stay explicit instead of enum guesses.

Actual script consumers establish delayed effects/sounds, uniform scale and
position tweens, texture animation and model fading.7270 pauses texture deltas,
not primitive-type masking. Getter pointer declarations for raw type/angle-step
words remain ABI inconsistencies, not valid object-pointer assertions. Particle
count31 is a count, not merely an enable boolean. Native aliases stay unit-scoped.

Legacy farm/scorer compatibility evidence is not shared workspace acceptance
or scorer migration. Shared acceptance needs configured frozen namespaces and
compiler inputs. Refresh downstream source-bound evidence against current
catalog/header identities. Preserve unrelated runtime-adoption work.

Battle render/UI-access naming acceptance (BV-03/BV-08, P2): FF9 commit
ee7c4e910 adds32 reviewed definitions, reaching2877 unique unit/symbol names.
Unit ovl_02d800 now has109/151 primary definitions named;42 remain. Four newly
named sources retain raw assembly default branches with C alternatives.
Evidence: docs/function-names-battle-render-ui-access.json and binviz/target/
ff9-names-battle-render-ui-access/. All32 complete native objects and installed
WASM preprocessed token comparisons agree; original strict scores unchanged
(31 exact,one existing partial;2916/3220 matched code bytes). No new matching
credit, linked image/module or gameplay proof. Catalog/current-source/header
and36 isolated committed-path checks pass across2877 names and40 alias headers.

Complete32 primary bodies,19 semantic consumers and four shared-header
identities establish random/math helpers, model blending/projection, particle
state stepping, effect render setup and battle menu/queue/panel/message access.
Consumer evidence establishes purpose without selecting cross-unit providers.
Three reviewed uncertain roles remain explicitly deferred. Names do not repair
matching quirks, ABI inconsistencies, unchecked accesses or compiler inputs.

Concrete semantic corrections to transplanted comments: f3eb4 returns PARTY
PANEL ACTIVE byte, not selected row; f4fc8 returns text-box DURATION word at+84,
not a picker pointer, retaining its pointer declaration. Queue entries are
selectable units rather than messages. Visible-party count does not test HP or
living status. Keep cf5f8 legacy IRs(dx,a.x,dy) and unused dz, signed square-root
division, raw remainder paths, saturation, origin axis permutation in particle
stepping and gravity update that does not reload IR before GPL. Model blending
retains next-vertex preloads and triplet projection; panel backgrounds retain
OT16 linking, unchecked row and count-minus-one arithmetic. Unit-scoped aliases
preserve native strings, linker identities, source filenames and body tokens.

Reused maintained naming/farm/scoring interfaces need no new private analysis
walker. Legacy compatibility evidence remains distinct from shared readability
workspace acceptance and scorer migration; shared acceptance needs configured
frozen namespaces/compiler profiles and verified outputs. Refresh downstream
source-bound evidence against current catalog/header identities. Preserve
unrelated runtime-adoption and tooling work.

Battle resource-state naming acceptance (BV-03/BV-08, P2): FF9 commit
70707669f adds30 reviewed definitions, reaching2907 unique unit/symbol names.
Unit ovl_02d800 now has139/151 primary definitions named. Complete primary-body
review across this and previous passes leaves12 unresolved roles with explicit
source hashes/reasons. Evidence: docs/function-names-battle-resource-state.json
and binviz/target/ff9-names-battle-resource-state/. All30 complete native objects
and installed WASM preprocessed token comparisons agree; original strict scores
remain30/30 exact,1588/1588 code bytes. Catalog/current-source/header and34
isolated committed-path checks pass across2907 names and41 alias headers.
No new matching credit, linked image/module or gameplay proof.

Actual32 semantic consumers and three shared-header identities establish unit
anchor bone, ground-marker gate, unit-info pager, shared command parameter pairs,
effect loader/buffer/cache state, effect/summon records, camera-choice queue,
positional sound sources and UI frame/controller/help/target getters. Context
establishes purpose without asserting cross-unit selected-provider identity.
Resource record id misses return2, loader advance preserves signed bookkeeping,
cache-flush polling is a handshake rather than an immediate flush, model reset
clears only documented fields and summon reset clears ONLY FIRST record. No
capacity/index checks are added. Empty release hook stays empty despite calls
with arguments; pointer/int inconsistencies and matching tricks remain intact.

Concrete stale-comment pitfalls: cf560 writescf588 render level, while the
transplanted cf58c twin writescf5a0 depth shift; cd808 binds pager owner and clears
all1520 countdown/bit/page bytes rather than setting numeric counters. Camera
choice queue is not a sound-id queue. Sound source word is not immediate pan or
volume; subsequent service projects a unit/tagged vector. Cached multiple-target
flag and party-info request do not prove a fully open window. The12 deferred
roles include seven zero/failure stubs, support mask200, one global flag, two UI
requests/flags and a target classifier; raw values alone are not semantic names.

Reused maintained naming/farm/scoring interfaces add no private analysis walker.
Compatibility results remain distinct from shared workspace acceptance and
scorer migration. Shared acceptance requires configured frozen namespaces,
compiler inputs and verified outputs. Refresh source-bound downstream evidence
against current catalog/header identities. Preserve unrelated adoption/tooling
work; other canonical units still need purpose review.

Scene session/resource naming acceptance (BV-03/BV-08, P2): FF9 commit
d706d72af adds 44 reviewed definitions in canonical ovl_009800, reaching 2951
unique unit/symbol names and 42 alias headers. Evidence lives in FF9
docs/function-names-scene-session-resources.json and Binviz
target/ff9-names-scene-session-resources/. All 56 complete native objects agree;
pinned strict-relocation original scores retain 48 exact and 8 partial baselines,
11300/26988 exact code bytes, with no failures. All 56 installed WASM
preprocessed token comparisons agree. Current catalog/header/source hashes and
all 60 isolated committed paths independently verify. No new original-matching
credit, linked image/module or gameplay proof.

Unit progress is 44/159 named primary definitions. Complete-body review includes
five explicit deferred roles and one additional voice-release consumer; 109
primary bodies remain unreviewed. Actual callers establish nested whole-session
loops versus the true frame loop, resource lifetime/registration, selection
history, sector-range restoration, pending asset disc order and sprite texture
origin/packet allocation. Preserve old-style externs, extra call arguments,
unchecked capacities, pointer/int differences, fixed layouts and matching hacks.

Concrete comment pitfall for BV-03: FxEnt f4 is a buffer address and f8 a byte
size, despite source and generated-header comments claiming expiry/timed slots.
ac3d8 reserves memory below the lowest cached buffer, aa4e8 reads/copies actual
data there, ac474 compacts it upward, and ac67c invalidates by address boundary.
Names reflect these complete bodies and consumers; generated headers and all
comments remain untouched. aa01c resource fields30/32 are texture origins, not
dimensions: ad44c forwards them to boot62e3c texture-page and UV calculations.
Unresolved tag1E control/tag1F auxiliary resources and the table mapping that
repeatedly tests only its first entry remain deferred rather than guessed.

Reused maintained naming/farm/scoring adapters add no new private analysis
walker. Compatibility checks remain distinct from shared workspace acceptance
and scorer migration; shared acceptance still requires configured frozen
namespaces, compiler inputs and verified outputs. Refresh source-bound evidence
against current catalog identities. Preserve unrelated runtime/adoption/spec
changes and continue purpose review of remaining canonical functions.

Scene render/stream/menu naming acceptance (BV-03/BV-08, P2): FF9 commit
a03672218 adds 86 reviewed canonical ovl_009800 definitions, reaching 3037
unique unit/symbol names and 43 alias headers. Evidence: FF9
docs/function-names-scene-render-stream-menu.json and Binviz
target/ff9-names-scene-render-stream-menu/. All 95 complete native objects are
byte-identical; pinned strict-relocation original scores retain 81 exact and
14 partial baselines, 24540/46208 exact code bytes, with no failures. All 95
installed WASM preprocessed token comparisons agree. Catalog/current-source/
header hashes and 99 isolated committed paths independently verify. No new
matching credit, linked image/module or gameplay proof.

Canonical unit progress is 130/159 named definitions (29 remain). Selected
complete bodies establish actor resource loading, parent-ordered collection,
eight-pass rendering, four ground-shadow quads, colour/shadow tables, sprite
records, animated VRAM textures, seven-phase movie playback, audio commands/
streamed banks, two archive-request queues and the operation-menu handler loop.
Eight actual semantic consumers/services and two shared headers are pinned.
Cross-unit consumers corroborate roles without claiming selected provider
resolution. Specific menu choices and fixed resource1000b purpose stay unnamed.

Concrete stale-comment pitfalls for BV-03: b2528 allocates and loads sound BANK
slots rather than live voices; complete b0124 and b29f8 establish class3/kind7
loading and sound-slot selection. b2c80 returns ramp STILL PENDING, not elapsed.
ae68c checks active movie playback, not CD busy: actual resident12ad4 packs MBG
initialization and frame count. b3378 compares FOUR packed key bytes against a
signed16 load through a byte pointer, despite the three-byte comment. Third
shadow quad omits centre depth; A-queue removal swaps the last entry and changes
order. Preserve all these behaviors, ABI inconsistencies, capacities, masks,
register pins, scratchpad stack changes and matching tricks. Generated headers
and comments remain unchanged. Identifiers only, scoped to this canonical unit.

Maintained naming/farm/scoring adapters introduce no private analysis walker.
Compatibility results remain distinct from shared workspace acceptance/scorer
migration, which require configured frozen namespaces, compiler inputs and
verified outputs. Refresh downstream source-bound evidence against current
catalog/header identities. Preserve unrelated runtime/adoption/tooling work;
remaining scene model/primitive and other canonical functions still need review.

Scene model/primitive naming acceptance (BV-03/BV-08, P2): FF9 commit
78a6f2981 adds 28 reviewed canonical ovl_009800 names, reaching 3065 unique
unit/symbol names and 44 alias headers. Evidence: FF9
docs/function-names-scene-model-primitives.json and Binviz
target/ff9-names-scene-model-primitives/. All 36 complete native objects are
byte-identical; pinned strict-relocation original scores retain 19 exact and
17 partial baselines, 17112/44564 exact code bytes, with no failures. All 36
installed WASM preprocessed token comparisons agree. Current catalog/source/
header hashes and all 40 isolated committed paths verify. No new matching
credit, linked image/module or gameplay proof.

Complete-body review of all 29 previously unnamed primaries finishes this
canonical unit's primary review: 158/159 named, with a9860 explicitly deferred
pending external renderer/resource helper roles. Other units remain pending.
Reviewed roles cover static/animated bone matrices and the aim-bone variant,
ordinary and Y-limited vertex projection, four primitive sort variants, double
packet templates, colour passes, textured transparency/blending, plane-based
model copy, swirl/audio transition, pause and timed splash. Nine actual semantic
consumer/service definitions and five header inputs are pinned. Equal overlay
addresses and donor comments do not establish selected provider resolution.

BV-03 semantic corroboration examples: a9814/aa0a8 are vibration pattern bind/
reset, despite the old sound-driver interpretation: actual boot54b78 writes
D739b0 motor values, and boot19194 forwards that buffer to pad actuators.
b9e4c sets only the first vertex colour in each Gouraud packet. ba190 modifies
command bit1, despite a bit2 comment. ba0f0 narrows sums to signed16 BEFORE
clamping. Source-colour F4 passes retain OR rather than assignment. bb714 has
no part depth bias, uses flag-masked average depths and preserves tag-clearing
order; bc600 reverses winding tests. Plane-transform products/shift11 are
retained without mathematical correction. Preserve raw assembly, register
pins, mismatched ABI declarations, out-of-count projections and matching hacks.

Reused maintained naming/farm/scoring adapters introduce no private analysis
walker. Compatibility checks remain distinct from shared workspace acceptance/
scorer migration, which require configured frozen namespaces, compiler inputs
and verified outputs. Refresh downstream source-bound evidence against current
catalog/header identities. Preserve unrelated runtime/adoption/tooling changes.

Field command and scene-init naming acceptance (BV-03/BV-08, P2): FF9
32c35c0bb names102 canonical ovl_009000 functions; 02bb46465 names the last
ovl_009800 initializer. Total3168 unique unit/symbol names,46 alias headers.
Evidence: FF9 docs/function-names-field-commands.json and
docs/function-names-scene-render-init.json; Binviz target/ff9-names-field-commands/
and target/ff9-names-scene-render-init/. All102+2 complete native object pairs
are byte-identical; pinned strict-relocation scores remain100exact/2partial
(9324/9768 code bytes) and2exact (1416/1416), with no failures. All104 installed
WASM preprocessed token comparisons agree. Catalog/current-source/header hashes
and isolated106/6 committed paths verify. No new matching credit, linked image,
compiled portable module or gameplay proof.

All112 canonical field-command bodies reviewed:102 named,10 explicit deferrals
(five effect-free opcode stubs, one output1 stub and four unresolved fields).
All159 scene-unit primaries now reviewed/named. Other canonical units remain
pending. Complete semantic consumer review21 field inputs plus8 initialization
inputs corroborates camera, clip/parallax/scroll layers, packet colour gates,
background animation, actor collision radius, triangle/group controls, region
animations, motion channels and party cursor roles. No cross-unit alias is
applied and equal addresses/donor comments do not establish selected providers.

BV-03 pitfalls: c1100 copies current bounds INTO the preset, reversing its
comment's direction. c1c04 returns a relative offset, not a pointer. Motion
channel c8010 changes flags18 while c808c changes the curve halfword. History
removal keeps copying the original last entry while count falls. Preserve these,
unchecked capacities/indexes, byte/halfword narrowing, masks and matching hacks.
Three native-only arithmetic functions expose C aliases without rewriting raw
assembly symbols/strings: shifted-denominator divide, fixed16 integer rounding,
and (v+8000)>>8 HALF UNIT bias (not ordinary fixed8 nearest-value rounding).

BV-08 concrete metadata gate: retain the leading /* asm */ marker BEFORE the
alias include. tools/clang-audit.py:34, tools/hack-ablate.py:786, tools/twins.py:746,
wasm/difftest.py:659 and portable runtime preparation/refusal gates classify
native-only files via lstrip().startswith. The first prepared include position
hid this marker; corrected snapshots were recompiled, byte/token verified and
amended before batch acceptance. Reusable readability acceptance should pin
source classification/leading metadata, alongside non-name tokens, to prevent
format/include edits bypassing portable-source refusals. Three original marker
bytes and raw assembly strings remain intact. No new reusable feature claimed.

Maintained naming/farm/scoring adapters add no private analysis walker. Shared
workspace acceptance/scorer migration still needs configured frozen namespaces,
compiler inputs and verified outputs. Refresh source-bound downstream evidence
using current catalog/header identities; preserve foreign runtime/tooling work.

Field renderer/collision naming acceptance (BV-03/BV-08, P2): FF9
a715de40d names all81 canonical ovl_01f000 functions: background tile packets,
camera follow/scripted moves, layer window masks, colour-fade records, walkmesh
geometry/contact resolution, region frame changes, motion channels, animated
sprite models and party-menu lifecycle/input/drawing. Total3249 unique reviewed
unit/symbol names,47 alias headers. Evidence: FF9
docs/function-names-field-render-collision.json and Binviz
target/ff9-names-field-render-collision/. All81 complete native object pairs are
byte-identical; pinned strict-relocation scores remain65exact/16partial,
25856/43912 code bytes, zero failures. All81 installed WASM preprocessed token
pairs agree. Independently rehashed current catalog/headers, primary snapshots,
object pairs and85 isolated committed paths. Other canonical units remain.
No new matching credit, portable module, linked-image or gameplay proof.

All81 primary bodies reviewed, including21 unchanged complete consumers pinned
in the preceding field-command report. Two actual BOOT helper bodies establish
important semantic corrections:630e8 initializes a flat quad (length5/code28),
so layer window records are mask strips rather than light/matrix groups.
631ac waits/advances signed-byte position deltas and detaches a completion
script; c95b8 steps window tweens rather than releasing resources as its comment
claims. c44f4/c73ac actually step actors rather than merely querying status.
Semantic naming acceptance should bind actual consumer bodies; address equality,
donor comments and local field labels alone are insufficient provider evidence.
No cross-unit alias applied or selected-provider resolution claimed.

Preserved c07d8's double arena increment and unsupported-mode behavior,
c1e14's asymmetric crossing bounds, c2af0's expanding retry displacement despite
a contraction comment, c31f8's existing uninitialized/incorrect accumulator
slots, c3ec0's mutated triangle/point state, first-containing floor-height
selection, early whole-walker stop on negative duration, unchecked indexes and
capacities, narrowing, flags, K&R/mismatched signatures, GPU tags, inline GTE
assembly, compiler flags and register/scheduling tricks. Only identifiers and
unit identity aliases changed. Native-only leading metadata rule remains intact.

Maintained naming/farm/scoring adapters were reused without a private analysis
walker or tooling rewrite. Existing BV-03/BV-08 readability acceptance proposals
still need shared frozen namespace/compiler-input/output gates. Refresh
downstream source-bound evidence using the current catalog; preserve other
sessions' runtime work. No new shared feature or workspace acceptance claimed.

Movie streaming naming acceptance (BV-03/BV-08, P2): FF9 1c60ad9a0
names74 canonical ovl_146000 functions, including five native-only assembly
C aliases. All78 primary bodies reviewed; four explicit opaque cross-overlay
deferrals. Total3323 unique reviewed unit/symbol names,48 alias headers.
Evidence: FF9 docs/function-names-movie-streaming.json and Binviz
target/ff9-names-movie-streaming/. All74 complete native object pairs identical;
pinned strict-relocation scores unchanged:66exact/8partial,12372/18692 code
bytes, zero failures. All74 installed WASM preprocessed token comparisons agree.
Independent current catalog/header/primary snapshot/object hashes and78 isolated
committed paths verify. No new matching credit, portable module, linked-image,
runtime selected-provider or gameplay proof. Other canonical units remain.

Complete producer/consumer review establishes movie start/pump/finish, frame side
data, metadata/payload rings, pause/resume, display and pixel modes, callbacks,
MDEC input/output and waits, VLC table/decode/version3 blocks, pixel-mask runs
and disc-prompt CD shell/probe/panel behavior. Five shared release Binviz native
disassembly observations independently corroborate direct stream field aliases:
DE20 selects sector+14 versus+38 header variant; DE2C is the start sector;
DE4C is terminal group. Capture pins CLI545beb01...44b6e, executable, overlay,
notes and each actual output. No private ownership/type/CFG walker added.

Consumer evidence contradicts several comments: D110 sends MDEC input DMA,
not SPU data; sprite97A54 is194by168, not78by28. Descriptor1000 means available
for writing;8000 is ready for fetching. C884 still divides before its zero-unit
guard, C1B0 ignores its first argument, CF34 remains a no-argument output thunk
despite argument-bearing callers, and CE20 retains existing copy/command-table
pointer arrangement. Flags, polling bits/timeouts, register pins, compiler
selection, narrowing, callback sequencing and signature mismatches unchanged.

Raw assembly aliases preserve every instruction/string/shared continuation
label/custom register ABI/external epilogue jump and the FIRST leading asm
marker. Native97B68 probe role is established by its CD command/status checks;
opaque98278 return-bearing external801f6368 remains deferred because a different
canonical unit at that address defines a void flag setter. Equal addresses and
donor comments do not establish runtime provider correspondence. Also defer
97DA4's three opaque calls and97EAC/97EF4's unresolved table lifecycle roles.

Maintained naming/farm/scoring and classification adapters reused. Existing
BV-03/BV-08 shared readability acceptance remains a proposal requiring frozen
namespace/compiler/input/output identities; this batch grants no shared workspace
acceptance. Refresh downstream source-bound evidence from current catalog;
preserve foreign runtime work and the historical scorer selection.

World-map frame/resource naming acceptance (BV-03/BV-08, P2): FF9
3ce2627b2 names48 canonical ovl_09b800 functions: frame initialization/update,
script parameters, actor/event/dialogue step, resource loading and relocation,
arena marks, texture uploads/scroll/frame sequences, walkmesh height lookup,
GTE lighting matrices, polygon submission, marker UI, sprites and map sound.
Total3371 unique reviewed unit/symbol names,49 alias headers. Evidence: FF9
docs/function-names-world-frame-resources.json and Binviz
target/ff9-names-world-frame-resources/. All60 affected complete native object
pairs identical; unchanged pinned strict-relocation scores51exact/9partial,
24376/35680 code bytes, no failures. All60 installed WASM preprocessed token
comparisons agree. Current catalog/header/body snapshot/object hashes and64
isolated committed paths verified. No new matching credit, compiled portable
module, linked image, selected-provider, shared acceptance or gameplay proof.

Bounded whole-body review64/149 primaries;48 named,15 purpose deferrals and
one concurrent-edit exclusion,85 unreviewed. Seven full BOOT semantic inputs
corroborate actor/event/dialogue and GTE roles. Many comments mislabel this unit
as battle: 8024/43F0 submits sound, 83FC invokes combined actor/event/dialogue
step, 8664 loads lighting matrices, and 93B8 uploads texture groups. 8B10 uses
StoreImage to capture rows;8068 uses MoveImage rather than a clear. Preserve
signed shift/division differences, wrap/narrowing, unchecked widths/indexes,
walkmesh cache/null ordering, h[6] beyond array bounds in resource setup,
register pins, alignment branches, compiler flags and signature mismatches.

Concurrent-edit acceptance case: another session changed8574's64E88 prototype
and call while native verification ran. Maintained source hash guard refused
installation BEFORE any source writes. Entire8574 file was excluded; its
canonical references and foreign changes stayed untouched. Remaining candidates
were recompiled and verified, then committed with an isolated index. Naming
checkpoint replacement preserved all foreign CLAUDE sections and index entries.

BV-08 acceptance requirement for reusable readability verification: scoring
must consume exactly the declared object namespace, with no stale extras from
a previously prepared/refused batch. Two excluded8574 scratch objects remained
after re-preparation; removed only those known owned files, reran the60-object
proof and amended the report before acceptance. A future shared consumer should
reject missing/extra objects and assert scoredFunctions equals the manifest,
alongside source/compiler/header/output identities and leading asm metadata.
Maintained farm/scoring/preprocessor adapters reused; no private analysis walker
or tooling rewrite added. Refresh source-bound downstream evidence from current
catalog, preserve other sessions' work and historical scorer selection.

World streaming/weather/sound naming acceptance (BV-03/BV-08, P2): FF9
c3bf5115d names60 more canonical ovl_09b800 functions;3431 unique reviewed
unit/symbol names,50 alias headers. Unit has108/149 named,128 complete primary
bodies reviewed cumulatively,20 reviewed unnamed and21 awaiting review.
Current batch reviews70 whole primary bodies and records10 purpose deferrals.
Evidence: FF9 docs/function-names-world-streaming-weather-sound.json and Binviz
target/ff9-names-world-streaming-weather-sound/. All74 affected complete native
object pairs identical, unchanged pinned strict-relocation scores66exact/8partial,
30308/38616 code bytes, zero failures. All74 installed WASM preprocessed token
comparisons agree. Current catalog/source/header/object hashes and78 isolated
committed paths verify. No new matching credit, portable module, linked image,
selected runtime provider, shared workspace acceptance or gameplay proof.

Producer/consumer review resolves previously generic effect-pool descriptions:
32-node coarse-cell cache with16 mesh buffers;40-node visible fine-cell mesh
pool with20 position payloads;6-node/3-buffer texture data cache and4-node/
2-upload-slot pool. Names cover streaming target/window, row/column sector
descriptors, contiguous CD scatter reads and completion, terrain height,
visible mesh gathering/binding/drawing, texture requests/read-done/uploads,
weather region/mode/fog/blend and music requests/play/stop/volume wrappers.
Three actual maintained shared Binviz captures of Weather, w_music and Texture
strings on the full08C000 original container corroborate exact diagnostics.
CLI545beb01...44b6e, original image, overlay config and outputs all hash-pinned.
Full canonical08D800 sound-command body corroborates protocol; external code
and equal addresses do not establish selected runtime-provider correspondence.

Preserve one-step coordinate wrap, negative z interval, fixed13/fixed14 cells,
hardcoded24-row read stride, unchecked10-entry chains and slot bounds, linked
list compaction behavior, field/signature mismatches, compiler flags and pins.
BBBA8 keeps uninitialized k on no-match;BC528 is a group index, not the count
claimed in comments. BBE24 transforms wrapped relative coordinates and range
gates; it does not itself project screen pixels. C24D8 writes descriptor ob+id
despite its comment claiming id+60. Weather keeps mode5/flag2 sentinels,
invalid-mode side effects, region precedence and alignment-dependent94-byte
copies; sound keeps masked arguments, halved flags and uninitialized copied
request words. Foreign8574 ABI work remains completely untouched.

Maintained naming/farm/scoring/preprocessor workflow reused; no private analysis
walker added. Thin batch adapter and audit enforce the previously flagged BV-08
exact object namespace: no missing/extra objects; score count equals74 records.
Shared readability acceptance remains proposed until frozen namespaces and
compiler/input/output identities are configured and independently verified.
Refresh downstream source-bound evidence using the current catalog; keep the
historical scorer selection, foreign work/index entries and CLAUDE sections.

World UI/effects naming acceptance (BV-03/BV-08, P2): FF9 commit
97f906504 adds19 canonical ovl_09b800 behavioral names. Current catalog3450
unique unit/symbol names,51 alias headers. All149 world-map canonical primary
bodies reviewed cumulatively:127 named,22 documented purpose deferrals,zero
unreviewed. Other canonical units remain pending; no whole-tree completion.
Evidence: FF9 docs/function-names-world-ui-effects.json and Binviz
target/ff9-names-world-ui-effects/. All25 affected complete native object pairs
identical, scoring namespace exactly25 manifests, unchanged pinned strict-reloc
scores16exact/9partial,12960/33132 code bytes,zero failures. All25 installed WASM
preprocessed token comparisons agree. Current catalog/source/header/object hashes,
149 reviewed source identities,prior reports and29 isolated committed paths audit.

Names cover cursor motion, location visibility, world-to-map coordinates,
full-map/minimap compasses, window/auxiliary panels, banner fade, sprite effect
spawn/step/release, weather model rendering/event spawning, both model-part
packet submission routines and animation bone matrices. Re-reviewed8574 after
the foreign geometry ABI correction was committed; that three-argument64E88
baseline remains intact. Native tests grant no new original matching credit.
Preserve unchecked indexing, sequential cursor attraction, different marker
skip lists, sine/cosine choices, fade progression side effect, type23 fallthrough,
biased random jitter, effect-kind versus weather-mode distinction, GTE flags/pins,
typed FT3 depth shift-before-store and four-to-two argument frame call mismatch.
Unknown hooks/flag meanings stay explicit deferrals; donor comments and equal
addresses do not establish a selected runtime provider.

Maintained naming/farm/scoring/preprocessor workflow reused with thin adapter;
no private ownership/type/CFG walker or tooling rewrite added. Previously flagged
BV-08 exact object namespace acceptance is enforced again. Shared readability
acceptance remains proposed until frozen namespaces and compiler/input/output
identities are configured and independently verified. No compiled portable
module,linked image,gameplay,selected-provider or shared workspace acceptance
claim. Refresh downstream source-bound evidence using the current catalog,
retain historical scorer and preserve foreign work/index/checkpoint sections.

World scene/camera/sound naming acceptance (BV-03/BV-08, P2): FF9
9775afbaa adds72 canonical ovl_08d800 behavioral names;catalog3522 unique
unit/symbol names,52 alias headers. Review81/109 whole primary bodies in this
unit:72 named,9 documented deferrals,28 unreviewed. Other units remain pending.
Evidence: docs/function-names-world-scene-camera-sound.json and Binviz
target/ff9-names-world-scene-camera-sound/. All83 affected complete native
object pairs identical;exact manifest/scoring namespace83;unchanged pinned
strict-relocation scores61exact/22partial,15564/41848 code bytes,zero failures.
All83 installed WASM preprocessed token comparisons agree. Current catalog,
source/header/object and81 reviewed source bindings audit;87 isolated committed
paths agree. No new original matching credit or whole-tree completion claim.

Actual producers/consumers correct misleading comments: AA15C creates model
objects despite a per-frame callback comment;AE280/AE4C8 produce camera
projection/orbit parameters rather than backdrop lighting;B01F0 eases actor
y/terrain offset rather than list-row scroll. 8D78/9A44/9B54 stage and restore
sound-bank data through VRAM, without guessed portrait asset names. Names cover
scene lifecycle and resource loading, model/animation lookup and flat grid
effect packets,music/sound/stream commands and bank chunks,frame presentation,
exit/pause transitions,camera target/eye/follow/look-at/parameter blending,
actor/palette/terrain updates and analog/digital axis readers.

Preserve source signatures/callback casts and argument mismatches,missing lookup
outputs,linked-id=-1 search,AA39C first-row re-test bug,third grid quad's three
depth corners,available-sound ramps returning-1 on success,unsigned song timer,
asymmetrical camera easing,terrain/exclusion clamps,packed signed offsets,
first-record music selection,repeated actor queries and digital-button precedence.
Unknown hooks/camera flag meanings stay explicit deferrals. External canonical
bodies/equal addresses do not establish selected runtime providers.

Maintained naming/farm/scoring/preprocessor interfaces reused;thin batch/audit
adapter only,no private ownership/type/CFG walker or tooling rewrite added.
Previously flagged BV-08 exact object namespace gate is enforced again. Shared
readability acceptance remains proposed until frozen namespaces and actual
compiler/input/output identities are configured and independently verified.
No portable module,linked image,gameplay,selected-provider or shared workspace
acceptance. Refresh source-bound downstream evidence through current catalog;
retain historical scorer and preserve foreign index/work/checkpoint sections.
