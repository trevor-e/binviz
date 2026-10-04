# Reusable decompilation tooling for binviz

Status: implementation handoff and living backlog, 2026-10-03. This spec records gaps observed while
porting FF9 disc 1. It proposes reusable capabilities; it does not authorize ABI
exceptions or establish game correctness. The FF9 goal continues separately.

## Handoff summary

Use [the concise implementation handoff](decompilation-tooling-handoff.md) for
delivery order and a prompt for another session; this document supplies the
detailed records, observed cases and acceptance requirements.

Follow-up adoption regressions: see the handoff section **Adoption findings:
statement context and proof-derived explanations**. Its two retained switch-label
void-call specimens require a maintained compiler-facts fix, exact span/declaration
preservation, immutable old facts and dependency invalidation. Scope explanations
must use structured certificate/audit references and distinguish physical kills,
not-consumed/may-write summaries and explicitly discarded return boundaries.
Reject missing transitive witnesses; do not add a private game analyzer.

Further retained adoption specimens are in handoff **Adoption findings: computed
branch targets and composed edits**. Require content-bound indirect-target
certificate parity, branch-delay capture semantics and complete transitive
dependencies. For multi-policy edits require one pristine validation buffer,
explicit transformation stages and full selected-caller consumer validation;
preserve the sr6 GPU drift rejection as a negative composition control.

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

The rebuilt-tool adoption supplies a second concrete fixture: Clang14 leaves
tentative asm-labelled `$8`/`$17` register declarations as undefined DATA even
with `-fcommon`; `--allow-undefined` collapses their references to address zero.
The genuine GPU native-instruction campaign detects corrupted saved-coordinate
state. Explicit initialized definitions give distinct verified linked locations.
Acceptance must retain the failing tentative specimen, detect missing required
DATA allocation/exports and accidental required-location aliasing, and accept
actual allocated definitions or a separately reviewed external-memory mapping.
Function-type inspection alone currently reports no problem for the bad module;
that observation must not establish complete link/provider closure.

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
refusals. The subsequent installable game adapter passed six acceptance cases
and 105 scope/drift refusals and is installed in the game builder. A full
file12 replay moved from 214 accepted/81 rejected to 215 accepted/80 rejected;
two remaining callers now expose independent argument/printf blockers. Shared
Binviz service-selection and campaign migration remain pending. Historical
evidence and the current adapter are distinct inputs; do not overwrite the
frozen baseline or infer native device-timing equivalence from these results.

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
used-return closure, cross-register callee contracts, external loop bounds,
IRQ/recursive stack upper bounds and automatic compiler allocation/lifetime
extraction remain open explicit frontiers.

Reproduction and acceptance tests are documented in
[workspaces](decompilation-workspaces.md), [build batches](build-batches.md),
[proof campaigns](proof-campaigns.md) and [progress images](progress-images.md).
No legacy FF9 helper is retired, and no game-source change or matching credit is
claimed from these synthetic demonstrations. Compare frozen game collectors before
migration; retain the concrete gap records above as acceptance targets.

The geometry mixed kill/preserve gap now has shared single-GPR certificates:
exact current native bytes are reaudited, killed and discarded-return endpoints
stay separate, and `not-consumed-may-write` conservatively carries the word through
caller continuation. Current acyclic child certificates compose only with their
full native/review dependency closure. Synthetic tests cover mixed returns,
composition, a later caller read, stale reviews and recursive-proof refusal.
CLI `workspace --callees`, MCP `callee_certificates` and the browser expose the
same proofs. Cross-register/callback contracts and game migration remain open.

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

### Intrinsic identities and unrelated-header invalidation (2026-10-03)

BV-01, priority P0 for complete caller explanations: the maintained compiler
adapter excludes implicit FunctionDecl records, then fails to resolve a direct
reference to such a declaration. The resulting extraction gap contains a
transient Clang AST ID. In the FF9 provider-ranking batch, this is
`__builtin_trap()` inside `asm_add`/`asm_sub`, not an unknown physical call target.
Successful call observations still have real callee names. Local evidence is
`ff9-decomp/build/agent-file12-provider-ranking/facts.json` and its prepared
artifacts; the 81 failed game callers themselves have no extraction gaps.

Classify compiler intrinsics explicitly, retain their stable named identities,
target/profile and source spans, and describe traps as effects. Do not invent
native provider addresses or erase exception behavior. Acceptance: synthetic
overflow guards using implicit `__builtin_trap` declarations have inspectable
intrinsic records and exception outcomes; unsupported intrinsics remain named
gaps with concrete reasons. A transient AST ID may be diagnostic provenance,
but cannot become a shared declaration/call identity. Extend the existing
maintained adapter and shared compiler-facts schema; no second parser is needed.

BV-03/BV-08, priority P1 for legacy collector migration: adding the unrelated
`include/options-menu-names.h` changed the game's compiler fingerprint because
`typed-abi.py::_fingerprint` hashes every top-level include/wasm header. Three
CD discovery records changed only their cache key, despite identical native
bytes, C, CPP, raw objects and actual recorded dependencies. Root reproduced
both exact keys by including/excluding that one unused header, preserved old
metadata and separately repeated the scope/refusal gates. Evidence is
`ff9-decomp/build/root-file12-integration/cd-discovery-profile-transition.json`.

The shared extraction/build stages should key on compiler/target, producer
version, actual preprocessing inputs and relevant include-name resolution.
Adding an unrelated header must reuse facts for an already prepared input and
must not invalidate a source job that never requests that name. Adding a header
that shadows an actually requested include still invalidates its consumers.
Acceptance compares both cases, displays the precise reason, and measures
compiler subprocesses as well as elapsed time. Do not relax stale-output gates;
replace the legacy global-header key through a demonstrated migration.

Exception-order acceptance additionally needs structured WASM control-flow
witnesses. A trap block can physically follow the normal return while a guard
branches to it before visible stores. Physical disassembly order alone cannot
prove execution order; combine existing Binviz branch evidence with the native
BREAK/candidate campaign's pre-exception memory comparison.

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

### Follow-on implementation checkpoint (2026-10-03)

The shared implementation now exposes the remaining follow-on workflows:
`matchingPublications`, `readabilityBatches`, and `proofClosures` in the workspace,
corresponding CLI/MCP inspections, and the Contracts report panels. Scoped notes
publication independently recompiles no code: it requires current successful
object producers, recomputes strict exact original-code matches and writes only
the reviewed selection through the existing journal. Outside-scope notes retain
their prior values without a current-validation claim.

Compiler-visible build namespaces support actual complete-object readability
acceptance; canonical prepared names/ABI and original-code scores remain fixed,
including partial matches. Optional actual LLVM IR records storage/lifetime
instructions with explicit unsupported extents. Named intrinsic/trap effects are
retained in compiler facts instead of becoming unresolved physical providers.

Campaign roles support baseline/candidate modules as well as native/WASM pairs.
Exception records compare mapped trap identity and required exception-point
checkpoint, memory/registers and selected ordered effects without fabricating a
return. The executed fixture includes actual WASM unreachable traps and native
BEQ/delay-slot/BREAK execution through the existing independent CPU adapter.
An early cursor store fails both two-WASM and native/WASM comparisons.

Reviewed closure claims bind actual service imports/signatures/call edges,
shared-memory instance limits and instantiation order, exact live-memory
restoration, scalar variadic home prefixes, missing-input candidates and finite
callback/interrupt stack models. Actual compatible table initializers constrain
callback targets. Dynamic or branching frames need path-sensitive evidence and
refuse the current constant-frame profile. Cross-register child facts and reviewed
external loop bounds reuse the original native auditor and retain all witnesses,
byte identities, dependency scope and explicit assumptions.

Acceptance/refusal evidence is in `crates/binviz/tests/{closures,publication,
workspace,campaign}.rs`, `tests/tools/test_{compiler_facts,build_batch,campaign}.py`
and the tracked compiler-generated acceptance fixtures. Build/campaign logs are
under `target/`. The frozen FF9 migration comparison agrees on six native audits,
six selected module hashes and the zero/six-word CD imports (13 checks), retaining
the CLI/input digests in `target/ff9-shared-migration/migration-report.json`.
This demonstrates the shared collector connections; it does not retire collectors
or establish current source, linked/gameplay or scheduler equivalence from
historical observations. No game checkout writes were made.

### Concurrent source work and immutable runtime batches (2026-10-03)

BV-03/BV-08, priority P1 for migration: runtime integration and source naming
currently share a mutable FF9 tree. A captured 79-source file12 naming stage
preserved all 295 prepared CPP files, plain objects, discovery CPP files and
discovery objects at their original compiler-visible paths. Public preparation
also reproduced all 295 unchanged CPP/object outputs. Before integration could
finish, a later naming stage changed another 37 live sources, including an
installed formatter caller and three preparation gates. Correct source-bound
checks then refused the newer inputs. Repeating the whole reconciliation after
each naming batch wastes work despite valid evidence for the captured stage.

Local evidence: `ff9-decomp/build/file12-naming-transition/frozen.json`,
`compiler-transition.json`, `discovery-transition.json` and
`live-drift-refusal.json`; independent public preparation and selected-provider
refreshes are in `ff9-decomp/build/root-formatter-integration/`. The selected
BOOT refresh rebuilt 161 definitions with zero raw/LTO object changes. These
are source-stage transition observations, not a newly linked gameplay result.

Extend and consume the existing `build-batch` frozen inputs, include snapshots,
stable compiler-visible namespaces, stage identities and promotion plans. Do
not add another snapshot walker or generic build queue. A game-owned adapter
must declare its complete source/header/preparation/policy/provider/tool inputs
and run the maintained producers against that captured namespace. Keep valid
snapshot results inspectable while separately reporting whether the live tree
can consume them. Capture include files using the compiler's emitted dependency
paths, including relative paths; do not silently omit them because they lack
an absolute workspace prefix.

Current connection gap: `build-batch` freezes individual inputs into stable
slot directories and runs the configured command from that stage directory.
FF9's maintained builder derives its root from `wasm/runtime/` and opens
repository-relative source, policy and cache paths. Equal baseline/candidate
slot paths therefore do not reproduce its original `/work` tree. Support a
declared relative input layout and an explicit execution-root/mount mapping
through the existing stage model. Reject path escapes, undeclared inputs and
overlapping mutable mounts. A game-owned launch adapter may invoke the existing
compiler container against a captured tree; it must not rewrite semantic
builders or grant a copied cache record current authority without validating
its recorded producer/input/output lineage. This layout connection is proposed;
the existing flat-slot interface does not demonstrate full FF9 builder migration.

Acceptance: naming-only edits during a batch cannot contaminate its inputs;
historical proof records remain valid for their captured stage, while live
publication refuses drift. A reviewed subsequent transition compares actual
CPP, complete objects and definition ABIs before replacing exactly the affected
identities. A semantic edit such as FF9's B7F20 pre-exception trap correction
must remain distinguishable from an alias rename even if both match the native
object. Preserve compiler-visible paths and ordered exception effects. Measure
cold/warm wall time, compiler subprocesses, reused stages and refused promotions.

Migration status: existing Binviz batch facilities are the starting point;
FF9's runtime builders still use private reconciliation adapters. None has been
retired. The formatter bundle is now installed in the game builder and its
installed host reproduces the frozen production module and comparisons, but
later live naming inputs remain refused. The earlier formatter paragraph's
unintegrated status describes its historical private package. No fresh full
file12 module, matching percentage or playable-game claim follows from this
installation.

### Native call identity versus selected linkage symbol (2026-10-03)

BV-01/BV-02/BV-06, priority P1 for adapter migration: the file12 GPU integration
cannot link a true one-word `sub_800130a4` definition alongside the retained
twenty-word HLE alias with that same symbol. The scoped software provider needs
a fresh linkage name while its call findings retain the original native callee,
physical site and source spans. VSync likewise has a true one-word helper and
a retained twenty-word resolver entry; both import `hle.VSync`, with distinct
one-word and six-word signatures. Root's private frame candidate demonstrates
18 actual resolver/helper frame-history pairs without replacing the legacy map.

The maintained game adapter accepts a callee-keyed definition table, but lacks
an explicit selected linkage target and a public complete parsed-reference
inventory. The temporary GPU action therefore consumes that adapter's existing
declaration/direct-call spans and rejects unlisted address or callback tokens.
Evidence: `ff9-decomp/build/root-file12-gpu-current-review/` and
`build/root-file12-frame-platform/{compiled.json,proof.json,input-basis.json}`.
These are private configured candidates, not migrated Binviz callers.

Expose native identity, provider identity, emitted linkage symbol and import
module/field/signature separately in the shared bridge plan. Return declaration,
direct-reference and address-reference spans with completeness and revision
evidence; reuse maintained compiler facts instead of scanning another AST.
Acceptance keeps the true 1/2/2 GPU helpers and one-word frame helper separate
from the existing dispatcher entries, preserves full result words and expression
evaluation, and refuses an unlisted token/site, conflicting helper, callback
reference or changed prepared input. Applying a plan must report every changed
span and actual selected definition; a global callee-name replacement is not
equivalent to an exact caller/site binding.

Rebuilt-runtime migration check (2026-10-03): the shared `CallBinding` already
separates the original callee from the selected definition/export, and the
adapter emits `definition.name`. Do not implement that identity split again.
The remaining BV-06 gap is executable lowering: `workspace` can assess a
`discarded-result` policy, but `adapters::plan` accepts only extra-word nonuse,
scalar varargs and missing inputs, and refuses unequal result types even for an
actually discarded result. This prevents retirement of the file12 GPU linkage
adapter for its reviewed void-to-integer/pointer service sites.

Add a plan for eligible discarded-result policies using the exact compiler call,
selected body, native V0 correspondence and existing scope evidence. Preserve
argument evaluation and any observable call effects. Reject consumed results,
unreviewed conversions, stored function addresses, changed source spans and
missing native/provider correspondence. Exercise the actual 31-site GPU fixture,
report supported and refused sites individually, then compile/link/replay the
candidate before declaring the private helper retired. Passing the current
13-check register/module migration fixture does not establish this lowering.

There is also no current plan path for a verified direct selected-service binding
that needs only a different linkage name. Thirteen original VSync sites already
supply the true single parameter and return `int`; an extra-word policy correctly
refuses them because no extra word exists. AC5C8 supplies two words and discards
its original declared `void` result, so it also needs the result lowering above.
Provide an exact source-span binding action for an independently verified selected
service/body without inventing an argument exception. Acceptance retains the
original native identity, verifies the actual selected helper and service-chain
execution obligations, and refuses extra/address/indirect references. Keep this
identity-only action separate from claims about argument or result semantics.

The fresh GPU inventory makes the split concrete: all 31 reviewed calls discard
their result and supply the true formal count. Ten DrawSync calls already have
matching `int` results; 21 additionally differ (15 DrawSync `void` to `int`, five
StoreImage `void` to `int`, one ClearOTagR `void` to `u32 *`). All 31 need the
direct binding representation, and those 21 also need discarded-result lowering.
Do not use extra-word policies for any of these equal-arity calls.

### Genuine void execution records (BV-07, 2026-10-03)

The new shared exception campaign reproduces FF9's real B7F20 BREAK checkpoint
without inventing a return. Ordinary executed records still require
`returnWord`, however, while genuine item/marker C/WASM entries return `void`.
The remaining 314 histories therefore keep their original comparator rather than
serialize a fabricated zero. Exception support does not close this case.

Add an explicit, independently compiler/linkage-verified void result profile.
Require actual execution/return evidence and compare selected RAM, registers,
ordered provider events and checkpoints as usual. Keep arbitrary original MIPS
V0 available as diagnostic register state without treating it as a C return.
Word/getter profiles must continue to require actual full return bits; refuse an
unverified void declaration, a missing result for a word provider or any attempt
to evade used-return checks. Exercise real void item/marker entries alongside
word-return getters and wrong-memory/event negative controls before retiring the
legacy comparator for additional histories.

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

### Actual LoadImage compiler/ownership fixture (BV-04/BV-05)

The rebuilt tool's seven frozen FF9 file12 LoadImage units retain twelve Clang
call expressions but eleven native JALs. B8DC0 has five C expressions and four
native calls because two control paths share800B9024. Future selected-provider
plans must preserve this many-to-one physical correspondence and verify actual
linked call identities; do not pair occurrences by ordinal. Concrete shared
compiler/native/storage evidence is in `target/ff9-file12-loadimage-next/`.

Canonical typedef expansion currently re-expands same-named struct tags:
`typedef struct RECT ... RECT` produces `struct struct ... RECT *` in real Clang
facts. This is a Binviz producer defect, not a game ABI correction. Preserve raw
spellings and use compiler identities or tag-aware cycle-safe expansion. Require
idempotent results for same-name typedef/tag, chained aliases, qualifiers and
multiple pointer levels. Retain target/category/width observations and a distinct
adapter identity; old captured reports remain unchanged.

Actual storage extraction also demonstrates environment-key sensitivity: random
Docker HOSTNAME caused seven cold misses on a repeated invocation. The game fixes
the runner hostname and gets seven warm hits, zero AST/LLVM extraction processes,
and identical facts, while preparation/bootstrap still execute. An optional
narrower environment profile must declare and verify its relevant compiler inputs;
do not discard environment hashes implicitly for speed.

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


### Additional BV-01 acceptance: generated register-map dependencies and Windows roots

Use FF9's real refused strict-subset specimen, not a fabricated caller fixture.
The formatter generator consumes a project-wide fixed-register map through the
maintained asmgen.pinned_registers scan of src/include. Removing unrelated source
files changes p17 from real shared storage to a local initialized register and
changes generated source identity. The generator refuses its reviewed source seal.
A builder's direct DATA/name lookup returning an empty requested set does not
remove this independent transitive dependency. Generic pipeline registration must
bind directory-scan metadata and every contributing artifact, or bind an explicitly
produced fixed-register inventory whose input provenance is complete. Support
conservative dependency supersets without introducing a second private parser.
Require actual maintained generator output and downstream compiler parity before
accepting any narrower execution root. A pin removal/content change must invalidate
the affected generated-code stage; a source-only matching/naming waiver is insufficient.
Specimens: target/ff9-root-combined-strict-subset and its input/output captures;
corrected projection/parity is separate and must not retroactively relabel failure.

Long Windows configuration roots can make owned cache extraction paths exceed the
host path limit. Preserve setup errors separately from semantic/game compilation
results; report the offending path/phase. Provide a short workspace-owned staging
root or handle Windows extended paths consistently across all extraction/read/write
boundaries. Do not bypass ownership by accepting ../ cache directories and do not
change global host registry settings. Acceptance compares the same declared bytes/
recipe in long failing and short owned roots, preserving actual outputs, diagnostics,
compiler-visible namespace and cache-key rationale. FF9 target/ff9-sr2 is the short
root; setup refusals remain at target/ff9-root-combined-strict-subset-registers.

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


### Rebuilt-tool adoption specimens: exact snapshot and span identity

The adopted release545beb01f now executes shared build-batch, register-use,
workspace callee certificates, linked inspection and original/C campaigns.
Actual grouped replay target/ff9-sr3 accepts271/295 with24 explicit refusals;
all8 new batch artifact identities verify. Cold152.20561s, warm0.33860s with0
subprocesses. Frozen17-file manifest1c2455fae…1ea2a92b retains the failed final
producer and two verified outputs. This is subsystem compilation, not playability.

Two remaining repeatability gaps have concrete specimens for future work:

* Snapshot assembly must distinguish supplied input artifacts from regenerated
  completed output artifacts. ClearOTag, memcpy and Reset scopes bind discovery
  stamps from completed producer output. An old stamp in an input tar can differ
  while source/CPP/object bytes remain equal. Expose both artifact IDs, producer
  stage and exact digests; require an explicit transition when replacing a member.
  Never resolve a stale stamp by silently resealing an unrelated policy. The
  existing build-batch graph already preserves stages/digests; the missing piece
  is a reusable reviewed snapshot-overlay operation and declared producer scan
  or collection closure. Keep game recipes outside the shared analyzer.
* Call facts need explicit source-buffer identity and a mapping when compiler
  preprocessing changes whitespace. Reset AC5C8 call facts prepared span3971:3986
  and literal3984 refer to a different buffer than rawCPP3972:3987/literal3985.
  Existing maintained binder establishes raw `(1)`; B69B4 spans coincide. Display
  both buffers/stage IDs and mapped ranges, or refuse a cross-buffer rewrite.
  Acceptance must include a removed blank line, coincident spans, changed source
  and unmappable ranges. A blanket offset adjustment is insufficient.

The sr3 workspace also retains25 stale inherited records for5 live BOOT inputs
and their dependents. These coexist with8 verified immutable build artifacts.
Queries/UI should clearly scope summaries to the requested stage/artifact set;
never label the entire workspace clean because a selected build stage verifies.
The same stale records exist in sr2. Preserve their rejection reasons.

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

World movement/rendering naming acceptance (BV-03/BV-08, P2): FF9
e0cf9aa1a adds31 canonical ovl_08d800 behavioral names; catalog3553 unique
unit/symbol names,53 alias headers. All109 primary bodies in this unit are
cumulatively reviewed:103 named,six empty/zero-return hook roles deferred.
This batch reviews28 previously unreviewed primaries plus three resolved
deferrals; prior81 source bindings independently verified before snapshots.
Other units remain pending. Evidence: docs/function-names-world-movement-rendering.json
and Binviz target/ff9-names-world-movement-rendering/.

All33 complete native object pairs are identical; exact manifest/scoring
namespace33. Pinned strict-relocation scores unchanged:24exact/nine partial,
16600/28500 code bytes,zero failures. All33 installed WASM preprocessed token
comparisons agree. Current catalog/source/header/object/review bindings audit;
37 isolated committed paths agree. No new original matching credit.

Actual movement consumers resolve AE828 as camera heading-follow toggle and
B35F4/B3638 as start/cancel travel to the selected map destination. Names also
cover visible actor rendering, height-based palette shading, actor model and
ground effects, terrain collision probes/walkable-position search, controlled
actor mode changes, movement input dispatch and three input modes, camera
heading easing, projected-point visibility and palette depth cueing.

Eleven raw rendering primaries expose public descriptive C aliases only; raw
assembly strings are independently compared, every label/instruction/register
convention/delay slot stays canonical, and leading asm marker stays first.
B3844/B3998 retain existing conditional C/default assembly selection. Actual
vertex stages distinguish bent/unbent Y, wide/compact depth formats, single/dual
depth-color streams and GTE/software bend. Triangle emitters distinguish fixed
depth, mip UV adjustment, quantized depth selection and compact semitransparent
or layered packets. Renderer caller pairings corroborate stage roles without
proving cross-overlay runtime provider identity; avoid guessed asset subtypes.

Preserve bitwise camera flag complement, overwritten packed ground-effect word,
mutated collision fallback vector, early-return query flag, pre-null dereference,
unchecked indexes, signature mismatches, negative-speed rescaling, reversed-looking
analog gate, asymmetric clamp, terrain RNG effects, sound ramps and every raw
packet/depth/texture detail. Naming makes these inspectable without fixing them.

Maintained naming/farm/scoring/preprocessor interfaces and existing thin raw
snapshot adapter reused; no new ownership/type/CFG walker or tooling rewrite.
Previously flagged BV-08 exact object namespace gate is enforced again; no new
reusable tooling gap found. Shared readability acceptance remains proposed until
frozen namespaces and actual compiler/input/output identities are independently
verified. No portable module,linked image,gameplay,selected-provider or shared
workspace acceptance. Refresh source-bound downstream evidence from the current
catalog; historical batch reports/scorer stay pinned. Foreign work/index and
unrelated FF9 checkpoint sections remain preserved.

Blackjack/intro naming acceptance (BV-03/BV-08, P2): FF9 f4c0d7aaa
adds12 canonical ovl_0cc000 behavioral names; catalog3565 unique unit/symbol
names,54 alias headers. All33 recovered primary bodies in this unit reviewed;
21 deferrals cover nine empty/zero hooks,nine unknown byte accessors and three
timer records whose subsystem purpose is not established. Other units pending.
Evidence: docs/function-names-blackjack-intro.json and Binviz
target/ff9-names-blackjack-intro/. All12 complete native object pairs identical;
exact manifest/scoring namespace12,pinned strict-relocation scores unchanged:
10exact/two partial,988/3508 code bytes,zero failures. All12 installed WASM
preprocessed token comparisons agree. Current catalog/source/header/object and
all33 review bindings audit;16 isolated committed paths agree.

Recovered code contradicts old line/column/selection-marker comments: six
52-card rows,card/4 rank scoring2..10/face10/ace11or1,soft-ace adjustment and
two-card21 predicate establish blackjack hand/deck helpers. Card-face sprites
use suit texture columns,rank corners,pip layouts,picture ranks and central ace
pip. Preserve six-deck initialization without introducing shuffle;shoe threshold
249,22 stored-card words,numeric rank classes0/2/3,ace flag handling,sprite page
pool/CLUT/UV/tag writes,q+2 pip loop,matching do-while/typeof wrapper and distinct
fixed background sprite palette. No back-design or wager/player/coordinate guess.

Intro text/bit helpers measure encoded text,emit packed glyph sprites,resolve
byte-offset entries,find the lowest of four mask bits and test unmatched low
nibble bits. Retain one/two-byte encoding,FF terminator,duplicated width branch,
zero-mask index0,register pins/asm barriers,packet masks/advance/optional output.
Shared header/transplant comments are reviewed evidence,not semantic authority.
Byte/timer roles stay deferred without opaque address-shaped replacement names.

Maintained naming/farm/scoring/preprocessor workflow and thin audit adapters
reused;no new ownership/type/CFG walker or tooling rewrite. Previously flagged
BV-08 exact namespace gate enforced again;no new reusable tooling gap found.
No new matching credit,portable module,linked image,gameplay,selected-provider
or shared workspace acceptance. Refresh source-bound downstream evidence from
current catalog;historical reports/scorer remain pinned. Foreign work/index and
unrelated checkpoint sections preserved;full-tree review remains active.

World module geometry naming acceptance (BV-03/BV-08, P2): FF9
ab1bd0ef7 adds34 canonical ovl_08c000 names;catalog3599 unique unit/symbol
names,55 alias headers. All42 primaries covered:31 direct complete body reviews
and11 entire raw bodies independently token-equal pinned previously reviewed
sources;34 named,eight explicit semantic deferrals. Other units remain pending.
Evidence: docs/function-names-world-module-geometry.json and Binviz
target/ff9-names-world-module-geometry/. All34 native object pairs identical;
exact affected manifest/scoring namespace34,pinned strict-relocation scores
unchanged29exact/five partial,18092/27096 code bytes,zero failures. All34 installed
WASM preprocessed token comparisons agree. Current catalog/source/header/object
and review/donor bindings audit;38 isolated committed paths agree.

Names cover spatial pan/volume,sound pack links,camera transition/target/eye/
axis easing,actor reset/auto travel,mesh staging/group ranges,fixed interpolation,
model relocation/arena carving,paired packet templates,skinned vertex projection,
scaling/behind-camera flags,height-clamped projection and polygon linking,
depth sorting and raw bent/mipped/layered triangle stages. Actual pinned consumers
establish roles without address-only/transplant-comment inference or runtime
provider claims. Eleven raw bodies match all tokens after removal of the donor
naming include;B6598 freshly reviewed with screen-bit rejection,NCLIP,full depth
sorting and mip UV/page/CLUT adjustment. Twelve native-only aliases do not claim
portable C recovery. Original provenance prefix and asm marker positions stay.

Preserve matrix/texture/packet layouts,zero/unchecked count behavior,triplet
read-ahead,8-bit part masking,signed parent flags,volatile dead flags store,
register pins/barriers/compiler flags,two-sided tests,packet masks/depth bias,
clamped depth flags,culling order and FT3 dz shift before GTE store in C8950.
No behavioral fixes or new original matching credit. Constant-address getters,
capability predicates and opaque byte/empty hooks retain explicit deferrals.

Reusable gap BV-03/P2: raw alias recognition must accept leading provenance
comments before the asm marker. Current legacy wide.prepare skips12 such raw
files in ovl_08c000 as already descriptive despite canonical assembler labels.
Temporary thin adapter in target/ff9-names-world-module-geometry/run.py merges
aliases and snapshots after the existing marker,preserves every prefix byte and
assembler string,and refuses unsnapshotted C references. Shared acceptance must
recognize canonical raw definitions through arbitrary leading comments without
moving classification markers or rewriting assembler strings;cover both plain
asm-first and provenance-first inputs,conditional C/asm B3844,reference closure,
header collision/identity,full object equality and installed preprocessing.
No shared implementation or broader acceptance claimed. Existing farm/scoring/
preprocessor tools reused;no private ownership/type/CFG walker added. BV-08 exact
namespace gate enforced. No portable module,link,gameplay,selected-provider or
shared workspace acceptance. Refresh source-bound evidence from current catalog;
historical reports/scorer stay pinned;foreign work/index/checkpoints preserved.

Title scene naming acceptance (BV-03/BV-08, P2): FF9 cf66ec7ef
adds five canonical ovl_0c7800 names;catalog3604 unique unit/symbol names,
56 alias headers. All15 recovered primary bodies reviewed;ten explicit semantic
deferrals cover opaque global bytes/word,timer record,work890 phase and empty
hook. Other units remain pending. Evidence:docs/function-names-title-scene.json
and Binviz target/ff9-names-title-scene/. All five complete native object pairs
identical,exact manifest/scoring namespace five,pinned strict-relocation scores
unchanged four exact/one partial,288/636 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree;current catalog/source/
header/object and15 review bindings audit;nine isolated committed paths agree.

Names cover scene archive range resolution,mixed scene resource tracking reset,
song slot reset and two low-nibble bit helpers. Actual pinned sound-command and
scene-resource consumers support shared field roles;cursor donor comments do
not supersede song id/playback handle/level operations. Preserve EXACT mask
0xbfffff,which clears more than bit22,reverse loops,unchecked directory ranges,
failed-search output behavior,register pins,matching wrapper and compiler flag.
No runtime-provider inference from equal addresses/offsets;no body corrections.

Existing naming/farm/scoring/preprocessor tools and thin audit adapters reused;
BV-08 exact namespace gate enforced,no new reusable tooling gap found. Commit
guard caught concurrent unrelated CLAUDE checkpoint edits;fresh inspected
snapshot and isolated index retained those working edits while committing only
our marked naming checkpoint. Foreign work/index remain preserved. No new
matching credit,portable module,linked image,gameplay or shared workspace
acceptance. Refresh source-bound evidence from current catalog;historical reports
and scorer stay pinned;full-tree review remains active.

Collection menu naming acceptance (BV-03/BV-08, P2): FF9 10a21cb87
adds28 canonical ovl_0e4000 behavioral names;catalog3632 unique unit/symbol
names,57 alias headers. All29 recovered primary bodies reviewed;one empty hook
deferred. Other units remain pending. Evidence:docs/function-names-collection-menu.json
and Binviz target/ff9-names-collection-menu/. All28 complete native object pairs
identical,exact affected manifest/scoring namespace28,pinned strict-relocation
scores unchanged28exact,7196/7196 code bytes,zero failures. All28 installed WASM
preprocessed token comparisons agree. Current catalog/source/header/object and
29 review bindings audit;32 isolated committed paths agree.

Names cover initialization,action/list input,selection,threshold lookup,save-mask
counts,help/cursors,panels/rows,frame/shadow primitives and tile VRAM staging/
restoration. Bound collection-menu roles to actual operations without guessing
the asset subtype or relying on old portrait/nine-slice labels. Preserve24-bit
masks,strict threshold<progress,selected index+1,zero-selection sentinel,list
tail base17,fade/busy/input priorities,threshold+1 icon row,redundant calls,
caller/definition arity/return/RECT-buffer mismatches and all matching spellings.
Atlas staging starts its save pointer one0x518-byte record before the source;
preserve negative offset,stride,sync ordering,mode3 selected-tile moves,packet
tag/CLUT/UV masks and four-frame/two-shadow loops. No behavioral fixes.

Existing naming/farm/scoring/preprocessor tools and thin proof/commit adapters
reused;BV-08 exact namespace gate enforced,no new reusable tooling gap found.
No ownership/type/CFG walker or tooling rewrite added. No new matching credit,
portable module,linked image,gameplay,selected-provider or shared workspace
acceptance. Refresh source-bound downstream evidence from current catalog;
historical reports/scorer stay pinned;foreign files/index and unrelated checkpoints
preserved;full-tree review remains active.

Movie API naming acceptance (BV-03/BV-08, P2): FF9 313472bcc
adds12 canonical ovl_147000 behavioral names;catalog3644 unique unit/symbol
names,58 alias headers. All13 recovered primary bodies reviewed;one volatile
mailbox deferred because its consumer/limit purpose is unresolved. Other units
remain pending. Evidence:docs/function-names-movie-api.json and Binviz
target/ff9-names-movie-api/. All12 complete native object pairs identical;
exact affected manifest/scoring namespace12,pinned strict-relocation scores
unchanged12exact,1012/1012 code bytes,zero failures. All12 installed WASM
preprocessed token comparisons agree;current catalog/source/header/object and
all13 review plus actual consumer bindings audit;16 isolated committed paths agree.

Names cover decoder buffer state clearing,frame counter,delayed sound event,
stop callback,display-record buffer switch/copy,display center,metadata counts,
start CD position,current ring record/state,free frame-slot accumulation and
MDEC decode-table copy. Complete pinned movie pump/finish/path/ring/MDEC and
BOOT sound-command bodies establish roles. Older finished/played flag comment
does not define a completion metric:1000bit is set by initialization and slot
release;8000 marks fetch-ready. Accumulator is deliberately not initialized.
Preserve signed flag/count results,int callback setter signature,negative defaults,
event postincrement/gating,first-frame output gates,uncopied padding,fourth CD
position byte,state-bit precedence and untouched result when no state matches,
2292-byte payload stride,16+16+32 table loops andmno-split-addresses flag.
Opaque volatile mailbox retainsgcc2.7.2.3 and$8pin without semantic guess.

Maintained naming/farm/scoring/preprocessor tools and thin adapters reused;
BV-08 exact namespace gate enforced,no new reusable tooling gap found. No private
ownership/type/CFG walker or tooling rewrite. Native aliases remain unit-scoped;
equal addresses/fields do not assert selected provider identity. No new matching
credit,portable module,link,gameplay or shared workspace acceptance. Refresh
source-bound evidence from current catalog;historical reports/scorer stay pinned;
foreign work/index/checkpoints preserved;full-tree review remains active.

Main menu naming acceptance (BV-03/BV-08, P2): FF9 1c74df50c
adds25 canonical ovl_0af800 names;catalog3669 unique unit/symbol names,
59 alias headers. All25 complete canonical primary bodies reviewed and named,
including every instruction of one native-only raw sprite packet builder.
Other units remain pending. Evidence:docs/function-names-main-menu.json and
Binviz target/ff9-names-main-menu/. All25 complete native object pairs identical;
exact affected manifest/scoring namespace25,pinned strict-relocation scores
unchanged24exact/one partial,6492/7148 code bytes,zero failures. All25 installed
WASM preprocessed token comparisons agree;current catalog/source/header/object
and review/consumer bindings audit;29 isolated committed paths agree.

Names cover page/party input,reordering,portrait VRAM swaps,status/caption/time/
gil panels,help substitutions,cursors,label position tweens,resource image uploads
and sprite animation/frame packet stages. Actual tween and level/equipment/gil
consumers correct misleading comments:label records hold position scripts,not
strings;time/gil panel does not display level;level progress helper prepares
text substitutions rather than directly drawing a gauge. Same-slot party selection
toggles bit9 without naming it locked;unequal selection swaps actual party pointers
and portraits. Preserve global indices,extra caller arguments,all signature
mismatches,MP color division,level99 zero remaining,five item ids andFFsentinel,
script pointer offsetbase+14,sixteen-byte stride,matching pins/wrappers/barriers.

Raw sprite packet helper retains asm-first prefix,every assembler string,custom
register/stack/caller-area convention,delay slots,NCLIP-zero whole-loop exit,
RTPT/RTPS and depth>>3+bias ordering bounds. Native-only alias does not recover
a portable C body. Existing raw snapshot adapter reused. BV-08 exact namespace
gate enforced,no new reusable tooling gap found;no private ownership/type/CFG
walker or tooling rewrite. No new matching credit,portable module,link,gameplay,
provider or shared workspace acceptance. Refresh source-bound evidence from
current catalog;historical reports/scorer stay pinned;foreign work/index and
unrelated checkpoints preserved;full-tree review remains active.

Shared UI tween naming acceptance (BV-03/BV-08, P2): FF9 a3c2e765f
adds five canonical BOOT names;catalog3674 unique unit/symbol names,994 BOOT
names,60 alias headers. Six selected complete BOOT bodies reviewed;one opaque
menu-table status accessor deferred. Full BOOT/tree review remains pending.
Evidence:docs/function-names-ui-tweens.json and Binviz target/ff9-names-ui-tweens/.
Resident rename closure reaches48 canonical sources across ten units;all48
complete native object pairs identical,exact unit/function manifest-scoring
namespace48,pinned strict-relocation scores unchanged43exact/five partial,
10664/12420 code bytes,zero failures. All48 installed WASM preprocessed token
comparisons agree;current catalog/source/header/object and actual review/caller
bindings audit;52 isolated committed paths agree.

Names cover position tween start/step,signed-byte sum,menu finish with saved
fade stride and literal BIOS A0/15 strcat. Preserve script wait/index/halfword
wrap,signed delta sums,NULL axis pointers,callback with script argument before
detach,negative length zero-result behavior,optional back-position adjustment,
incompatible old-style declarations and all existing record layouts. BIOS entry
keeps four-wordu32 ABI,$9/$10 selector bindings,asm barrier,literal table/number
and host implementation ownership;repository library-map and actual formatted
number suffix callers support strcat without replacing it with host libc.
Menu-table status remains unnamed rather than generalized from one slot-picker
consumer. Six source bindings describe this fresh subset,not full BOOT coverage.

Maintained naming/farm/scoring/preprocessor tools and thin audit/commit adapters
reused;BV-08 exact unit/function namespace gate enforced across the whole closure.
No new reusable tooling gap found,no private ownership/type/CFG walker. No new
matching credit,portable module,link,gameplay,provider or shared workspace
acceptance. Refresh source-bound evidence from current catalog;historical reports
and scorer stay pinned;foreign work/index/checkpoints preserved;goal remains active.

Effect track naming acceptance (BV-03/BV-08, P2): FF9 98f2caa0b
adds30 canonical ovl_1233a800 names;catalog3704 unique unit/symbol names,
61 alias headers. All30 complete canonical primary bodies directly reviewed
and bounded effect/track/geometry behaviors named;original spell/enemy/ability
labels remain unresolved. Other units pending. Evidence:docs/function-names-effect-tracks.json
and Binviz target/ff9-names-effect-tracks/. All30 complete native object pairs
identical,exact affected manifest/scoring namespace30,pinned strict-relocation
scores unchanged27exact/three partial,4916/15224 code bytes,zero failures. All30
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object and30 review bindings audit;33 isolated changed commit paths and
34 HEAD paths verified;marked naming checkpoint was already present in priorHEAD.

Names cover four dynamic tracked-point callbacks,parent actor/grid/ribbon/fade/
spark handlers,keyframe initialization/fetches,scalar/grayscale interpolation,
position-event pool,ribbon vertices,texture-colored quad grid and wrapping strip.
Actual parent fills consecutive eight-byte tracked vectors and assigns each
callback to its corresponding object;these are dynamic positions,not constant
descriptors. Preserve volatile late stack limit,signed-halfword-to12-bit-fixed
shift,old-style arity mismatches,upper-only key clamps,mirrored X/Z,optional
paired outputs,script/opcode layout differences,zero-count/denominator hazards,
width division order and every matching compiler flag/pin/barrier/wrapper.

Event script cursor and played count advance inside each nonzero target nibble;
do not silently make that once per frame or auto-free completed slots. Preserve
exact channel start-time equality,64slot/four-clock lifecycle,last-triplet reuse,
resource/frame thresholds and partially initialized state. Quad grid tests
MAC0/NCLIP nonzero rather than the misleading depth-word comment;keep allocation
before loop tests,negative texture read-ahead,zero color borders,36-byte packets,
render-skip upload,unchecked texture divisor and two24-byte wrapping-strip slots.
No effect subtype/provider identity inferred from legacy transplant comments.

Existing naming/farm/scoring/preprocessor tools and thin audits reused;BV-08
namespace gate enforced. Concurrent commit guard caught a shared checkpoint
stage;fresh authoritative HEAD inspection showed the other completed commit
already contained the exact naming checkpoint. Thin commit adapter permits
CLAUDE absence from changed paths only when constructed bytes equal HEAD bytes;
all other owned paths remain exact andforeign work/index preserved. Reusable
BV-03/P2 acceptance edge: concurrent isolated batches may already have committed
a shared checkpoint;verify exact current HEAD bytes and real changed path set,
preserve foreign index and outside-marker content,and reject actual drift rather
than requiring an unchanged checkpoint to appear as a diff. No shared feature
implementation claimed,no private ownership/type/CFG walker. No new matching
credit,portable module,link,gameplay,provider or shared workspace acceptance.
Refresh source-bound evidence from current catalog;historical reports/scorer stay
pinned;full-tree goal remains active.

Scripted effect naming acceptance (BV-03/BV-08, P2): FF9 de5ff3604
adds23 canonical ovl_115e1800 names;current catalog3727 unique unit/symbol
names and62 alias headers. All23 complete primary bodies directly reviewed,
including four large handlers and own-unit keyframe/event consumers. No semantic
deferrals in this unit;other units pending. Evidence:docs/function-names-scripted-effects.json
and Binviz target/ff9-names-scripted-effects/. All23 complete native object pairs
identical,exact affected/scored namespace23,pinned strict-relocation baselines
unchanged20exact/three partial,7408/11320codebytes,zero failures. Installed WASM
preprocessed tokens identical for23sources. Current catalog/source/header/review/
object bindings and27isolated committed paths independently audit. Progress:
3727/5812canonicalprimaryfiles named64.1%,2085remaining.

Names cover actor/position-event sequences,keyframed host handles,ribbon sets,
object offsets/flash,position blend/spawn,screen fade,keyframe initialization/
fetches,scalar/grayscale interpolation and64slot/four-clock position events.
Original spell/ability and selected runtime-provider identities stay unresolved.
Direct body review overrides inaccurate comments when selecting names:8A10
releases handles atframe41,notframe1;9AA8 modifies cachedYto-3000 before its
blend,so does not establish original-position restoration. Preserve all comments,
matching flags,register pins,duplicate branches,assignment wrappers,types,
layouts,signatures,resources and frame thresholds unchanged.

Preserve opcode-skipping/count-at2 record layouts,upper-only clamps,unchecked
negative/zero-count/denominator cases,paired output order/NULL handling,halfword
bit patterns and sign extension,ribbon reverse indices/width division order,
position blend(target,current)weight order andscreen-fadehostargument2.
Event cursors and played counts advance pernonzero target nibble,not perframe;
retain exact start-time equality,silent full-pool drop,last-triplet reuse and
no automaticfree. Existing maintained naming/farm/scoring/preprocessor tools
andthin proof/commit adapters reused;BV-08namespace gate enforced. No additional
shared implementation gap discovered in this batch;existing raw-prefix and
concurrent-checkpoint acceptance proposals remain separate. No private ownership/
type/CFG walker,new matching credit,linked image,gameplay,provider or shared
workspace acceptance. Historical reports/scorer remain pinned;refresh downstream
source-bound evidence fromcurrentcatalog. Foreign work/index preserved.
Full-tree naming goal remains active.

Ribbon scene naming acceptance (BV-03/BV-08, P2): FF9 b92871c5e
adds 26 canonical ovl_11a96800 names. Current catalog: 3,753 unique unit/symbol
names and 63 alias headers. All 26 complete primary bodies were directly
reviewed, including every chunk of the two large handlers and the five-track
handler. All established behavioral roles are named; original spell/ability
labels and selected runtime providers remain unresolved. Adjacent ovl_11a99800
is outside this canonical review and alias scope. Other units remain pending.
Evidence: docs/function-names-ribbon-scenes.json and Binviz
target/ff9-names-ribbon-scenes/. All 26 complete native object pairs are identical.
The exact affected/scored namespace is 26; pinned strict-relocation baselines
remain 19 exact and seven partial, 4,932/21,248 code bytes, zero failures.
Installed WASM preprocessed tokens agree for all 26 sources. Current catalog,
source, header, review and object bindings and 30 isolated committed paths audit.
Progress: 3,753/5,812 canonical primary files named (64.6%), 2,059 remaining.

Names cover paired/blended/reverse/rotated ribbon builders, waypoint projection,
textured Gouraud ribbon quads, actor/ribbon/particle sequences, five-track handle
sequence, descending object orbits, position blend/trail spawn, keyframe helpers,
position-event scheduling and wrapping strips. Direct consumers establish track
layouts. Preserve upper-only clamps, optional paired outputs, signed narrowing,
eight-byte vertices, 12-byte projected rows, two RTPS operations, OTZ shifts,
conditional Z rotation and every assembler string, compiler flag and register pin.
Quad drawing reserves n packets but draws n-1; preserve 52-byte packet stores,
eight-segment UV cycles, fade boundaries, UV clamp and conditional ordering depth.

Handler review identified reasons to bound names more carefully than old comments:
89DC updates particles only in its explicit frame range and advances the strip
counter inside the five-object loop. A95C has an eight-argument screen call and
its first spawn uses previous track state rather than a fixed point. AFF4 can
decrement the same fade counter in two blocks during a single frame. C030 first
changes cached Y to -3000, so original-position restoration is not established.
Preserve all bodies and comments, partial initialization, resources, frame
thresholds, duplicate branches, assignment/do/typeof wrappers and host arities.

Keep target/current blend order, unchecked denominators and signed remainder,
64-slot/four-clock event lifecycle, exact start-time equality, per-target-nibble
cursor advance, last-triplet reuse and no automatic freeing. Existing maintained
naming/farm/scoring/preprocessor tools and thin proof/commit adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found in
this batch; existing raw-prefix and concurrent-checkpoint proposals stay separate.
No private ownership/type/CFG walker or new matching credit, linked image,
gameplay, provider or shared workspace acceptance. Historical reports/scorer
stay pinned; use current catalog hashes for downstream source-bound evidence.
Foreign work/index/checkpoints preserved. Full-tree naming goal remains active.

Ring ribbon naming acceptance (BV-03/BV-08, P2): FF9 c0c790a2b
adds 21 canonical ovl_13c58800 names; current catalog has 3,774 unique unit/symbol
names and 64 alias headers. All 21 complete primary bodies directly reviewed;
no semantic deferrals in this unit, other units pending. Evidence:
docs/function-names-ring-ribbons.json and Binviz target/ff9-names-ring-ribbons/.
All 21 complete native object pairs identical, exact affected/scored namespace21,
pinned strict-relocation baselines unchanged:17 exact/four partial,3,996/9,972
code bytes,zero failures. Installed WASM preprocessed tokens agree for21 sources.
Current catalog/source/header/review/object bindings and25 isolated commit paths
audit. Progress:3,774/5,812 canonical primary files named (64.9%),2,038 remaining.

Names cover projected waypoint/blended ribbons, GT4 quads, five-model position
tracks, projected-ribbon sequence, descending orbits, blend/trail spawn, position
event scheduler and keyframe/interpolation/strip helpers. The zero-state handler
returns a positive-frame completion predicate; no original effect subtype guessed.
The unused stream reader gets a generic operand-reading name rather than a
paired-track label without a local consumer. Explicit pair layout establishes
the paired fetcher. Direct body review rather than transplant comments supplies
evidence. Original spell/ability and selected runtime-provider identities unresolved.

Preserve all bodies/comments/layouts/types/signatures,matching flags/pins,assembly
strings,partial initialization,resources/frame boundaries,fan scale at8/A/C,
overlapping fade decrements,halfword counters,upper-only clamps,optional outputs,
unchecked counts/indices/denominators,target/current weight order,n packets
reserved versus n-1 drawn,UV/depth rules and48-byte wrapping strip reservation.
Event cursor advances per nonzero target nibble;retain exact start-time equality,
last-triplet reuse,silent full-pool drop andno automaticfree. Existing maintained
naming/farm/scoring/preprocessor tools andthin adapters reused;BV-08 namespace
gate enforced. No additional shared implementation gap found;existing proposals
remain separate. No private ownership/type/CFG walker,new matching credit,link,
gameplay,provider orshared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence fromcurrent catalog. Foreign work/index
preserved; full-tree goal remains active.

Object flash naming acceptance (BV-03/BV-08, P2): FF9 fec25c1d3
adds 21 canonical ovl_13c24000 names. Current catalog: 3,795 unique unit/symbol
names, 65 alias headers. All 21 complete primary bodies directly reviewed;
all established behavioral roles named, other units pending. Evidence:
docs/function-names-object-flashes.json and Binviz target/ff9-names-object-flashes/.
All 21 native object pairs identical; exact affected/scored namespace21.
Pinned strict-relocation baselines unchanged:20 exact/one partial,4,804/5,404
code bytes,zero failures. All21 installed WASM preprocessed token comparisons
agree. Current catalog/source/header/review/object bindings and25 isolated
committed paths audit. Progress:3,795/5,812 named (65.3%),2,017 remaining.

Names cover position events, object-offset/flash sequence, blend/spawn, four
keyframe layouts, interpolation and event-pool helpers. Both zero-state positive
frame handlers are retained as distinct canonical symbols. A copy suffix names
the second literal duplicate without inventing a separate game role; their
complete token streams were checked equal after changing only the identifier.
Four initializer roles follow complete own-unit consumers, rather than donor
comments. Original effect labels and selected runtime providers stay unresolved.

Preserve exact n-1 range/fetch thresholds, scalar changes at0/4, swapped fade
counters, scene offset restoration, unchecked mean-position division/scene
counts, partial initialization, resource IDs, old-style arities, all comments,
types/layouts/signatures, compiler flags and assignment/do/typeof wrappers.
Blend/spawn first changes cachedY to-3000; original-position restoration is not
established. Preserve target/current weight order, upper-only clamps, optional
paired outputs, signed scalar/vector extension and unchecked denominators.
Events retain64 slots/four clocks, exact start-time equality, per-target-nibble
cursor advance, last-triplet reuse, silent full-pool drop and no automaticfree.

Existing maintained naming/farm/scoring/preprocessor tools andthin adapters
reused; BV-08 exact namespace gate enforced. No additional shared implementation
gap found in this batch; existing proposals remain separate. No private
ownership/type/CFG walker, new matching credit, linked image, gameplay, selected
provider or shared workspace acceptance. Historical reports/scorer stay pinned;
refresh downstream source-bound evidence fromcurrent catalog. Foreign work,
index and outside-marker checkpoints preserved; full-tree goal remains active.

Spark burst naming acceptance (BV-03/BV-08, P2): FF9 cebc408be adds
22 canonical ovl_13640000 names; current catalog has3,817 unique unit/symbol
names and66 alias headers. All22 complete primary bodies directly reviewed;
no semantic deferrals in this unit, other units pending. Evidence:
docs/function-names-spark-burst.json and Binviz target/ff9-names-spark-burst/.
All22 complete native object pairs identical; exact affected/scored namespace22;
pinned strict-relocation baselines unchanged:20 exact/two partial,3,924/8,788
code bytes,zero failures. Installed WASM preprocessed tokens agree for22 sources.
Current catalog/source/header/review/object bindings and25 isolated commit paths
audit. Progress:3,817/5,812 canonical primary files named (65.7%),1,995 remaining.
Concurrent transition-input commit already captured our marked checkpoint;
the naming commit changes25 owned paths and preserves its other work/index.

Names cover ribbon burst, keyframed spark shower, position-event sequence,
mirrored position/pair tracks, signed three-component scale keyframes,
texture-colored quad grid, wrapping texture strip and scheduler/interpolation.
Count-at0 versus opcode/count-at2 layouts remain distinct. Preserve duplicated
helper/caller mirroring, unsigned position versus signed scale widening,
23 spark records/24 main handles, physics transitions, exact frame/resource
boundaries, partial initialization, random calls, matching flags/register pins,
all assembler strings and do/assignment wrappers. Grid retains arena allocation
before loop tests, optional result fallback, negative scale bypass, row read-ahead,
colors/depth rules and upload when rendering is skipped; W/H increment only on
the rendering branch, so skip-mode texture window differs. Event cursor advances
per target nibble; preserve exact start-time equality, no automaticfree, silent
pool overflow, last-triplet reuse and unchecked arithmetic/index behavior.
Original spell/asset labels and selected runtime-provider identities unresolved.

Evidence correction: FF9 11b726122 corrects earlier effect-track grid review
wording, without source/name changes or changes to historical native proofs.
Direct full-body review of1233A800 ABB4 and13640000 92D4 shows MAC0 stored using
swc2 $24 after RTPT and its saved value tested for nonzero. Neither body issues
NCLIP. This supersedes the earlier MAC0/NCLIP wording in the effect-track
acceptance note and report. The correction binds the current ABB4 source hash;
no runtime interpretation beyond the observed register test is asserted.

Existing maintained naming/farm/scoring/preprocessor tools andthin adapters
reused;BV-08 namespace gate enforced. No additional shared implementation gap
found;existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit,link,gameplay,provider orshared workspace acceptance.
Historical reports/scorer remain pinned; refresh downstream source-bound evidence
fromcurrent catalog. Foreign work/index preserved; full-tree goal remains active.

Radial preset naming acceptance (BV-03/BV-08, P2): FF9 b339ee5c2
adds 22 canonical ovl_1320f000 names. Current catalog has 3,839 unique unit/symbol
names and 67 alias headers. All 22 complete primary bodies directly reviewed,
including every wrapper; no semantic deferrals in this unit. Evidence:
docs/function-names-radial-presets.json and Binviz target/ff9-names-radial-presets/.
All 22 entire native object pairs identical, exact affected/scored namespace 22,
unchanged pinned strict-relocation baselines: 22 exact, 4,444/4,444 code bytes,
zero failures. Installed WASM preprocessed tokens agree for all 22 sources.
Current catalog/source/header/review/object bindings and 26 isolated commit paths
audit. Progress: 3,839/5,812 canonical primary files named (66.1%), 1,973 remaining.

Names distinguish nine expanding and nine jittered radial presets by their
actual angle/radius halfword pairs and array indices, plus the two position
interpolation helpers and two effect handlers. Alternate-endpoint helper keeps
the different globals despite identical arithmetic. Expanding handler updates
angle/radius deterministically after scheduled starts; jittered handler advances
angle deterministically and jitters radius using separate random calls. The old
comment suggesting angle jitter does not supply naming evidence. No original
asset/spell names or callback-table ownership inferred from transplant comments.

Preserve ignored callback arguments, exact host arities, gcc 2.8.1, signed random
remainders, halfword wrapping, unsigned vector narrowing then signed widening,
unchecked shifts/divisions, completion after output writes, partial state setup,
duplicate endpoint writes, resources/frame boundaries, redundant pointer tests,
unchecked header pointers and host calls with potentially NULL ended objects.
Only identifiers and the unit-scoped alias header change; every other source
token/comment/type/layout/signature remains stable. Existing maintained naming,
farm/scoring/preprocessor tools and thin adapters reused; BV-08 namespace gate
enforced. No additional shared implementation gap found; existing proposals
remain separate. No private ownership/type/CFG walker, new matching credit,
linked image, gameplay, provider or shared workspace acceptance. Historical
reports/scorer stay pinned; refresh downstream source-bound evidence from the
current catalog. Foreign work/index preserved; full-tree goal remains active.

Staggered object naming acceptance (BV-03/BV-08, P2): FF9 fd36fabd7
adds 22 canonical ovl_12544800 names. Catalog has 3,861 unique unit/symbol names,
68 alias headers. All 22 complete primary bodies directly read, including every
wrapper and the large keyframe handler; no semantic deferrals in this unit.
Evidence: docs/function-names-staggered-objects.json and Binviz
target/ff9-names-staggered-objects/. All 22 entire native object pairs identical,
exact affected/scored namespace 22, unchanged pinned strict-relocation baselines:
20 exact/two partial, 1,992/6,064 code bytes, zero failures. Installed WASM
preprocessed tokens agree for all 22 sources. Current catalog/source/header,
review/object bindings and 26 isolated commit paths audit. Progress:
3,861/5,812 canonical primary files named (66.4%), 1,951 remaining.

Names cover staggered groups of six objects, six keyframed objects, two-stage
position spawns, grayscale fade, pair tracks, interpolation and six fixed-position
presets with six literal duplicates. Duplicate guards compare complete tokens
after normalizing only the function identifier; suffix _copy retains separate
identities without inventing behaviors. Track initializer is established by the
own-unit consumer and explicit two-vector layout. Preserve ignored arguments,
void extern/int definition discrepancy, volatile final argument, signed shifts,
partial initialization, halfword casts of word-sized positions, frame boundaries,
resource tables, exact host calls, redundant checks, potential NULL pointers,
negative fade arithmetic, do wrapper and -fno-cse-skip-blocks. Fade handler
finishes at 32 even though its second countdown may still be active; do not
extend it. No original ring/spell/camera provider identity inferred from comments.

Identifiers and unit-scoped aliases only; every other token, source comment,
type/layout/signature/compiler flag remains stable. Existing maintained naming,
farm/scoring/preprocessor interfaces and thin adapters reused; BV-08 namespace
gate enforced. No additional shared implementation gap found; existing proposals
remain separate. No private ownership/type/CFG walker, new matching credit,
linked image, gameplay, provider or shared workspace acceptance. Historical
reports/scorer remain pinned; refresh downstream source-bound evidence from
the current catalog. Foreign work/index preserved; full-tree goal remains active.

Actor segment naming acceptance (BV-03/BV-08, P2): FF9 e88338a07
adds 20 canonical ovl_126b1000 names. Current catalog has 3,881 unique unit/symbol
names and 69 alias headers. All 20 complete primary bodies directly read,
including both complete sequence handlers; no semantic deferrals in this unit.
Evidence: docs/function-names-actor-segments.json and Binviz
target/ff9-names-actor-segments/. All 20 entire native object pairs identical,
exact affected/scored namespace 20, unchanged pinned strict-relocation baselines:
18 exact/two partial, 3,220/8,376 code bytes, zero failures. Installed WASM
preprocessed tokens agree for all 20 sources. Current catalog/source/header,
review/object bindings and 24 isolated commit paths audit. Progress:
3,881/5,812 canonical primary files named (66.8%), 1,931 remaining.

Names cover individual/shared-track actor segment dispatch, offset keyframes,
blend segments with added/subtracted X wave, plus/minus X offset callbacks,
staggered/shared-track coordinators, five-object position spawn and track/buffer
helpers. Distinct tables and branch guards prevent misnaming alleged copies;
only two offset callbacks are literal duplicates, guarded by complete token
comparison after normalizing only the function identifier. Segment kind supplies
blend weight; callback n only tests completion. Actual output shifts by 12,
despite older comments calling the position format 16.16. No format assertion
is embedded in names and no source comments are altered.

Preserve unchecked actor id-1, upper-only clamps, unsigned halfword narrowing,
signed shifts/negative weights, repeated wave calls, volatile completion limit,
old-style void/int callback declarations, exact resource IDs/start boundaries,
partial state/counter setup, and random-call/remainder order. First-table X jitter
occurs twice while second-table X is unchanged; 8060 also writes first-table h12
twice and leaves second-table h12 untouched. These asymmetries remain intact.
Preserve register19 pin, do wrapper, -fno-cse-skip-blocks/-fforce-addr, redundant
pointer tests and unchecked header pointers. Buffer-byte helper gets a generic
indirect-store name; byte7 meaning and host provider identities remain unresolved.

Function identifiers and own-unit aliases only; all other source tokens,
comments/types/layouts/signatures/matching tricks remain stable. Existing
maintained naming/farm/scoring/preprocessor tools and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found;
existing proposals remain separate. No private ownership/type/CFG walker, new
matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer remain pinned; refresh downstream source-bound evidence
from the current catalog. Foreign work/index preserved; full-tree goal active.

Rotated track naming acceptance (BV-03/BV-08, P2): FF9 ec886de76
adds 18 canonical ovl_13836800 names. Catalog has 3,899 unique unit/symbol names,
70 alias headers. All 18 complete primary bodies directly read; no semantic
deferrals in this unit. Evidence: docs/function-names-rotated-tracks.json and
Binviz target/ff9-names-rotated-tracks/. All 18 entire native object pairs
identical, exact affected/scored namespace 18, unchanged pinned strict-relocation
baselines: 16 exact/two partial, 1,364/2,920 code bytes, zero failures. Installed
WASM preprocessed tokens agree for all 18 sources. Current catalog/source/header,
review/object bindings and 22 isolated commit paths audit. Progress:
3,899/5,812 canonical primary files named (67.1%), 1,913 remaining.

Names distinguish the two rotated tracks by the primary table's X80 bias, five
offset-position tracks by descriptor index, the two-object/five-part handlers,
track fetchers and bounded host kind/last-index helpers. Rotation loads a host
matrix, zeros GTE translation, fetches idx*4 and adds the saved module offset;
preserve every GTE string, narrowing and output-before-completion behavior.
Five-part resource tables advance ONE halfword per loop, as current corrected
C shows; the stale header comment about four-byte entries supplies no evidence.
No new behavior change or boss/spell identity claim. Preserve resource/frame
boundaries, partial state, redundant pointer tests, unchecked record pointers,
q state alias, pins18/23, do wrapper, exact differing host1DC arities and all
old-style callback declarations. Host-kind helpers describe only the observed
8C->88 then98 sequence, nonzero32C guard and9C==328-1 predicate; specific kind
meanings/provider contracts remain unresolved. Generic indirect buffer store
name does not assert an undocumented byte meaning.

Only identifiers and own-unit aliases change; every other source token/comment,
type/layout/signature/compiler choice remains stable. Existing maintained naming,
farm/scoring/preprocessor interfaces and thin adapters reused; BV-08 namespace
gate enforced. No additional shared implementation gap found; existing proposals
remain separate. No private ownership/type/CFG walker, new matching credit,
linked image, gameplay, provider or shared workspace acceptance. Historical
reports/scorer stay pinned; refresh downstream source-bound evidence from the
current catalog. Foreign work/index preserved; full-tree goal remains active.

Track timeline naming acceptance (BV-03/BV-08, P2): FF9 9f2befca7
adds 17 canonical ovl_12c0f800 names. Catalog has 3,916 unique unit/symbol names,
71 alias headers. All 17 complete primary bodies directly read, including each
entire timeline; no semantic deferrals in this unit. Evidence:
docs/function-names-track-timelines.json and Binviz target/ff9-names-track-timelines/.
All 17 entire native object pairs identical, exact affected/scored namespace 17,
unchanged pinned strict-relocation baselines: 15 exact/two partial,
3,560/6,448 code bytes, zero failures. Installed WASM preprocessed tokens agree
for all 17 sources. Current catalog/source/header, review/object bindings and
21 isolated commit paths audit. Progress: 3,916/5,812 canonical primary files
named (67.4%), 1,896 remaining. Maintained native CLI SHA256 remains
545beb01f1e1e8fd6dccd2c2f66b80224bc152e23b12a2ebcbe1a93822644b6e.

Names distinguish compressed and offset position tracks, 18-track and 30-track
object sequences, fading handle/actor sequence, object motion with grayscale
ramp, position blending, track access and bounded host-kind helpers. Preserve
separate origin globals, signed Y/Z compression, unsigned halfword addition,
upper-only clamps, unchecked IDs/indices/counts, and output before completion.
Actual object-motion grayscale runs 0->255, despite the old 255->0 comment;
names and review evidence follow code, source comment unchanged. Handle sequence
fade only applies counter8..1 on frames42..49 and leaves counter0 when its block
ends; no completion of that full fade assumed. Preserve negative grayscale,
register2 pin, -fno-rerun-cse-after-loop, identical branches and unsigned gate.

Preserve all captured/derived vectors, exact corner publication/store order,
current int coordinates, do wrappers, typeof absolute Y=-4000 assignment,
partial initialization, callback casts/arity, resources/frame events, host
last-index/kind transitions and all host finish/teardown boundaries. Position
blend captures start only at cur0, then blends cached start toward destination
with optional wave and publishes before completion; no clamp or new provider
semantics asserted. Original asset/spell names and selected host contracts remain
unresolved. Only identifiers and own-unit aliases change; all other source
tokens/comments/types/layouts/signatures/compiler choices remain stable.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace
acceptance. Historical reports/scorer remain pinned; refresh downstream
source-bound evidence from current catalog. Foreign work/index preserved;
full-tree goal remains active.

Triangle particle naming acceptance (BV-03/BV-08, P2): FF9 50579e14d
adds 17 canonical ovl_fd32800 names. Catalog has 3,933 unique unit/symbol names,
72 alias headers. All 17 complete primary bodies directly read, including every
large scene/ribbon/model-particle handler; no semantic deferrals in this unit.
Evidence: docs/function-names-triangle-particles.json and Binviz
target/ff9-names-triangle-particles/. All 17 entire native object pairs identical,
exact affected/scored namespace 17, unchanged pinned strict-relocation baselines:
14 exact/three partial, 9,016/12,400 code bytes, zero failures. Installed WASM
preprocessed tokens agree for all 17 sources. Current catalog/source/header,
review/object bindings and 21 isolated commit paths audit. Progress:
3,933/5,812 canonical primary files named (67.7%), 1,879 remaining.

Names cover inline scalar/vector/pair/triple keyframes, rotating track handles,
four-phase tracked scene, keyframed ribbon phases/groups, triangle source/pool
setup, free-slot search, Y-threshold particle spawning, pool stepping, model-group
particle callback/coordinator and paired vector publication. Source vertices,
centroid arithmetic, age/free markers and exact GTE transforms establish particle
roles; no original effect/spell label inferred. Inline reader count/key layouts
remain distinct from pointer-backed tracks. Preserve signed counts and upper-only
clamps, negative/zero indexing, optional outputs, halfword narrowing, random order,
silent pool exhaustion, unchecked capacities and provider-dependent integration.

Four-phase scene resets job frame but not its separate cumulative counter;
ribbon parent starts groups1/2 in the same frame. Preserve raw resource-offset
table layouts, phase2 ribbon4*t versus stale2*t comment, fixed buffer strides,
possible uninitialized past-start point, widths/colors/fades, all pins/barriers,
do wrappers, descriptor callbacks, differing old-style declarations and partial
state. Pool loading marks FIRST n entries free; do not change to an append range.
Callback phase transition writes the halfword before its record; do not infer
that external field's meaning. Preserve group>=6 initialization at supplied
index versus later fixed pool6, shared globals/handles, flags not reset at init,
negative grayscale, resource events and return-before-tail boundaries. No source
comments, bodies, types, layouts, signatures or matching tricks changed.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace
acceptance. Historical reports/scorer stay pinned; refresh downstream
source-bound evidence from the current catalog. Foreign work/index preserved;
full-tree goal remains active.

Save screen state naming acceptance (BV-03/BV-08, P2): FF9 c24b64640
adds 16 canonical ovl_112800 names. Catalog has 3,949 unique unit/symbol names,
73 alias headers. All 16 primary bodies fully reviewed; no semantic deferrals
in this unit. Evidence: docs/function-names-save-state.json and Binviz
target/ff9-names-save-state/. All 16 complete native object pairs byte-identical;
exact affected/scored namespace 16, pinned strict-relocation baseline unchanged:
16 exact, 324/324 code bytes, zero failures. Installed WASM preprocessed tokens
agree for all 16 sources. Current catalog, review inputs, native reference
bindings, objects and 20 isolated commit paths audit. Progress: 3,949/5,812
canonical primary functions named (67.9%), 1,863 remaining.

Names distinguish confirmation, directory scan, message and transfer active
getters/exchanges, widget-open callbacks, file-list and port-selector fade
completion, timed notice opening, aborted transfer closure and raw memory-card
operation state. Reviewed recovered save-screen consumers and callback
registrations plus maintained Binviz native refs establish physical archive
addresses. Native CLI, executable, archive, notes and output hashes are pinned.
Transplant comments alone do not establish purpose. UI-control flag name stays
bounded to the reviewed flag calculation; no guessed button or enable polarity.

Canonical 112800 and 118800 share physical archive file57 addresses but retain
unit-scoped headers. No callback D_address label rewrite or alias propagation
into 118800. Preserve byte narrowing, old unsigned-byte returns, raw state-word
return, mode1 message-input exception on file-list fade completion, timed-notice
state6, transfer phase store, callback store ordering and alternate aborted
completion path. Identical store bodies have distinct caller-established names.
No function bodies, types, signatures, comments or matching tricks changed.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace
acceptance. Historical reports/scorer stay pinned; refresh downstream
source-bound evidence from the current catalog. Foreign work/index preserved;
full-tree goal remains active.

Wavy mesh naming acceptance (BV-03/BV-08, P2): FF9 ca32eb8b7 adds
16 canonical ovl_10fea800 names. Catalog has 3,965 unique unit/symbol names,
74 alias headers. All 16 complete primary bodies and g10/g19/g13 headers
reviewed; no semantic deferrals in this unit. Evidence:
docs/function-names-wavy-meshes.json and target/ff9-names-wavy-meshes/.
All 16 complete native object pairs identical; exact affected/scored namespace16,
unchanged pinned strict-relocation baselines: 11 exact/five partial, zero failures.
Installed WASM preprocessed tokens agree for all 16 sources. Current catalog,
review inputs, object bindings and 20 isolated commit paths audit. Progress:
3,965/5,812 canonical primary files named (68.2%), 1,847 remaining.

Names cover inline keyframes, wrapping strips, six-handle ring blending,
keyframed crossfades, ribbon and sheet meshes, mesh-sprite scenes, asymmetric
wave sampling, sheet attachment callback, actor offsets and scene vector tracks.
Five small donor bodies separately pass exact token normalization using the
existing lexer; only explicit function/global/include identities differ.
Larger sibling handlers require own-body review; no asset/spell label inferred.

Ribbon20x6 tests MAC0 after RTPT without NCLIP, despite stale depth prose.
Sheet18x19 performs NCLIP: positive results halve only textured red/green,
leaving blue/overlay colors unchanged. Preserve skipped-row attachment data,
unsigned narrowed viewport check, cursor wrap16 versus18 published rows,
source flag ignored by attachment callback, all GTE sequences, barriers/pins,
primitive reservations, UV seams, do wrappers and unchecked capacities.
Preserve shared ring/handle dependencies, duplicate local handle identity,
unreachable early fade branch, exact resource times, phase-local frame reset
with cumulative tick, partial vectors/color bytes, duplicate discarded calls,
unsigned offset wrap and return-before-tail boundaries. No body/type/signature,
layout/comment/compiler/matching-trick changes.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace
acceptance. Historical reports/scorer stay pinned; refresh downstream
source-bound evidence from the current catalog. Foreign work/index preserved;
full-tree goal remains active.

Half-scale track naming acceptance (BV-03/BV-08, P2): FF9 a47c55c6e
adds 15 canonical ovl_1291c000 names. Catalog has 3,980 unique unit/symbol names,
75 alias headers. All 15 complete primary bodies and g26/g10/g13 headers read;
no semantic deferrals in this unit. Evidence:
docs/function-names-half-scale-tracks.json and target/ff9-names-half-scale-tracks/.
All 15 entire native object pairs identical; exact affected/scored namespace15,
unchanged pinned strict-relocation baselines: 14 exact/one partial,
2,084/3,332 code bytes, zero failures. Installed WASM preprocessed tokens agree
for all 15 sources. Catalog/review/object bindings and19 isolated commit paths
audit. Progress: 3,980/5,812 canonical primary files named (68.5%),1,832 remaining.

Names cover six indexed half-scale rotated track callbacks, keyframe transform,
randomized track-handle scene, six-object spawn sequence, host12bit interpolation,
integer blend, pointer-backed keyframe initialization/read, indirect byte store
and wrapping strip. Five wrappers pass exact existing-lexer token equality after
only function/table identifier normalization. Own-unit bodies/consumers establish
names; no donor-comment-only, original asset/ability or callback-table claim.

Preserve signed16 division by2 before rotation, unsigned payload narrowing,
zero-translation GTE sequence, signed offset, upper-only key clamps, zero/negative
index hazards and partially initialized padding. Main scene preserves random
row selection, three handles/six descriptors, exact hostDC registrations,
register17 pin, same-body if/else strip and unsigned frame wrap. Fixed handle
color gate remains fixed; no smooth fade inferred. Six-object sequence preserves
subrecord callback-word assignment, redundant null checks, exact resource events,
unsigned weight decrement and frame cap, partial object state and no added
cleanup. Preserve two-halfword initializer advancement, h0 untouched, integer
overflow/division/extrapolation, indirect host buffer identity and strip capacity.
No bodies/comments/types/layouts/signatures/compiler/matching tricks changed.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace
acceptance. Historical reports/scorer stay pinned; refresh downstream
source-bound evidence from the current catalog. Foreign work/index preserved;
full-tree goal remains active.

Actor track group naming acceptance (BV-03/BV-08, P2): FF9 8817abae4
adds 15 canonical ovl_134b8800 names. Catalog has 3,995 unique unit/symbol names,
76 alias headers. All 15 complete primary bodies and g30/g13 headers reviewed;
no semantic deferrals in this unit. Evidence:
docs/function-names-actor-track-groups.json and target/ff9-names-actor-track-groups/.
All 15 entire native object pairs identical; exact affected/scored namespace15,
unchanged pinned strict-relocation baselines: 14 exact/one partial,
2,792/3,768 code bytes, zero failures. Installed WASM preprocessed tokens agree
for all 15 sources. Current catalog/review/object bindings and19 isolated commit
paths audit. Progress: 3,995/5,812 canonical primary files named (68.7%),
1,817 remaining.

Names cover eight indexed offset track callbacks, limited keyframe publisher,
two groups of eight tracked objects, four-object flash/mask sequence,
two-stage seven-object sequence, single actor-offset object and pointer-backed
track initializer/reader. Seven wrappers pass exact existing-lexer token equality
after only function/table identifier normalization. Own-unit actor position,
track consumers and object events establish roles; asset/ability labels unresolved.

Wrappers remain six-int/void, discard worker return and ignore both a1/a3.
Worker publishes clamped XYZ with unsigned offset wrap, then tests ORIGINAL idx
against explicit limit independently of key count. Preserve this distinction,
zero/negative indexing, old-style declarations and GCC2.8.1 marker. Track groups
preserve eight+eight objects at0/23, differing spawn vectors/payload strides,
resource41 at37, shared offset, register22 pin and unchecked pointers/capacity.
Flash handler has one explicit white flash at30, then contiguous mask intervals
31..90/91..105 with exact12-byte E6000001 packets, OT1 bitfield linkage and
fixed/growing scales. Preserve every-update resource timeline even at end140,
duplicate packet blocks, partial arrays, redundant null checks, signed/unsigned
halfword narrowing, exact lookup order and existing cleanup behavior. No bodies,
types/layouts/signatures/comments/compiler choices/matching tricks changed.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace
acceptance. Historical reports/scorer stay pinned; refresh downstream
source-bound evidence from the current catalog. Foreign work/index preserved;
full-tree goal remains active.

Whiteout track naming acceptance (BV-03/BV-08, P2): FF9 f7eb35e4c
adds 15 canonical ovl_13915800 names. Catalog has 4,010 unique unit/symbol names,
77 alias headers. All 15 complete primary bodies reviewed, including every
scripted scene and actor sequence; no semantic deferrals in this unit.
Evidence: docs/function-names-whiteout-tracks.json and
target/ff9-names-whiteout-tracks/. All 15 entire native object pairs identical,
exact affected/scored namespace15, unchanged pinned strict-relocation baselines:
13 exact/two partial, 2,800/5,484 code bytes, zero failures. Installed WASM
preprocessed tokens agree for all 15 sources. Current catalog/review/object
bindings and19 isolated commit paths audit. Progress: 4,010/5,812 canonical
primary files named (69.0%),1,802 remaining.

Names cover compressed/offset position callbacks, eighteen/thirty-track object
timelines, fading handle and actor whiteout, offset resolver, fixed-weight blend,
integer interpolation/grayscale, pointer-backed track init/read and bounded
host-object kind/last-index helpers. Own-unit full bodies establish names;
larger sibling scenes reviewed independently despite prior12C0F800 similarities.
No original effect/ability or provider claim inferred from donor comments.

Preserve signed Y4/5/Z3/4 compression, unsigned offsets/narrowing, original-index
limit after sampled clamped output, id0/negative-index hazards, partial padding
and distinct descriptors. Preserve unused object slots4/6, actual four spawns at48,
all resource/hook events and differing one/two-argument destroy calls. Later
position Y is absolute-4000, not relative anchor. Crossfade window30..49 retains
unreachable clamps and tintcounter0 unprocessed after49; no final-255 call added.
Whiteout runs0..255 inclusive32..92 then34C/flag; later white draw repeats.
Actor kind3 at32 and repeated last-index kind4/284 updates retain exact timing,
unsigned halfword handles, completion after updates and no once-only latch.
Preserve index4*word pointer grayscale destination, two-halfword initializer,
all flags/layouts/signatures/comments/compiler choices/matching tricks.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace
acceptance. Historical reports/scorer stay pinned; refresh downstream
source-bound evidence from the current catalog. Foreign work/index preserved;
full-tree goal remains active.

Radial ribbon scene naming acceptance (BV-03/BV-08, P2): FF9 b247896b4
adds 14 canonical ovl_1167e800 names. Catalog:4,024 unique unit/symbol names,
78 alias headers; all14 complete primary bodies reviewed, no semantic deferrals
in this unit. Evidence: docs/function-names-radial-ribbon-scenes.json and
target/ff9-names-radial-ribbon-scenes/. All14 entire native object pairs identical,
exact affected/scored namespace14, unchanged pinned strict-relocation baselines:
nine exact/five partial,2,548/14,624 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all14 sources; current catalog/review/source/header/
object bindings and18 isolated commit paths audit. Progress:4,024/5,812 canonical
primary files named(69.2%),1,788 remaining.

Names cover texture-colored quad grid, radial sprite grid, four inline key readers,
position ribbon history, radial XY scaling, counter-rotating handle scene,
round-robin scenehandle id callback, horizontal wave offset, seven-phase ribbon/grid
sequence, host flag clearing and paired vector tracks. Own-unit complete bodies
and consumers establish bounded behavioral names. Original spells/abilities and
selected host providers remain unresolved; no clone equality inferred from comments.

Preserve shared grid OTZ versus MAC0 reads afterRTPT withoutNCLIP, both c11 shifts,
skip-specific W/H upload differences, fixed17-entry grid buffers, geometry callback
after UV/page calculation, byte/halfword narrowing and packet/tag sizes. Ribbon
reset uses n>1000 then n-999; validity count precedes new head write; pm/fr have
no explicit pins despite comment. Preserve true register pins/compiler flags,
all hints, partial initialization and unchecked arithmetic/indices. Seven-phase
timeline resets jobtype=-1 while keeping cumulative counter, repeats transforms,
uses six0xc0 ribbon buffers and retains phase6 paZ/pbZ mismatch. Flag-clear handler
is not a no-op; actual signed shifts and completion order remain untouched.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from the current catalog. Foreign work/index preserved; full-tree goal remains active.

Keyed burst trail naming acceptance (BV-03/BV-08, P2): FF9 8afd30c7b
adds14 canonical ovl_111c5800 names. Catalog4,038 unique unit/symbol names,
79 alias headers; all14 complete primary bodies reviewed, no semantic deferrals
in this unit. Evidence: docs/function-names-keyed-burst-trails.json and
target/ff9-names-keyed-burst-trails/. All14 entire native object pairs identical,
exact affected/scored namespace14, unchanged pinned strict-relocation baselines:
ten exact/four partial,4,920/20,040 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all14 sources; current catalog/review/source/header/
object bindings and18 isolated commit paths audit. Progress4,038/5,812 canonical
primary files named(69.5%),1,774 remaining.

Names cover horizontal gradient, three-phase handle fade, randomized25-point
trail, four-phase ring ribbon scene, lowered pair tracks, keyed particle trails,
adjacent-key front end, particle update/spawner, random trail point, fading
trail callback, ring cursor reset,17-vertex particle spawn and jittered angle
emitter. Actual cursor consumer establishes the tiny reset function purpose.
Own-unit full bodies and headers reviewed; no original spells/abilities/provider
or transplant equality inferred from comments.

Preserve differing random-table strides, selected spawner phase rather than
invented transitions, uninitialized kind2 alternate-origin Y/unknown-kind return,
unsigned rejection-loop predicate, random/allocation order, count1 division,
16384 blend extrapolation and all partial padding. Preserve cursor%5 and reverse
17-vertex fixed12 stores, explicit zero velocities, empty asm barriers, pins,
compiler flags and wrappers. Scene counters stay distinct from reset job frame;
duplicate draws, possibleNULL destroys, raw unchecked key reads, seam duplication,
phase3 second key-vector doubled as SCALE and exact flags/events remain untouched.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from the current catalog. Foreign work/index preserved; full-tree goal remains active.

Sprite debris naming acceptance (BV-03/BV-08, P2): FF9 c7fb690a3
adds12 canonical ovl_133fe800 names. Catalog4,050 unique unit/symbol names,
80 alias headers; all13 complete primary bodies reviewed, one explicit semantic
deferral for80D0 pending selected host2F4 provider/buffer semantics. Evidence:
docs/function-names-sprite-debris.json and target/ff9-names-sprite-debris/.
All12 entire native object pairs identical, exact affected/scored namespace12,
unchanged pinned strict-relocation baselines12 exact,3,332/3,332 code bytes,
zero failures. Installed WASM preprocessed tokens agree for all12 sources;
current catalog/review/deferred/source/header/object bindings and16 isolated
commit paths audit. Progress4,050/5,812 canonical primary files named(69.7%),
1,762 remaining. Naming goal remains active across other units.

Names cover repeated debris/scale timeline, delayed paired debris, sprite fades,
fixed-weight blend, interpolation/grayscale, pointer-backed position track init/
signed-halfword to word key read and bounded host-object kind/last-index helpers.
Own-unit complete bodies and consumers establish roles; no selected provider,
original effect/ability or transplant equality inferred from comments. Opaque
single host call is recorded as reviewed, without a timer-only replacement name.

Preserve16-frame separate event blocks, mode1 fadecounter1, partialobjects/level,
w64!=2 actor gate, metadata stride2400 and endedpointer checks. Last -128 repeated
sequence tint is stored but not submitted after counter decrement. Standalone
fadeout runs only unsignedframe<50; finalcounter0 is not processed at50, finish80.
Track init consumes twohalfwords; wordreader upper-only clampsunsignedcount,
signextends payload and reportsended. Preserve count0/negative-index hazards,
integerdivision/overflow, int*p20 byteoffset4idx, objectkind pass-through, exact
arity/flags/types/layouts/comments and no invented final color/cleanup.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Target waypoint naming acceptance (BV-03/BV-08, P2): FF9 ea1aff550
adds13 canonical ovl_12e13000 names. Catalog4,063 unique unit/symbol names,
81 alias headers; all13 complete primary bodies reviewed, no semantic deferrals
in this unit. Evidence: docs/function-names-target-waypoints.json and
target/ff9-names-target-waypoints/. All13 entire native object pairs identical,
exact affected/scored namespace13, unchanged pinned strict-relocation baselines
13 exact,3,544/3,544 code bytes,zero failures. Installed WASM preprocessed tokens
agree for all13 sources; current catalog/review/source/header/object bindings
and17 isolated commit paths audit. Progress4,063/5,812 canonical primary files
named(69.9%),1,749 remaining. Full-tree naming goal remains active.

Names cover four target callbacks, origin/offset target blend, primary/delayed
keyframe callbacks and evaluators, four-target and six-model sequences, track
initialization and halfword key read. Own-unit full bodies, data layouts and
actual consumers establish roles. Declarations/direct calls and explicit
callback-table assignments use readable names; aliases preserve native/runtime
symbols and own-unit scope. No original ability or provider identity inferred.

Preserve per-call cumulative X offsets-150/+150/-50/+50, globalframe blend weight
without clamp, output narrowing/fixed12 before original t0/t1 completion test,
primary indexframe*3 and delayed(frame-5)*3 with negative-index hazard. Initializer
skips TWO halfwords; reader unsignedcount upper-only clamp remains. Four-target
initializer subtracts target0Y twice (net-400), preserves callback blocks384 bytes,
repeatedNULL tests and partialpadding. Six-model events0/3/5/8 retain exact
resources/positions, signed constants added as unsignedhalfwords, uninitialized
fourth waypoint fields, independent published positions and upper-only blend cap.
Finish40 remains after updates without extra host finish/release calls.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Model triangle particle naming acceptance (BV-03/BV-08, P2): FF9
ad916d8a9 adds13 canonical ovl_12f4b800 names. Catalog4,076 unique unit/symbol
names,82 alias headers; all13 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-model-triangle-particles.json
and target/ff9-names-model-triangle-particles/. All13 entire native object pairs
identical, exact affected/scored namespace13, unchanged pinned strict-relocation
baselines9 exact/four partial,1,688/6,692 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all13 sources; current catalog/review/source/
header/object bindings and17 isolated commit paths audit. Progress4,076/5,812
canonical primary files named(70.1%),1,736 remaining; full-tree goal active.

Names cover four inline key readers, wrapping texture strip, pool initialization/
source load/free search/spawn/step, model-group callback, model particle sequence
and paired vector tracks. Own-unit complete bodies and consumers establish roles;
FD32800 similarity guides vocabulary only, no equality inferred from transplant
comments. Ancillary7A5C .i/.s artifacts untouched. Readable source references
and callbacks expand to stable own-unit native/runtime identities.

Preserve signed inline count upper-only clamp, optional outputs, signed remainder
and unconditional48-byte strip reservation. Preserve cumulative poolcount, free
cursor, FIRST n entry reset, initial source cursor, vertex1Y threshold, signed
centroid/3, exact randomcall order, GTE sequences and no used-flag reset at expiry.
Model callbacks retain external timer halfword[-1], phases/flags, partialpadding
and group>=6 supplied poolindex init versus fixed pool6 update. Scene flags are
not reset atinit; frame20 skips thresholdupdate, overlapping flash/fade calls,
jobframe reset and cumulativecounter distinction stay. Preserve explicit api18
pin/emptyasm barrier, all event resources/timing, descriptor/input dualrole and
row*out-3 paired-track publication/completion ordering.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Follower satellite naming acceptance (BV-03/BV-08, P2): FF9
7611e21b3 adds12 canonical ovl_1261b800 names. Catalog4,088 unique unit/symbol
names,83 alias headers; all12 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-follower-satellites.json
and target/ff9-names-follower-satellites/. All12 entire native object pairs
identical, exact affected/scored namespace12, unchanged pinned strict-relocation
baselines12 exact,4,720/4,720 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all12 sources; current catalog/review/source/header/object
bindings and16 isolated commit paths audit. Progress4,088/5,812 canonical primary
files named(70.3%),1,724 remaining; full-tree goal active.

Names cover shared/indexed scatter callbacks, paired object position sequence,
follower/five-satellite burst sequence, complementary weights/scalar interpolation
and pointer-backed pair/position/scalar track initialization/readers. Complete
own-unit bodies and consumers establish roles; donor comments are historical
hints only. Readable source calls and callbacks retain own-unit native/runtime
identities through alias headers.

Preserve callback shift12 despite16.16 comments, unclamped weights, unchecked
id-1, stored snapshot position and independent id calls. Pair reader returns
second vector into first output and first into second, with no mirror; upper-only
clamp/unsignedcounts/signedscalar retained. Scene event45 consumes prior-frame
v90 before current GTE and preserves exact randomcall order. Main/five satellite
transforms share scratchrotation; later follower GTE sees last satelliteY.
Postdecrement color gates, partially initialized state, resource/timing/custom
records, unconditional possiblyNULL host1D8 and exact asmstrings stay unchanged.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Crossfade trail naming acceptance (BV-03/BV-08, P2): FF9
d07c86eaa adds12 canonical ovl_1389b800 names. Catalog4,100 unique unit/symbol
names,84 alias headers; all12 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-crossfade-trails.json
and target/ff9-names-crossfade-trails/. All12 entire native object pairs identical,
exact affected/scored namespace12, unchanged pinned strict-relocation baselines
11 exact/one partial,2,556/5,464 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all12 sources; current catalog/review/source/header/
object bindings and16 isolated commit paths audit. Progress4,100/5,812 canonical
primary files named(70.5%),1,712 remaining; full-tree goal active.

Names cover scaled/rotated position trail builders, paired object snapshot
sequence, crossfading model/trail scene, screen fade, weight/scalar/grayscale
helpers and pointer-backed position/pair track initialization/readers. Own-unit
complete bodies and consumers establish roles; comments do not establish original
camera/spell labels or donor equality. Readable C references retain native and
runtime identities through own-unit aliases.

Preserve inactive trail coordinates, descending keys, signed /10 expansion with
per-addition halfword narrowing, actororigin offset, per-point matrix/GTE calls
and exact asmstrings. Scene ph<39 enables four intervals, despite five incomment;
retains postdecrement tint gate, reused scratch and redundant stores/identical
arms. Two builders write record i but draw i+1; six rotatedtrail angles repeat
0/1365/2730 in pairs. Preserve $23/$22 pins/compilerflag/do-while wrapper,
resource/event order, partiallyinitialized state, unconditional destroys,
snapshot positions, independent fades, differing OT targets and completion order.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Six-path object naming acceptance (BV-03/BV-08, P2): FF9
e6b845129 adds12 canonical ovl_130f5000 names. Catalog4,112 unique unit/symbol
names,85 alias headers; all12 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-six-path-objects.json
and target/ff9-names-six-path-objects/. All12 entire native object pairs identical,
exact affected/scored namespace12, unchanged pinned strict-relocation baselines
12 exact,2,036/2,036 code bytes,zero failures. Installed WASM preprocessed tokens
agree for all12 sources; current catalog/review/source/header/object bindings
and16 isolated commit paths audit. Progress4,112/5,812 canonical primary files
named(70.8%),1,700 remaining; full-tree goal active.

Names cover six distinct position track callbacks, shared offset keyframe reader,
six-model ring sequence, variant tracked-object sequence, timed single-object
sequence and pointer-backed position track initialization/reader. All own-unit
complete bodies and consumers reviewed; callback suffix0..5 denotes table channel,
without guessed original variant labels. Readable C calls retain native/runtime
identity through own-unit aliases; opaque address-valued data remain stable.

Preserve sharedframe*2 sampling independent of callbackt0 completion, unsigned
halfword origin additions/narrowing, signed shift12 and upper-only clamp. Ring
loads six resource3 models/scales32, transforms onlyframe<12 and unconditionally
passes possiblyNULL objects to host1D8. Variant initialization uses outinputs,
uncheckedindex, sharedorigin overwrite and pointer-backed resource stream;
running stores globalframe beforecreation, copies opaque integer headerword8,
customrows/redundant guards and optional resource4/1 scales128. Preserve partially
initialized state, timedresource19 load, finish20 and no added cleanup.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Target vector step naming acceptance (BV-03/BV-08, P2): FF9
958cb81f1 adds12 canonical ovl_11373000 names. Catalog4,124 unique unit/symbol
names,86 alias headers; all12 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-target-vector-steps.json
and target/ff9-names-target-vector-steps/. All12 entire native object pairs
identical, exact affected/scored namespace12, unchanged pinned strict-relocation
baselines12 exact,3,376/3,376 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all12 sources; current catalog/review/source/header/object
bindings and16 isolated commit paths audit. Progress4,124/5,812 canonical primary
files named(71.0%),1,688 remaining; full-tree goal active.

Names cover offset object spawn, position blend callback, target-slot paired
sequence, fifteen/four-step host vector handlers/helpers, hostkind/index checks,
activekind2/3 and indirectbuffer byte store. Complete own-unit bodies/consumers
establish boundedroles. No provider animation/orientation or original effect
labels inferred; historical transplant comments do not establish equality.

Preserve upper-only slotchecks, partiallyinitialized state, sharedcontext/vector
overwrites, event4/5/13 ordering, callbackcast and followedobject nullchecks.
Fifteen-step handler stores targetreference but helper ignores it; delta remains
external/unspecified. Preserve duplicate firstframe host20C calls, perstep signed
division/narrowing and completion-after-publication. Derivedvector helper Y>=2048
uses4096-Y reflection, lowernegativebranch adds4096; no conventionalwrap fix.
Preserve post-update byte7 finishflag, hostkind/index call order, unusedformals,
exact resource/id calls and indirectbuffer indexwithoutboundscheck.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Jittered shared position naming acceptance (BV-03/BV-08, P2): FF9
69fd1ab43 adds11 canonical ovl_10c6e800 names. Catalog4,135 unique unit/symbol
names,87 alias headers; all11 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-jittered-shared-positions.json
and target/ff9-names-jittered-shared-positions/. All11 entire native object pairs
identical, exact affected/scored namespace11, unchanged pinned strict-relocation
baselines11 exact,2,544/2,544 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all11 sources; current catalog/review/source/header/object
bindings and15 isolated commit paths audit. Progress4,135/5,812 canonical primary
files named(71.1%),1,677 remaining; full-tree goal active.

Names cover five sharedposition callbacks, fixedposition/completion copier,
indexed jittered-object track sequence and four hostkind/index helpers. Complete
own-unit bodies/consumers establish roles; suffix0..4 denotes sharedchannel only.
No original star/ability/provider labels or donor equality inferred. Readable C
references retain native/runtime identities through own-unit aliases.

Preserve wrapper void/int callee mismatch, bothunusedformal slots, sourceglobal
case/spelling and copier volatilelimit late-load matching trick. Preserve exact
independent jitter randomcalls/halfwordwrap, index>=5 upper-only rejection,
partialfourthvector/transform fields, integercallbacktable/custombuffer1064stride,
resource/id order and redundantguard. Event2 offset-200,4 groundY0 models,6 marker
kinds19/20/21,8 unconditional destroy retained. Allvariants complete40; onlyindex4
calls host78, otherindices applyhostkind2 at5 without earlyreturn. Helper context
case differs from sequence; no provider/link closure claim and no spellingfix.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Rippling sheet naming acceptance (BV-03/BV-08, P2): FF9
cbc61292c adds11 canonical ovl_12f0f000 names. Catalog4,146 unique unit/symbol
names,88 alias headers; all11 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-rippling-sheet.json
and target/ff9-names-rippling-sheet/. All11 entire native object pairs identical,
exact affected/scored namespace11, unchanged pinned strict-relocation baselines
nine exact/two partial,1,744/6,644 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all11 sources; current catalog/review/source/header/
object bindings and15 isolated commit paths audit. Progress4,146/5,812 canonical
primary files named(71.3%),1,666 remaining; full-tree goal active.

Names cover four inline key readers, wrapping strip, asymmetricwave,18x19 sheet
renderer, cyclicattachment reader, full two-phase scene, actoroffset spawn and
pairedscene vector tracks. Complete own-unit bodies/consumers establish roles;
10FEA800 similarity informs vocabulary only, not inferred donor equality or
original ability/camera/provider identity. Readable C names retain own-unit
native/runtime symbols through aliases, including integerhook casts.

Preserve signedcount upper-onlyclamp, signedremainder/reservation and wavebranch.
Renderer reserves22320bytes, preserves $21/flags/barriers/wrappers/GTE sequences;
bothclip signs draw, positiveclip halves onlyR/G, depthliteral0x158002d precedes
finalvertexRTPS, fourthXY written afterpacket submission. Firstcolumn skippedcells
leave stale row/on; attachment cycles16 of18 rows, ignoreson andneverresetscursor.
Scene resource5 loads atframe18 beforeh10 explicitinit50, transition81 publishes
beforeoptional stateY/Z adjustment, jobframe reset/cumulativecounter distinction,
spriteflag windows/discardedcalls and finish24 afterdraw retained. Actorrec h4
height/h2-512 remain distinct; hostpairedtrack publication/completion order stays.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Three-track crossfade naming acceptance (BV-03/BV-08, P2): FF9
345c3290d adds10 canonical ovl_138d7800 names. Catalog4,156 unique unit/symbol
names,89 alias headers; all10 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-three-track-crossfade.json
and target/ff9-names-three-track-crossfade/. All10 entire native object pairs
identical, exact affected/scored namespace10, unchanged pinned strict-relocation
baselines10 exact,2,704/2,704 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all10 sources; current catalog/review/source/header/object
bindings and14 isolated commit paths audit. Progress4,156/5,812 canonical primary
files named(71.5%),1,656 remaining; full-tree goal active.

Names cover paired snapshot objects, tenframe purewait, three-track crossfade,
actor-slot object spawn and weight/scalar/paired/scalartrack helpers. Complete
own-unit bodies/consumers establish boundedroles; no original flyby/ability/
provider labels or donor equality inferred. Calls retain own-unit native/runtime
identity through aliases; storedsnapshots are not livefollow sampling.

Preserve upper-only actorcheck, independent idcalls, partialstate, exact event
resource/customrecord order andhq=$16 pin. Three tracks share rotation/position
scratch; firsttrack scalarcrossfade from22 usesfourthhandle. Tintcounter10 starts
30 anddecrements beforegates; windowends40 withoutterminal sample. Unsignedh50
subtracts512 from24 everyframe, destroy atzero doesnotclear storedpointer and
laterframescontinue touching/wrapping it. Preserve NULLdestroy possibility,
finish60 afterfade, pairedoutput reversal and signedscalar/unsignedcount clamp.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Mesh burst naming acceptance (BV-03/BV-08, P2): FF9
682dac2bf adds10 canonical ovl_11d1e000 names. Catalog4,166 unique unit/symbol
names,90 alias headers; all10 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-mesh-burst.json
and target/ff9-names-mesh-burst/. All10 entire native object pairs identical,
exact affected/scored namespace10, unchanged pinned strict-relocation baselines
nine exact/one partial,3,740/8,524 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all10 sources; current catalog/review/source/header/
object bindings and14 isolated commit paths audit. Progress4,166/5,812 canonical
primary files named(71.7%),1,646 remaining; full-tree goal active.

Names cover fadingimpact callback, rotatedgroundimpact allocation,112particle
burst init/bouncing update,300particle arena init/load/free/spawn/update and
fourphase meshburst sequence. Complete own-unit bodies/GTE/lifecycles reviewed;
no original ability/provider labels asserted. Calls/callbackcasts retain own-unit
native/runtime identities through aliases.

Preserve mode1-only callback no-op, allothermodes draw, allocator NULLguard/pins/
barrier, packedhalfword input versusint*formal and savedjob id+16/id+8 view mismatch.
Bothpool loops derive relativevertices beforeintegration, but112 bounces/diesbelow
speed60/life71,300 diesfirstfloorhit/life32. Preserve randomcall evenwhenage>=8,
usedtriangle flags/deadstores/headern[2]/ptr[3]/signedcounts andzero-coordinate
stores excludingpadding. Sceneinit loads withuninitializedv20, fullresource/
texturebitfield/strength arithmetic, fourphasejobcounter resets/cumulativecounter
distinction, threeindependent cosinecalls, GTE/pins/hitpoint order, spawn<47,
300-before112 updates andfinish71 beforecommonresource/counter stayunchanged.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Capped point pairs naming acceptance (BV-03/BV-08, P2): FF9
52b030655 adds10 canonical ovl_131ef800 names. Catalog4,176 unique unit/symbol
names,91 alias headers; all10 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-capped-point-pairs.json
and target/ff9-names-capped-point-pairs/. All10 entire native object pairs
identical, exact affected/scored namespace10, unchanged pinned strict-relocation
baselines ten exact,2,872/2,872 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all10 sources; current catalog/review/source/header/
object bindings and14 isolated commit paths audit. Progress4,176/5,812 canonical
primary files named(71.9%),1,636 remaining; full-tree goal active.

Names cover three point-pair callbacks, capped position worker, full variant
jittered four-object sequence, timed single-resource handler and four host-kind
helpers. Own primary bodies and local consumers reviewed; no original ability/
provider labels asserted. Definitions, declarations, direct calls and callback
references use readable C names; alias preprocessing retains native/runtime
identities. Opaque address-valued data remains unchanged.

Preserve upper-only weight cap, negative weights/signed shifts, completion after
publication and distinct8-byte point records. Variant check has no lower bound;
p18-4 writes state padding. Exactly two random sign calls; jitter magnitude uses
signed FUNCTION POINTER remainder200, not a call. Halfword narrowing, partially
initialized vectors, all table row strides/integer hooks, redundant guards,
frame6/8 resource/marker order, variant-dependent host-kind calls, ended-pointer
clears and completion40 after updates stay unchanged. No host-provider meaning
or original missing-jalr assembly claim independently established here.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Starfield ribbons naming acceptance (BV-03/BV-08, P2): FF9
9a30dc1c3 adds10 canonical ovl_1219b000 names. Catalog4,186 unique unit/symbol
names,92 alias headers; all10 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-starfield-ribbons.json
and target/ff9-names-starfield-ribbons/. All10 entire native object pairs
identical, exact affected/scored namespace10, unchanged pinned strict-relocation
baselines five exact/five partial,8,796/13,196 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all10 sources; current catalog/review/source/
header/object bindings and14 isolated commit paths audit. Full-tree goal active.

Full main event sequence and history/particle/keyed-strip/GTE bodies reviewed,
including bounded rereads after initial main-body truncation. Names identify
point-track channels from actual halfword vector reads and host/unit point stores;
historical comment calling these sounds retained, no speculative sound meaning.
Other names cover history ribbon, full-width gradient band, camera-offset streak
particles,17/64-key textured strips and full starfield/ribbon sequence.

Preserve history live count before insertion, MAC0 gate without NCLIP, GTE depth
before fourthRTPS, early packet linking and partially initialized colors/vectors.
Sixteen channels consume vectors only for enabled nibbles, decrement before
publication and do not automatically free/rearm atzero. Gradient band firstlink
has TWO arguments. Particlewrap fixed8192;17/64 renderers project allkeys even
nzero, reserve fixedpacketbytes independentn,64 depth has no freshdepthcommand.
Main clears33flags via n1032, retains exact timers/events/NULL calls/stale pointers/
camera bounds/star random overwrites and steps point channels LAST.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Radial ring burst naming acceptance (BV-03/BV-08, P2): FF9
19056aad2 adds10 canonical ovl_11969000 names. Catalog4,196 unique unit/symbol
names,93 alias headers; all10 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-radial-ring-burst.json
and target/ff9-names-radial-ring-burst/. All10 entire native object pairs
identical, exact affected/scored namespace10, unchanged pinned strict-relocation
baselines four exact/six partial,1,104/10,052 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all10 sources; current catalog/review/source/
header/object bindings and14 isolated commit paths audit. Progress4,196/5,812
canonical primary files named(72.2%),1,616 remaining; full-tree goal active.

Full radial-grid renderer, four-phase ring burst, orbit/direction/history
callbacks,14-segment ribbon builder/five-frame callback, history-or-radial emitter
and three-phase pulse reviewed. Bounded main reread resolves initialtruncation;
names describe own-unit consumers, no original ability/provider label asserted.
Definitions/declarations/calls/callbackcasts map tostable native/runtimeidentities.

Preserve unchecked cols division/17-entry rowbuffers, UVclamp beforecallback,
two-page flags/packet chains/colorB partialcodebyte, ringindexreload and24wordpairs.
Orbitcallback keepshalfwordoverflow/threeindependent sinecalls; helperintformals
stillreceivepointers. Ribbonheaderbyte3 untouched,15records/124bytes andterminal
draw retained. Emitter actualthree-argumenthost200 preserved despite stalecomment,
count advancesbeforeallocation, fourindependent historyindices and INTstride
randomtableoffset stay. Fourphase keepspins/barrier/wordunion/Xoffset512/jobframe
resets/cumulativecounter/axispermutation/two sinecalls;pulse earlyterminalsample
andthree-phase timing stay. No nativebaseline cleanup or matchingcreditadded.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Six track offset naming acceptance (BV-03/BV-08, P2): FF9
4870eb84b adds10 canonical ovl_11093800 names. Catalog4,206 unique unit/symbol
names,94 alias headers; all10 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-six-track-offset.json
and target/ff9-names-six-track-offset/. All10 entire native object pairs
identical, exact affected/scored namespace10, unchanged pinned strict-relocation
baselines nine exact/one partial,944/2,352 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all10 sources; current catalog/review/source/
header/object bindings and14 isolated commit paths audit. Progress4,206/5,812
canonical primary files named(72.4%),1,606 remaining; full-tree goal active.

Six channel wrappers, shared half-X/Z then1.2-scaled/offset position worker,
halfword reader initialization/clamped paired-key fetch and full timed object
handler reviewed. Names describe local consumers; no original ability/provider
labels. Definitions/declarations/calls/callback references retain native/runtime
identities via own-unit aliases, opaque integer hooks unchanged.

Preserve wrapperunused thirdworkerarg/four-byte timerstride/eight-byte reader
stride and incompatible historicalextern signatures. Worker staged signed
division/narrowing/unsignedoffset/signedfixed publication stays; completion after
publication independentclamp. Reader leavesfirsthalfword, upper-only clamp and
empty/negative indexing stay. Handlerinit leavesobjectpointers/timercounts/seed
points/padding untouched;6starts20..30,selectedresources/3456-byte rows and
redundantguards stay. Standalonep24frame25 isneversteered; otherfour clearended
thenpublishconstantXYZ, counts incrementregardlessallocation, completion90
afterallupdates withoutnewhost78/destroy. Staleallfive comment retained.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Fixed height waypoint naming acceptance (BV-03/BV-08, P2): FF9
11b730c93 adds9 canonical ovl_10165800 names. Catalog4,215 unique unit/symbol
names,95 alias headers; all9 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-fixed-height-waypoint.json
and target/ff9-names-fixed-height-waypoint/. All9 entire native object pairs
identical, exact affected/scored namespace9, unchanged pinned strict-relocation
baselines nine exact,2,032/2,032 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all9 sources; current catalog/review/source/header/
object bindings and13 isolated commit paths audit. Full-tree goal active.

Complete fixed-height waypoint callback, two object handlers, path reader/fetcher
and four host-kind helpers reviewed. Callback publishesunoffset savedXYZ with
Y=-300 thenaddsXZoffsetonly; secondhandler usesactualsavedY withnofallback.
Preserve upper-only clamp/empty/negative indexing, readerfirsthalfword, partial
state, timedresource/host220 calls, redundantguards/integercallbackcasts and
completion40 afterhelpers. g13canonicaldeclaration retained; own-unit aliases
preprocesstostable symbols. No prototype/provider or original ability claim.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Blended strip fountain naming acceptance (BV-03/BV-08, P2): FF9
14e4a91f6 adds9 canonical ovl_130ae000 names. Catalog4,224 unique unit/symbol
names,96 alias headers; all9 complete primary bodies reviewed, no semantic
deferrals in this unit. Evidence: docs/function-names-blended-strip-fountain.json
and target/ff9-names-blended-strip-fountain/. All9 entire native object pairs
identical, exact affected/scored namespace9, unchanged pinned strict-relocation
baselines six exact/three partial,2,692/12,968 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all9 sources; current catalog/review/source/
header/object bindings and13 isolated commit paths audit. Full-tree goal active.

Complete four-phase blended-strip sequence and fountain, curve/ribbon builders,
particle callback, camera tracks, centered gradient and random vertex selector
reviewed; bounded main rereads coverinitialtruncation. Actualfivehandles and
thresholds15/9/43/20 retained despiteoldercomments. Preserve partialstate/union/
color/headerbytes, legacyint/voidprototype mismatch, random order/count, signed
remainders/INT-versusTRIPLE strides, upper-onlycamera clamp/Y-100 andnoaddedcleanup.
Fountain counters advancebeforeallocation;kind0 secondspawn starts11,kind1
secondpair starts13. Fullrecord28bytes, callbackpositioncopiedeachkind0frame.
Main stripseams/lowerclamps/hostcallorder/NULLdestroy/flags/pins/asmbarriers stay.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found; existing proposals remain separate. No private ownership/type/CFG walker,
new matching credit, linked image, gameplay, provider or shared workspace acceptance.
Historical reports/scorer stay pinned; refresh downstream source-bound evidence
from current catalog. Foreign work/index preserved; full-tree goal remains active.

Five phase ring burst naming acceptance (BV-03/BV-08, P2): FF9
bb576cd6c adds nine canonical ovl_1071e800 names. Catalog: 4,233 unique
unit/symbol names and 97 alias headers; all nine complete primary bodies
reviewed, no semantic deferrals in this unit. Other units remain pending.
Evidence: docs/function-names-five-phase-ring-burst.json and
target/ff9-names-five-phase-ring-burst/. Nine entire native object pairs
are identical, with exact affected/scored namespace nine. Pinned original
strict-relocation baselines unchanged: four exact, five partial,
1,104/10,320 code bytes, zero failures. Installed WASM preprocessed tokens
agree for all nine sources. Current catalog/source/header/object/review
bindings and 13 isolated committed paths audit. Full-tree goal remains active.

Complete radial grid, five-phase ring sequence, orbit/history/spark callbacks,
host direction helper and ribbon builder/driver/emitter reviewed. Full standalone
main reread covers initial truncated output. Preserve resource IDs 3/8/20,
phase durations 50/10/14/8/50, cumulative tick, per-frame ring-index reset,
the resource handle used as a sine argument, phase-three bitwise-OR scale,
signed halving, independent trig/random calls and allocation-attempt counts.
Retain two-formal/three-argument legacy direction prototype mismatch, partial
records/padding/header byte, unchecked grid dimensions, UV clamping before
callback, integer-stride history selections, pins/barriers and compiler flags.
Definitions, declarations, direct callers and callback references use readable
names; scoped identity aliases retain canonical compiler/linker identities.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin
adapters reused; BV-08 namespace gate enforced. No additional shared tooling
gap found. No private ownership/type/CFG walker, new matching credit, linked
image, gameplay, provider or shared workspace acceptance. Historical reports
and scorer remain pinned; refresh downstream evidence from current catalog.
Foreign work and index preserved; full-tree goal remains active.

Six object fade naming acceptance (BV-03/BV-08, P2): FF9
4124eb436 adds eight canonical ovl_12cac800 names. Catalog4,241 unique
unit/symbol names,98 alias headers. All eight complete own-unit primary
bodies reviewed, no semantic deferrals. Other units remain pending.
Evidence: docs/function-names-six-object-fade.json and
target/ff9-names-six-object-fade/. Eight entire native object pairs identical;
exact affected/scored namespace eight, unchanged pinned strict-relocation
baselines seven exact/one partial,1,928/2,948 code bytes,zero failures.
Installed WASM preprocessed tokens agree for all eight sources. Current
catalog/source/header/object/review bindings and12 isolated commit paths audit.
The audit adapter's expected header count corrected from88 to98; passed rerun.

Roles cover position-following object pair, lowered-start interpolation,
six-track object fades,indexed three-object hooks,weighted integer and host
blends,halfword reader and dual-vector key fetch. Preserve partial initialization,
legacy helper declarations,unsigned/signed halfword conversions,upper-only key
clamps,zero/negative index behavior,two-byte state walks,resource/hook tables,
threaded fade goto,tint condition after counter decrements and host call order.
No original game effect/provider identity inferred. Identifiers and aliases only;
canonical runtime exports retained,opaque data and matching tricks unchanged.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer remain
pinned; refresh downstream source-bound evidence from current catalog. Foreign
work/index preserved; full-tree goal remains active.

Ribbon particle shower naming acceptance (BV-03/BV-08, P2): FF9
a18d84fa7 adds eight canonical ovl_119b7800 names. Catalog4,249 unique
unit/symbol names,99 alias headers. All eight complete primary bodies reviewed,
including full main and callback; no semantic deferrals in this unit.
Evidence: docs/function-names-ribbon-particle-shower.json and
target/ff9-names-ribbon-particle-shower/. Eight entire native object pairs
identical,exact affected/scored namespace eight,unchanged pinned strict-relocation
baselines six exact/two partial,4,664/12,696 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all eight sources. Current catalog/source/
header/object/review bindings and12 isolated commit paths audit. Other units pending.

Names cover four active main phases6/7/8/10,ribbon builder,flying particle trail,
three-phase14-frame sprite pulse,mirrored/repeated/transformed textured quads
and24x24 radial ripple grid. Actual ribbon loop and late-transform loop each
execute once despite older three-buffer comments. Preserve job+16 word allocation,
particle threshold cubed,negative halfword write,partial vectors/padding,independent
trig/random calls,signed narrowing,host call order and main cumulative count.
Grid reserves576 packets but advances local packet pointer only on accepted cells;
NCLIP rejects zero only,link precedes fourth SXY store,depth sample before fourth
projection. No matrixrestore/bounds/cleanup/provider guesses added. All identifiers,
declarations,calls and callbacks alias back to canonical runtime symbols.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active.

Mesh fragment breakup naming acceptance (BV-03/BV-08, P2): FF9
a6f31eb30 adds seven canonical ovl_13387800 names. Catalog4,256 unique
unit/symbol names,100 alias headers. All eight complete primary bodies reviewed;
seven named,one explicit semantic deferral for the unknown slot354 wrapper.
Evidence: docs/function-names-mesh-fragment-breakup.json and
target/ff9-names-mesh-fragment-breakup/. Seven entire native object pairs
identical,exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines seven exact,5,460/5,460 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all seven sources. Current catalog/source/header/
object/review and deferred-source bindings,11 isolated commit paths audit.
Other units remain pending; full-tree goal remains active.

Names cover1980-slot triangle fragment pool,object record runs,first-free-slot
finder,triangle-centroid spawning,fragment motion/transformation,cosine-scaled
particle path and two-phase mesh breakup sequence. Actual caller/layout evidence
binds roles without guessing the original ability label or host provider.
Preserve unsigned size/24 and signed halfword totals,unchecked runs/indices,
centroid division before fixed shift,partial slot initialization,random order,
old-relative vertices before host motion update,new-Y floor test,GTE write pipeline,
triangle flags on release and retirement at128. Path Xvelocity grows17/16 while
Y decays7/8; output sampling precedes particle motion and terminal return.
Main state92,phase thresholds90/128,eight handles,partial colours/padding,
legacy int/void declarations,extra stepper argument,empty asm barrier and all
matching wrappers remain. Slot354 purpose deferred with source-bound reason.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved. Current release executable observed externally changed this turn;
the naming checks retain their existing scorer/compiler selection without rebuild.

Threshold mesh breakup naming acceptance (BV-03/BV-08, P2): FF9
c91a1aef4 adds seven canonical ovl_13522800 names. Catalog4,263 unique
unit/symbol names,101 alias headers. All eight complete primary bodies reviewed;
seven named,unknown slot354 wrapper deferred with source-bound reason.
Evidence: docs/function-names-threshold-mesh-breakup.json and
target/ff9-names-threshold-mesh-breakup/. Seven entire native object pairs
identical,exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines six exact/one partial,1,920/3,588 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review/deferred bindings and11 isolated commit paths audit.

Complete own-unit reviews establish1800-slot pool,seven record runs,unused
triangles above second-vertex depth threshold,fragment transform/update,
two-phase breakup and raised camera target roles. Preserve actual Yvelocity>>5
despite older /16 comment,uniformscale3712,host motion kind11,no floor retirement,
age60 and matrix call order. Main state100,threshold durations24/60,cutoff49,
seven handles,cursoroffset0xC5A8,exactscratchSP switch around spawn/update,
word vector/padding copies and partial initialization stay. Preserve original
global symbol casing,legacy caller/return mismatches,types,flags and asmstrings.
Identifiers/declarations/calls map through own-unit aliases to canonical exports.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active and other units pending.

Ribbon paired strips naming acceptance (BV-03/BV-08, P2): FF9
d75bb0319 adds eight canonical ovl_11813000 names. Catalog4,271 unique
unit/symbol names,102 alias headers. All eight complete own-unit primary bodies
reviewed,including both main handlers; no semantic deferrals in this unit.
Evidence: docs/function-names-ribbon-paired-strips.json and
target/ff9-names-ribbon-paired-strips/. Eight entire native object pairs identical;
exact affected/scored namespace eight,unchanged pinned strict-relocation baselines
five exact/three partial,2,676/10,044 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all eight sources. Current catalog/source/header/
object/review bindings and12 isolated commit paths audit. Other units pending.

Bounded roles cover two-phase rotating ring sparks,14-segment ribbon,two-segment
strip and polyline wrapper,paired strip callback,random ribbon vertex,
three-variant radial emitter and three-phase9-frame pulse. Actual headers/vertex
counts used rather than older segment comments. Preserve paired submissions at
the same destination,unchecked resource/point indices,done-after-builder callback
behavior,signed byte widths and remainders,INT-stride random table slices and
partial records. Variant2 advances counter twice before its first allocation,
updates history on allocation failure and retains terminal odd-frame random flash.
Keep independent trig calls,resource IDs,legacy declarations,flags,pins/wrappers.
No original game label/provider identity guessed. C references use readable
names while own-unit aliases retain canonical runtime/linker identities.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active.

Polygon center burst naming acceptance (BV-03/BV-08, P2): FF9
7d138fcb7 adds eight canonical ovl_10b72000 names. Catalog4,279 unique
unit/symbol names,103 alias headers. All eight complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence:
docs/function-names-polygon-center-burst.json and
target/ff9-names-polygon-center-burst/. Eight entire native object pairs identical;
exact affected/scored namespace eight,unchanged pinned strict-relocation baselines
eight exact,2,960/2,960 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all eight sources. Current catalog/source/header/object/review
bindings and12 isolated commit paths audit. Other units pending.

Complete own-unit table/center producers and consumers,paired-object burst/fade,
timed host object and two weighted blend helpers establish bounded roles.
Center-derived polygon offsets translate each polygon without scaling its local
shape. Preserve centroid halfword narrowing after EACH addition before division,
staged delta/center/vertex halfword arithmetic,signed overflow/division preconditions,
eight-kind loops reusing same table entries and center pointer,legacy INT/pointer
caller mismatches and matching wrapper. Main restores2600halfword vertex backup,
animates only unsignedframe<40,startsfade24,resource12update evenontterminal90,
partial state/padding and exactcalls unchanged. No original effect/provider guess.
Readable identifiers in definitions,declarations and calls retain canonical
runtime/linker identities through own-unit alias header.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active.

Bone ribbon ring burst naming acceptance (BV-03/BV-08, P2): FF9
5c13f17f3 adds eight canonical ovl_11bd3000 names. Catalog4,287 unique
unit/symbol names,104 alias headers. All eight complete own-unit primary bodies
reviewed; full36KB main read in contiguous1..290/291..590/591..end chunks.
No semantic deferrals in this unit. Evidence:
docs/function-names-bone-ribbon-ring-burst.json and
target/ff9-names-bone-ribbon-ring-burst/. Eight entire native object pairs
identical,exact affected/scored namespace eight,unchanged pinned strict-relocation
baselines six exact/two partial,2,380/17,048 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all eight sources. Current catalog/source/
header/object/review bindings and12 isolated commit paths audit. Other units pending.

Bounded roles cover12/4/56-segment ribbon builders,random polyline-edge strip,
three-row textured ring band,word-position rotation and six-phase bone-driven
ribbon/ring burst. Rotation role follows actual main consumers without claiming
hostF8 algorithm/provider. Preserve phase thresholds35/48/28/2/14/77,ten handles,
partial union/padding/packet bytes,signed remainders/random/trig order,legacy
INT/pointer declarations,resource IDs and terminal-before-common-tail behavior.
Ring packet reservation1944bytes,twoGT4 endpoints/46FT4s,NCLIPzero rejection,
depth sampling before fourth projection and linking before fourth SXY store
remain. Phase4 consumes randomY then overwrites it; cumulative frame is retained.
Definitions/declarations/callers use names with scoped canonical identity aliases.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active.

Cosine position path naming acceptance (BV-03/BV-08, P2): FF9
b08424c49 adds eight canonical ovl_11cd1800 names. Catalog4,295 unique
unit/symbol names,105 alias headers. All eight complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence:
docs/function-names-cosine-position-path.json and
target/ff9-names-cosine-position-path/. Eight entire native object pairs identical;
exact affected/scored namespace eight,unchanged pinned strict-relocation baselines
eight exact,1,776/1,776 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all eight sources. Current catalog/source/header/object/review
bindings and12 isolated commit paths audit. Other units pending.

Bounded roles cover four indexed destination callbacks/shared cosine-lowered
position worker,indexed object path,delayed object sprite fade and generic
buffer-data-byte store. Actual indexed position producers establish descriptors
without original actor labels. Preserve frame/18 independent of passed limit,
cosineY800/sign division,fixed publication before completion,void/int legacy
callback declarations and unchanged data callback table. Delayed sequence draws
before fading/clamping,keeps u16 arithmetic and partialvectors. Indexed rejection
short-circuits host2F8 calls,only checks upper index,storesbyte7 and returns early;
bufferflag meaning remains unresolved. Names retain canonical linker/runtime
identities through own-unit aliases; no type/layout/padding/bounds repair.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active.

Sprite debris and spiral trail naming acceptance (BV-03/BV-08, P2): FF9
035fe56f5 adds seven canonical ovl_13702800 names. Catalog4,302 unique
unit/symbol names,106 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence:
docs/function-names-debris-spiral-trails.json and
target/ff9-names-debris-spiral-trails/. Seven entire native object pairs identical;
exact affected/scored namespace seven,unchanged pinned strict-relocation baselines
four exact/three partial,2,320/11,088 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all seven sources. Current catalog/source/header/
object/review bindings and11 isolated commit paths audit. Other units pending.

Bounded roles cover four phase sprite/debris sequence,random horizontal radial
hook,debris callback,variable length curve strip builder,polyline vertex wrapper
and two variant spiral trail callback/emitter. Main registers both callback and
hook and publishes the trail anchors/radius/debris threshold. Preserve terminal
returns before common tail,shared scratch/register pins,partial vectors,signed
rand remainder,byte narrowing and counter/alloc order. Trail phase1 actually
advances frame*0x300 despite old0xC00 comment; strip flag128 brackets submission
after geometry generation; vertex wrapper reverses endpoints. Variant1 radius
becomes negative at frame29. All operation order/types/layouts/data/comments
unchanged; names scoped to own unit and retain linker/runtime identities.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active.

Ten object keyframe naming acceptance (BV-03/BV-08, P2): FF9
79ea16157 adds seven canonical ovl_10673800 names. Catalog4,309 unique
unit/symbol names,107 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence:
docs/function-names-ten-object-keyframes.json and
target/ff9-names-ten-object-keyframes/. Seven entire native object pairs identical;
exact affected/scored namespace seven,unchanged pinned strict-relocation baselines
seven exact,1,556/1,556 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all seven sources. Current catalog/source/header/object/review
bindings and11 isolated commit paths audit. Other units pending.

Actual consumer establishes ten round-robin object transforms,mirrored paired
vector track,scale track and mirrored position track. Actor position sampled
only at init; no continuous following claim. Three identical stream initializers
gain names for their actual table consumers; preserve HalfReader versus caller
KeyTab prototypes. Readers clamp only upper index,retain negative/zero-count
behavior,conditional output writes,unsigned negation/narrowing and signed scale
widening. Main transforms only signed frames0..44,always samples position and
moves pointer even if cleared,then completes60 after movement. Opaque callback
data unchanged; preserve pads/redundantnullchecks/all types and operation order.
Own-unit aliases retain canonical linker/runtime identities.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active.

Three phase ring and ribbon naming acceptance (BV-03/BV-08, P2): FF9
86bbdb707 adds seven canonical ovl_108a6000 names. Catalog4,316 unique
unit/symbol names,108 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence:
docs/function-names-three-phase-ring-ribbons.json and
target/ff9-names-three-phase-ring-ribbons/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines five exact/two partial,2,676/9,872 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review bindings and11 isolated commit paths audit. Others pending.

Actual roles: three phase ring/spark burst,14segment ribbon,two segment strip,
polyline wrapper,paired strip callback,random ribbon vertex hook,three independent
radial emitter variants. Preserve burst29/2/50 transitions,volatile reload,
separate division then shifts,repeated trig calls,16sparks underflag32. Paired
strips reuse same destination and complete only after submission; actual builder
has3vertices/2segments despite oldcomment. Emission counts precede allocations,
INT pool offsets/signed biases retained. Variant2 history/localvector published
even first allocation failure; preserve original uninitialized local possibility,
oddframe randomflash even at terminal31,all types/layouts/pins/wrappers/data.
No original effect/provider identity claim; own-unit aliases keep linker/runtime.

Existing maintained naming/farm/scoring/preprocessor interfaces and thin adapters
reused; BV-08 namespace gate enforced. No additional shared implementation gap
found. No private ownership/type/CFG walker,new matching credit,linked image,
gameplay,provider or shared workspace acceptance. Historical reports/scorer pinned;
refresh downstream source-bound evidence from current catalog. Foreign work/index
preserved; full-tree goal remains active.

Seven step position naming acceptance (BV-03/BV-08, P2): FF9
b4db001c4 adds seven canonical ovl_1307e800 names. Catalog4,323 unique
unit/symbol names,109 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-seven-step-position.json
and target/ff9-names-seven-step-position/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines seven exact,1,916/1,916 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all seven sources. Current catalog/source/header/
object/review bindings and11 isolated commit paths audit. Other units pending.

Actual roles: actor-position resource1 spawn/wait40,seven-step actor offset
position blend,fixed-weight wrapper,scalar interpolation,host vector delta and
derived-object vector steppers. Actor position sampled at init only. Preserve
radius1300/total7,legacy pointer/INT mismatch,frame0 host reads,publication before
completion,duplicate20C atstep0,unused1FC and scratch,halfword narrowing,signed
division and asymmetric Y adjustment4096-Y orY+4096. No provider/vector-kind
assumption or type/bounds repair; unit aliases retain linker/runtime identities.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Twelve step position naming acceptance (BV-03/BV-08, P2): FF9
3db56220e adds seven canonical ovl_11558800 names. Catalog4,330 unique
unit/symbol names,110 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-twelve-step-position.json
and target/ff9-names-twelve-step-position/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines seven exact,1,916/1,916 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all seven sources. Current catalog/source/header/
object/review bindings and11 isolated commit paths audit. Other units pending.

Actual roles: actor-position resource1 spawn/wait40,twelve-step actor offset
position blend,fixed-weight wrapper,scalar interpolation,host vector delta and
derived-object vector steppers. Actor position sampled at init only. Preserve
radius800/total12,legacy pointer/INT mismatch,frame0 host reads,publication before
completion,duplicate20C atstep0,unused1FC and scratch,halfword narrowing,signed
division and asymmetric Y adjustment4096-Y orY+4096. No provider/vector-kind
assumption or type/bounds repair; unit aliases retain linker/runtime identities.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Recorded path crossfade naming acceptance (BV-03/BV-08, P2): FF9
dc2caa3a3 adds seven canonical ovl_1380a000 names. Catalog4,337 unique
unit/symbol names,111 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence:
docs/function-names-recorded-path-crossfade.json and
target/ff9-names-recorded-path-crossfade/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines seven exact,3,036/3,036 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all seven sources. Current catalog/source/header/
object/review bindings and11 isolated commit paths audit. Other units pending.

Actual roles: reverse keyframe ribbon builder/fade sequence,two handle crossfade
and seven object sequence,fixed/scalar interpolation,halfword track initialization
and position reader. Preserve header padding,prefixrecord0,reverseactiveindices,
signed6/10 scale,narrowing,GTEcalls/matrixbuildEACHvertex,partialGTE8byteaccess.
Ribbon33vertices grows4perframe,capsbybuilder,unsignedfade subtract1024before
draw24..31 wraps,angleprovider executes even beyond draw window. Crossfade
weight counter,initsYoverwrites,cumulativeearlyY/sway,frame8loadsevenopaque
objects,independentfadewrites and tintonlyfirsthandle remain. Reader upperonly
clamp/zero-count/negative indices and opcode-skip initializer types unchanged.
No gameplay/provider identities asserted; unit aliases retain linker/runtime.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Scheduled ribbon pulse naming acceptance (BV-03/BV-08, P2): FF9
df5a2f971 adds seven canonical ovl_117d7000 names. Catalog4,344 unique
unit/symbol names,112 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-scheduled-ribbon-pulses.json
and target/ff9-names-scheduled-ribbon-pulses/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines five exact/two partial,3,308/6,180 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review bindings and11 isolated commit paths audit. Others pending.

Actual builders16vertices/15segments and4vertices/3segments despite oldcomments;
INTtriples and wrapperstep55. Two-phase driver retainspairedsamebuffer draws,
unsignedbias/frame0resources24/2,precedinghalfword transition. Scheduledemitter
retainskindtable signednegativeindex,counts/jitterbeforeallocation,kind3samecount
twospawns,unknownpositivekindpartialfield andINTpoolstride. Actualpulse callback
usesa2 despite oldunusedcomment; resources26/25/29/27/28 and two-word position
copy unchanged. Spritepulse phases3/9/9,provider/trigbeforeterminal9,flag128draw
tail skippedoncompletion. Pins/types/pads/wrappers/data preserved,unitaliases
retain linker/runtime identities; no originalgameeffect/provider identity.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Drifting threshold fragment naming acceptance (BV-03/BV-08, P2): FF9
c8a1c9315 adds six canonical ovl_1357c000 names. Catalog4,350 unique
unit/symbol names,113 alias headers. All seven complete primary bodies reviewed;
slot354 handler8734 semantically deferred because actualproviderpurpose unresolved.
Evidence: docs/function-names-drifting-threshold-fragments.json and
target/ff9-names-drifting-threshold-fragments/. Six entire native object pairs
identical; exact affected/scored namespace six,unchanged pinned strict-relocation
baselines five exact/one partial,1,956/4,144 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all six sources. Current catalog/source/header/
object/review/deferral bindings and10 isolated commit paths audit. Others pending.

Actual1700slot pool/eighttriangle runs,Ythreshold vertex1spawner,drifting32frame
GTE mesh update and44/130two phase eight-handle sequence established. Preserve
UPPERCASEglobals versuslowercaseexterns,W29Pool1980typeusedonlyprefix,centroid
signedshift/randomorder,host08 vectorprovider/deriveddrift (no directvelocity
integration claim),uniformscale3968,triangleXYZclear butpadflagsretained. Main
identicalifarms on uninitializedt,partialvectors,Yoverwrites,resourcecallorder,
threshold2200/signedshift,SCRATCHSP0x1F8003F8 aroundspawn/update andterminal130
beforecountertail remain. Slot354numericpurpose insufficientforsemanticname;
bodyunchanged/hashbound. Unit aliases retain canonical linker/runtime identities.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Ring ribbon history naming acceptance (BV-03/BV-08, P2): FF9
070864119 adds seven canonical ovl_fb5e000 names. Catalog4,357 unique
unit/symbol names,114 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-ring-ribbon-history.json
and target/ff9-names-ring-ribbon-history/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines four exact/three partial,1,740/9,604 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review bindings and11 isolated commit paths audit. Other units pending.

Actual threephase29/2/50ring/spark burst,14segment ribbon/twosegment strip,
polyline wrapper,paired samebuffer callback,random ribbon vertex hook and three
independent radial/history emitter variants established. Preserve mainv60second
word read froma8+4halfwords (followingphaseword,notZ/pad),alternatingspark6144/2048
size andfp&7 beforeincrement,partialvectors/pins/repeatedtrig/resources. Emitter
variant2 incrementscount twice beforefirstactualallocation,computesradialpoint
beforeallocation,historypublishes evenfail,extraobject<19,terminaloddflash31.
Signedbiases/INTpoolstride/wordcopies anddifferentvariantconstants remain.
No siblingbody substitution/type/layout/boundsrepair; unit aliases retain
canonical linker/runtime identities, gameeffect/provider purpose unresolved.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Bone sparks vertical fade naming acceptance (BV-03/BV-08, P2): FF9
15e6859f1 adds seven canonical ovl_129bd800 names. Catalog 4,364 unique
unit/symbol names,115 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-bone-sparks-vertical-fade.json
and target/ff9-names-bone-sparks-vertical-fade/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines five exact/two partial,2,316/7,268 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review bindings and11 isolated commit paths audit. Other units pending.

Established vertical center gradient,radial spark initialization,projected spark
lines,21-bone keyframes,advancing Z callback,four-phase bone/spark sequence and
three host-point tracks. Preserve actual spark wrap bounds and projection before
Z motion,current/previous XY history,sentinel seeding,only screen-H restoration,
upper-only track bounds and bone16..18 skipping after lookup. Main scratch vectors
overlap; common tail uses new phase after transitions,particle counters/random
advance only on success,terminal169 returns before common draw/count. Exact
game-effect/provider identity unresolved; comments,types,layouts,pins,partial
state and oldstyle calls preserved. No sibling body substitution or body repair.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Ten phase ripple particles naming acceptance (BV-03/BV-08, P2): FF9
37d6be513 adds seven canonical ovl_12847800 names. Catalog 4,371 unique
unit/symbol names,116 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-ten-phase-ripple-particles.json
and target/ff9-names-ten-phase-ripple-particles/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines five exact/two partial,3,856/20,760 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review bindings and11 isolated commit paths audit. Other units pending.

Established mirrored sprite pair,repeated rows,transformed sprite,24x24radial
ripple grid,random24segment ribbon,five-kind particle callback and ten actual
phases0..8/10. Comments eleven phases/two tailbands differ from actual body;
names follow body. Preserve cube proximity threshold,predecessorhalfword[-1],
case2 overwritten color used as cosine argument,partial XYZ/pad and overlapping
scratch union,mode0 goto,unused randomcalls,allocation success timing and
new-phase scrollband gate. Grid reserves576slots but advances emitted pointer
only admitted cells,and links before fourth-corner SXY store. Native symbols/
runtime exports retain canonical identities. No sibling-body replacement or
collision/radius/type/layout/tint/body fixes. Game-effect/provider purpose unresolved.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Debris variable trails naming acceptance (BV-03/BV-08, P2): FF9
fceb0432b adds seven canonical ovl_12b63800 names. Catalog 4,378 unique
unit/symbol names,117 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-debris-variable-trails.json
and target/ff9-names-debris-variable-trails/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines four exact/three partial,2,320/11,092 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review bindings and11 isolated commit paths audit. Other units pending.

Established horizontal radial hook,debris callback,four phase sprite/debris
burst,variable-length strip,polyline wrapper and two independent trail variants.
Preserve Xsign/Z-dependent spin,predecessorhalfword[-1],17/5 actual strip vertices,
phase1 frame<<10,flag128 set only after vertex loop,unchecked size/divisor and
INT/void declaration mismatch. Main retains four-halfword read from three-halfword
global,signed s0 narrowed before halfscaling,halfword jitter preserving adjacentY,
nullable f1D8 call,extra-word rand(sy),24 initial particles and two largeparticles
at40,terminal49 before common flag/overlay/count. Spiral radius remains negative
at29 and counter increments before allocation. No types/layouts/bounds/body fixes;
native/runtime identities stable,exact game-effect/provider identities unresolved.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Countdown gradient strips naming acceptance (BV-03/BV-08, P2): FF9
70bf420f7 adds seven canonical ovl_1110e800 names. Catalog 4,385 unique
unit/symbol names,118 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-countdown-gradient-strips.json
and target/ff9-names-countdown-gradient-strips/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines three exact/four partial,796/12,156 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review bindings and11 isolated commit paths audit. Other units pending.

Established linear/quadratic decay,byte-countdown sprite-strip/gradient timeline,
8x4XZgrid,three tiledgradients/fill,fullscreen warmgradient,hostgrayscale and
recorded-point fixed-target callback. Preserve unconditional squared-divisor,
void definition consumed through int declaration,initial next-strip pointer
submission,t12 next-entry color reads,unsigned particle velocities,byte timers
and extra-word hostcalls. Grayscale helper makes one call with three equal RGB
arguments despite oldcomment. Main returns>=308 after fulltail; path publishes
before terminal. Source scene labels are not accepted game identity claims.
Partial vectors,pins,casts,redundant scales,flags and call/random order untouched.
No body/type/layout/bounds/ABI repair; unit aliases retain native/runtime symbols.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Wavy band ribbons naming acceptance (BV-03/BV-08, P2): FF9
775d14be2 adds seven canonical ovl_11f99000 names. Catalog 4,392 unique
unit/symbol names,119 alias headers. All seven complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-wavy-band-ribbons.json
and target/ff9-names-wavy-band-ribbons/. Seven entire native object pairs
identical; exact affected/scored namespace seven,unchanged pinned strict-relocation
baselines one exact/six partial,164/15,144 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all seven sources. Current catalog/source/
header/object/review bindings and11 isolated commit paths audit. Other units pending.

Established segment-history ribbon,32x16 wavy XYgrid,44segment wavy band,
two-phase particle callback,seven-phase band/ribbon sequence,host-particle
wobble and lowered host-point initializer. Geometry names follow actual advancing
X and XY coordinates; comments ring/cone/spiral/water/cloth are not accepted game
identity claims. Preserve history count before newhead,MAC0 guard without added
NCLIP,link before fourth-SXY/color stores,shared last-active-slot amplitude,
cosine motion with captured XYZ before randomdrift,unchecked bytecount,partial
vectors and seven phase transitions. Main phase0 resets localt before common
tail; final32 releases27handles then returns before tail/count. Extra handles,
global initialization and bounds are not repaired. Native/runtime symbols stable.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

History ribbon mesh burst naming acceptance (BV-03/BV-08, P2): FF9
587df0dcd adds six canonical ovl_13147800 names. Catalog 4,398 unique
unit/symbol names,120 alias headers. All six complete primary bodies reviewed;
no semantic deferrals in this unit. Evidence: docs/function-names-history-ribbon-mesh-burst.json
and target/ff9-names-history-ribbon-mesh-burst/. Six entire native object pairs
identical; exact affected/scored namespace six,unchanged pinned strict-relocation
baselines three exact/three partial,1,408/8,408 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all six sources. Current catalog/source/
header/object/review bindings and10 isolated commit paths audit. Other units pending.

Established segment-history ribbon,130-triangle fragment initialization/update,
ten-cycle arc particle callback,five-phase ribbon mesh burst and slot16 scalar fade.
Preserve history count before head publication,MAC0 guard without addedNCLIP,
link before fourthSXY/colors,initializer/update signedness,relativevertices before
delegate and translation afterdelegate,partialfields and predecessorhalf writes.
Actual sequence timings93/11/12/29/final100 are retained despite different comments.
RandomY overwritten later remains called; terminal cleanup returns before framecount.
Provider34C at75 has no invented role; slot16 scalar may turn negative,not clamped.
Readable definitions,declarations,calls and callbacks map to canonical native/runtime
symbols via own-unit alias header. No provider identity or original spell guess.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional shared implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider or shared workspace acceptance. Historical scorer/reports pinned;
refresh downstream source-bound evidence. Foreign work/index preserved;
full-tree goal remains active.

Keyed point track ribbons naming acceptance (BV-03/BV-08, P2): FF9
f2faf973b adds six canonical ovl_10f67800 names. Catalog4,404 unique unit/symbol
names,121 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-keyed-point-track-ribbons.json and
target/ff9-names-keyed-point-track-ribbons/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
five exact/one partial,1,756/5,184 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established16-channel point-track reset/queue/update,decay blend,keyed paired
ribbon sequence and party-member object fade. Preserve nibble selectors,stream
consumption only for selected destinations,pinned registers,partial fields,
VOID definition/INT caller declaration mismatch,nullable object submission,
negative fade and terminal channel update. Exact game/provider identities are
unresolved; no body/type/layout/signature/ABI repair. Readable references alias
back to canonical native/runtime symbols within the unit.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Keyed ribbons actor spawn naming acceptance (BV-03/BV-08, P2): FF9
15adf1b76 adds six canonical ovl_1394d000 names. Catalog4,410 unique unit/symbol
names,122 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-keyed-ribbons-actor-spawn.json and
target/ff9-names-keyed-ribbons-actor-spawn/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
five exact/one partial,1,120/4,548 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established16-channel point-track reset/queue/update,decay blend,keyed paired
ribbon sequence and per-actor resource spawn/wait48. Each own-unit body read
completely; preserve actual resources3/4/5 and29 ribbon,which differ from
transplant comments. Preserve e2=$23 pin,buffer-write order/nesteddo wrappers,
VOID definition/INT caller declaration mismatch,nullable object submission,
partial vectors,terminal channel update and unsigned actor count without clamp.
Exact game/provider identities unresolved; no body/type/layout/signature repair.
Readable references alias to canonical native/runtime symbols within the unit.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Random ribbon ring sequence naming acceptance (BV-03/BV-08, P2): FF9
56fa3009a adds six canonical ovl_12ee2000 names. Catalog4,416 unique unit/symbol
names,123 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-random-ribbon-ring-sequence.json and
target/ff9-names-random-ribbon-ring-sequence/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
four exact/two partial,1,560/11,044 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established randomized12/56-segment ribbon builders/submission,three-row
textured band,word-position rotation evaluation and three-phase ribbon/ring
sequence. Preserve phaseIDs0/4/5,random overwritten values and extra arguments,
partial/uninitialized vector words,VOID/INT declarations,link before fourthSXY,
nonzeroNCLIP guard,pins/do wrappers and terminal return before common tail/count.
Exact game/provider identities unresolved;no body/type/layout/signature repair.
Readable references alias to canonical native/runtime symbols within the unit.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Slot16 model position naming acceptance (BV-03/BV-08, P2): FF9
6279a7b97 adds six canonical ovl_12cbd000 names. Catalog4,422 unique unit/symbol
names,124 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-slot16-model-position.json and
target/ff9-names-slot16-model-position/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
six exact,1,656/1,656 code bytes,zero failures. Installed WASM preprocessed tokens
agree for all six sources; current catalog/source/header/object/review and10
isolated commit paths audit. Other units remain pending.

Established slot16 interpolated model sequence,30-frame actor object sequence,
slot16 position blend/wait,host-position blend,fixed weights and scalar lerp.
Preserve actualYZ angle inputs,unsignedhalf handle,partial vectors,six-byte local,
externaljobframe versusinternalcur,emptyasm,unguarded division/extrapolation,
equality-only waittermination and terminalcallordering. No body/type/layout/
signature repair or original game/provider identity guess. Readable references
alias to canonical native/runtime symbols within the unit.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Actor jitter anchors naming acceptance (BV-03/BV-08, P2): FF9
e79b0ec28 adds six canonical ovl_10130800 names. Catalog4,428 unique unit/symbol
names,125 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-actor-jitter-anchors.json and
target/ff9-names-actor-jitter-anchors/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
six exact,1,940/1,940 code bytes,zero failures. Installed WASM preprocessed tokens
agree for all six sources; current catalog/source/header/object/review and10
isolated commit paths audit. Other units remain pending.

Established four anchorX-/+200/400 jitter callbacks,shared jitter/fixed-point
output blend and60-frame actor offset object sequence. Preserve six-byte locals,
unused callback args,VOID wrappers without return forwarding,signed randommodulo,
two-stepdivision,unsignednarrowing,partialfields and terminalwork-before-return.
Only identifiers/header change;native/runtime symbols and dataword callbacks stable.
Exact original game/provider identities unresolved;no body/type/layout/ABI repair.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Keyed ribbons actor fades naming acceptance (BV-03/BV-08, P2): FF9
a01c8af53 adds six canonical ovl_139f0000 names. Catalog4,434 unique unit/symbol
names,126 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-keyed-ribbons-actor-fades.json and
target/ff9-names-keyed-ribbons-actor-fades/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
five exact/one partial,1,772/5,200 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established16-channel track reset/queue/update,decay blend,keyed paired ribbons
and per-actor resource/two-scalar-fade sequence. Eachownbody completelyread;
actualresources20/19/21 and18ribbon retained despitetransplantcomment differences.
Preserve p2/e1pins,bufferordering,VOID/INT mismatch,nullable submission,partial
vectors and terminalchannelstep. Actorfade resource13 scaleswithslot0 rather
thani;twoindependentcounters decrementbeforeloops andterminalreturnwhileBpositive.
No body/type/layout/ABI repair or exactgame/provideridentity guess. Readable
references alias to canonical native/runtime symbols within the unit.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Wavy grid history sequence naming acceptance (BV-03/BV-08, P2): FF9
c21f57820 adds six canonical ovl_119ff800 names. Catalog4,440 unique unit/symbol
names,127 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-wavy-grid-history-sequence.json and
target/ff9-names-wavy-grid-history-sequence/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
one exact/five partial,56/9,828 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established RGB555 Gouraud grid/texture upload,animated textured history ribbon,
wavy grid with fading bottom row,five-value callback,pair-scale callback and
five-phase sequence. Preserve rowpacket reuse,MAC0 guard without addedNCLIP,
history count beforehead,link beforefourthSXY/UV,fullverticalproducts,pins,
partialfields,provider/callback arities,globalX baseforbothorbitcoordinates,
phase2duration159 and earlyterminaltail omission. Callbackobjectreference renamed;
canonical native/runtime identities retained. No originaleffect/providerguess.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Five ribbon particle pulse naming acceptance (BV-03/BV-08, P2): FF9
feacf0172 adds six canonical ovl_1179f800 names. Catalog4,446 unique unit/symbol
names,128 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-five-ribbon-particle-pulse.json and
target/ff9-names-five-ribbon-particle-pulse/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
four exact/two partial,2,668/4,452 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established15/3-segment builders,polyline strip,two-phase ribbon callback,
five-attempt particle scheduler and three-phase sprite pulse. Preserve three-word
table stepping,signed mode/index,paired samebuffer writes,pins,predecessorhalf,
count-before-allocation includingfailures,successful-onlyrandoms,partialfields,
mode>=2earlycompletion and pulse provider-before-terminalreturn/tailomission.
Descriptorcallback reference renamed,canonicalnative/runtime identities stable.
No body/type/layout/ABI repairs or originalgame/provideridentity claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Keyed ribbons and particle burst naming acceptance (BV-03/BV-08, P2): FF9
0bb8b85ca adds six canonical ovl_139a1000 names. Catalog4,452 unique unit/symbol
names,129 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-keyed-ribbons-particle-burst.json and
target/ff9-names-keyed-ribbons-particle-burst/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
four exact/two partial,864/6,584 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established point-track reset/queue/step,decay blend,keyed paired ribbons and
twelve-group rotating particle burst. Preserve own-unit resource choices/e2pin,
write order,VOID/INT declaration mismatch,nullable submission,explicit48resource
calls,shrinking radius,delay/life updates before rendering,partialfields and
terminal ordering. Definitions/declarations/callers renamed; alias headers keep
canonical native/runtime identities. Exact game effect/provider identities unresolved.
No body/type/layout/ABI repairs or original developer name claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Scheduled ribbon particles and sprite pulse naming acceptance (BV-03/BV-08, P2):
FF9 f7197a8ec adds six canonical ovl_f911800 names. Catalog4,458 unique unit/symbol
names,130 alias headers. All six complete bodies and both g08/g09 headers reviewed;
no semantic deferrals. Evidence: docs/function-names-scheduled-ribbons-sprite-pulse.json
and target/ff9-names-scheduled-ribbons-sprite-pulse/. Six entire native objects
identical;exact affected/scored namespace six,unchanged pinned strict-relocation
baselines four exact/two partial,2,140/5,316 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established ribbon/shortstrip/polyline helpers,two-phase ribbon callback,cosine
sprite/expanding ring pulse and scheduled particle sequence. Preserve signed
mode/table indices,three-word table strides,pads,pins,predecessorhalf,allocation
failure/random ordering,two callback arities and terminal ordering. Definitions,
declarations,calls,descriptor and resource callback references renamed; own-unit
aliases retain canonical native/runtime identities. Exact game effect/provider
identities unresolved. No body/type/layout/ABI repairs or original-name claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Timed resource sequences and host object kinds naming acceptance (BV-03/BV-08, P2):
FF9 5bfa2dfe1 adds six canonical ovl_120bf000 names. Catalog4,464 unique unit/symbol
names,131 alias headers. All six complete bodies,threeheaders and four reference
helpers reviewed; no semantic deferrals. Evidence:
docs/function-names-resource-sequences-host-kinds.json and
target/ff9-names-resource-sequences-host-kinds/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
all six exact,1,552/1,552 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all six sources; current catalog/source/header/object/review
and10 isolated commit paths audit. Other units remain pending.

Established five-resource/host-kind and transformed-anchor three-resource
timelines,apply-kind/last-index/conditional-kind2/3 helpers. Preserve partial
fields,rawdata callbacks,redundantnullcheck,provider ordering,signed indices,
terminal updates and signatures. Consistent helper names independently checked
against own-unit bodies and reviewed ovl_10165800 references. Own-unit aliases
retain canonical native/runtime identities. Exact game effect/provider identities
unresolved. No body/type/layout/ABI repairs or original developer name claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Six object paired keyframes naming acceptance (BV-03/BV-08, P2): FF9
f16fdbab1 adds six canonical ovl_118f9000 names. Catalog4,470 unique unit/symbol
names,132 alias headers. All six complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-six-object-paired-keyframes.json and
target/ff9-names-six-object-paired-keyframes/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
five exact/one partial,2,208/3,040 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established six-object paired-track crossfade,mirrored pair/scale tablehelpers
and three-phase sprite ring pulse. Preserve upper-only clamp,signature differences,
pads,initialanchor,handlepairs,nullablecall,pins,terminalordering and actual
resource choices. No inferred actor-follow despite donor comments. Own-unit
aliases retain canonical native/runtime identities. Exact game effect/provider
identities unresolved. No body/type/layout/ABI repairs or original developer names.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Scheduled ribbons and actor trails naming acceptance (BV-03/BV-08, P2):
FF9 a6da78146 adds six canonical ovl_102de000 names. Catalog4,476 unique unit/symbol
names,133 alias headers. All six complete bodies and g08/g14 headers reviewed;
no semantic deferrals. Evidence: docs/function-names-scheduled-ribbons-actor-trails.json
and target/ff9-names-scheduled-ribbons-actor-trails/. Six entire native objects
identical;exact affected/scored namespace six,unchanged pinned strict-relocation
baselines two exact/four partial,868/5,644 code bytes,zero failures. Installed
WASM preprocessed tokens agree for all six sources; current catalog/source/header/
object/review and10 isolated commit paths audit. Other units remain pending.

Established ribbon builders/polyline helper,cosine sprite/expanding ring pulse,
two-phase ribbon callback,scheduled particles and per-actor trails fromframe34.
Preserve byvalue/flat argument and VOID/INT differences,pins,barriers,wrappers,
signedindices,unsignedrounding,randomcallarity,allocation-failure ordering,pads,
b15specialcase and terminalactorattempt. Callback/caller references renamed;
own-unit aliases retain canonical native/runtime identities. Exact game effect/
provider identities unresolved. No body/type/layout/ABI repairs or original names.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Spinning model and grayscale naming acceptance (BV-03/BV-08, P2): FF9
282253ee8 adds six canonical ovl_103f5800 names. Catalog4,482 unique unit/symbol
names,134 alias headers. All six complete bodies,bothheaders and four reference
helpers reviewed; no semantic deferrals. Evidence:
docs/function-names-spinning-model-grayscale.json and
target/ff9-names-spinning-model-grayscale/. Six entire native objects identical;
exact affected/scored namespace six,unchanged pinned strict-relocation baselines
all six exact,2,140/2,140 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all six sources; current catalog/source/header/object/review
and10 isolated commit paths audit. Other units remain pending.

Established spinning model shrink/color blend,three-resource spawn timeline,
grayscale fade cycle and interpolation helpers. Preserve signedvalues,partial
fields,rawcallbacks,pins/asm,pose-before-rotation ordering,unclampedscalar math
and terminal calls. Negativecolor blend is observed,not assumedstandardfade.
Own-unit aliases retain canonical native/runtime identities. Exact game effect/
provider identities unresolved. No body/type/layout/ABI repairs or original names.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Six object keyframes and scene point naming acceptance (BV-03/BV-08, P2):
FF9 b8f6b035f adds five canonical ovl_10556800 names. Catalog4,487 unique unit/symbol
names,135 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-six-object-scene-point.json and
target/ff9-names-six-object-scene-point/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
all five exact,2,248/2,248 code bytes,zero failures. Installed WASM preprocessed
tokens agree for all five sources; current catalog/source/header/object/review
and9 isolated commit paths audit. Other units remain pending.

Established six-object paired-track crossfade with first-track scene-point writes,
mirrored pair/scale tablehelpers. Preserve upper-only clamp,signature differences,
pads,initialanchor,handlepairs,scene-write ordering,nullablecall and terminal
ordering. Scene receives first position before scale/pose; secondposition does
not overwriteit. No inferred actor-follow. Own-unit aliases retain canonical
native/runtime identities. Exact game effect/provider identities unresolved.
No body/type/layout/ABI repairs or original developer name claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Screen rings and grayscale capture naming acceptance (BV-03/BV-08, P2):
FF9 4c745fc0a adds five canonical ovl_10b42000 names. Catalog4,492 unique unit/symbol
names,136 alias headers. All five complete bodies/g17 header reviewed,no semantic
deferrals. Evidence: docs/function-names-screen-ring-grayscale-capture.json and
target/ff9-names-screen-ring-grayscale-capture/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
three exact/two partial,1,948/4,392 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all five sources; current catalog/source/header/
object/review and9 isolated commit paths audit. Other units remain pending.

Established four-phase ring particles,twelve-attempt scheduler,grayscale screen
capture/bands,sixteen-segment capture-textured ring and capture/ring fade timeline.
Preserve pins/asm,failedallocation counts,phase-transition table choice,pads,
packet/tag/write order,signed dimensions,fixed radius and terminalrender. Alias
headers retain canonical native/runtime identities. Exact game effect/provider
identities unresolved. No body/type/layout/ABI repairs or original developer names.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Six phase scene and bone ribbons naming acceptance (BV-03/BV-08, P2):
FF9 348873969 adds five canonical ovl_11b3c000 names. Catalog4,497 unique unit/symbol
names,137 alias headers. All five complete bodies reviewed,including fulllarge
80D4 scene timeline; no semantic deferrals. Evidence:
docs/function-names-six-phase-bone-ribbons.json and
target/ff9-names-six-phase-bone-ribbons/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
three exact/two partial,1,428/12,472 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all five sources; current catalog/source/header/
object/review and9 isolated commit paths audit. Other units remain pending.

Established textured history ribbon,randomized12/48-segment ribbon builders,
sine-offset callback and six-phase scene/camera/bone-ribbon sequence. Preserve
sharedstack aliases,pins/wrappers,partialfields,pads,provider/signature differences,
phase-transition oldlocals,bufferreuse,MAC0/SZ3 gates and terminaltail/count
ordering. Callback/caller references renamed; own-unit aliases retain canonical
native/runtime identities. Exact game effect/provider identities unresolved.
No body/type/layout/ABI repairs or original developer name claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Slot16 sprites and twelve point spark trails naming acceptance (BV-03/BV-08, P2):
FF9 5a9d000b0 adds five canonical ovl_120e4800 names. Catalog4,502 unique unit/symbol
names,138 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-slot16-twelve-point-sparks.json and
target/ff9-names-slot16-twelve-point-sparks/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
three exact/two partial,3,092/3,756 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all five sources; current catalog/source/header/
object/review and9 isolated commit paths audit. Other units remain pending.

Established slot16 sprite/model timeline,eight-frame callback,emission-ring reset,
twelve-point spark trail and jittered emitter. Preserve negativeheader offsets,
provider arity,pins,failedallocation/random ordering,partials,pads,phase-gated
transition,nullablecall and terminaltail omission. Callback/caller references
renamed; own-unit aliases retain canonical native/runtime identities. Exact game
effect/provider identities unresolved. No body/type/layout/ABI/originalname claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Slot16 spiral finale and fifteen point spark trails naming acceptance (BV-03/BV-08, P2):
FF9 dd5c1b02d adds five canonical ovl_12d57800 names. Catalog4,507 unique unit/symbol
names,139 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-slot16-fifteen-point-sparks.json and
target/ff9-names-slot16-fifteen-point-sparks/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
four exact/one partial,1,420/4,820 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all five sources; current catalog/source/header/
object/review and9 isolated commit paths audit. Other units remain pending.

Established three-phase slot16 sprite/spiral finale,twelve-frame callback,ring
reset,fifteen-point trail and timed jittered emitter. Preserve negativeheader
offsets,provider arity,pins,failedallocation/random ordering,partials,pads,
phase transition,nullablecall and terminaltail omission. Callback/caller
references renamed; own-unit aliases retain canonical native/runtime identities.
Exact game effect/provider identities unresolved. No body/type/layout/ABI claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Paired object positions and handle growth fade naming acceptance (BV-03/BV-08, P2):
FF9 e79c53334 adds five canonical ovl_100c1800 names. Catalog4,512 unique unit/symbol
names,140 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-paired-object-growth-fade.json and
target/ff9-names-paired-object-growth-fade/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
five exact,1,812/1,812 code bytes,zero failures. Installed WASM preprocessed tokens
agree for all five sources; current catalog/source/header/object/review and9
isolated commit paths audit. Other units remain pending.

Established position-following object pair,timed four-resource sequence,paired
handle growth/fade,host weighted blend and scalar interpolation. Preserve
provider arity,signed/unsigned gates,partial initialization,pads,pose-before-fade,
countdown priority,unguarded divisor and terminal ordering. Caller references
renamed; own-unit aliases retain canonical native/runtime identities. Exact game
effect/provider identities unresolved. No body/type/layout/ABI/originalname claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Jittered ribbon particle naming acceptance (BV-03/BV-08, P2):
FF9 d1ffc4688 adds five canonical ovl_1019a800 names. Catalog4,517 unique unit/symbol
names,141 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-jittered-ribbon-particles.json and
target/ff9-names-jittered-ribbon-particles/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
four exact/one partial,2,416/3,600 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all five sources; current catalog/source/header/
object/review and9 isolated commit paths audit. Other units remain pending.

Established long/short ribbon builders,polyline branch,two-phase callback and
24-attempt jittered emitter. Preserve provider arity,pins,signed modes,
failedallocation/random ordering,partial initialization,pads,same shortbuffer
reuse,prefix mark,word-offset pool index and terminal ordering. Caller/callback
references renamed; own-unit aliases retain canonical native/runtime identities.
Exact game effect/provider identities unresolved. No body/type/layout/ABI claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Seven phase camera sprite and keyframe trail naming acceptance (BV-03/BV-08, P2):
FF9 ee0bf7659 adds five canonical ovl_12aee000 names. Catalog4,522 unique unit/symbol
names,142 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-seven-phase-keyframe-trails.json and
target/ff9-names-seven-phase-keyframe-trails/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
two exact/three partial,544/8,188 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all five sources; current catalog/source/header/
object/review and9 isolated commit paths audit. Other units remain pending.

Established tagged two-point strip,three-layer keyframe trails,track/radial burst,
three offset instances and seven-phase camera/sprite/trail sequence. Preserve
flags,pins,provider arity,sharedscratch,pads,halfword stores,previouspoint collapse,
inclusive ring gates,partial initialization,repeatedtrig and terminal ordering.
Caller references renamed; own-unit aliases retain canonical native/runtime
identities. Exact game effect/provider identities unresolved. No body/type/ABI claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Matrix position and interpolation hook naming acceptance (BV-03/BV-08, P2):
FF9 fa4be0331 adds five canonical ovl_125dc000 names. Catalog4,527 unique unit/symbol
names,143 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-matrix-interpolation-hooks.json and
target/ff9-names-matrix-interpolation-hooks/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
five exact,2,980/2,980 code bytes,zero failures. Installed WASM preprocessed tokens
agree for all five sources; current catalog/source/header/object/review and9
isolated commit paths audit. Other units remain pending.

Established five-frame interpolation callbacks,fixed12 output,matrix-position
hook sequence and captured-anchor object sequence. Preserve provider arity,
callback signedness,unguarded division,nullablecalls,innerhook access,partial
initialization,pads,repeatedmatrix calls and terminal ordering. Actual hook/caller
references renamed; own-unit aliases retain canonical native/runtime identities.
Exact game effect/provider identities unresolved. No body/type/layout/ABI claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Timed object hooks and reversed track rising curve naming acceptance (BV-03/BV-08, P2):
FF9 49a346b3c adds five canonical ovl_12705800 names. Catalog4,532 unique unit/symbol
names,144 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-reversed-track-rising-curve.json and
target/ff9-names-reversed-track-rising-curve/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
five exact,1,468/1,468 code bytes,zero failures. Installed WASM preprocessed tokens
agree for all five sources; current catalog/source/header/object/review and9
isolated commit paths audit. Other units remain pending.

Established timed two/three-object sequences,reversed-track rising curve,host
blend and scalar interpolation. Preserve provider arity,capturedanchors,partial
initialization,pads,reversed copies,U16handle narrowing,post-step height increment,
unguarded divisor and terminal ordering. Own-unit aliases retain canonical
native/runtime identities. Exact game effect/provider identities unresolved.
No body/type/layout/ABI/originalname claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Resource vertex centers and centroid displacement fade naming acceptance (BV-03/BV-08, P2):
FF9 69d8b2c98 adds five canonical ovl_ff6c000 names. Catalog4,537 unique unit/symbol
names,145 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-centroid-displacement-fade.json and
target/ff9-names-centroid-displacement-fade/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
five exact,2,836/2,836 code bytes,zero failures. Installed WASM preprocessed tokens
agree for all five sources; current catalog/source/header/object/review and9
isolated commit paths audit. Other units remain pending.

Established resource vertex centers,three/four-group averaging,scaled-center
displacement and sprite fade. Preserve provider/caller arity,signed halfword
narrowing,last-unit cachedtable reuse,partial initialization,pads,pose-before-scale
decrement,double counter increment and terminal ordering. Caller references
renamed; own-unit aliases retain canonical native/runtime identities. Exact game
effect/provider identities unresolved. No body/type/layout/ABI/originalname claims.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No additional reusable implementation gap found.
No private ownership/type/CFG walker,new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Seven phase ripple and tumbling fragment naming acceptance (BV-03/BV-08, P2):
FF9 8fc0d671f adds five canonical ovl_137ec800 names. Catalog4,542 unique unit/symbol
names,146 alias headers. All five complete bodies reviewed,no semantic deferrals.
Evidence: docs/function-names-seven-phase-ripple-fragments.json and
target/ff9-names-seven-phase-ripple-fragments/. Five entire native objects identical;
exact affected/scored namespace five,unchanged pinned strict-relocation baselines
three exact/two partial,3,552/9,288 code bytes,zero failures. Installed WASM
preprocessed tokens agree for all five sources; current catalog/source/header/
object/review and9 isolated commit paths audit. Other units remain pending.

Established RGB555 grid,14x14 ripple,actual table-distance wrapper,model growth/
tumbling callback and seven-phase burst. Preserve GTE/cull differences,arena row
reuse,pins,partials,pads,three/four-arg wrapper discrepancy,failedallocation/random
order,updated-phase gate and terminaltail omission. Caller/callback references
renamed; own-unit aliases retain canonical native/runtime identities. Exact game
effect/provider identities unresolved. No body/type/layout/ABI/originalname claims.

Existing BV-06 provider/ABI frontier (P2): src/ovl_137ec800/sub_801e8284.c defines
a THREE-argument wrapper and calls hostDC with three words. Actual82D0/8A9C callers
pass FOUR words including an output vector; the recovered wrapper drops word4.
Shared caller/selected-provider inspection should surface that discrepancy and
keep it unresolved without native/ABI evidence; successful naming compilation
must not certify output initialization or invent a widened provider contract.
Acceptance inputs are these three actual sources,report review bindings and
unchanged naming object/token proof. No private ABI walker or interface repair.

Maintained naming/farm/scoring/preprocessor interfaces and thin adapters reused;
BV-08 namespace gate enforced. No new matching credit,linked image,gameplay,
provider/sharedworkspace acceptance. Historical scorer/reports pinned; refresh
downstream source-bound evidence. Foreign work/index preserved;full-tree goal active.

Scheduled mesh particle and nine vertex strip naming acceptance (BV-03/BV-08, P2):
FF9 8b925ccdd adds five canonical ovl_13438800 behavioral names. Catalog4,547
unique unit/symbol names,147 scoped alias headers. All five complete bodies
directly reviewed,zero semantic deferrals. Other units remain pending.
Evidence: docs/function-names-scheduled-mesh-strip.json and Binviz
target/ff9-names-scheduled-mesh-strip/. Five complete native object pairs
identical; exact affected/scored namespace five; pinned strict-relocation scores
unchanged three exact/two partial,1,552/14,472 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish grayscale interpolation,eased host weight,eight-part triangle/
quad mesh particle initialization and advance,and scheduled nine-vertex strips.
Preserve halfword narrowing,partial initialization,random order,fixed rotation,
GTE pipeline,six distinct restart gates (mode5 outside XY distance),unsigned
attractor arithmetic,strip countdown thresholds,pins/flags,duplicate releases,
four strip patterns and frame668 return before the common tail. Readable
identifiers propagate through definitions,declarations and callers; own-unit
aliases retain canonical linker/address/runtime identities. Exact game effect
and selected provider contracts unresolved; no body/type/layout/ABI repair.

Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 namespace gate enforced. No new reusable tooling logic or implementation
introduced. Existing BV-06 selected-provider frontier remains separate from
naming object/token proof. No new original matching credit,linked image,
gameplay or sharedworkspace acceptance. Refresh downstream source-bound
evidence; historical scorer/reports stay pinned. Foreign work/index preserved;
full-tree naming goal active.

Spark and twisted tube naming acceptance (BV-03/BV-08, P2):
FF9 5eadb9773 adds five canonical ovl_1202b800 behavioral names. Catalog4,552
unique unit/symbol names,148 scoped alias headers. All five complete bodies
directly reviewed,zero semantic deferrals. Other units remain pending.
Evidence: docs/function-names-spark-twisted-tube.json and Binviz
target/ff9-names-spark-twisted-tube/. Five complete native object pairs
identical; exact affected/scored namespace five; pinned strict-relocation scores
unchanged zero exact/five partial,0/16,628 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish a20-ring textured tube,spark/debris/smoke callback,seven phase
case inventory,and indexed host vector drift/arc/restoration. Preserve actual
phase0 fallthrough,uninitialized nc read,second scale store through w20 again,
unchecked indices/table accesses/nulls,partial fields,random order,narrowing,
GTE projection/cull/packet ordering,pins/flags and terminal ordering. Readable
identifiers propagate to declarations,calls and callback reference; own-unit
aliases retain canonical linker/address/runtime identities. Exact game effect,
provider contracts and complete phase reachability unresolved. No body/type/
layout/null/ABI repair; raw .i/.o/.s neighbors excluded from canonical C scope.

Existing BV-06 provider/ABI frontier (P2): actual F04BC initialization calls
host1F8 with ONE word; its restoration and F0074 use TWO. D684 phase5 host100
calls use FOUR words at t0/t1 and THREE by default,while other phases use FIVE.
Selected-provider/caller inspection should expose these actual unresolved
differences without inventing output/RGB words. Inputs are these three current
sources,review hash bindings and unchanged object/token proof. Naming proof
does not certify a provider signature or repair those recovered call sites.

Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 namespace gate enforced. No private ABI/type/ownership/CFG walker or
shared implementation introduced. No new original matching credit,linked image,
gameplay or sharedworkspace acceptance. Refresh downstream source-bound
evidence; historical scorer/reports stay pinned. Foreign work/index preserved;
full-tree naming goal active.

Stored point direction and timed object naming acceptance (BV-03/BV-08, P2):
FF9 17bcbcaa3 adds five canonical ovl_10e44000 behavioral names. Catalog4,557
unique unit/symbol names,149 scoped alias headers. All five complete bodies
directly reviewed,zero semantic deferrals. Other units remain pending.
Evidence: docs/function-names-point-direction-hooks.json and Binviz
target/ff9-names-point-direction-hooks/. Five complete native object pairs
identical; exact affected/scored namespace five; pinned strict-relocation scores
unchanged five exact,1,580/1,580 code bytes,zero failures. All five installed WASM
preprocessed token comparisons agree. Current catalog/source/header/object/
review bindings and nine isolated committed paths audit.

Names establish two stored-point callback wrappers,host direction helper with
n<<9 angle and XYZ<<12 output,a frame4 four-object callback-hook sequence ending
at30,and a captured-anchor five-object sequence loading at0/4/6 and ending40.
Preserve return-1 threshold after output writes,ignored callback arguments,
hook assignment before endpoint publication,endpointX overwrite after actor
copy,partial fields and actual nullable-object guards (no nested table guard).
Existing uppercase/lowercase data symbols are unchanged; selected data alias/
link identity and exact game effect/provider meanings remain unresolved.
Readable definitions,declarations,calls and callback references propagate;
own-unit aliases retain canonical linker/address/runtime identities.

Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 exact namespace gate enforced. No new reusable tooling logic or private
type/ownership/CFG walker introduced. No body/type/layout/ABI repair or new
original matching credit,linked image,gameplay or sharedworkspace acceptance.
Refresh downstream source-bound evidence; historical scorer/reports stay
pinned. Foreign work/index preserved; full-tree naming goal active.

Eleven vertex trail naming acceptance (BV-03/BV-08, P2):
FF9 978d7892a adds five canonical ovl_10e1e000 behavioral names. Catalog4,562
unique unit/symbol names,150 scoped alias headers. All five complete bodies
directly reviewed,zero semantic deferrals. Other units remain pending.
Evidence: docs/function-names-eleven-vertex-trails.json and Binviz
target/ff9-names-eleven-vertex-trails/. Five complete native object pairs
identical; exact affected/scored namespace five; pinned strict-relocation scores
unchanged four exact/one partial,1,508/2,060 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review/RAMpolicy bindings and nine isolated committed paths audit.

Names establish resource3 timed placement,eight-frame sprite callback,trail
slot reset,reverse eleven-vertex trail/particle builder and two-trails-per-frame
sequence. Preserve all vertex generation before allocation,success-only
velocity random rolls,age/index order,halfword narrowing,partial fields,
four-argument host8,variant resource1/4 and frame24 pulse before completion.
Reset body uses D_801e8528 and builder D_801E8528: actual wasm/mkmod.py address
regex,integer parsing and .set emitter map both spellings to the same RAMoffset.
That inspected policy plus the actual own-unit init/builder establishes the
cursor role; source data names remain unchanged. Relevant policy blocks were
inspected,not a whole-tool review or fresh linked/runtime acceptance.

Readable definitions,declarations,calls and callback references propagate;
own-unit aliases retain canonical linker/address/runtime identities. Exact
game effects/providers unresolved. Maintained naming/farm/scorer/preprocessing
interfaces and thin adapters reused; BV-08 exact namespace gate enforced.
No new reusable tooling logic or private type/ownership/CFG walker introduced.
No body/type/layout/ABI repair or new original matching credit,linked image,
gameplay or sharedworkspace acceptance. Refresh source-bound evidence;
historical scorer/reports pinned; foreign work/index preserved;full-tree goal active.

Variable trail scene naming acceptance (BV-03/BV-08, P2):
FF9 8af3276d1 adds five canonical ovl_12b35800 behavioral names. Catalog4,567
unique unit/symbol names,151 scoped alias headers. All five complete bodies
directly reviewed,zero semantic deferrals. Other units remain pending.
Evidence: docs/function-names-variable-trail-scene.json and Binviz
target/ff9-names-variable-trail-scene/. Five complete native object pairs
identical; exact affected/scored namespace five; pinned strict-relocation scores
unchanged three exact/two partial,1,444/7,260 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish variable-length curved strips,reference-segment wrapper,
17/5-vertex trail callback,six-slot emitter and four-phase transformed-point/
sprite sequence. Preserve source/pool WORD offsets (no triple-record scaling),
signed divisions,unchecked n-1,preallocation count increment,success-only
randoms,global pause/stop0/2/1,callback phase gates and drawing before completion.
Main phases66/100/18/14 keep source point overwrites,address-of-object-pointer
release,uninitialized identical-arm test,scratch/pad/color partial writes,
camera publication,narrowing,register pins and final return before counter.
Readable definitions,declarations,calls and callback references propagate;
own-unit aliases retain canonical linker/address/runtime identities. Original
game effect and selected providers remain unresolved.

Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 exact namespace gate enforced. No new reusable tooling logic or private
type/ownership/CFG walker introduced. No body/type/layout/ABI repair or new
original matching credit,linked image,gameplay or sharedworkspace acceptance.
Refresh downstream source-bound evidence; historical scorer/reports stay
pinned. Foreign work/index preserved; full-tree naming goal active.

Paired cosine swing naming acceptance (BV-03/BV-08, P2):
FF9 3c5340145 adds five canonical ovl_12e72800 behavioral names. Catalog4,572
unique unit/symbol names,152 scoped alias headers. All five complete bodies
directly reviewed,zero semantic deferrals. Other units remain pending.
Evidence: docs/function-names-paired-swing-hooks.json and Binviz
target/ff9-names-paired-swing-hooks/. Five complete native object pairs
identical; exact affected/scored namespace five; pinned strict-relocation scores
unchanged five exact,1,384/1,384 code bytes,zero failures. All five installed WASM
preprocessed token comparisons agree. Current catalog/source/header/object/
review bindings and nine isolated committed paths audit.

Names establish signed-radius swing callbacks,host point path with cosine-X
offset,paired model hooks ending30 and four-object captured-position sequence
ending40. Preserve upper-only fraction clamp,signed division (not shifts),
radius halving fromstep4,halfword narrowing,output before return-1 threshold,
ignored callback arguments,object guards without a nested table guard,
partial initialization and actual resource/frame order. Readable definitions,
declarations,calls and callback references propagate; own-unit aliases retain
canonical linker/address/runtime identities. Original game effect/providers
remain unresolved. No body/type/layout/ABI repair or new original matching credit.

Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 exact namespace gate enforced. No new reusable tooling logic or private
type/ownership/CFG walker introduced. No linked image,gameplay or sharedworkspace
acceptance. Refresh source-bound evidence; historical scorer/reports pinned;
foreign work/index preserved; full-tree naming goal active.

Six particle ripple burst naming acceptance (BV-03/BV-08, P2):
FF9 9dbd84be8 adds five canonical ovl_10be8000 behavioral names. Catalog4,577
unique unit/symbol names,153 scoped alias headers. All five complete bodies
directly reviewed,zero semantic deferrals. Other units remain pending.
Evidence: docs/function-names-six-particle-ripple-burst.json and Binviz
target/ff9-names-six-particle-ripple-burst/. Five complete native object pairs
identical; exact affected/scored namespace five; pinned strict-relocation scores
unchanged three exact/two partial,3,596/9,520 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish RGB555 grid/texture transfer,14x14 radial ripple,four-argument
tripled-table-distance host wrapper,3072 model growth/tumbling callback and
seven-phase six-initial-particle burst. Actual bodies independently reviewed,
including GTE/cull differences,grid row arena reuse,partial fields,pins/random
ordering,p84 allocation/release,extra resource loads and updated-phase tail
gates. This wrapper FORWARDS all four words and returns the host result;
VOID K&R callers ignore the result. The137EC800 three-word/dropped-output fact
does not apply here. Keep selected-provider contracts/output initialization
and texture-transfer direction unresolved; neither similar comments nor
matching success establishes them. New grid name says transfer rather than
guessing slot4 direction. No body/type/layout/ABI repair.

Readable definitions,declarations,calls and callback references propagate;
own-unit aliases retain canonical linker/address/runtime identities. Maintained
naming/farm/scorer/preprocessing interfaces and thin adapters reused; BV-08
exact namespace gate enforced. Existing BV-06 caller/provider frontier must
preserve the actual four-word wrapper and distinct caller return declarations.
No new reusable tooling implementation or private type/ownership/CFG walker.
No new original matching credit,linked image,gameplay or sharedworkspace
acceptance. Refresh source-bound evidence; historical scorer/reports pinned;
foreign work/index preserved; full-tree naming goal active.

Nine vertex trail naming acceptance (BV-03/BV-08, P2):
FF9 fd6a6c0f5 adds five canonical ovl_1385b800 behavioral names. Catalog4,582
unique unit/symbol names,154 scoped alias headers. All five complete bodies
and current g24/g28/transitive headers directly reviewed;zero semantic
deferrals in this unit. Other units remain pending. Evidence:
docs/function-names-nine-vertex-trail.json and Binviz
target/ff9-names-nine-vertex-trail/. Five complete native object pairs identical;
exact affected/scored namespace five;pinned strict-relocation scores unchanged
four exact/one partial,3,260/3,604 code bytes,zero failures. All five installed
WASM preprocessed token comparisons agree. Current catalog/source/header/
object/review bindings and nine isolated committed paths audit.

Names establish host-gated three-phase sprite/ring sequence,eight-frame fading
flash callback,eight-slot cursor reset,nine-vertex builder and emitter. Actual
own-unit references establish roles. Preserve host b10 gate/no fixed deadline,
brightness gate,two sprite poses,terminal ring/count ordering,callback index
versus frame distinction,partial vector/pad stores,signed modulo8,U16 angle
wrapping,vertex generation before allocation and success-only velocity rolls.
Keep original game effect identity and selected-provider contracts unresolved.
No body/type/layout/ABI repair. Readable definitions,declarations,calls and
callback references propagate;own-unit aliases retain canonical linker/address/
runtime identities. Maintained naming/farm/scorer/preprocessing interfaces
and thin adapters reused;BV-08 exact namespace gate enforced.

Reusable progress/prioritization gap BV-03/P2: existing thin count adapter
target/ff9-names-movie-api/progress.py ranks all unnamed canonical files
together. ovl_0cc000 appears with21 although existing
docs/function-names-blackjack-intro.json records all33 bodies reviewed,
12 named,21 explicit semantic deferrals,zero unreviewed. This batch's
prepare-tooling-note.py verifies all21 current deferred source hashes still
equal that report. Temporary policy: inspect existing unit review metadata and
hash-bound deferrals before selecting another review;retain total unnamed
count for naming coverage. Shared acceptance should expose distinct named,
reviewed-but-deferred and unreviewed counts keyed by(unit,symbol),bound to
current source/catalog identities. Reject or visibly mark stale review hashes,
preserve historical reports,avoid double counting later batches and never
equate empty hooks/opaque byte accessors with confidently nameable work.
Use this33/12/21 unit and the newly reviewed five-name unit as fixtures;cover
current/stale source drift and unchanged naming denominator. This is a
proposal,not an implemented shared feature or full-tree review claim.

No new reusable tooling implementation or private type/ownership/CFG walker.
No new original matching credit,linked image,gameplay or sharedworkspace
acceptance. Refresh source-bound evidence;historical scorer/reports pinned;
foreign work/index preserved;full-tree naming goal active.

Scaled six particle ripple naming acceptance (BV-03/BV-08, P2):
FF9 086717e5c adds five canonical ovl_10bad000 behavioral names. Catalog4,587
unique unit/symbol names,155 scoped alias headers. All five complete own-unit
bodies reviewed;zero deferrals in this unit. Other units remain pending.
Evidence: docs/function-names-scaled-six-particle-ripple.json and Binviz
target/ff9-names-scaled-six-particle-ripple/. Five complete native object pairs
identical;exact affected/scored namespace five;pinned strict-relocation scores
unchanged three exact/two partial,3,776/9,672 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish RGB555 Gouraud grid/texture transfer,14x14 ripple grid,
scaled-table-distance host wrapper,model growth/tumbling callback and seven-phase
six-initial-particle burst. Actual bodies independently reviewed,not inferred
from transplant comments or donor equality. Preserve grid row arena reuse,
partial colour/pad writes,GTE/cull/link ordering,pinned registers/wrappers,
2218 model growth,7678/4948 main scales,200 angular offset,actual resource ids,
success-only random rolls/allocation count,p84 lifecycle and updated-phase
common tail/terminal ordering. Wrapper computes -((signed table*1109)>>9),
forwards FOUR words and RETURNS host result;VOID K&R callers ignore result.
Do not transplant three-word dropped-output facts or tripled-distance constants
from other units. G17_SetRotMatrix macro writes both rotation and translation
despite its label. Preserve selected-provider/output initialization and transfer
direction as unresolved. No body/type/layout/ABI repair.

Readable definitions,declarations,calls and callback references propagate;
own-unit aliases retain canonical linker/address/runtime identities. Maintained
naming/farm/scorer/preprocessing interfaces and thin adapters reused;BV-08 exact
namespace gate enforced. Existing BV-06 caller/provider frontier must preserve
actual four-word forwarding and distinct caller return declarations. Existing
BV-03 reviewed-deferral versus unreviewed prioritization proposal retained.
No new reusable tooling implementation or private type/ownership/CFG walker.
No new original matching credit,linked image,gameplay or sharedworkspace
acceptance. Refresh source-bound evidence;historical scorer/reports pinned;
foreign work/index preserved;full-tree naming goal active.

Layered column trails naming acceptance (BV-03/BV-08, P2): FF9
991cd8f24 adds five canonical ovl_13693000 behavioral names. Catalog4,592
unique unit/symbol names,156 scoped alias headers. All five complete bodies
reviewed;zero deferrals in this unit. Other units remain pending. Evidence:
docs/function-names-layered-column-trails.json and Binviz
target/ff9-names-layered-column-trails/. Five complete native object pairs
identical;exact affected/scored namespace five;pinned strict-relocation scores
unchanged two exact/three partial,544/4,024 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish tagged two-point strip,three layers of eighteen keyframe trails,
three-track/ring instances,three offset-table instances and two-phase light
column/trail sequence. Own-unit bodies/calls reviewed independently. Preserve
signed tag shift/inheritance,delayed-point repetition,unchecked signed counts,
U8 header truncation,U16 source/output narrowing,ring step0 inclusion atn0,
partial vectors/pads,compiler flags,pins/typeof/do wrappers,host camera target
and final helper calls before completion/count increment. No donor comments
treated as game identity or selected-provider proof. Instance drawing has no
particle allocator;do not invent allocation semantics. No body/type/layout/
ABI repair. Readable definitions/declarations/calls propagate;own-unit aliases
retain canonical linker/address/runtime identities.

Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 exact namespace gate enforced. Existing BV-03 reviewed-deferral versus
unreviewed prioritization proposal retained. No new reusable implementation
or private type/ownership/CFG walker. No new original matching credit,linked
image,gameplay,selected-provider or sharedworkspace acceptance. Refresh source-
bound evidence;historical scorer/reports pinned;foreign work/index preserved;
full-tree naming goal active.

Timed model flash grid naming acceptance (BV-03/BV-08, P2): FF9
8400dd421 adds five canonical ovl_1367c000 behavioral names. Catalog4,597
unique unit/symbol names,157 scoped alias headers. All five complete own-unit
bodies reviewed;zero semantic deferrals. Other units pending. Evidence:
docs/function-names-timed-model-flash-grid.json and Binviz
target/ff9-names-timed-model-flash-grid/. Five complete native object pairs
identical;exact affected/scored namespace five;pinned strict-relocation scores
unchanged three exact/two partial,924/6,196 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish eased host vector blend,eased host weight action,timed model/
spark/flash-grid sequence,eight-by-four gradient textured patch and eased
indexed-ring-point callback. Actual own-unit calls and installed hook establish
roles. Preserve unconditional division/unclamped weights,64 random XZ endpoints
with centerZ1048/Y0,byte decrement timing,old camera position publication before
schedule,exact frame172 termination with no host78,partial vectors/pads,random/
host call order,UV64..159/all32packets depth0/GTE and matching flags/wrappers.
No donor comment treated as game effect identity. No body/type/layout/ABI repair.

Concrete existing BV-06 used-return frontier (P1): canonical
src/ovl_1367c000/sub_801e7840.c defines VOID five-argument eased hostB4 weight
call with no return expression. sub_801e7978.c declares that same callee INTK&R
and uses results for b1position,b3scale,spark-scale and b12speed. Native object
equality and preprocessing success do not establish meaningful return or portable
caller/provider ABI. Current thin adapter preserves the mismatch and names the
explicit hostweight action,not a pure scalar interpolator. Shared existing
caller/callee/used-result certificates should retain definition-return type,
caller declaration,actual native return register/continuation and selected host
provider identity at each real callsite. Reject unsupported return contracts;
never infer a scalar result from constant arguments,donor labels or native
matching score. The six-argument VOID vector blender has a separate INTK&R
declaration with ignored result;do not conflate the two use classes. No new
admission,compiler repair or provider contract claimed here.

Readable definitions,declarations,calls and installed callback references
propagate;own-unit aliases retain canonical linker/address/runtime identities.
Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 exact namespace gate enforced. Existing BV-03 reviewed-deferral versus
unreviewed prioritization proposal retained. No new reusable implementation
or private type/ownership/CFG walker. No original matching credit,linked image,
gameplay or sharedworkspace acceptance. Refresh source-bound evidence;historical
scorer/reports pinned;foreign work/index preserved;full-tree naming goal active.

Spinning ring flash grid naming acceptance (BV-03/BV-08, P2): FF9
d3c8ce03b adds five canonical ovl_1074f000 behavioral names. Catalog4,602
unique unit/symbol names,158 scoped alias headers. All five complete own-unit
bodies reviewed;zero semantic deferrals. Other units pending. Evidence:
docs/function-names-spinning-ring-flash-grid.json and Binviz
target/ff9-names-spinning-ring-flash-grid/. Five complete native object pairs
identical;exact affected/scored namespace five;pinned strict-relocation scores
unchanged two exact/three partial,2,024/4,972 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish callback-deformed radial sprite grid,parent-gated ring callback,
spinning-ring particle controller,radius-dependent grid orbit callback and
three-phase projected flash/grid sequence. Actual own-unit callback/direct
references reviewed independently. Preserve UV clamp/page selection BEFORE
geometry callback,previous shared orbit factor BEFORE update,uninitialized
shared factor/counter,partial packed colour/vector/pad stores,manualOT24-bit
pointer/topbyte/length chains,file-scoperegisterpins,do/typeof/compiler tricks,
unconditional colsdivision,unchecked17buffer dimensions,eleven allocation
ATTEMPTS including failures,parentflag/scheduler transitions and terminal
drawing/count ordering. No donor comments treated as game/provider identity.
No body/type/layout/ABI repair.

Existing BV-06 declaration/callback frontier retained: orbit callback definition
VOID while builder acceptsINTK&R callback and ignores its result;parent ring
callback definitionINT while descriptor declaresVOIDK&R and casts its address.
Keep these distinct from the preceding1367C000 meaningful used-result mismatch.
Naming/native/preprocessor identity grants no callback/provider/scheduler ABI
admission or global initialization proof. Readable definitions,declarations,
directcalls,descriptor callback and grid callback argument propagate;own-unit
aliases retain canonical linker/address/runtime identities.

Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 exact namespace gate enforced. Existing BV-03 reviewed-deferral versus
unreviewed prioritization proposal retained. No new reusable implementation
or private type/ownership/CFG walker. No original matching credit,linked image,
gameplay or sharedworkspace acceptance. Refresh source-bound evidence;historical
scorer/reports pinned;foreign work/index preserved;full-tree naming goal active.

Scheduled wave strips naming acceptance (BV-03/BV-08, P2): FF9
5eb870c37 adds five canonical ovl_f8a2000 behavioral names. Catalog4,607
unique unit/symbol names,159 scoped alias headers. All five complete bodies
reviewed;zero semantic deferrals. Other units pending. Evidence:
docs/function-names-scheduled-wave-strips.json and Binviz
target/ff9-names-scheduled-wave-strips/. Five complete native object pairs
identical;exact affected/scored namespace five;pinned strict-relocation scores
unchanged four exact/one partial,2,668/3,620 code bytes,zero failures. All five
installed WASM preprocessed token comparisons agree. Current catalog/source/
header/object/review bindings and nine isolated committed paths audit.

Names establish sixteen/four-vertex wave strips,reference-segment branch builder,
wave-strip particle callback and scheduled five-attempt emitter. Actual own-unit
typed declarations/calls/callback refs establish roles. Table advances THREE
words pervertex despite old pairs comment;names say vertices rather than segments.
Preserve signeddivision/cos ordering,partialvectors/pads,same branch destination
twice,externalphasehalf reset,mode-dependent4/6-frame work/16-frame wait,third
attempt preallocation jitter,count increment beforeallocation,success-only
randomrolls,U8 modulo14 indexes and tab pointer INTWORDoffset0..31,nottriples.
No silent retries/buffer advancement/count correction or invented effect identity.
No body/type/layout/ABI repair. Readable definitions/typeddeclarations/calls and
descriptorcallback propagate;own-unit aliases retain canonical linker/address/
runtime identities.

Maintained naming/farm/scorer/preprocessing interfaces and thin adapters reused;
BV-08 exact namespace gate enforced. Existing BV-03 reviewed-deferral versus
unreviewed prioritization proposal retained. No new reusable implementation
or private type/ownership/CFG walker. No original matching credit,linked image,
gameplay,selected-provider or sharedworkspace acceptance. Refresh source-bound
evidence;historical scorer/reports pinned;foreign work/index preserved;full-tree
naming goal active.

Spark spiral ribbon naming acceptance (BV-03/BV-08, P2): FF9
eb495a490 adds four canonical ovl_108e4800 behavioral names. Catalog4,611
unique unit/symbol names,160 scoped alias headers. All four complete own-unit
bodies reviewed;zero semantic deferrals. Other units pending. Evidence:
docs/function-names-spark-spiral-ribbon.json and Binviz
target/ff9-names-spark-spiral-ribbon/. Four complete native object pairs identical;
exact affected/scored namespace four;pinned strict-relocation scores unchanged
one exact/three partial,340/9,760 code bytes,zero failures. All four installed
WASM preprocessed token comparisons agree. Current catalog/source/header/object/
review bindings and eight isolated committed paths audit.

Names establish sixteen-tile spiral ribbon,cylindrical spark field,projected
spark trails and three-phase spark/ribbon/burst sequence. Actual own-unit bodies/
typed calls independently reviewed. Ribbon has TWO GT4 endpoint packets and
FOURTEEN FT4 middle packets despite swapped old-comment types. Spark buffer
history words retain PREVIOUS screenXY/sentinel,not RGBcolours/deadstate:
project OLDposition beforeZmovement,wrap skipsstore/draw,firstvalidframe seeds
history,next draws current/old endpoint trail. Restore GTEscreen distance and
preserve rejectedtile cursors,manualpartialfields,pins/barriers/compilerflags.
Main actually creates EIGHThandles without blanketstate initialization;
phase2Z usescapturedX andphase3uses24-byte strides through64x8 burst table.
Keep these actual own-unit operations,zero randomranges,camera/draw/tick order;
do not correct using misleading comments or donor assumptions. No original
game effect or selected-provider identity claimed. No body/type/layout repair.

Existing BV-06 declaration frontier retained: nine-argument VOID sparkdrawer
defines first/fifth arguments INT while actual typed caller declaresu8*/void*.
Native/preprocessor identity grants no linked/provider ABI admission. Preserve
current contracts and distinguish them from meaningful used-return mismatches.
Readable definitions,typed declarations and calls propagate;own-unit aliases
retain canonical linker/address/runtime identities. Maintained naming/farm/
scorer/preprocessing interfaces and thin adapters reused;BV-08 exact namespace
gate enforced. Existing BV-03 reviewed-deferral versus unreviewed prioritization
proposal retained. No new reusable implementation or private type/ownership/
CFG walker. No original matching credit,linked image,gameplay or sharedworkspace
acceptance. Refresh source-bound evidence;historical scorer/reports pinned;
foreign work/index preserved;full-tree naming goal active.

Paired rise tilt fade naming acceptance (BV-03/BV-08, P2): FF9
615f83457 adds four canonical ovl_11c64800 behavioral names. Catalog 4,615
unique unit/symbol names, 161 scoped alias headers. All four complete own-unit
bodies reviewed; zero semantic deferrals. Other units pending. Evidence:
docs/function-names-paired-rise-tilt-fade.json and Binviz
target/ff9-names-paired-rise-tilt-fade/. Four complete native object pairs
identical; exact affected/scored namespace four; pinned strict-relocation scores
unchanged, four exact, 1,836/1,836 code bytes, zero failures. All four installed
WASM preprocessed token comparisons agree. Current catalog/source/header/object/
review bindings and eight isolated committed paths audit.

Names establish delayed shrinking sprite sequence, paired-handle rise/tilt/fade
sequence, complementary host weights and integer-by-step interpolation. Actual
own-unit bodies and typed helper calls reviewed. Sprite draws before scale/step
updates, only frames 35..47. Paired sequence snapshots positions before motion,
has seven rise samples and nine tilt samples; fade only applies steps 0..15
because the outer unsigned frame gate ends before its final interpolation
sample. Preserve that timing, unreachable nested negative-frame branch, partial
state/pads, unclamped weights, signed division/multiply order, resource events
and terminal thresholds. The scalar helper explicitly returns its computed
value; do not transfer prior VOID host-wrapper used-return mismatch assumptions.
No original game effect or selected-provider identity claimed; no body/type/
layout repair, matching credit, linked-image, gameplay or workspace acceptance.

Readable definitions, declarations and callers propagate; own-unit aliases
retain canonical linker/address/runtime identities. Maintained naming/farm/
scorer/preprocessor interfaces and thin adapters reused. Existing BV-03
reviewed-deferral versus unreviewed prioritization proposal and BV-06 provider
frontier retained. No new reusable implementation or private type/ownership/CFG
walker. Refresh source-bound evidence; historical scorer/reports pinned; foreign
work/index preserved; full-tree naming goal active.

Paired pulse particles naming acceptance (BV-03/BV-08, P2): FF9
81ba203c5 adds four canonical ovl_114b8000 behavioral names. Catalog 4,619
unique unit/symbol names, 162 scoped alias headers. All four complete own-unit
bodies reviewed, zero semantic deferrals; other units pending. Evidence:
docs/function-names-paired-pulse-particles.json and Binviz
target/ff9-names-paired-pulse-particles/. Four whole native object pairs
identical; exact affected/scored namespace four; pinned strict-relocation scores
unchanged, two exact/two partial, 600/3,876 code bytes, zero failures. Four
installed WASM preprocessed token comparisons agree. Current source/header/
catalog/object/review bindings and eight isolated committed paths audit.

Names establish callback-deformed radial sprite grid, table-relative particle
pose, spinning paired-handle pulse callback and its particle/sprite sequence.
Own-unit code/callers directly reviewed, independent of transplanted comments.
Grid UV clamps/page choice precede callback geometry changes; retain 17-entry
buffers, unchecked dimension division, packed partial fields, manual 24-bit
ordering-table links and register/compiler tricks. Pulse actually uses scale
times THREE times cos-like lookup plus scale/2, not the old amp-based formula;
mode1 zeroes amp but leaves scale/position unchanged. Sequence uses jobtype for
timeline and a separate call counter, success-only random draws, paired shared
handles and three sprite draws before terminal return. Descriptor six is not
claimed as the total spawn count; normal types0..30 have eleven attempts.

Retain BV-06 declaration frontiers: table pose wrapper defines INT arguments
but receives pointers through K&R; INT callback result definition is exposed via
VOID K&R declaration and void* descriptor. No signature repair or selected-host
provider/linked ABI admission. Readable definitions/declarations/calls/callback
reference propagate; scoped aliases retain linker/address/runtime identities.
Existing naming/farm/scorer/preprocessor and thin adapters reused. BV-03
reviewed-deferral versus unreviewed prioritization proposal retained; no new
reusable implementation or private type/ownership/CFG walker. No matching gain,
linked-image, gameplay or sharedworkspace acceptance. Refresh source-bound
evidence; historical tools/reports pinned, foreign work/index preserved;
full-tree naming goal active.

Delayed paired tracking naming acceptance (BV-03/BV-08, P2): FF9
4c4ea6949 adds four canonical ovl_12679000 behavioral names. Catalog 4,623
unit/symbol names, 163 scoped alias headers. All four full own-unit bodies
reviewed, zero deferrals; other units pending. Evidence:
docs/function-names-delayed-paired-tracking.json and Binviz
target/ff9-names-delayed-paired-tracking/. Four complete native object pairs
identical, exact affected/scored namespace four, unchanged pinned strict-reloc
scores four exact, 1,968/1,968 bytes, zero failures. Four installed WASM token
comparisons agree. Current source/header/catalog/object/review bindings and
eight isolated committed paths audit.

Names cover delayed paired-object tracking, actor-position object timer,
indexed target object with completion flag and host-buffer byte setter. Targets
computed ONCE at init; actor X overwritten and Y changed asymmetrically. Preserve
untouched pads/delayed pointers, exact frame6 loads/skipped-frame behavior,
ended-object pointer clearing before tracking, tracking before terminal return.
Indexed target compares signed index only against byte upperbound, signals flag7
at init/frame4 separately from frame40 completion; no lowerbound added. Setter
stores narrowed byte through first context pointer at buffer+16+index. Its VOID
definition/INT K&R caller discarded-result mismatch remains; no type/return
repair or meaningful used-return admission. Original effect/provider identity
unresolved. No matching gain, linked-image/gameplay/workspace acceptance.

Readable declarations/calls/definitions propagate; aliases retain canonical
linker/address/runtime identities. Existing naming/farm/scorer/preprocessor and
thin adapters reused; BV-03 reviewed-deferral prioritization and BV-06 provider
frontiers retained. No new reusable implementation or private ownership/type/
CFG walker. Refresh source-bound evidence, pin historical reports/tools,
preserve foreign work/index; full-tree naming goal active.

Inward spiral particles naming acceptance (BV-03/BV-08, P2): FF9
0477cc33a adds four canonical ovl_104d4800 behavioral names. Catalog 4,627
unit/symbol names, 164 scoped alias headers. Four full own-unit bodies reviewed,
zero deferrals; other units pending. Evidence:
docs/function-names-inward-spiral-particles.json and Binviz
target/ff9-names-inward-spiral-particles/. Four whole native object pairs
identical; exact affected/scored namespace four, unchanged pinned strict-reloc
scores four exact, 1,588/1,588 code bytes, zero failures. Four installed WASM
token comparisons agree; current source/header/catalog/object/review bindings
and eight isolated committed paths audit.

Names establish table-selected fading callback, inward spiral emitter and
three/single object timers. Actual emitter contracts XY radius while Z grows,
advances before allocation, emits even at terminal phase0 frame6 and resets
jobtype=-1. Callback draws before terminal8 with remaining weight1366, no fade
endpoint correction. Preserve partial host250 record and SV pad, unchecked
indices/negative arithmetic, random-call argument, success-only two-word stores,
release calls on ordinary updates but no release at phase1 completion, and
externally mutable timer gates. Original game/provider identity unresolved.

Readable declarations/definitions/calls/registration references propagate;
scoped aliases preserve canonical linker/address/runtime identities. Existing
naming/farm/scorer/preprocessor and thin adapters reused; BV-03 reviewed-deferral
prioritization and BV-06 provider frontiers retained. No new implementation or
private ownership/type/CFG walker. No original matching gain, linked-image,
gameplay or sharedworkspace acceptance. Refresh source-bound evidence, retain
historical reports/tools, preserve foreign work/index; full-tree goal active.

Hooked companion objects naming acceptance (BV-03/BV-08, P2): FF9
06ee891ad adds four canonical ovl_1041d000 behavioral names. Catalog 4,631
unit/symbol names, 165 scoped alias headers. Four full own-unit bodies reviewed,
zero deferrals; other units pending. Evidence:
docs/function-names-hooked-companion-objects.json and Binviz
target/ff9-names-hooked-companion-objects/. Four whole native object pairs
identical; exact affected/scored namespace four, unchanged pinned strict-reloc
scores four exact, 1,376/1,376 code bytes, zero failures. Four installed WASM
token comparisons agree; source/header/catalog/object/review bindings and eight
isolated committed paths audit.

Names establish paired-object tracking, hook table plus frame10 companion,
indexed four-object sequence with flag7 signaling, and host buffer byte setter.
Preserve init-fetched target vector, ended-object clearing, nullable stores,
partial state/pads, skipped/repeated frame10 behavior, signed index with only
upperbound check, frame4 signaling separately from frame40 completion, and
tracking before terminal return. Hook table contents/provider identity remain
unresolved. Setter current declaration/definition both VOID; do not transfer
other units INT extern mismatch facts. Historical lane difftest trap asymmetry
not fixed or accepted. No original effect identity or gameplay claim.

Readable declarations/definitions/callers propagate; aliases retain canonical
linker/address/runtime identities. Existing naming/farm/scorer/preprocessor and
thin adapters reused; BV-03 reviewed-deferral prioritization and BV-06 provider
frontiers retained. No new reusable implementation or private ownership/type/
CFG walker. No matching gain, linked-image or workspace acceptance. Refresh
source-bound evidence, pin historical reports/tools, preserve foreign work/index;
full-tree naming goal active.

Scheduled five object pairs naming acceptance (BV-03/BV-08, P2): FF9
9a26289f4 adds four canonical ovl_11d96800 behavioral names. Catalog 4,635
unit/symbol names, 166 scoped alias headers. Four full own-unit bodies reviewed,
zero deferrals; other units pending. Evidence:
docs/function-names-scheduled-five-object-pairs.json and Binviz
target/ff9-names-scheduled-five-object-pairs/. Four complete native object pairs
identical, exact affected/scored namespace four, unchanged pinned strict-reloc
scores three exact/one partial, 416/964 code bytes, zero failures. Four installed
WASM token comparisons agree; current source/header/catalog/object/review
bindings and eight isolated committed paths audit.

Names cover five scheduled object pairs, raised paired objects, halfword operand
reader and vector triplet reader with upper-only index clamp. Actual host vector
fetch does not prove zeroing despite old comments. Preserve partial state/pads,
exact row frame equality and repeated/skipped-frame semantics, row order,
U16 offset narrowing, nullable fields, dead register pin, and loads before
completion. Stream reader skips one word, reads operand, advances/stores pointer;
no original opcode identity assumed. Table reader retains negative indices and
empty-count entry-1 behavior, read/store alias order and unclamped lowerbound.
No original effect/provider/stream identity or body/type repair claimed.

Readable definitions and scoped aliases retain canonical linker/address/runtime
identities. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
BV-03 reviewed-deferral prioritization and BV-06 provider frontiers retained.
No new reusable implementation or private ownership/type/CFG walker, matching
gain, linked-image/gameplay/workspace acceptance. Refresh source-bound evidence,
pin historical reports/tools, preserve foreign work/index; full-tree goal active.

Position shake flash emitter naming acceptance (BV-03/BV-08, P2): FF9
296412207 adds four canonical ovl_12ea7800 behavioral names. Catalog 4,639
unit/symbol names, 167 scoped alias headers. Four full own-unit bodies reviewed,
zero deferrals; other units pending. Evidence:
docs/function-names-position-shake-flash-emitter.json and Binviz
target/ff9-names-position-shake-flash-emitter/. Four complete native object pairs
identical, exact affected/scored namespace four, unchanged pinned strict-reloc
scores four exact, 1,652/1,652 code bytes, zero failures. Four installed WASM
token comparisons agree; source/header/catalog/object/review bindings and eight
isolated committed paths audit.

Names cover cross-axis 2D displacement, absolute-event-row position offset,
two-phase shake/flash/emitter sequence and single object with published position.
Preserve both old-coordinate host lookups before stores, X/Z/Y offset order,
baseline copy each update, exact table flash interval, phase/job reset without
fallthrough, two distinct sine-like calls, partial color/vector/state pads,
register pin, fourteen-argument emitter and emitter before terminal return.
No original quake/spell or selected camera/provider identity claimed.

BV-06/P1 frontier example: shared D_801e7e04 has pointer-to-24-byte-record-array
declaration in callback, INT in controller storing frame<<6. Callback passes
record ADDRESSES to host38/3C, unlike controller angle calls. Retain actual
representation and argument contracts; do not silently convert to angles or
repair globals. VOID callback/offset definitions versus INT K&R declarations
also retained: direct offset result discarded, host callback result consumption
unresolved. Shared provider/ABI admission must distinguish these concrete
source-bound conflicts; naming object equality grants no linked/portable proof.

Definitions/declarations/directcalls/callback reference propagate; aliases
retain canonical linker/address/runtime identities. Maintained naming/farm/
scorer/preprocessor and thin adapters reused; existing BV-03 reviewed-deferral
prioritization retained. No new reusable implementation/private ownership/type/
CFG walker, matching gain, linked-image/gameplay/workspace acceptance. Refresh
source-bound evidence, pin historical reports/tools, preserve foreign work/index;
full-tree goal active.

Hooked decaying orbit naming acceptance (BV-03/BV-08, P2): FF9
f84d0dc80 adds four canonical ovl_11caa000 behavioral names. Catalog 4,643
unit/symbol names, 168 scoped alias headers. Four full own-unit bodies reviewed,
zero deferrals; other units pending. Evidence:
docs/function-names-hooked-decaying-orbit.json and Binviz
target/ff9-names-hooked-decaying-orbit/. Four whole native object pairs identical;
exact affected/scored namespace four, unchanged pinned strict-reloc scores four
exact, 1,456/1,456 code bytes, zero failures. Four installed WASM token comparisons
agree; source/header/catalog/object/review bindings and eight committed paths audit.

Names cover decaying radial orbit callback, hook owner with delayed frame80
object, complementary host weights and explicit integer interpolation. Actual
trajectory uses cos-like X/sin-like Z, one chosen angular direction without
reversal, signed reads of U16 state and decaying radial oscillation. Preserve
first16 host-DC path, partial pads, state/output updates before callback -1
termination, signed random remainder and exact delayed-load behavior. Do not
assume selected B0 sqrt contract or original pendulum/spell identity.

BV-06 frontier retained: meaningful INT six-argument callback is installed through
VOID K&R declaration and VOIDf8 table pointer. Hostweight wrapper result ignored;
integer helper explicitly returns scalar and its actual caller consumes it.
Distinguish these contracts, no type/cast/body repair or provider admission.
Readable declarations/calls/hook reference propagate; aliases preserve canonical
linker/address/runtime identities. Existing naming/farm/scorer/preprocessor and
thin adapters reused, BV-03 reviewed-deferral prioritization retained. No new
implementation/private ownership/type/CFG walker, matching gain, linked-image/
gameplay/workspace acceptance. Refresh source-bound evidence, pin historical
reports/tools, preserve foreign work/index; full-tree naming goal active.

Four ribbon mesh breakup naming acceptance (BV-03/BV-08, P2): FF9
be13c7a46 adds four canonical ovl_12953800 behavioral names. Catalog 4,647
unit/symbol names, 169 scoped alias headers. Four full own-unit bodies reviewed,
zero deferrals; other units pending. Evidence:
docs/function-names-four-ribbon-mesh-breakup.json and Binviz
target/ff9-names-four-ribbon-mesh-breakup/. Four complete native object pairs
identical; exact affected/scored namespace four, unchanged pinned strict-reloc
scores two exact/two partial, 1,124/4,800 code bytes, zero failures. Four installed
WASM token comparisons agree; source/header/catalog/object/review bindings and
eight isolated committed paths audit.

Names cover 160-triangle breakup initialization, particle/triangle update,
sixteen-tile spiral ribbon and four-ribbon/breakup controller. Preserve centroid
division before fixedpoint shift, signed narrowed center differences, random
rotation order, host8 integration frontier, relative geometry before host8,
matrix translation after host8, delay/dead/age order and partial pads. Ribbon
reserves two GT4/fourteen FT4 packets, accepts both signs of nonzero NCLIP,
writes colors before rejection, advances only accepted pool and links before
fourth SXY store. Keep -fno-strength-reduce and register/wrapper compiler tricks.
Controller X is zero, job frame differs from local ribbon counter; transition
draws four ribbons before zero-center breakup and EIGHT companion object loads,
despite old seven-object comment. Phase1 updates particles before completion.
No original effect/provider identity or signature/body/layout repair claimed.

Readable declarations/definitions/callers propagate; aliases retain canonical
linker/address/runtime identities. Maintained naming/farm/scorer/preprocessor
and thin adapters reused; existing BV-03 reviewed-deferral prioritization and
BV-06 provider frontiers retained. No new reusable implementation/private
ownership/type/CFG walker, original matching gain, linked-image/gameplay/workspace
acceptance. Refresh source-bound evidence, pin historical reports/tools, preserve
foreign work/index; full-tree naming goal active.

Four phase paired trails naming acceptance (BV-03/BV-08, P2): FF9
485e87150 adds four canonical ovl_11d4b800 behavioral names. Catalog 4,651
unit/symbol names, 170 scoped alias headers. Four full own-unit bodies reviewed,
zero deferrals; other units pending. Evidence:
docs/function-names-four-phase-paired-trails.json and Binviz
target/ff9-names-four-phase-paired-trails/. Four complete native object pairs
identical; exact affected/scored namespace four, unchanged pinned strict-reloc
scores two exact/two partial, 1,236/5,388 code bytes, zero failures. Four installed
WASM token comparisons agree; source/header/catalog/object/review bindings and
eight isolated committed paths audit.

Names establish fading quad history, host-matrix vector transform/GTE restore,
four-phase model/paired trails and camera path polling/intensity ramp. Preserve
history count AFTER shift but BEFORE newest points, seventeen flag-only clears,
projection MAC0 gate with NO NCLIP instruction, rejected output cursor advance,
link before final SXY/colors, unsigned fades/division and partial packed fields.
Camera setup has asymmetric third-block Y, quantized inclusive direction range,
partial pads and typed pointer differences. Keep ordinary trail draws after
phase2 transition but terminalphase3 return before trails/counter, signed Y
decays, unbounded -2/-1 polling, decreasing wait limit and flag completion.
Retain -fno-rerun-cse-after-loop/-fno-force-mem, pins and empty asm barrier.
No original effect/provider identity or body/type/layout repair claimed.

Readable declarations/definitions/callers propagate; aliases preserve canonical
linker/address/runtime identities. Maintained naming/farm/scorer/preprocessor
and thin adapters reused, existing BV-03 reviewed-deferral prioritization and
BV-06 provider frontiers retained. No new reusable implementation/private
ownership/type/CFG walker, original matching gain, linked-image/gameplay/workspace
acceptance. Refresh source-bound evidence, pin historical reports/tools, preserve
foreign work/index; full-tree naming goal active.

Paired hooked position paths naming acceptance (BV-03/BV-08, P2): FF9
715a23edb adds four canonical ovl_1246f000 behavioral names. Catalog 4,655
unit/symbol names, 171 scoped alias headers. Four full own-unit bodies reviewed,
zero deferrals; other units pending. Evidence:
docs/function-names-paired-hooked-position-paths.json and Binviz
target/ff9-names-paired-hooked-position-paths/. Four complete native object pairs
identical; exact affected/scored namespace four, unchanged pinned strict-reloc
scores four exact, 1,568/1,568 code bytes, zero failures. Four installed WASM
token comparisons agree; source/header/catalog/object/review bindings and eight
isolated committed paths audit.

Names cover primary/secondary four-entry path hooks, host-key-pair position
helper and hooked-pair/delayed-object controller. Primary/secondary come from
actual first/second companion roles, not uninspected table constants or address
labels. Preserve clamp BEFORE completion comparison (large index becomes3),
unguarded negative index, output halfword readback/alias order, twelve-bit
fixedpoint shifts (old16.16 label wrong), exact0/12/30 events, partial state and
nested hook access. Controller repeats host220 and assignment to pc twice;
do not dedup or fix second store to p10. INT callback/helper definitions and
INT hook declarations stay distinct from earlier VOID return frontiers.
Original effect/path/provider identities unresolved, no body/type repair.

Definitions/declarations/calls/hook references propagate; aliases retain
canonical linker/address/runtime identities. Maintained naming/farm/scorer/
preprocessor and thin adapters reused; BV-03 reviewed-deferral prioritization
and BV-06 provider frontiers retained. No new implementation/private ownership/
type/CFG walker, matching gain, linked-image/gameplay/workspace acceptance.
Refresh source-bound evidence, pin historical reports/tools, preserve foreign
work/index; full-tree naming goal active.

Five phase particle breakup naming acceptance (BV-03/BV-08, P2): FF9
4ea69e4fd adds four canonical ovl_118c3000 behavioral names. Catalog 4,659
unit/symbol names, 172 scoped alias headers. Four full own-unit bodies and
g17/g22/g19 headers reviewed and bound, zero deferrals; other units pending.
Evidence: docs/function-names-five-phase-particle-breakup.json and Binviz
target/ff9-names-five-phase-particle-breakup/. Four complete native object
pairs identical; exact affected/scored namespace four, unchanged pinned
strict-relocation scores two exact/two partial, 2,296/7,024 code bytes, zero
failures. Four installed WASM token comparisons agree; current catalog,
source/header/object/review bindings and eight isolated committed paths audit.

Names cover scaled negative table offset, growing paired particle/tumbling
fragment callback, five-phase six-initial-particle breakup controller and
three-phase paired sprite pulse. Own-unit controller has five phases2..6,
not its donor's seven. Preserve success-only initial count and six allocation
attempts, four-way callback split with original reuse/failure-dependent totals,
full word/pad copies, partial initialization, repeated trig calls and register
pins. Transition5->6 skips band yet retains old fade for three same-call
sprites; terminal24 returns before tail. Pulse terminal9 fetches host vector
and computes before returning, skips draws/flag/frame tail. No body repair.

BV-06/P1 representation and callback frontier retained: offset helper has
INT definition with INT arguments but both actual VOID K&R callers pass
pointers and discard return; meaningful INT particle callback is referenced
through VOID declaration. These remain unresolved, no inferred provider/ABI
admission. Original game/resource identities remain unknown. Definitions,
declarations, helper calls and callback descriptor references propagate;
aliases preserve canonical linker/address/runtime identities.

Maintained naming/farm/scorer/preprocessor and thin adapters reused. Existing
BV-03 reviewed-deferral prioritization and BV-06 provider proposals retained;
no new implementation/private ownership/type/CFG walker, matching gain,
linked-image/gameplay/workspace acceptance. Refresh source-bound evidence,
pin historical reports/tools, preserve foreign work/index; full-tree goal active.

Four band hooked objects naming acceptance (BV-03/BV-08, P2): FF9
129544a68 adds four canonical ovl_13249800 behavioral names. Catalog 4,663
unit/symbol names, 173 scoped alias headers. All four complete own-unit bodies
and g29/g15/g13 headers reviewed and bound, zero deferrals; other units pending.
Evidence: docs/function-names-four-band-hooked-objects.json and Binviz
target/ff9-names-four-band-hooked-objects/. Four complete native object pairs
identical, exact affected/scored namespace four; unchanged pinned strict-reloc
scores two exact/two partial, 588/7,288 code bytes, zero failures. Four installed
WASM token comparisons agree; current catalog/source/header/object/review
bindings and eight isolated committed paths audit.

Names cover primary/secondary late-progress pose callbacks, sixteen-quad band
renderer and three-phase four-band/hooked-object controller. Preserve unclamped
fade and zero-denominator behavior, scalar/array handle declarations and reads,
upfront664-byte pool reservation, two GT4/fourteen FT4, colors before rejection,
actual both-sign NCLIP gate, accepted-only packet cursor advancement and link
before final SXY store. No ending matrix restore. Controller's phase2 first6
Y is replaced rather than offset; two w64 and four w5C poses repeat same handle.
Phase1 captures band scale before reset; terminal32 follows phase operations,
skips four-band tail/frame increment. Preserve partial pads/pins/flags/wrappers.

BV-06/P1 frontier retained: callbacks have VOID definitions and INT K&R externs
used only as addresses through void-pointer hook fields. Main passes INTscale
words through K&R call while renderer consumes a halfword-vector pointer.
No corrected layout/ABI/provider admission or inferred scale interpretation.
Definitions/declarations/calls/hook references propagate; aliases retain native
symbols/address/runtime exports. Original effect/resource/provider identities
unresolved; no body/type repair.

Maintained naming/farm/scorer/preprocessor and thin adapters reused; existing
BV-03 reviewed-deferral prioritization and BV-06 provider proposals retained.
No new implementation/private ownership/type/CFG walker, matching gain,
linked-image/gameplay/workspace acceptance. Refresh source-bound evidence,
pin historical reports/tools, preserve foreign work/index; full-tree goal active.

Tracking point pair pulse naming acceptance (BV-03/BV-08, P2): FF9
defb85958 adds four canonical ovl_10d01800 behavioral names. Catalog 4,667
unit/symbol names, 174 scoped alias headers. All four complete own-unit bodies
and g18/g16/g08/ff9 headers reviewed and bound, zero deferrals; other units pending.
Evidence: docs/function-names-tracking-point-pair-pulse.json and Binviz
target/ff9-names-tracking-point-pair-pulse/. Four complete native object pairs
identical, exact affected/scored namespace four; unchanged pinned strict-reloc
scores four exact, 1,592/1,592 code bytes, zero failures. Four installed WASM
token comparisons agree; current catalog/source/header/object/review bindings
and eight isolated committed paths audit.

Names cover depth-cued Gouraud line, tagged two-point strip descriptor, fading
resource/early point-pair pulse callback and two-model tracking/eight-frame fade.
Line RTPT uses p0/p1/p0; AVSZ3 retains duplicated firstpoint, no invented average.
Preserve negative-weight handling, pool reservation/GTE order and partial pads.
Callback uses global API/job but incoming context flags, exact0 extra draw and
signedframe<40 pulse. Controller transition is >=41 (old comment40), callback
installation outside m1 null guard, unconditional move even after clearing m2,
and terminal two-argument release(m1,first-vector-word). No new guards/cleanup.

BV-06/P1 frontier retained: callback's INT h is passed as U16 pointer to typed
seven-argument descriptor helper. Preserve scalar/pointer representation and
selected provider uncertainty; no repaired ABI or inferred missing words.
Callback VOID definition/declaration agrees here, unlike prior return conflicts.
Definitions/declarations/helper calls/callback table references propagate;
aliases preserve native symbols/address/runtime exports. Original effect and
resource visual/provider identities remain unknown, no body/type repair.

Maintained naming/farm/scorer/preprocessor and thin adapters reused. Existing
BV-03 reviewed-deferral prioritization and BV-06 provider proposals retained;
no new implementation/private ownership/type/CFG walker, matching gain,
linked-image/gameplay/workspace acceptance. Refresh source-bound evidence,
pin historical reports/tools, preserve foreign work/index; full-tree goal active.

Five trail target swirl naming acceptance (BV-03/BV-08, P2): FF9
0f7a9572d adds four canonical ovl_106bd800 behavioral names. Catalog 4,671
unit/symbol names, 175 scoped alias headers. All four complete own-unit bodies
and g15/g11/g13 headers reviewed and bound, zero deferrals; other units pending.
Evidence: docs/function-names-five-trail-target-swirl.json and Binviz
target/ff9-names-five-trail-target-swirl/. Four complete native object pairs
identical, exact affected/scored namespace four; unchanged pinned strict-reloc
scores three exact/one partial, 3,224/7,428 code bytes, zero failures. Four
installed WASM token comparisons agree; current catalog/source/header/object/
review bindings and eight isolated committed paths audit.

Names cover eased blend and decreasing host-pair weight wrappers, five-trail
controller with delayed target motion, and64FT4/32G4 swirl/ring renderer.
Preserve unconditional quadratic division even linear/unknown mode, unclamped
weights, sixteen-word clear despite fourteen-word declared array, X/Z/Y initial
position order, dynamic target count without eight-slot clamp and crosscounter
radius calls. Return-target final tick rereads current position; eleven trail
samples omit endpoint atj11, spawn does not draw same branch. Renderer reserves
3712 bytes, retains asymmetric12/16-bit geometry shifts, no clipping/depthsort,
and final host-FC link. Terminal70 follows all scheduled updates, no host78.

BV-06/P1 meaningful return frontier: 78E4 definition VOID discards host-B4
result, while actual INT caller consumes it in radii, scales and target offsets.
Name records applied weight; no invented pure interpolation/return repair.
77A8 INT argument definition versus pointer caller and renderer six/eight-byte
vector declarations preserve separate representation/readextent frontiers.
Native equality grants no provider/portable admission. Definitions/declarations/
calls propagate; aliases preserve canonical linker/address/runtime identities.
Original effect/resources/providers unresolved, no body/type repair.

Maintained naming/farm/scorer/preprocessor and thin adapters reused; existing
BV-03 reviewed-deferral prioritization and BV-06 provider proposals retained.
No new implementation/private ownership/type/CFG walker, matching gain,
linked-image/gameplay/workspace acceptance. Refresh source-bound evidence,
pin historical reports/tools, preserve foreign work/index; full-tree goal active.

Host vector delta steps naming acceptance (BV-03/BV-08, P2): FF9
f0364e713 adds four canonical ovl_1092e800 behavioral names. Catalog 4,675
unit/symbol names, 176 scoped alias headers. All four complete own-unit bodies
and g16/ff9 headers reviewed and bound, zero deferrals; other units pending.
Evidence: docs/function-names-host-vector-delta-steps.json and Binviz
target/ff9-names-host-vector-delta-steps/. Four complete native object pairs
identical, exact affected/scored namespace four; unchanged pinned strict-reloc
scores four exact, 1,620/1,620 code bytes, zero failures. Four installed WASM
token comparisons agree; current catalog/source/header/object/review bindings
and eight isolated committed paths audit.

Names cover hooked stored-vector object/frame12 companion, fifteen-step host
delta controller, signed delta fraction stepping and object-pair derivation.
Controller copies a saved target, but actual step helper ignores target argument
and controller never initializes delta. Avoid inferred toward-target movement.
Preserve unused host1FC result, duplicate step0 host20C, integer division/narrow
stores and completion AFTER vector writes, even outside normal step range.
Pair helper's signed Y correction >=2048 becomes4096-Y; below-2047 adds4096.
That asymmetric reflection is not conventional angle normalization; later+2048
has no second normalization. Hooked object uses init-derived stored vector,
frame12 companion uses current actor coordinates; no fabricated refresh/release.

BV-06 provider/representation frontiers retained: actual host20C/2A8 vector
interpretation unresolved and VOID object pointer reaches K&R host1FC. No
portable/provider admission or guessed visual beam/flash identity. Definitions,
declarations and actual helper caller propagate; own-unit aliases preserve native
symbols/address/runtime exports. No body/layout/signature/pad initialization fix.

Maintained naming/farm/scorer/preprocessor and thin adapters reused. Existing
BV-03 reviewed-deferral prioritization and BV-06 provider proposals retained;
no new implementation/private ownership/type/CFG walker, matching gain,
linked-image/gameplay/workspace acceptance. Refresh source-bound evidence,
pin historical reports/tools, preserve foreign work/index; full-tree goal active.

Scheduled quadratic spin naming acceptance (BV-03/BV-08, P2): FF9
6e9ce6993 adds four canonical ovl_131a5800 names. Catalog 4,679 unit/symbol names,
177 scoped alias headers. All four full own-unit bodies and g29/g13 headers
reviewed and bound, zero deferrals; other units pending. Evidence:
docs/function-names-scheduled-quadratic-spin.json and Binviz
target/ff9-names-scheduled-quadratic-spin/. Four full native object pairs equal,
exact affected/scored namespace four; unchanged pinned strict-reloc scores
four exact, 2,588/2,588 code bytes, zero failures. Four installed WASM token
comparisons agree; current source/header/catalog/object/review bindings and
eight isolated committed paths audit.

Names cover scheduled tracking/quadratic offsets, flag-gated three spinning
handles/tint, complementary host blend and integer-ratio interpolation.
Preserve pre-decrement offsets versus post-decrement tint, extra >=0 countdown
tick, unconditional moves after null-clearing, dormant second offset countdown,
partial object/vector initialization and unsigned frame<66 draw gate. Spinning
poses precede spin/scale updates; Y scale stays after frame12, X/Z shrink.
Actual spin increments128/128/64 differ from old comment. Context saves before
flag earlyreturn. Original effect/provider/byte26 meanings remain unresolved.

Definitions/declarations/direct calls propagate; aliases preserve canonical
linker/address/runtime identities. INT interpolation definitions/declarations
and consumed returns agree here; no inherited VOID-return conflicts or
provider admission. Keep exact signed/unclamped arithmetic and zero division.
Maintained naming/farm/scorer/preprocessor and thin adapters reused, existing
BV-03 reviewed-deferral prioritization/BV-06 provider proposals retained.
No new implementation/private ownership/type/CFG walker, matching gain,
linked-image/gameplay/workspace acceptance. Refresh source-bound evidence,
pin historical tools/reports, preserve foreign work/index; full-tree goal active.

Camera layered burst naming acceptance (BV-03/BV-08, P2): FF9
4967a176e adds four canonical ovl_13c7e800 names. Catalog 4,683 unit/symbol names,
178 scoped alias headers. All four full own-unit bodies and nine current header
inputs reviewed and bound, zero deferrals; other units pending. Evidence:
docs/function-names-camera-layered-burst.json and Binviz
target/ff9-names-camera-layered-burst/. Four full native object pairs equal,
exact affected/scored namespace four; unchanged pinned strict-reloc scores
two exact/two partial, 168/4,596 code bytes, zero failures. Four installed WASM
token comparisons agree; current source/header/catalog/object/review bindings
and eight isolated committed paths audit.

Names cover RGB555 Gouraud-grid/texture transfer, index-phased cosine callback,
three-phase camera/layered burst and zero-state host flag clear. Preserve pool
row restart despite upfront whole-grid allocation, MAC0 gate without NCLIP,
shared captured centerdepth, rejected-cell advancement and skip dimensions.
No packet-storage fix or newly certified transfer direction. Main repeated
h34 poses and seven h3C poses use same handles; discarded sine/repeated cosine
calls remain. Phase transitions37/32/87 retain operation order, exact outro32
mutates pos10Z while loading at pos18. Byte52 clear is a sideeffect, not no-op;
actual flag/effect/resource/provider identities remain unresolved.

Definition/declaration/reference propagation and own-unit alias scope preserve
canonical linker/address/runtime identities; data/types/layouts/signatures,
pads/compiler flags/pins/strings unchanged. Current review does not admit
host providers, portable runtime or uninspected data-table dispatch. Maintained
naming/farm/scorer/preprocessor/thin adapters reused, BV-03 reviewed-deferral
prioritization and BV-06 provider proposals retained. No new implementation/
private ownership/type/CFG walker, matching gain, linked-image/gameplay/workspace
acceptance. Refresh source-bound evidence, pin historical tools/reports,
preserve foreign work/index; full-tree naming goal active.

Scheduled random particle emitter naming acceptance (BV-03/BV-08, P2):
FF9 c498c84ac adds three canonical ovl_ffbd800 names. Catalog 4,686 unit/symbol
names, 179 scoped alias headers. Three full own-unit bodies and g11/g13 headers
reviewed and bound, zero deferrals; other units pending. Evidence:
docs/function-names-scheduled-random-particle-emitter.json and Binviz
target/ff9-names-scheduled-random-particle-emitter/. Three full native object
pairs equal, exact affected/scored namespace three; unchanged pinnedstrict-reloc
scores three exact, 1,224/1,224 code bytes, zero failures. Three installed WASM
token comparisons agree; current source/header/catalog/object/review bindings
and seven isolated committed paths audit.

Names cover indexed/random-offset particle init callback, scheduled emitter
and companion objects, signed-halfword host-value fade. Preserve three ordered
signed-modulo random calls, unbounded global table index without controller
reset, partial state vectors including uninitialized a24, nested emitter access
and frame1/5/9 load order. Final emitter scale assigned after companion loads.
Fade emits before subtraction, ordinary17 values128..0 thenendsat-8; no clamp.
INT callback definition/declaration/hook agree; no inherited VOID conflict.

Definitions/declarations/callback reference propagate; own-unit aliases preserve
canonical linker/address/runtime identities. Original effect/provider/table
meanings unresolved, no type/body/layout repair/provider admission. Maintained
naming/farm/scorer/preprocessor/thin adapters reused; existing BV-03 reviewed
deferral prioritization/BV-06 provider proposals retained. No new implementation/
private ownership/type/CFG walker, matching gain, linked-image/gameplay/workspace
acceptance. Refresh source-bound evidence, pin historical tools/reports,
preserve foreign work/index; full-tree naming goal active.

Rising camera handle transition naming acceptance (BV-03/BV-08, P2):
FF9 5ab4b27ba adds three canonical ovl_11bf0800 names. Catalog 4,689 unit/symbol
names, 180 scoped alias headers. Three full own-unit bodies and g23/g13 headers
reviewed and bound, zero deferrals; other units pending. Evidence:
docs/function-names-rising-camera-handle-transition.json and Binviz
target/ff9-names-rising-camera-handle-transition/. Three full native object
pairs equal; exact affected/scored namespace three. Pinned strict-relocation
scores unchanged: two exact, one partial, 156/1,524 code bytes, zero failures.
Three installed WASM token comparisons agree; current source/header/catalog/
object/review bindings and seven isolated committed paths audit.

Names describe rising camera writes, created-handle transition, tint and two
blend helpers. Preserve frame7 object loads before frame8 rise endpoint,
fixed initial sprite anchor, literal versus created handle identities, unsigned
draw gate, partial initialization and uninitialized transition handle frontier.
Normal tint emits sixteen samples before the omitted endpoint; camera/pose
publication precedes tail position increment. No guessed resource/provider role.
Definitions, declarations and direct helper calls propagate, scoped aliases
preserve linker/address/runtime identities. All types/bodies/layouts remain.

Maintained naming/farm/scorer/preprocessor and thin adapters reused; existing
BV-03 reviewed deferral prioritization/BV-06 provider proposals retained. No new
tool implementation/private ownership/type/CFG walker, matching gain, provider
admission, linked-image/gameplay/workspace acceptance. Refresh source-bound
evidence and pin historical reports/tools; preserve foreign work/index. Full-tree
naming goal active.

Hooked trail and ring finale naming acceptance (BV-03/BV-08, P2):
FF9 08a37e039 adds three canonical ovl_10493800 names. Catalog 4,692 unit/symbol
names, 181 scoped alias headers. Three full own-unit bodies and g14 header
reviewed and bound, zero deferrals; other units pending. Evidence:
docs/function-names-hooked-trail-ring-finale.json and Binviz
target/ff9-names-hooked-trail-ring-finale/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
zero exact, three existing partials, 0/5,976 scored code bytes, zero failures.
Three installed WASM token comparisons agree; current source/header/catalog/
object/review bindings and seven isolated committed paths audit.

Names cover fading trail and camera-vector blend callback, three-phase hooked
trail/model/sprite fade and four-phase growth/particle-ring finale. Preserve
frame29..60 camera interpolation, buffer lookup each callback, INT declaration
versus VOID callback definition used only as address here, halfword/word views,
partial/uninitialized vectors and provider read-extent frontiers. Controller
fade tail draws five sprites, not its comment's seven. Ring phase emits78 per
update including its transition update, not once; preserve six load order and
packed-copy Y adjustment. Repeated trig, release ordering, terminal early returns,
matching pins/flags/wrappers and all types/layouts/bodies remain unchanged.

Definitions, declarations and callback reference propagate; scoped aliases
preserve linker/address/runtime identities. Original effect/resource/provider
identities unresolved. Maintained naming/farm/scorer/preprocessor and thin
adapters reused; existing BV-03 reviewed deferral prioritization/BV-06 provider
proposals retained. No new tool implementation/private ownership/type/CFG walker,
matching gain, provider admission, linked-image/gameplay/workspace acceptance.
Refresh source-bound evidence, pin historical reports/tools, preserve foreign
work/index; full-tree naming goal active.

Fixed camera hooked actor loads naming acceptance (BV-03/BV-08, P2):
FF9 3bfa50f36 adds three canonical ovl_13b41800 names. Catalog
4,695 unit/symbol names, 182 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-fixed-camera-hooked-actor-loads.json and Binviz
target/ff9-names-fixed-camera-hooked-actor-loads/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 668/668 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 reports 12-byte state, mode1 sets SIGNED Y=-7000, unsigned X=0/Z=2000, publishes three halfwords directly at ctx p2C, saves context and loads resource1. G28H3 has only three lanes; padding not initialized, no actor-relative offset arithmetic. Other modes return signed frame>=50, no per-frame pose, finish service or release. Name describes actual fixed position, not original summon identity.

77D8 reports12, mode1 saves context and loads resource5 at supplied four-halfword position without initializing it. Nullable loaded object gets table p14, h12=30 and hc=104. Later modes return at frame>=40. No p0 callback assignment, position tracking, per-frame work or release added; supplied state may be host-filled, remains a frontier.

7894 reports16. Initialization stores SIGNED *out index and saved context BEFORE count test; only idx>=unsigned actor-count-byte rejects, negative indices are not excluded. Valid path asks f1FC(index,pos), loads resource6 then nullable object h22=f220(stored index,32), with signed narrowing retained. Later modes return frame>=40 with no movement. No guessed meaning for index32 provider selector, no reset of stale obj on rejected init. Context global has different local pointer declarations across files, preserved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Directional z growth tint naming acceptance (BV-03/BV-08, P2):
FF9 873a49e7d adds three canonical ovl_11e20800 names. Catalog
4,698 unit/symbol names, 183 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-directional-z-growth-tint.json and Binviz
target/ff9-names-directional-z-growth-tint/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,076/1,076 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 reports92. Mode1 saves context, registers table70(count1), copies THREE halfwords from pc+56 without pad, selects f210(0) flag into mode50=1/0. Nonzero1 sets step200 and zeroXYZrotation; zero sets step-200/Yrotation2048. Translation0/0/500, scale2730 each, create resource2(handle U16), copy THREE translation lanes to v40, resource8 loaded at captured v38, two signed tint countdowns=-1. Retain uninitialized pads, fade, obj1 and unused w2C; no camera publication in this controller.

Exact frame17 captures current translation into v48 BEFORE movement, loads resource9 and arms first count3; exact21 arms count16. Unsigned frame-17<20 permits exactly17..36 ordinary frames, excludes negative/outside frames. Pose/visibility happen BEFORE tint and Z updates. First countdown computes before decrement and normally emits four samples elapsed0..3 at17..20, including0 endpoint. Second normally emits16 samples elapsed0..15 at21..36, stops before -192 endpoint at37. If both externally active, second overwrite wins; preserve both calls/order and signed gates. Translate Z by +/-200, grow scaleZ by256, resource3 f134(frame-17), finish f78 only frame>=60. No added release/clamp/event backfill.

7A9C four-argument VOID calls host f48(a,b,4096-w,w,n), host result discarded. 7AE0 four-argument INT exact (a*(d-w)+b*w)/d; controller consumes return via INT K&R declaration, agrees with definition. Preserve overflow/division-zero/unclamped arithmetic and expression order.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Shared position hook and derived track naming acceptance (BV-03/BV-08, P2):
FF9 3e29aacd5 adds three canonical ovl_101ae800 names. Catalog
4,701 unit/symbol names, 184 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-shared-position-hook-and-derived-track.json and Binviz
target/ff9-names-shared-position-hook-and-derived-track/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 724/1,148 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 six-argument INT callback computes signed frame<<10, clamps ONLY upper4096, host BC(shared start, shared end,w,signed-halfword output), widens three signed lanes <<12 into destination. Interpolates/writes BEFORE testing frame>=total and returning-1, otherwise0. Negative frames are not lower-clamped; invalid shifts/arithmetic kept. No effect easing curve beyond linear host weight or inferred provider read extent.

77C0 reports28. Mode1 saves context, 200(0,0,v14), unsigned Y-=100 with narrowing; copies THREE lanes to shared end. Host84(16,0,vc) fills start, THREE lanes copied to shared start and direct ctxp2C. Loadsresource2 at global constant vector, nullable sets table/h12=11/hc32, repeated outer-null guard then unguarded innerp0 f8=(VOID pointer) INT callback. Own declaration is INT K&R; callback return meaningful to caller unknown and VOID hook cast preserved. No camera-update claim for callback itself; controller publishes camera only once. Later modes return frame>=30, no finish/release.

7968 reports40. Mode1 captures two two-WORD vectors via200/1FC, D4(vec8,shared start,v20), DC(vec8,v20,200,v18); unsigned v18Y-=100. Resource1 atv18 nullable table/h12=52/hc32, resource3 atvec10, THEN nullablep0h22=220(0,32). No invented normalizing/subtraction semantics for D4/DC. Update clears p0 if h30==-1, otherwise publishes THREEv20 lanes into h5C/h5E/h60; terminalframe>=40 checked AFTER writes. No reacquisition of actor position, actual v20 may change externally; no release or resetting unused p4. Preserve partial vectors/pads and shared-global initialization ordering frontier.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Cosine position hook and delayed companion naming acceptance (BV-03/BV-08, P2):
FF9 13e70108d adds three canonical ovl_1221a000 names. Catalog
4,704 unit/symbol names, 185 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-cosine-position-hook-and-delayed-companion.json and Binviz
target/ff9-names-cosine-position-hook-and-delayed-companion/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,220/1,220 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 six-argument INT callback: weight=(i-4)<<8, host BC(shared start, shared end,weight,pos), cosine(weight>>1), subtract cos*200>>12 from SIGNED posY with narrowing, widen three lanes<<12. Writes happen before return-1 if i>=n, otherwise0. Unclamped weight, negative frames, arithmetic right shift versus division and undefined negative-shift frontier retained. No rotation or orbital interpretation of BC arguments; global vectors are u8 extern here and u16[3] in controller, preserve declaration/read-extent frontier.

77F8 reports40. Initialization saves context,20C(16,a),200(0,0,b), THREE-lane b->shared end,84(16,0,c),DC(c,a,0,d),THREE-lane d->shared start. Loads resources1 atconstant global9410 and2 atd BEFORE any hook/scale writes. Nullable first gets cbtable/h12=35/hc32; repeated outer guards then unguarded hdrw8=(void*)INT callback, then f220(0,128) scale; second nullable scale f220(16,128). Callback is INTdecl/definition but stored as untyped address, ABI consumption unknown. Three-lane globals/arrays and partial pads remain, no semantic admission. Later modes frame>=40 return with no finish/release.

7A28 reports28. Saves context,1FC(0,a), copies onlythree lanes to b then unsigned Y-=400; resources3 atb/4 ata, first nullable hooktable/h12=51/hc32. Exactupdate frame4 loadsresource5 ata and nullable table/h12=30/hc112 into third pointer. Terminal signedframe>=40 after optionalload; no per-frame position refresh, padding/object2 initialization orrelease. Provider may fill more than declaredthree-lane arrays, read/write extent remains unknown.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Two handle growth fade texture rect naming acceptance (BV-03/BV-08, P2):
FF9 4ae5a55d4 adds three canonical ovl_10a32800 names. Catalog
4,707 unit/symbol names, 186 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-two-handle-growth-fade-texture-rect.json and Binviz
target/ff9-names-two-handle-growth-fade-texture-rect/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,540/1,540 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7798 state descriptor 108, rotation1C/translation24/scale34, fade44, counts60/64, handles68/6A. Initialization saves context, captures200(0,1,firstvector)/1FC(0,secondvector); copies THREE second-vector lanes to offsetvector with unsignedY-=450. Loadsresource6 atsecondvector, rotationXYZ0, scale2048/4096/2048, SIGNED-widens three inputXYZ intoINTtranslation. f70(table,2) TWOarguments retained; job id read through Obj16 cast+8. Creates resources14/15 U16handles, counts=-1; all untouched vectors/pads/objectslots/fade kept. No extra init or cross-unit type/layout repair.

Exact events: frame6 resources7 then5 atsecondvector; nullable7 gets table/h12=9/hc160. Frame22 resources3 then16 atsecondvector and count18; frame28resource1 atoffsetvector; frame42resource13 atfirstvector; frame36count24. Counters compute fade BEFOREdecrement. First normally19 samples22..40; second25samples36..60, second overwrite wins36..40 though both interpolatecalls retained. Tint draw ends55 even though secondcount continues through60, no endpoint clamp or backfill.

Unsigned frame-22<10 draws first handle at22..31 with scaleY=((frame-22)*4096/10)/2, endpoint omitted. Unsigned frame-32<24 transitions two handles32..55 with weight4096-(frame-32)*4096/24; no zero endpoint at56. Onlyfirst handle receives268/26C after28C. Table rectangle submitted EVERYframe>=22 INCLUDINGterminal>=70: tableindex signedframe%15, x/yU16, width32/height128,298(rect,704,256). Terminal calls78/returns1 AFTERcountdowns/draw/rectangle. No invented screen flash or optional light-pillar semantics, no releases.

7D00 four-argVOID calls host48 with4096-w andw, discarded hostreturn; 7D44 four-argINT weightedratio arithmetic unchanged. INT K&Rmain declaration matches consumed helper return. Preserve division-zero/overflow/unclamped weights and all expression/store ordering.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Two handle growth fade texture rect wide state naming acceptance (BV-03/BV-08, P2):
FF9 36390b7a4 adds three canonical ovl_f832800 names. Catalog
4,710 unit/symbol names, 187 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-two-handle-growth-fade-texture-rect-wide-state.json and Binviz
target/ff9-names-two-handle-growth-fade-texture-rect-wide-state/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,540/1,540 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7798 state descriptor 112, rotation20/translation28/scale38, fade48, counts64/68, handles6C/6E. Initialization saves context, captures200(0,1,firstvector)/1FC(0,secondvector); copies THREE second-vector lanes to offsetvector with unsignedY-=450. Loadsresource6 atsecondvector, rotationXYZ0, scale2048/4096/2048, SIGNED-widens three inputXYZ intoINTtranslation. Pin ta to native $5 and f70(table,2,tb,tc) FOURarguments retained. Creates resources14/15 U16handles, counts=-1; all untouched vectors/pads/objectslots/fade kept. No extra init or cross-unit type/layout repair.

Exact events: frame6 resources7 then5 atsecondvector; nullable7 gets table/h12=9/hc160. Frame22 resources3 then16 atsecondvector and count18; frame28resource1 atoffsetvector; frame42resource13 atfirstvector; frame36count24. Counters compute fade BEFOREdecrement. First normally19 samples22..40; second25samples36..60, second overwrite wins36..40 though both interpolatecalls retained. Tint draw ends55 even though secondcount continues through60, no endpoint clamp or backfill.

Unsigned frame-22<10 draws first handle at22..31 with scaleY=((frame-22)*4096/10)/2, endpoint omitted. Unsigned frame-32<24 transitions two handles32..55 with weight4096-(frame-32)*4096/24; no zero endpoint at56. Onlyfirst handle receives268/26C after28C. Table rectangle submitted EVERYframe>=22 INCLUDINGterminal>=70: tableindex signedframe%15, x/yU16, width32/height128,298(rect,704,256). Terminal calls78/returns1 AFTERcountdowns/draw/rectangle. No invented screen flash or optional light-pillar semantics, no releases.

7D00 four-argVOID calls host48 with4096-w andw, discarded hostreturn; 7D44 four-argINT weightedratio arithmetic unchanged. Typed INT main declaration matches consumed helper return. Preserve division-zero/overflow/unclamped weights and all expression/store ordering.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Tracked pair flash growing sprite naming acceptance (BV-03/BV-08, P2):
FF9 bc9b0569b adds three canonical ovl_109b7000 names. Catalog
4,713 unit/symbol names, 188 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-tracked-pair-flash-growing-sprite.json and Binviz
target/ff9-names-tracked-pair-flash-growing-sprite/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,568/1,568 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

770C descriptor92. Mode1 saves context,1FC/20C actor16 vectors, DC with1800/2000 then BOTH derivedY=-200. Calls2C4(actor16,5,INT3 local)/2B0(local,400,v28),2C4(actor16,34,INT8local)/2B0(local,400,v30), adds SIGNED Y50 to both. Loads resources1/2 at these vectors, nullable scales via220(actor16,256), D4 to two tracking vectors. Native/provider vector extents and D4/DC meanings remain unresolved; no guessed bone identities or normalization. Other object pointers and pads remain partially initialized.

770C exactframe10 setslevel128, screen RGB128/128/128 and loadsresources14/15/8 atderivedv38; exact11 logs original flash string then sameflash, withoutloads. Frame>=13 ALWAYS submits growing framed sprite248 with INTXYZscale=(frame<<7)+2432, rotation SIGNED3 all0, SIGNEDbyteRGB40, original FOURTEEN arguments; three-lane rotation/color extents unchanged. Object0/1 clear whenh30==-1, otherwise publish THREE derivedtracking lanes, including terminalframe>=40 before returning1. No finish/release, no claimed beam/summon identity.

7BD8 descriptor16. Initialization saves SIGNED *out index/context BEFORE only upper count check; negatives not rejected. Invalidslot invokes shared-byte writer(ctx,7,1), returns1, leaves old object/vector. Validslot clearsbyte7 then1FC(slot,vec), loadresource6, nullableh22=220(slot,32). Update exactframe1 setsbyte7=1, thenframe>=40 return. No moving marker update or invented meaning for byte7. Callerctx first word viewed as Buf10** by helper while Ctx16 declares opaque pad0; ABI/layout ambiguity retained.

7D18 three-argVOID writes (*pp)->d[i]=v, where data starts16 bytes after pointed buffer header. Unchecked signedindex and U8 narrowing retained; helper name describes indexed shared-buffer write without guessed flag ownership.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Host point drop return hooked objects naming acceptance (BV-03/BV-08, P2):
FF9 efc576121 adds three canonical ovl_11ce1800 names. Catalog
4,716 unit/symbol names, 189 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-host-point-drop-return-hooked-objects.json and Binviz
target/ff9-names-host-point-drop-return-hooked-objects/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,240/1,240 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor60, mode1 captures1FC(0,v14)/1F8(0,v1C), counters-1/hold0, replacesv1C X/Z fromv14. No blanket zeroing of objectarray/unusedpads. Exactframe0 count12, copies THREE lanes tov24/v2C thenv2CY=-1000; resource3 atv24 and2 atv14 before nullable3scale220(0,128). Exact28 count3/hold0, v2CY=storedv1CY, resource1 atv14. These are event snapshots, not continuous object-target mirroring.

Drop gate SIGNEDcount>=0: gets freshpoint1FC, blend stored SIGNEDY->-1000 elapsed12-count, writes204 then decrements. Newcount==0 arms HOLD on normalframe11 before endpoint; same update separately gets freshpoint and overwritesY=-1000. Nextframe12 also executes interpolationendpoint beforecount-1 andhold write. Preserve redundant calls and earlyhold, not oldcomment twelve-step endpoint assumptions.

Return gate count>=0: eachupdate1FC and1F8 to refresh targetY, replaces targetX/Z withcurrentpoint; blend -1000->refreshed SIGNEDY elapsed3-count,204 then decrement. Atnewcount==0 (normalframe30 elapsed2, beforeendpoint31) fetches1F8 AGAIN and204 restores FULL freshly fetched vector, THEN replaces storedX/Z withoriginalv14; loads7then8 atv14, nullable8hooktable/h12=60/hc24. Normalframe31 still runs count0 endpoint using freshtarget; no added finish/release. If counters overlap externally, preserve drop/hold/return order.

For frame>=0 nullableobj0 clears ifh30==-1 then sets w2C pointer to statev2C, not copies of current hostpoint. Terminalframe>=80 checked after all work, no host78. Both helpers retained exactly; VOIDhost48 resultdiscarded, INT weightedhelper consumed via agreeing INT K&R declaration. Preserve casts/U16 stores/overflow/division behavior.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Six radial placements cycling handles naming acceptance (BV-03/BV-08, P2):
FF9 732012e69 adds three canonical ovl_12063800 names. Catalog
4,719 unit/symbol names, 190 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-six-radial-placements-cycling-handles.json and Binviz
target/ff9-names-six-radial-placements-cycling-handles/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 156/1,264 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor192. Savescontext/table70(count3),200(0,0,THREE-SIGNED-lane anchor), ifY>=-279 setsY=-280. ScaleXYZ32768. t=1F0(256), uppercap2200 when t>=2201, no lowercap; table SIGNEDx=t, y=z=i*682 for6entries. Initializes ONLYthree rotationlanes perentry, notpads/Wvectors/obj/wa4; creates THREEU16handles from resource-ID table, counters=-1. Pin17/18 and matching assignment wrappers retained.

Exactframe0 armsfade4,20fade8,22loadsresource4 atanchor then nullableh22=220(16,128). Ordinary drawgate SIGNEDframe>=0 and<28. Counters interpolate BEFOREdecrement; tint gate is checked AFTER decrement for EACH placement. Normal first calculates five valuesframes0..4 but submits tint ONLY0..3, omitting its zeroendpointframe4. Second calculates/submits eight values20..27 elapsed0..7; frame28 excluded before its endpoint/count decrement, count remains0. Keep overwrite order if externallybothactive.

SIX placements each update: local SIGNED3 rotation0/tableY/0, DC(anchor,local,tableX,localSIGNED3result), SIGNED-widen XYZ into16-byte W with untouched fourthword. Pose vectors addressed via literal i*8+4 and i*16+52. Handle beginsstoredhnd0, increment numeric h++ andwrap ifstoredhnd2<h; STOREDhnd1 neverread in drawloop, contiguous handle assumption not repaired. Visibility/tint gated as above. TableY+=85, Z+=512 but Z is UNUSED by this body; X radius fixed. RotationstateY+=128. No growing-radius claim, no invented usage of Z.

Local3-lane vector provider read/write extent unknown, pads stay. Finish78/return1 atframe>=40 AFTER exactevents/drawgate; no releases. Host48 complementary-weight wrapper VOID/unusedresult; integerhelper INT typed caller agrees, unclamped/division-zero/overflow arithmetic retained.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Table offset xy jitter tint naming acceptance (BV-03/BV-08, P2):
FF9 2a1be7f32 adds three canonical ovl_12509800 names. Catalog
4,722 unit/symbol names, 191 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-table-offset-xy-jitter-tint.json and Binviz
target/ff9-names-table-offset-xy-jitter-tint/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,424/1,424 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor80. Mode1 saves context, clears UNKNOWNbyte52 of ctxp2C, fetches1FC(0,v30)/20C(0,v38), rotationX/Z0 andY=v38Y+2048 withU16narrow, scaleXYZ8192. If210(0), gets1F4(0) keyX then1F0(256) keyZ and scans17 records in order. Firstmatch cbit15 selects originalZ-(c XOR32768), otherwise originalY-c; copiesoriginalotherlanes. No matching record and (keyX>=600 ORkeyZ>=750) subtracts keyX fromY, NOTZ despitecomment; otherwise unchanged. No lower-key clamp. BaseY storedSIGNEDhalfword, directcameraXYZpublished onlyinit. Counters-1,table70(count1), resource1handle338U16; pads/unusedp0/h2C remain.

Everyupdate poses BEFOREjitter/tint/events. Only25<=frame<40 jitterX andY; Z unchanged contrarycomment. Oddframe first draws conditionalrandom-bit call; even skipsit. Signbranch then distinct signed random%4 magnitude, Y=SIGNEDbaseheight+separaterandom%4. Preserve short-circuit/random call count/order and negative remainder semantics; jitter may replace table-adjustedX withoriginalX. Camera vector notrepublished bythisbody.

Exact0 armsSIGNEDcount8; exact64 arms16. First tint -128->-64 computed BEFOREdecrement gives9samples0..8; when NEWcount==0 normalframe7 captures THREE currentpositionlanes toU16spawnvector, Y-=200, loadsresource5 andnullableh22=220(0,128). This is before tintendpointframe8, not comment40/64 spawn. Second40->-128 starts64, ordinaryterminal79 occurs elapsed15 withcount becoming0, stops before80endpoint. Activeboolean set BEFOREdecrement, therefore both endpoint samples tint ifreachable. Both counters can overwrite in originalorder. Finish78/return1 AFTERpose/jitter/tint atframe>=79. No positional hop orrelease claimed.

Host48 wrapper four-argVOID/discardedresult; integerhelper four-argINT consumed via agreeing INT K&R declaration, exact weighted expression retained. Keep casts/partial vectors/arithmetic/flags/matching assignments and provider unknowns.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Delayed host point drop original return naming acceptance (BV-03/BV-08, P2):
FF9 4490c7441 adds three canonical ovl_fcc5800 names. Catalog
4,725 unit/symbol names, 192 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-delayed-host-point-drop-original-return.json and Binviz
target/ff9-names-delayed-host-point-drop-original-return/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,252/1,252 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 reports60. Savesctx,1FC(0,A)/1F8(0,B), SIGNEDcounters-1/hold0, replacesBX/Z fromA. Exactframe5 setsdropcount30, copies ONLYthreeABC/Dlanes thenDY=-1000; resource2 atC and1 atA, nullable2scale220(0,128). Exact40 setsrise3/hold0 andDY=storedBY, resource3 atA. Objectslots andfourthvectorlanes notblanketinitialized; mixed unsignedXZ/signedY views retained.

Dropcount>=0 obtainsfresh1FCpoint, weighted storedBY->-1000 elapsed30-count,204then decrement. At NEWcount0 (normalframe34 elapsed29) hold activates and sameupdate overrideswithfreshpointY=-1000; frame35 stillcomputesendpointandhold beforecount-1. Return gate40..43 refreshes1FC/1F8, BX/Z=currentpoint; rise weightedfreshBY then204beforedec. NEWrise0 at42 elapsed2 causes204(0,ORIGINAL A) BEFOREanother1F8refresh and storedBX/Z=A; loads7then8 atA, nullable8 table/h12=60/h0C24. This differs from11CE1800 freshlyfetched-fullvector restore. Frame43 stillcomputesfresh endpoint. Preserve allrepeatcalls/earlyrestore, notfixoffbyone.

Onlyframe>=5 nullableobj0 clearsifh30==-1 elsep2C=&D; no per-frame copying of currenthostpoint intoD. Terminal signedframe>=80 AFTERallwrites, no finish78/release. Typed INT weightedhelper declaration matchesconsumeddefinition; VOIDcomplementaryhostwrapperdiscardedresult unchanged. Allpartial pads/data/layouts/local vectors/overflow/division/provider frontiers preserved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Clamped position hook delayed companion naming acceptance (BV-03/BV-08, P2):
FF9 845774aff adds three canonical ovl_1316c000 names. Catalog
4,728 unit/symbol names, 193 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-clamped-position-hook-delayed-companion.json and Binviz
target/ff9-names-clamped-position-hook-delayed-companion/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,208/1,208 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 six-argINT callback weight=(n-4)<<9, uppercap4096ONLY, BC(sharedstart,sharedend,w,vec). Calls3C(w>>1) and DISCARDSreturn: unlike1221A000 NOcosineYoffset. Retains vecY=(vecY<<16)>>16 native no-op and shift frontier; widens threeSIGNEDlanes<<12 BEFOREreturn-1 ifn>=lim. Negativeweights/nframes notclamped; globalsu8[]decl here versusU16[3]controller retained.

77EC reports40. Mode1 savesCtx24context,20C(16,a),200(0,0,b), THREEb->sharedend;84(16,0,c),DC(c,a,400,d), THREEd->sharedstart. Distance400 differs1221A000zero. Loads1atconstant9404 then2atd BEFOREhookscales. Nullable1 getscbtable/h12=35/hc32, repeatedguard, unguardedinnerhdrw8=(void*)INTcallback, repeatedguardthen220(0,128)scale; nullable2scale220(16,128). INTcallback declaration agrees with definition butuntypedhookreturnconsumption unknown; Ctx24/W29Ctxcrossfileviews unchanged. Terminal40,no per-frame camera update/finish/release.

7A1C reports28. Captures1FC(0,a), copies THREElanestob then unsignedY-=400; resources3atb/4ata, nullablefirsttable/h12=51/hc32. Exactframe4resource5ata thirdpointertable/h12=30/hc112. Terminal40afterload, no eventbackfill/actorrefresh/release. Preserve own-unit tableaddresses, partialpads/thirdpointer initialization andprovider vectorread/writeextents.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Early two handle growth fade texture rect naming acceptance (BV-03/BV-08, P2):
FF9 9e5946688 adds three canonical ovl_10abc000 names. Catalog
4,731 unit/symbol names, 194 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-early-two-handle-growth-fade-texture-rect.json and Binviz
target/ff9-names-early-two-handle-growth-fade-texture-rect/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,712/1,712 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor116. Copies THREE rawctxp0C+56 halfwords separately into THREEpositionvectors; v58Y-=800/v60Y-=1200 U16narrowing, v60unusedlater. Savesctx, loads5then14 atv50; nullable5hooktable/h12=12/hc160, nullable14h22=384. THEN rotationXYZ0 and re-reads hostposition SIGNEDXYZ intoINTtranslation AFTERloaders (do not hoist freshreads), scaleXYZ4096, table70(count2), handles11/13 U16, counters-1. Preserve otherobjectslots/partialpads/fade and job Obj16cast+8.

Exactframe18 loads12then15 atv50, nullable12h22=384, count18;22loads3then2 atv58;39loads16atv58, notunusedv60;28armscount26. First interpolation -128->0 emits19 computedvalues18..36; second0->-128 emits27computedvalues28..54 and overwrites first28..36 while both calls retained. Countdown math occurs outside drawingwindow, no negative-frame guards added.

Unsignedframe-18<6 growth18..23 scaleY=k*4096/6, omittedendpoint24. Unsignedframe-24<26 handleblend24..49 weight4096-k*4096/26, omittedzeroendpoint50. 268/26Conlyfirsthandle after28C. Tintcountcontinues to54 afterlastposed49. Everyframe>=18 rectangle from signedframe%15 table, U16x/y32x128 to298(rect,704,256), INCLUDINGterminal70+ before78/return1. No audio/flash/beam identity inferred. Helpers VOIDhost48discardsresult, INTweightedratio agreesusedINT K&R declaration; unchanged arithmetic/layout/partialproviders.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Six radial tint companion objects naming acceptance (BV-03/BV-08, P2):
FF9 47b08029a adds three canonical ovl_12089800 names. Catalog
4,734 unit/symbol names, 195 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-six-radial-tint-companion-objects.json and Binviz
target/ff9-names-six-radial-tint-companion-objects/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 156/1,380 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor204, TWOobjectslots, rotations+8/W+56/scale152/tint168/anchor172/secondvector180/counts188/handles196. Savesctx/table70count3,200(0,0,anchor) then1FC(0,secondvector); upperYbound-280 appliedONLYanchor, scaleXYZ32768. Radius1F0(256) uppercap2200ONLY; SIXtable xradius/y=z=i*682 andthree initializedrotationlanes. THREE338createdU16handles, counts-1. Keep fourthlanes/Wwordpadding/obj/wa8uninitialized andpin17/wrappers.

Exactframe2 loads8 atcaptured secondvector, nullableh22=220(16,128); exact22loads4 atanchor andsamescale. Exact0armsfade4,20fade8. Drawgate SIGNED0<=frame<28. Fade compute beforedecrement, visibility/tintgate AFTERdecrement: firstcomputesfive0..4 buttints0..3, zeroendpointomitted. Secondcomputes/tintseight20..27 elapsed0..7, frame28drawgate blocksendpoint/countdown andcountstays0. Provider call ordering and externallyoverlappingcounter overwrites retained.

SixplacementsDC(anchor,localSIGNED3rotation0/tableY/0,tableX,localSIGNED3output); SIGNED-widen intoINTXYZ at16-byteW withfourthworduntouched. Nativepose addressesi*8+8 andi*16+56 retained. h startsstoredhnd0 thennumeric h++/wrapifhnd2<h, storedhnd1unused; no repairtoarraycycling. TableY+=85/Z+=512 (Zunusedhere), Xradiusfixed; rotationY+=128. Partial3-lane/providerread-writeextents unresolved. Finish78/return1 at40afterevents/drawgate, no releases. INTtypedblendcall agreesdefinition, VOIDhostwrapperdiscardedresult.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Derived pair flash growing sprite marker naming acceptance (BV-03/BV-08, P2):
FF9 c5418e8e2 adds three canonical ovl_fc0d000 names. Catalog
4,737 unit/symbol names, 196 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-derived-pair-flash-growing-sprite-marker.json and Binviz
target/ff9-names-derived-pair-flash-growing-sprite-marker/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,512/1,512 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor80. Savesctx,1FC/20C(actor16,vectors), copies THREEa14->a34 BEFORE DC(a14,a1C,1500,a34), thenSIGNEDa34Y=-200. 2C4(actor16,7,INT4local)/2B0(local,400,a24),2C4(actor16,15,INT8local)/2B0(local,400,a2C); NO+50Y adjustment unlike109B7000. Loads2ata24 then1ata2C nullableh22=220(actor16,256),D4(a24/a2C,a34,a3C/a44). Do not infer bone identities or D4normalization; preserve copied lanes, partialpads, untouchedobjectslots/level.

Exact10flash128/128/128 thenloads11/12/5ata34. Exact11sameflash butNOdebugprint. Frame>=13 submits FOURTEEN-arg248 withINTscaleXYZ=(frame<<7)+2432, firstTHREErotlanes0, firstTHREEcolorbytes40. Localvectors are4INT/4SIGNEDhalfwords/8U8 here, tails uninitialized (different109B7000three-lane buffers). Eachobject clearsifh30==-1 otherwise writesTHREEderivedtracklanes; terminal40afterdraw/tracking, no78/release.

7B98 descriptor16. StoresSIGNEDindex/contextBEFOREonlyuppercountbyte check, negativesallowed. Invalidindex writesbyte7=1 andreturns1 withoutclearingoldpointer; validwrites7=0,1FC(index,vec),loads3 (not109B7000resource6), nullableh22=220(index,32). Exactupdate1setsbyte7=1,terminal40,no continuousposition refresh. Shared-bytewriter VOID (*pp)->d[i]=v usespointedbuffer+16 uncheckedindex/U8narrowing; callerCtxF firstopaque word viewed asBuf10**, keepABI/layout/flagpurposefrontier.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Delayed dual handle growth tint naming acceptance (BV-03/BV-08, P2):
FF9 4902794fb adds three canonical ovl_11e6c000 names. Catalog
4,740 unit/symbol names, 197 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-delayed-dual-handle-growth-tint.json and Binviz
target/ff9-names-delayed-dual-handle-growth-tint/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 156/1,652 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor112, savesctx/table70(count2), initiallyreads hostX intoSIGNEDanchor then OVERWRITESX0, captures SIGNEDY/Z. Rotation1XYZ0, allsixINTscales0, rotation2X/Z0/Y512, INTtranslationfromanchor. Creates10/11 U16handles, lowhalf copies translation intoh60/62/64, loads2then3 atanchor, SIGNEDcounts68/6A=-1. Preserve redundant firstXread/store, partialpads/otherobjectslots and -fno-cse-skip-blocks/wrappers. h68 NEVERarmedhere; w3Cfade NOTinitialized.

Exact15 resource4nullableh22=-256; exact54 resources13then12nullableh22=128/256; exact82 armsSIGNEDfadecount16. Draw50..97: firsthandle pose/visibility onlyd>=4 (54..97), secondall50..97; both poses BEFOREfade/growth. Dormantfirstcount would interpolate-128->0 over3ifexternallyarmed. Second normally16samples82..97 elapsed0..15, missing -256 endpoint98 duewindow; countbecomes0. Tintcalls occur EVERYdrawupdate evenbefore82, so uninitialized w3C frontier50..81 remains; no invented fade initialization.

FirstscaleXZ+=64 onlyd>=4; firstY+=64 for4<=d<16, else-8. SecondXZ+=64all; secondY+=85 ford<12, else-8. UppercapsXZ1638/Y3276 appliedafterupdates withliteralconditionals; no lowerclamp. Thus poseshow PREgrowthscales and latesttint. Everyupdate resource134 uses14before50,8from50 (includingterminal120), then78/return1 at>=120; mode1 goes directlytotail without134. No release or sprite-identity inference. INTtypedweightedhelper agreesusedreturn; VOIDhostwrapperresultdiscarded.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Zero offset derived pair flash marker naming acceptance (BV-03/BV-08, P2):
FF9 f8c6c629b adds three canonical ovl_fe00000 names. Catalog
4,743 unit/symbol names, 198 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-zero-offset-derived-pair-flash-marker.json and Binviz
target/ff9-names-zero-offset-derived-pair-flash-marker/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,512/1,512 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor80. Savesctx,1FC/20C actor16 vectors,THREEa14->a34 beforeDC with2000 thenU16Y=-200. 2C4(actor16,31,Scratch32)/2B0(scratch,0,a24),2C4(actor16,45,Scratch32)/2B0(scratch,0,a2C), unlikeFC0D000selectors7/15 andoffset400. Loads1then2 atderivedvectors, nullableh22=220(actor16,256),D4derivestrackvecs. No guessed bone/matrixcontract/provider extent. Pin17 retained; seven actual vectorarrays not commentnine.

Exact10flash128/128/128 thenloads11/12/5 ata34; exact11sameflash, no debuglog. Flashargument&ctxp20w4 is byteoffset4, notarrayelement4. Frame>=13 FOURTEENarg248 withfirstTHREE32-bit scales(t<<7)+2432, firstTHREErotHalfwords0/colorU8s40; Scratch32 tails remainuninitialized. Sharedscratch reused frominitmatrixcalls, no new tailinit. Nullableeachtrackobject clearsifh30==-1 elseTHREEU16tracklanes storedintoSIGNEDh5C/h5E/h60. Terminal40AFTERdraw/tracking,no78/release.

7B98 descriptor16, ownbody reviewed despitetransplantcomment. StoresSIGNEDindex/contextbeforeuppercountcheck, negativesallowed; invalidwritesbyte7=1/returns1 leavingoldpointer. Validwrites7=0 then1FC/loadresource3, nullableh22=220(index,32). Exactupdate1sets7=1,terminal40. BytehelperVOID (*pp)->d[i]=v withpointedbuffer+16, uncheckedindex/U8narrowing. Contextfirstwordopaqueview versusBuf10** andUNKNOWNflagpurpose remain; no ABI/type fixes.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Four companion tracked pair flash marker naming acceptance (BV-03/BV-08, P2):
FF9 f30de5815 adds three canonical ovl_10200800 names. Catalog
4,746 unit/symbol names, 199 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-four-companion-tracked-pair-flash-marker.json and Binviz
target/ff9-names-four-companion-tracked-pair-flash-marker/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,516/1,516 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor84. Savesctx,1FC/20C actor16 two-WORDvectors, copiesTHREEhalfwords to v38 thenDC(distance1000)/SIGNEDY=-200. 2C4(actor16,28,INT3local)/2B0(local,800,two-WORDvec28),2C4(actor16,54,INT8local)/2B0(local,800,two-WORDvec30). Loads3then4, nullableh22=220(actor16,256),D4outputsdeclaredTHREEU16tracklanes pluspad. Keep providerread/writeextent frontiers and repeatedcopy/order, not guessed geometry.

Exact10 loadsFOURresources5/6/1/2atv38 and DOESNOTflash. Exact11onlyflashRGB128/128/128 atctxp20+1 INTpointer (byteoffset4). Frame>=13 FOURTEENarg248 firstTHREEscales(t<<7)+2432,threeSIGNEDrot0,threeU8RGB40, buffersdeclared3notpaddedlarger. Eachobject clearsifh30==-1 elseTHREEtracklanes toh5C/h5E/h60; terminal40afterdraw/tracking, no78/release. Preservepartialstate,pointer/wordvectors,unusedlocals.

7B9C descriptor16, SIGNEDindexstored andctxp0C readbeforecontextsave/countcheck; onlyindex>=unsignedcountrejects. Invalidwrites7=1andreturns1; validclears7/1FC(index,vec)/resource7/nullableh22=220(index,32). Update1sets7=1,terminal40. Helper declaredVOID(Efx3Ctx*,INT,INT) atcaller versus definitionVOID(Buf10**,INT,INT), retains opaque firstword view/frontier. Bytewriter(*pp)->d[i]=v atbuffer+16 unboundedindex/U8narrowing; unknownflagrole/type mismatch not repaired.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Offset hooked pair staggered records naming acceptance (BV-03/BV-08, P2):
FF9 67f8ea3df adds three canonical ovl_12442800 names. Catalog
4,749 unit/symbol names, 200 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-offset-hooked-pair-staggered-records.json and Binviz
target/ff9-names-offset-hooked-pair-staggered-records/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 2,008/2,008 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor24: saves context, f200(16,0,v8), copies exactly three U16 lanes to v10, Y+=300/Z-=300 with narrowing. Loads resources6 then2 at v10 before either nullable configuration; each gets its own table, h12=13/hc=144. Context extern uses G25Ctx while other bodies use unit Ctx, preserved. No pad initialization, callback replacement, movement or release; terminal40 only in update modes.

7888 descriptor40, host three-halfword anchor, saves context, loads3/9/10. Five shared records reset started/age, start=2*i, copy three lanes to BOTH positions, randomize only second X/Z with separate branch-selection and magnitude fc0 calls, signed modulo800 and U16 wrap retained. Resource fields and pads left untouched. Exact start frames0/2/4/6/8 load into p[1+i] using pointer to entire record and mark started; ELSE age++ only when already started, so no age increment on birth update. No interpolation performed here; downstream record use unknown. Pin18 and do/while single store kept. Later return frame>=50 after processing, allowing repeated/skipped frame behavior as written.

7C40 descriptor48, repeatedly reads host anchor for three separate vectors rather than copying snapshots; X offsets -400/+400 only, U16 narrowing. Saves context then resources11/14 at central anchor; nullable first object table/h12=8/hc=144. Exact5 loads12/13 centrally, exact13 loads13 at both X-offset positions. Terminal50 after scheduled work, no f78/release/camera publication. Partial pads/unused slots/provider extents stay unresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Ground level pair sequential actor objects naming acceptance (BV-03/BV-08, P2):
FF9 8dbbc52ec adds three canonical ovl_10327000 names. Catalog
4,752 unit/symbol names, 201 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-ground-level-pair-sequential-actor-objects.json and Binviz
target/ff9-names-ground-level-pair-sequential-actor-objects/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 860/1,384 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor12: w0=0, save context, copy two full words from pc+56/+60 including fourth halfword, THEN clear signed Y halfword at state+6. Resource2 nullable h22=-300, resource1 nullable h22=-600, no pointer retained in state. Update resource5 at current signed frame ONLY if w0==0; terminal60 check is inside same gate. Externally nonzero w0 stalls completion, not repaired. No f78/release/provider normalization.

7848 descriptor60: captures unsigned actor-count byte, clears context p0 byte18, fills vec[4][2] with 1FC and nulls obj[4] for every i<count; no bound4 or zero-count guard. phase/index0. Phase0 loads resource3 at indexed cached vector even count0, nullable h22=220(index,64), advances phase1 and resets job frame=-1. Phase1 only when frame>0 advances index then phase0 or phase2 with frame=-1; returns0 directly. Phase2 terminal10; default0. Preserve pin3, empty asm barrier, signed h22 narrowing and unguarded count/index behavior; no completion byte set here.

7A54 separately reviewed: same cached-vector initialization/count hazards and phase0/1/2 schedules, but loads resource4 and writes ctx p0 byte18=ph (normally1) only after final phase1 advance, before job frame=-1. Phase1 breaks rather than immediate return, then final0; terminal phase2 frame>=10. Pins3/4 retained. Shared flag purpose and mutable count/phase remain unknown; name records signal behavior without declaring host meaning or correcting copied comments.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Two handle y descent settling growth tint naming acceptance (BV-03/BV-08, P2):
FF9 50362e9a9 adds three canonical ovl_132b5800 names. Catalog
4,755 unit/symbol names, 202 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-two-handle-y-descent-settling-growth-tint.json and Binviz
target/ff9-names-two-handle-y-descent-settling-growth-tint/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 156/1,396 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

77C4 descriptor136, gcc2.8.1. Save context/table70(count2), captures U16 host three lanes. First rotationXYZ0/scales1024,2048,1024; first position hostX/Z but Y overwritten1200 after redundant initial hostY write. Second rotationXYZ0/translation signed hostXYZ, scales all0, w60/64/68=0. U16 handles resources2then1 via338(resource,1). Keep uninitialized k branch with identical arms, new_var assignments and all pads/unused fields; countdowns=-1, tint not initialized but gated until countdown. No invented geometry/provider contract.

Exact0 resource4, exact8 resource3 at captured v70; exact15 count3; exact32 copies three lanes to signed v78 then overwrites Y=-600 and loads8; exact47 count16. Draw15..62 exactly48 updates: 320(handle0,firstY),31C(first pose),60(second pose), all BEFORE tint/position/scale updates. First tint computes four samples15..18 including0 endpoint; second16 samples47..62 elapsed0..15, missing -128 endpoint63 outside window. Tint applied to both handles only while either count nonnegative; if both externally active second overwrite wins.

FirstY-=325 for first8 updates, then +=20; first scaleY shrinks64 while >1024, no lower guard beyond literal condition. SecondXZ+=64 each update, secondYscale+=170 first8 then-=8; literal ternary upper clamp1024 using new_var=1025 retained, no lower clamp. Every update advances resources5/7/6 in that order including terminal100 then78/return1. Helpers directly reviewed: VOID host48(a,b,4096-w,w,n) and INT exact weighted arithmetic consumed by main; no overflow/divide-zero/return-type repairs.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Shared position cosine arc scale naming acceptance (BV-03/BV-08, P2):
FF9 5a1478196 adds three canonical ovl_11465000 names. Catalog
4,758 unit/symbol names, 203 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-shared-position-cosine-arc-scale.json and Binviz
target/ff9-names-shared-position-cosine-arc-scale/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,516/1,516 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor24: saves context, 1FC(0,pos),200(0,0,dir), loads2then3 atpos; only nullable second gets h22=220(0,128). Provider-filled dir unused here, no invented orientation transfer. Update terminal40, no movement/finish/release. All four-lane buffers and unused pads untouched.

7828 sixarg INT copies THREE U16 shared lanes into SIGNED pos then dst0=pos0<<12, volatile cur read into new_var BETWEEN dst0 and dst1 stores, then dst1/dst2 shifts, volatile end read in terminal comparison. Stores occur even when cur>=end; return-1 then0. Keep negative signed shifts/volatile scheduling/unused args; no constant-vector claim or callback return ABI repair.

7894 descriptor88 savesctx/table70(1),200(16,1,target) and U16Y-=80;1FC(0,start),THREE start->cur lanes, load4 nullableh22=220(0,128). Rotation/translation firstTHREE0, scales27, U16handle58(resource1,279,0,0,192). Exact10load5atglobalbytevector, nullable table/h12=18/hc=176, redundant guards and unguarded innerrec->fn=(void*)callback thenh22=220(0,16). Exact26 captures cur to v4C BEFORE that frame interpolation, loads16.

For frame>=10,d=frame-10: d<17 publishes signed current cur to INTtranslation, then60pose/268visibility BEFORE scale and interpolation. First12 scale updates+1 (frames10..21); SHRINK starts d>=46/frame56, subtract8 all, resetall0 onlyifX<0. For d<17 (10..26) copycur to mutable shared vector BEFORE BC(start,target,(d<<12)/15,cur), then U16Y-=((3C(k>>1)*600)>>12). Thus17samples includes endpointd15 and extrapolationd16 with no upperclamp; hook observes prior cur, not new interpolation. Resource6 advanced with d everyframe>=10 includingterminal80;78/return1 afterwork. No releases/additional state initialization/provider fixes.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Grey derived pair flash growing sprite marker naming acceptance (BV-03/BV-08, P2):
FF9 85ac932d7 adds three canonical ovl_fcf3000 names. Catalog
4,761 unit/symbol names, 204 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-grey-derived-pair-flash-growing-sprite-marker.json and Binviz
target/ff9-names-grey-derived-pair-flash-growing-sprite-marker/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
3 exact/0 partial, 1,556/1,556 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor84, savesctx,1FC/20C actor16,A three lanes->E,DC(A,B,1000,E) thenSIGNEDY=-200. Selectors13/29 via2C4 into two32byte locals;2B0 offsets500 intoC/D. Loads13then14 nullableh22=220(16,256);D4(C,E,F)/(D,E,G). Exact10flashRGB128 at U8p20+4 (byteoffset4) thenFOURloads15/16/11/12 atE;exact11flashonly. Frame>=13 FOURTEENarg248, scale frame*128+2432 (multiplication retained), zero3U16rotation andRGB8 (not relatedRGB40). Scratchunion reuses first matrix, pads/tailsuninitialized, no extent assumptions.

Each nullable object clears on h30==-1 then guarded THREE tracklanes intoU16h5C/5E/60; SIGNED sourceY converts aswritten. Terminal !(frame<40) afterdraw/tracking; no78/release. Context p20 U8pointer plus4 differs INTpointer variants, unchanged.

7BC4 descriptor16, n=*io SIGNEDfullword, storedbefore p0C/contextsave/uppercountguard; negative indices notexcluded. Invalid byte7=1/return1leavesoldptr. Valid clear7 then1FC(n,vec),resource7,nullableh22=220(n,32). Exactupdate1setsbyte7, terminal!(frame<40). Localctx pointer declaration in bytehelper differs Buf10** definition; preserve opaque firstwordview/ABIfrontier. Writer (*pp)->d[i]=v, data+16, unboundedindex/U8narrowing; no flag-purpose guess.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Spread burst particle proximity finish naming acceptance (BV-03/BV-08, P2):
FF9 0c6f4bdae adds three canonical ovl_104bc000 names. Catalog
4,764 unit/symbol names, 205 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-spread-burst-particle-proximity-finish.json and Binviz
target/ff9-names-spread-burst-particle-proximity-finish/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 56/1,816 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 mode1 phase0,w4=(rand&31)+39,w8=rand&31,h26=585, no other init/descriptor. Phase0 exactframe0 ninearg110 resource2 thenw4+=32 upper768,w8+=102 upper4096, D4/E4/DC providercalls retained with in-place stored vector, localv28Z=frame<<5. Pose60 receives SIGNED currentXYZ and scale585,585,oldh26;h26+=151; temporarily signedh14+=2048 thenrestoreafterpose. Barrierforcesliteralone before110resource2. SIGNED-narrow target-minus-current differences used squared sum<=262143 ORframe>=17 tophase1; writes HALFWORD BEFOREstate=-1. If targetw20bit1 thenflash32/16/0 atctxp20+1INT (byte4). No claim generic Euclidean overflow safety, pointerprefix ownership or undocumented provider math.

Phase1 fillsfourhalfwordlocal{1024,0,frame<<5,ph}, copytwo storedwords then Y-=frame*4. Temporarilycontextword+40=64 for ninearg114resource9, thenZERO (doesnotrestorepriorvalue). Resource8 onlyframe<16, copiedY=0;110resource2always, terminalframe>=24 AFTERallcalls. No phase-frame reset, meaning skipped/repeatedframe and earlyproximity behavior preserved. Pins17,barriers,do/while,narrowing/overflow/uninitializedpad allretained.

7BC0 descriptorwords0=40/2=40/5=(INT)callback/1=32, words3/4 untouched. Initw0/n/w20=0, savesctx,200(0,0,h4),200(16,0,wC),20C(16,h14),U16Y+=2048,table70(1),338(resource10,1)->w24. Updateonlyw0==0: ifn<32 PREINCREMENTn before184(pooljob+16), allocfailure stillcounts. Success copiesTWOwords into particlewC/w10, THREEh4/h6/h8 into20/22/24; Y=U16h16-768+((n&3)<<9),X=U16h14+(n>>2)*255,Z0, recordptrbase+n*84. Indices1..32 (not0..31), actualtableextentunknown. Preserve asm on tempY and all opaque/prefix fields. w20 increments every activeupdate EVENalreadyatlimit; returns1 once n>=32 regardlessallocsuccess, nonzero w0 stalls.

7DE4 threearg INT handler: onlymode1savesctx/calls78, but EVERYmode returns1; no descriptor or allocator semantics invented. Different saved-context local declarations preserved. Body/name-onlypass no runtime/export changes.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Parabolic displacement staged tracking swap naming acceptance (BV-03/BV-08, P2):
FF9 b842e8792 adds three canonical ovl_132d2800 names. Catalog
4,767 unit/symbol names, 206 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-parabolic-displacement-staged-tracking-swap.json and Binviz
target/ff9-names-parabolic-displacement-staged-tracking-swap/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 2,548/2,792 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

77D0 VOID: unconditional k=0x20000000/(dur*dur) EVEN defaultmode; r0. Mode0 r=4096-((0x10000000/dur)*t>>16);mode1 k*(dur-t)*(dur-t)>>17;mode2 4096-(k*t*t>>17). These literal integer/truncated/overflow formulas retained; do not replace with comment claiming factor2 or clamp. Allmodes pass(handle,extra,r) to savedctx B4, no return. No other own-unit body directly calls this helper; external usage unresolved.

7908 descriptor96, init eightwords0 descending order,20C(0,v38)/20C(0,v44),fiveobjNULL;othervecs uninitialized. Everyupdate clears ended objpointers,80(16,4,v30) beforeevents. Exact0 captures1FC16/start0, resource5 into sharedglobal notfiveobjarray; nullable table/h12=4/hc156, unguardedinnerp0 hookINTcallbackcastVOIDaddress;count14. Exact14 2BC(0,0),1F8fresh/204samevec. Exact20/26/32 loadsresource1 atseparate80-derivedglobalpositions;46resource2;eachcount16. Exact50 w2=2,2BC(0,128),2A8(0,v44),1F8(0,global),resource3/count16.

w0 countdown decrements BEFOREk=14-w: samplesk1..14 onframes0..13, no k0. SharedX/Z useB8(start,dynamicend,k,14);Y=startY-(600*k-37*k*k) intoU16;204publishes;v38lane0-=170 then2A8. Fourtrackingblocks independently decrementcounts,2C4/2B4 tmp,nullable3positionwrites,80/1D8 call EVENwhenobjNULL. Normal windows20..35,26..41,32..47,46..61. Fifth window50..65 setspos3072/0/0 only, no1D8. w2 predecrements at50→1 weight2048 BC(start,capturedactor16,vec),51→0 usesfresh1F8 then204; no assumedrestoretoinitpoint. Terminal70 AFTERallwork;no78/release.

81C4 sixarg INT callback: s8selector-1 (zeroonlysel1) selects shared85A8 else85B0, capturesTHREEU16lanes BEFORE overwriting85B0 with oldpositionwords>>12; writes dstTHREE thenSIGNEDtmp<<12into positionwords; allstores BEFOREterminal n>=lim returns-1 else0. Alias-safe snapshot order retained, possible overlap/provider extent not normalized. D85A8 declaration4lanes inmain versus3here retained; duplicatedifarms/pins/new_var arithmetic unchanged.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Eight handle trig morph actor oscillation pulse naming acceptance (BV-03/BV-08, P2):
FF9 bfb75ca04 adds three canonical ovl_11938000 names. Catalog
4,770 unit/symbol names, 207 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-eight-handle-trig-morph-actor-oscillation-pulse.json and Binviz
target/ff9-names-eight-handle-trig-morph-actor-oscillation-pulse/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 1,548/3,820 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 descriptor56, w0=0,savesctx,200(0,0,a4),1FC(0,hostvec),table70(8);eight58handles resources5/2/3/4/13/7/6/8, firstfive+last flags57/15616, middlepair59/15617. k=(1F0(0)<<12)/720; ifk<1024replace1700, if>6144cap6144; no claim1F0random. Saveglobal/sharedscale. Updatesnonzero w0 return0 stalls. First t<24 cosine-size and Yrotation=-t<<5, four28C transitions using descending4096weights and strict thresholds12288/8192/4096;60last andTHREEseparatecosRGBcalls. Negativeframes allowed aswritten.

From24 constantbase scale(shared*818>>12),sin phase((t-24)<<12)/26>>2;pose24/30handlesandseparateRGBsin calls. 240uses table[t%23+1] EVERYactiveupdate. Additional28Cpair28/2C starts24 withX/Z=(shared<<11)>>12,Y=(cos(q>>2)/4+682)*shared>>12,rotationY=-t<<6,weight4096-q;cosRGB(q>>1). Preserveseparatereads/calls and different scale evaluation, no combine or safe-negativeindex. Exact16 resource1nullableh22=220(0,32);exact1 resource9(nullable64). Terminal50 afterallposes/table/events then78;no release.

7FE4 descriptor104, unsignedcount controls arrays8 withNO bound. For eachactor1F8 storesfullTWOword position, w44=(-400-(rand&127))*sharedscale>>12 OVERWRITTENperactor so last random value shared byALLactors. 80(i,0,tmp),offset[i]=-SIGNEDY. Update<32 startsfromcached2wordvector eachtime, adds sharedw44*cos(frame*32)>>12; >=32 adds offset*((frame-32)<<10)>>12 THEN w44*sin(phase>>2)>>12 withU16narrowaftereachaddition;204eachactor. At>=36 firstperformsoscillation/extrapolation, THEN loopsfresh1F8/204 resets andreturn1, preserving visiblecallorder. No storedoriginal substitute, per-actor amplitude fix or extra bound.

82B0 descriptor16, phase/counter0;everyupdate2B8(16,1,vec) beforephase andterminal. Phase0 n*4096/3,cos/c pulse;atn>=3 phase1/frame=-1 butstilldraw. Phase1 paritywobble andq=n*4096/9,phase2/frame=-1at>=9 butstilldraw. Phase2 sine/cos amplitudes androtation1024-q/4, returns1atn>=9 BEFOREdraw/reset/counterincrement. Otherwise rotationfourhalfwords{0,0,b,0},contextw28=128,twoSEVENarg128 resources14/15 thenw28=0 (notrestoreprior),counter++ evenunknownphase defaultszeros. No78/release/frameclamp/providerfix.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Five handle rising flash actor position naming acceptance (BV-03/BV-08, P2):
FF9 ff7541208 adds two canonical ovl_13336000 names. Catalog
4,772 unit/symbol names, 208 scoped alias headers. Three full own-unit
bodies reviewed and bound, one semantic deferral; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-five-handle-rising-flash-actor-position.json and Binviz
target/ff9-names-five-handle-rising-flash-actor-position/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 152/1,948 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor84,gcc2.8.1. Init1FC16SIGNED3lane position→INTs,20C16rotation,2A016scalevec,savejob/ctx,phase/frames0; A XYZhostbutYoverwritten0,rotationY+=2048,U16C X/Zhost,Y=A0;B NOT initialized. Phase0 waits signedframe>0 thenphase1/jobframe=-1,2BC(16,0),358(80),table70(5),five338resourcesSIGNEDtableIDs(count5),flash128/128/170 atctxp20 byte8. Provider meanings/resourceidentities unknown.

Phase1 s=(frame<<12)/180,flashbyte8 at0RGB64/64/85 and1/20/40RGB42/42/56. Forframe>=0: firstfourupdatesrotationZ+=12,CX+=14,AX+=12;laterZ+=2,CX+=8,AX+=6. Localtranslation initiallycopieshostY thenOVERWRITESY=s; independentC8(cos(s>>2)>>6) randomcalls offsetX/Z, B copiesU16localXYZ thenBX rewrittenX+(s>>4). Five320(handle,s)/31C(samerotation,translation,scale) AFTERupdates; no camera publication, no sharedrandomcall or lowerclamp.

Exact10 resources2/3/4 atA and10atC;30resources5/6atA;69resources7/8/9atA;75resource13atA. Allnullableh22=320,lastaddstable/h12=44/hc=76. Phase1terminal180 AFTERposes/events calls78andreturns1 beforeframes++; otherwiseeveryupdateincludingphase0/default frames++. Keeppins19/do-while/identicalifarms, unusedlocals, incompletepad/hostvectorinit and globals.

7E5C descriptor4, mode1 savesctx,1FC(16,SIGNEDlocal4) thenTHREEsignedlanes written directly to ctxp2C object+24/26/28 andreturn1. No nullguard/fourthlane copy or fixedpointconversion; mode0/otherupdates0. Localcontext declarations differ acrossmain/wrapper/publication and retained. 7E08 remains namedbycanonicaladdress, currenthash/reason bound inreport for future provider resolution.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Stretched texture gradient screen transition naming acceptance (BV-03/BV-08, P2):
FF9 846193f80 adds three canonical ovl_12a35800 names. Catalog
4,775 unit/symbol names, 209 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-stretched-texture-gradient-screen-transition.json and Binviz
target/ff9-names-stretched-texture-gradient-screen-transition/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 332/3,284 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

C704 elevenargVOID, -fno-expensive-optimizations,pins22/23. Reserve640bytes up front for16FT4s, 4x4loops; cellW/H arithmetic shifts>>2,originX=x-(w>>1)*3/2,Y=y-(h>>1). Xquad edges stretch3/2 with original integerdivision ordering; UVlastcolumn/row one texel shorter, RGBallr narrowedU8, opaque2C ifmode255 elsesemitransparent2E. TpageOR((mode&3)<<5) insideeachquad, CLUTpreserved; host234(0x09000000,arg0,p) perquad. No tag-length initialization/allocator bounds/negative-size fixes; pads/provider ownership stay.

C944 sixargVOID reservesfour36byteG4s, len8/code3A. Fori0..3,leftX=x,righttopX=x+128+16*i,rightbottomX=x+128+16*(i+1),Y=60*i..60*(i+1), so slanted/growing-width bands, notconstant16wide rectangles. LeftbothRGB(r,g,b),rightbothblack. 23C(0x08000000,arg0,p,arg1),typeof temp/do-while retained, no newtag/pad init. Mainpassespointerasfirstarg throughK&R versusINTdefinition, unchanged.

CA90 descriptor124, phase/framecounter0, savesctx/table70(2),handle338(resource158,1),68(sceneobjectw180,handlew60). Init firstpose zeroXYZ/scales2048,2048,1365, secondpos0,0,256/scales4096/rot-1024,0,0; buildlowhalfwordw18/w1C withX1024then+32,Y0,Z-12288, publishctxp2C vectors atbyte8and16. w8/wC NOT initialized here but used atphase transition, leavefrontier; allpartialpads/pointers remain.

Phase0 EVERYupdate decrements bothZby620 beforepose, evenframe0. s=(t<<12)/24; scenepose onlyt!=0 with104tint, main60/26Ctint thenpublishvectors andX+=32. 240everyframe;sinflashp20+8192 onlyt<12,redflashbyte8 onlyt>12 (noneatexact12). Exact8load157/8atv10,nullabletable/h12=224/hc28/h22=128; latert>8 clearsendedptr orwritesTHREErotationhalfwordsintopositionfields. >=5trail330→128withtemporarycontextw28=256 andtemporaryv10Z+600/restored;>=18gradientbands. At>=24 AFTERallphase0work, switchphase1/frame=-1,1DC(p74)evenNULL,firstY=-8192/scales819,819,3072, w18/w1Cfromopaque w8/wC,rotation1024,0,0,publishbyte24vecwithY-4096,load159nullableh22=128.

Phase1 alwaysdrawsgrid(slot4094,x=t/2+150,y112,w256,h240,RGB128,tpage155,CLUT16064,UV0,mode1),Y+=88, TWO60poses SAMEhandle sequentially with2/3XYscale thenfullscale,26C=-64. Trail128resource5 withcontext256/zero; nullablep70endsorreceivesrotationasposition. 240(t*12...). Terminal80 AFTERdraw/pose/trail then78,publishbyte24vec0,-4096,0,return1BEFORE finalcontextclear/framecounter++. Otherupdatesfinalcontextzero/counter++. No effectidentity, extra clamps or cleanup inferred.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Eased cubic actor descent handle pulse naming acceptance (BV-03/BV-08, P2):
FF9 19b591792 adds three canonical ovl_13a33800 names. Catalog
4,778 unit/symbol names, 210 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-eased-cubic-actor-descent-handle-pulse.json and Binviz
target/ff9-names-eased-cubic-actor-descent-handle-pulse/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 892/3,564 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 VOID unconditionalc=0x02000000/(den*den), t0default; mode0((0x01000000/den)*num)>>16, mode1 256-(c*(den-num)^2>>17),mode2 c*num*num>>17. v=256-t,weights{t^3,6*v*t^2,6*v^2*t,v^3}>>12 appliedctrl0..3, outputTHREEU16zero thenpertermSIGNctrl*weight>>12 withnarrowingafterEACHaddition. Reverseendpointorder andcoefficient6 retained, notnormalizedstandardBezier3; no invaliddur/clamp/overflowfix.

7948 ownVOIDhelper unconditionalk=0x20000000/(len*len),switchlinear/quadraticfalls literaltruncatedintegerformulas, defaultweight0; callhostB4(handle,extra,w), no declaredreturn. Mainconsumes INT return into scale/weight/spin; this unresolved caller/callee ABI mismatch retained and recorded, notrenamedasreturnsinterpolation. Also7704VOID declaredINT butreturndiscarded.

7A80 descriptor184, savesctx/table70(4),zeroexactfirst16statewords,createfourhandles3/5/6/7,scales6576/5120,rotation/translation/spin0. Foursharedrecords life128/delay-1/NULL. ActorcountunsignedNO bound4;each2BC(i,-2),1F8originalvec, startX=originalX,Y+12288,Z-4096,copytoC/D,204(i,start,UNSIGNEDstartZ)THREEargs,delaystaticindex; record0DY-=256 EVENcount0. Allpads/provider extents preserved.

Exact0globalpublicationcount50,50animationcount160,192threepulsecounts24,200flash32. Firstcountpredec/publishstartforall actors. Whileanimationcountpredec, clearendedptr; delay0life>0 PREDEClife128→127..0,constructfour3lane+padcontrols(start,midpoint,originalY-1024,original), cubic(mode0,128,remaininglife)→D;1D8EVENNULL,life<16nullablehalfword20=life<<8,204D thenDY-=256. Nextlife0 update setsdelay-1,fresh1F8/204,1DCptr EVENNULL withoutnullingit. Positive delays separatelypublishstart;exactdelay8load1/4atC/D nullableh22=128;delay<8changesobjhalfword20 thenpostdec. Startsnextupdateafterdelayreaches0, nobackfill.

Everyupdate camera/publishedctxp2CTHREEhalfwords0/record0DY/DZ; rotations+spin+32/-spin-32 BEFOREpulse updates. 240twocallsifOR(w10,w14,w18);w10/w14notarmedhere. Pulsew18predec consumesVOIDhelper(mode0,24,count,0,4096)→two28Cmorphs/tintwindows>=17or<8. Scalepulsew1C andspinw20predecconsumeVOIDhelpermode1; poseusesprior scale/spin values. Flashw24predec emitsFOURTEENarg248 withscale=count*4+4096/RGB=count*3,THREErot0. Terminal264 AFTERallwork,no78/newrelease. Originalreturn type, read order,pins/hacks/signnarrowing unchanged.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Radial sprite grid two phase swirl naming acceptance (BV-03/BV-08, P2):
FF9 7c61ba8da adds three canonical ovl_12d7b000 names. Catalog
4,781 unit/symbol names, 211 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-radial-sprite-grid-two-phase-swirl.json and Binviz
target/ff9-names-radial-sprite-grid-two-phase-swirl/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 892/2,864 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 tenargVOID reserves cols*rows*52 bytes beforechecks. Spritepresentopaque3C computesTWOpages fromoriginalsignedUV, mutatesspriteUVtoenvironment+center, reserves24extraspritebytes and1Cbuilder;NULLsprite semitransparent3Eusesenvironmentpages0x120. Init17previousgeometry0/currentUV(x0,y0),dang=4096/cols unconditional division. Forpositive rowsrad+=step,upper/lowerRGBB8 perrow, processj0..cols whennonnegativecols, no bounds16. Circularpb-relativeXY via38/3C, pdUVabsoluteclamped0..319/0..239;lastjclosesfrombuffer0. QuadsecondpagewhenanyfourUVX>=256 subtract128,callbackONLYj<cols AFTERUV/pagechoice butBEFOREgeometrypublication, so callbackaltersgeometry notclampedUV. WritesGT4wordfields/partialcolors, linktaglength12 onlyifunsignedslot<4096, low24addresspreserveOTtopbyte; optionalextraspritetaglength5linkedLAST. No bounds/divzero/negative-shift/pad fixes.

7EB8 VOID5arg callbackoutSIGNED2lanes only: t=cos((b<<11)>>4)*sharedamplitude>>12;radius+=t*cos(sharedangle+(b<<10))>>12;outXY=sin/cos(c)*radius>>12. b=row, c=angle, fourthcolumnargunused. Keepliteralshifts and repeatedhostlookups, truncation/overflow; actualgridexpectsINTcallbackpointerwhilemainVOIDK&Rreference, unresolvedreturndiscard/typefrontier preserved.

7FA0 descriptor28, savecontext200actor0/16vectors,220(0,32)storedunused,sharedangle/amplitude0,phase0; cntNOTinitialized. Phase0 q=(t<<12)/40,amplitudeCOS(q>>2)/320,angle-=512;at>=40phase1/jobframe=-1 butSTILLdraw. Phase1 qsame,SINamplitude/angle+=512, exact1resource1atcapturedv0C nullableh22=220(0,32);at>=40returns1BEFOREdraw/counterincrement (angle/amplitudealreadyupdated). OtherupdateslocalSIGNEDsprite4halfwords448,256,256,256;draw(1,160,104,16,16,8,136,136,sprite,swirlcallback),cnt++includingunknownphase. No78/release/newcounterinit orclaimunusedhostvectorpurpose.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Table guided split particle paired pulse naming acceptance (BV-03/BV-08, P2):
FF9 f92e10a01 adds three canonical ovl_11888000 names. Catalog
4,784 unit/symbol names, 212 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-table-guided-split-particle-paired-pulse.json and Binviz
target/ff9-names-table-guided-split-particle-paired-pulse/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 1,692/4,528 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 seven-argument INT callback: init phase/t=0, resource14 at handleblock+20, nullable h22=64; idx, position c, scale and h38/h3A not initialized here. Every non-init op selects unchecked table[idx]. Phase0 scales XY=(4096-sin(t*256))*(scale*5/2)>>12 and Z=cos(t*256)*scale>>12; creates unused offset local vector but loaded object receives UNOFFSET table XYZ. Clear ended object then write if still nonnull. Increment t, switch to phase1 at4/t0; tail uses updated phase.

Phase1 XY=scale*5/2/Z=scale, copies both table words then narrows local X-=2048; object receives offset local XYZ. Handle h24 gates phase2/t0, h38 += rand()%50-25 and h3A += (rand()&31)-16 with U16 narrowing. Copy c into d/e, copy both table words into x30/w34. Tail immediately uses new phase2 and three handles although phase1 scales/lum are still used.

Phase2 sin((t<<12)/20>>2) controls XY scale and lum; separate sine call produces a_b>>7. D8(table,a_b/2*a_b/3*a_b) outputs three motion vectors: c/d/e XYZ subtract these with separate Y adjustments (6-t), -(t-6)*4, -(t-6)*8. Switch pt to saved x30/w34 only after third D8; x30/x32 add unsigned h38/h3A with SIGNED narrow, t++ before eZ subtraction. Terminal t>=20 returns1 before any tail poses; normal t0..18 draw, t19 movement only. No assumed fall direction or effect identity.

Common tail halves XY scales arithmetically, leaves Z. Phase<2 calls2E4(handlea,2), poses a plus d at c, tints onlya. Phase>=2 poses d/c/b at c/d/e, tints allthree. GLOW visibility1 iflum<4000 withlum>>5, otherwisevisibility0 with(lum*3)>>6; subtract128 then26C triple. Preserve every repeated lookup, invalidphase frontier and uninitialized lane.

7ED8 descriptor state52/callback68/count10, reg3/4 untouched, registers callback address with explicit INT cast. Actual declared main state48 bytes preserved. Init five creates resources19/21/22/23 then11 overwrites same hnd[3], no cleanup invented. First four154/15808/128/0, final186/15680/0/128; phase/cnt/frame/h24/scroll0, capture200/1FC, resources13/20 nullable h22=220(0,70). Mutate all10 shared static table XY halfwords by92/512 on EVERY init, preserving accumulated mutation and Z/pad.

Phase0 one184 pool-allocation attempt every update, successful allocation uses signed cnt then increments it, DC(v14,table[idx],-16) fills three position INTs and scale=SIGNED tablepad*3. Failed allocation does not increment cnt. Scroll+=32; ordinary frames0..9 produce TEN attempts, phase1/frame-1 at>=9. Phase1 scroll+=sin((t<<10)/14)*32>>12, at>=14 phase2/frame-1/h24=1 then loads17/18/16/15 withnullable32. Phase2 t<5 blue-weighted flash, exact0resource25, terminal>=21 calls78/return1 before scrolling draw/counter. Other updates240(scroll,640,384,64,128,640,256) and frame++. No bounds/failure-backfill/newrelease fixes.

8574 descriptor16/init savedctx phase/frame0. Every update2B8(16,1,vec) before any terminal test. Phase0 t4096/3, size512, cos/2 and declining h, transitions>=3 after computing values and still draws. Phase1 parity wobble, size(t4096/9>>3)+512 with identical ifarms retained;>=9 transitions and stilldraws. Phase2 k=(t4096/9)>>2, size1024-k, x=sin(k), w=cos(k)+4096, h=cos(k)*3/2+4096 with separatecoscalls;>=9 returns before drawing/counter. Common tail fourSIGNEDrot0/0/size/0, ctxw28=128 around SEVENarg128 resources27 THEN26, NULL rotation then vec, w/3,x/2 and h/2,(x<<1)/3. Clears context then frame++. Pin23 retained; old donor resource comments not trusted.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Camera sweep depth particle history trail naming acceptance (BV-03/BV-08, P2):
FF9 6f07a8810 adds three canonical ovl_10a71800 names. Catalog
4,787 unit/symbol names, 213 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-camera-sweep-depth-particle-history-trail.json and Binviz
target/ff9-names-camera-sweep-depth-particle-history-trail/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 340/4,792 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 VOID scatter: calculate trackingword area buf+n*4 even n<=0, loop only n>0. Each8byte SIGNED position record gets sin/cos(randangle&4095)*rand_n2(r>>2,r>>1)>>12 XY and rand()%1280-256 Z. Associated 32bit trackingword reset7F7F7F7F; record pad untouched. Tracking words used as prior projected XY by renderer, not inferred as RGBA.

7858 nineargVOID flags-fno-strength-reduce/-fno-cse-skip-blocks, globalreg14/pin20. Negative fade becomes4096, reserve n*20 primitive bytes before n>0 check. RGBNULL writes THREE black bytes only; otherwise DPCS4096-fade with black farcolor. Load original context matrix then save/set GTE H, f14(INTa4,&mat) builds rotation (main supplies pointer through K&R), translation0/0/H, load newmatrix. At end restore H ONLY, not old matrix. No extra matrix restoration/pad initialization.

Perparticle compute priortracking!=7F sentinel, project OLD position with RTPS BEFORE narrowing Z-=dz. dz>0/newZ<-256 wraps1024 and resets tracking; dz<0/newZ>768 wraps-512/reset. Wrap path skips projectedXY/depth/draw. Otherwise store XY/unsigned depth; sz<512 and preexisting nonsentinel allow DPCS witht=(512-sz<<12)>>9, primitiveword+12=0/+16=priortracking, code50ifmode255else52,23C length4. Always store new projectedXY to tracking on nonwrap even if not drawn. Reserved but unlinked primitives, sentinel first-frame suppression, unsigned depth filter and all unclamped arithmetic retained.

7B88 descriptor72, init phase1/cnt0, scatter384/radius1024 into globalbuf. Screen halfwords160/120,0/120,200/120;70table8, eight handles resources17/4/26/12/23/24/25/15 with original selectors/modes. v14/v1C/object not initialized at init; no cleanup/typed-call fixes. Phase1 v38Y=(t<<11)/49-2048,Z=2048-(t<<11)/49, t>=8 three separate random jitter calls including rand_n(0) at8. Rotate512/0/-t64 and scale4096. Store v14Y+29/Z-70; publish ctxp2C+24, then publishedZ-=256 and +=t*900/49, Y+=384-t*900/49. Pose/tint/draw order kept.

Phase1 draw384 particles H128,dz-70 before24 else-100, rotationpointer viaINTcallee, RGB32/128/255 and signed sine-basedfade. Exact2resource11 saves object nullableh22=92;>=25 redflash. Two11C resources16/14 pulse coordinates from(t&7)*60 and((t+5)%10)*60. At>=49 AFTERallwork release object evenpossibleuninitialized/null, v1C0/-320/0, publish then X+=6144/X-=4096,2CC(1,1),phase2/frame-1. Ordinary phase1 frames0..49 FIFTYupdates, not comment49.

Phase2 h=((4096-(t<<12)/26)*3)/2 SIGNEDdivision; positionX0,Y=v1CY-h,Z=v1CX+h (actualXlane preserved). Uniformscale8192-2cos(t4096/104),rot512/0/-t64,v14positions;publishonly t<25. Exact7resource27nullable92. Poseh0/h1, ctxf28=512 around128resource1 then0. >=13 fourteenarg248 with partiallyinitializedSIGNED color/rot/scales. Exact0resource28 initial110, everyupdate resource28 SINscale/trail and134resource7. >=9 first124resource19 withrot;>=13 second124 resource19 NULLrot, both run. At>=24 switchphase3/frame-1 afterallwork: actual25updates0..24, not comment26.

Phase3 v14Z-=64/Y+=64, resource28 trail110 withsin(t4096/24) and signed(4096-t4096/6)/2. Terminal>=6 AFTERtrail78return1 beforecnt++; ordinary0..6 sevenupdates. Default onlyincrementscnt. Original sequential context writes, separately repeatedSIN/COS calls, incompletepads, provider footprints and context-pointer signatures untouched.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Seven band funnel interpolated particle strips naming acceptance (BV-03/BV-08, P2):
FF9 ae578d582 adds three canonical ovl_10fad000 names. Catalog
4,790 unit/symbol names, 214 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-seven-band-funnel-interpolated-particle-strips.json and Binviz
target/ff9-names-seven-band-funnel-interpolated-particle-strips/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 504/6,484 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 elevenargINT computes D4(a,b,&vb), writes buffer typebytes24/0/24 leavingbyte3 untouched, loops exactly25 eightbyte entries atbuf+4. Weight4096-(i<<12)/24+rand_n(256), BC(a,b,w,&va), cos(w>>1)*rand_n(160)>>12 plusB4(p4,p5,w), F0(&va,entry,0,&vb,m,p6); entryU16+6=p10. p6+=p7+rand_n(128). All25 iterations callrandom evenlast; local6byte vectors/pads retained. Returns110(resource4,0,0,p8,4096,p9,1,buf,0), though controller declaresVOID and discardsresult; no clamping or bufferextentfix.

78FC sixteenargVOID reserves2*n*52 GT4bytes then5*n*40 FT4bytes before size check. Load ctxmatrix/projectv, buildlocalrotation via14(INTrot,&m), scale20, GTE translation resultstoredm.t,27C(&m),loadmatrix. Seven outerbands j0..6 always run even n<=0 (radius B4 calls etc); inner onlyn>0. B4(p3,p4,j4096/7 and(j+1)4096/7), accumulatingphases/ph0 centers, 144 Zspacing, fourhalfwordpoints withpaduninitialized; division4096/n onlyinsidepositive loop. Circlecenters fromph1, ph1+dph1 and accA/7,accB/7; angular b has originalincrement/decrement/repeatedlookuporder. No matrixrestore or frame-independent phase normalization.

First/last bands GT4 gradientgray-to-black and reversed, five middlebands FT4gray. Colorcodeopaque2C/3C ifmode255 else2E/3E; blackword explicitlyzero, graycodes set, tpageOR(mode&3)<<5. UV(i/j&3)*28 tiles, everyfourthcell end27 not28, byte narrowing retained. RTPT thenNCLIP acceptANYnonzero MAC0 (notpositive-only), XYfirstthree and averagedZ/4, RTPSfourth;234 length9or12 BEFORE storefourthXY andadvance ONLYacceptedprimitive pointer. Reservation remains fullcapacity, tags/pads/providerlinkbounds unchanged. Emptybarriers and assignedconstant kept.

8178 descriptor32/initphase/tick0, captures TWO hostv38 words includingpad,rotation1024/32/0 withh1E untouched. Everyupdate beforephase: radius=tick*768/72, tw=4096-sin(tick*1500/72),point=base+(sin/cos(tw)*radius)XZ/sameY. Eachphase drawsTHREE funnel meshes atsegments16/8/8 withdifferenttextures andscales plus114resource1 atBurstrot1024/0/tick90/1. Exactcase constants and chainedwrites/order retained; no claim actual rain effect identity.

Phase0 usesk=t4096/12, firstmeshXYk/Z8192, othermeshSINradius/COSZ andthirdXY*3/2;transition>=12 afterphasework stilltail. Phase1 XYhh+4096 andothermesh8192;>=12 afterdraw switchesphase2/frame-1 andloads5nullableh22=88. Phase2 hh=t128, uniform8192+hh/2 withthirdXY*3/2, k stays4096;>=32 switchesphase3/frame-1 afterdraw. Phase3 fixed10240, sinegray and114fade4096-t256; k-=t256 beforethirdmesh,terminal>=16 returns1 AFTERthreemeshes/114 butBEFOREcommonrotation/trails/tick++. Actualnormalphase0/1 frames0..12, phase2 0..32, phase3 0..16; no truncated count fromcomments.

Common tail decrements ROTATIONv18Z by16+tick*2 withU16narrow,134resource2, TWO240screenbars. Ifk!=0, three stripbuilds into separate204byte rows, 25points each; v50THREEhalfwordsfrompoint/Y-=1600, v58TWOwordcopyincludingpad, randomangles/radii/Zpositions plus sin/cos, row+=204 notsymbol indexing. Allrandom calls/order preserved, no newpadinit. Atphase0frame0 k0skipstrips;terminalphase3 skipsentiretail. No78/release added.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Indexed model paired ribbon history burst naming acceptance (BV-03/BV-08, P2):
FF9 baa24a3c4 adds three canonical ovl_10c3f800 names. Catalog
4,793 unit/symbol names, 215 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-indexed-model-paired-ribbon-history-burst.json and Binviz
target/ff9-names-indexed-model-paired-ribbon-history-burst/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 292/4,320 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 eightargVOID unsignedn, negativefade→4096. n>=1000 clears firstn-999 a.flag halfwords andreturns; otherwise backwardsFOURword16bytehistorycopy nrecords. Count contiguouslivepairedflags BEFORE writingnewhead (notcommentorder);newhead copiesbothpaWORDS,forcesa.flag1,copiesbothpbWORDS includingpad, n=count. n0stillformsunderflowpointer thoughcopyloopempty. Reservecount36byteG4prims; colorNULLsetsTHREEblackbytes, otherwiseDPCS4096-fade. Loadctxmatrix, code38opaque255else3A. Forn!=0:RTPT THREEpoints, storeMAC0WITHOUTNCLIP instruction, acceptsnonzero only;XY3,AVSZ3/4,RTPSfourth,22Clength8 BEFOREfourthXY and4colorDPCS usingfa/fb. Pointersadvanceevenunculled. No matrixrestore exists inactualbody (commentwrong), no newtag/pad/overflow/bounds fix.

7B74 fourargVOID 290(a,b,&localmatrix),loadrot/trans,LDV0,RTV0TR,storeTHREEfullINT resultsintom.t,outputTHREEU16lowhalves viaindices0/2/4. Restores savedctxmatrix withemptyasm keepingctxlive. Outputpad remains; name selectedhostmatrix notinferred matrix coordinate system or handle API semantics.

7C98 descriptor72/initphase1/count0,198(jobid,2,1) then340(resource8,list1,table),index/counter-1,length=UNSIGNED info+36; clearctxp0byte16. Phase1 preincrementbothindices, ifcounter>=length phase4/frame-1, else1FC(index,vec,1),20C(index,dir),quantizeU16angle into0or2048 viaunsignedrange. D8(dir,1F0(index)+120), publishposition;200overwritesbasevec afterselectedpos. BuildTHREEctxp2C points withY-320, thirdpointY=offsetY-320 (NO baseY). Bindscene68/190(6),scales4096,rotationYquantized, launch15/16/17,phase2/frame-1. Processingoneindex percycle1→2→3→1; not continuousanimation guessed fromcomments.

Phase2 f64 modelpose, firstsixframes104tint. Ribbonfade=-1 onexact0, ELSE frame<22 uses LOCALx/22 withoutany initialization inthispath; retainandrecorduninitialized-read frontier, notfixornameinterpolatedfade. Laterfade4096. Exact24 launches18/19/20,26yellowflash,27 resource selectorXOR(directionranges) thennullable220(index,64)/launch21/22/23,28redflash+objects1/9nullable220(index,64),29calls228index. >=30 phase3/frame-1 afterwork.

Phase3 x=t4096/24 (onlyxassignmentinbody),exact2resources7/14 with7nullabletable/h12=24/hc68/h22=64. XZscales4096-cos(x>>2), Yprevious4096 unchanged;190(41-frame),64pose,sinefade/tint. >=6 returnsphase1/frame-1 afterwork. Phase4 >=8 setsctxp0byte16=1/return1 beforetail; no78. PerupdateifFPnonzero transformTHREEconstantvectors with290(scenehandle,9) then two ribbons. NegativeFP clears17historyflags via1016;positiveFP length16 each, saveTWOsp30words tostatew2C/w30; modifiesall17historyYhalfwords withsignedarithmeticshifts differingbyedge3/4/6 AFTERdrawing. Common134resource3/count++ exceptterminal. No repairs to undefinedx orprovider layouts.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Hooked trail camera return orbit sprite rows naming acceptance (BV-03/BV-08, P2):
FF9 31f884834 adds three canonical ovl_107dd000 names. Catalog
4,796 unit/symbol names, 216 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-hooked-trail-camera-return-orbit-sprite-rows.json and Binviz
target/ff9-names-hooked-trail-camera-return-orbit-sprite-rows/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
0 exact/3 partial, 0/6,008 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 nineargVOID callback:140buffer EVERYcall, frame0 extra110(resource18,vec,0,-1,0,0,0,h,0). Ordinary110same resource withrecword40,n,7168-(n<<12)/total,lastfourframes fade,flag1. Zero-totaldivide/unclampednegativeframe retained. Onlydirbyte31==1:ifglobalhandle!=0 1D8(handle,vec) MOVES not releases; copyTWOfullvecWORDS to sharedtracking. Before29 publishTHREEhalfwordsincoming directly; n29..60 BC(incoming,savedbaseline,4096-sin((n-29)<<5),local8bytes),after60 TWOwordcopybaseline, publishctxp2C+24. No bufferrelease/flag-save/init added.

799C descriptor56/init savedctx/job,globalfade0, THREEstatehalfwords0 butpaduntouched, copyTWOwordsbaseline, widenTHREEpositionINTs. Resource17nullableh22=24 thentable/h12=16/hc388 andnestedp18fn=callback; repeatedouterguards do notchecknestedpointer. Resources21/5 loaded atuninitializedv24, globalmovinghandle assigned21,nullable24/16. Table70(3),handlesresource33 via58/23via18. Sharedtrackingnotinitializedhere.

Phase0 secondaryweight4096,q=t4096/22,clearended3objs/writeh20 globalfade/4096/4096, spritescale(4096-q)*3/2+4096, firstfourcyanflash,>=22 phase1/frame-1 stilltail. Phase1globalfade=q=t4096/20,secondary4096-q,objecth20;>=20 switchesphase2/frame-1 releaseso30/o34 EVENNULL,clearsglobalhandle butleavesstateptrs anddrawsfullfade tail. Phase2fullglobalfade,clearendedhookedobject,exact19resource19nullable128/blueflash,terminal>=22 releaseshookedobject/return BEFOREtail. Threephase countsnormal23/21/23 updates; no78.

Secondaryweighttail THREESprites31/6/4 atsharedtrackedposition usingrotation0/0/-cnt20/0. Globalfadetail twohandlesposed/tinted/268255,134resource32, f28=128aroundONE128resource22then0; FIVE128spritecalls total resource22/31/35/35/35, notoldcommentseven. Lastthree size t*4,t*12>>1,t*20>>1 withseparateorientationvectors andSIN((cnt-22)<<10/42). Common134resource34 insidefade then134resource16/cnt++. Partialvectorpad andglobalreadorderretained.

8480 descriptor44/initphase/cnt0/base70,THREEzeroXYZ(no pad),resource15nullableh22=134. Phases0/1 eachframes0..7 drawtwo/threesprites and138resource11 before>=7transition. Phase0transitioncreatehandle10/blueflash; phase1yellowflash,phase2/frame-1,releaseglobalobject EVENuninitialized/null. CaptureTWOhostp0Cwords at56/60 into v0C withY-=512;v14word0copy thenY-=3072 beforeword1copy, widenTHREEv0Ccoords toINTs. Loadresources20/28/25/29/26/39 atcorrectv0C/v14 withactualnullable sizes134/198/70. Newhandle storednotposedhere.

Phase2 firstsixcyanflash,angle=t32,SINweight,radiusCOS*1550>>12+256. TWOoppositeXrings each12sprite calls resource12 viaF0 then114 (TOTAL24 perupdate), basecnt41+i128 advancing341,flags/frame&15. ThenTENtablerows indexedSIGNEDlength;positive-only each j0..length-1 DCposition withrotationX1024-i2048/9,Y=(off<<4)+(j4096/len-cnt32),radius*2/3;114resource37 sizeweight+2048. off+=phaseframeperrow, allpadsunknownproviderreadfrontiers. >=32switchphase3 AFTERallemissions,phase3>=15 calls78/return BEFOREcnt++. Actualcounts8/8/33/16; no fixedtablecountinferred beyond tenrows.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Four phase model descent paired ribbon naming acceptance (BV-03/BV-08, P2):
FF9 b31a00d69 adds three canonical ovl_10584000 names. Catalog
4,799 unit/symbol names, 217 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-four-phase-model-descent-paired-ribbon.json and Binviz
target/ff9-names-four-phase-model-descent-paired-ribbon/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 292/4,444 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 unsignedn>=1000 clears n-999 a.flags thenreturn. Otherwise backwards16bytehistorycopy, countcontiguouspairedflags BEFOREnewhead copies, forcefirsta.flag1, preservepbpad. n0 stillformsunderflowpointer; reserve n36byteG4s. Negativefade4096, NULLcolorwritesTHREEbytes only;DPCScolor thenloadctxGTE matrix. Code38if255else3A; RTPT thenMAC0read WITHOUTNCLIP, gateANYnonzero, AVSZ3/4 andRTPSfourth,22Clength8 BEFOREfourthXY/colorwrites. FourDPCSfa/fb gradientwrites; advance history/output evenunculled. No matrixrestore atfunctionend despitecomment, no allocator/pad/bounds repairs.

7B74 290(a,b,&matrix),GTErot/trans/LDV0/RTV0TR,storethreeINTs thenoutputTHREEU16lowhalves,restorectxmatrix,emptyasmretainsctx. Outputpad untouched; no original coordinate-system identity inferred.

7C98 descriptor64/initphase/cnt0. 1FC0position,20C0rotation,quantize maskedU16yaw into0/2048 viaunsignedrange. D8(rotation,1F0(0)+120),addhalfwordoffsets tooriginalposition for target, THEN200overwritesposition. PublishTHREEctxpoints targetY-320, target+D8(-210)Y-320, overwrittenbaseXZ+D8(-1F0(0)) butthirdY=offsetY-320 WITHOUTbaseY. TWOwordtargetcopyincludesuninitpad. 340(resource6,listresource2,1,table),68scenehandle,scale4096,rot0/yaw/0,70table1,338resource28 narrowedSIGNEDhalfword. Provider/pad frontiers unchanged.

Phase0 progress=t4096/24;exact0resources9/27 at target nullable64,27table/h12=24/hc60. t>0 targetINTposition withY+=1300*(4096-cos(progress>>2))>>12,320offset BEFORE31Cpose/tint. >=24 transitionsphase1/frame-1 AFTERwork (25ordinaryupdates0..24). Phase1 190(scene,0)/64pose;>=8 switchesphase2/frame-1 andANOTHER190(scene,0), notassertedrelease. Nineordinaryupdates0..8.

Phase2 assign s0=t<<12 BEFORE64pose; s7=-1 exact0, s0/24 for1..23,4096after. Exact31 resourcechoiceXOR yaw versusctxpoint10X range,nullable220(0,64);32yellowflash,33redflash/resources4/5 nullable220;38resources12/16,12table/h12=24/hc68/h22=64. Everyphase2update134resource11 withphaseframe. >=41 switchesphase3/frame-1 AFTERwork (42updates0..41, notcomment40). Phase3 angle=t128,XZscale4096-cos,Yunchanged4096;190(41-t)/64pose,sinefade/tint. >=8 calls78/returns1 AFTERphasework BEFOREribbontail/cnt++; no extra release.

Common tail onlys7!=0 getsTHREEselectedmatrixpoints. Negative clears17flags fromeachhistory via1016,positive buildsTWO16segment ribbons thenstoresTWOwordsfirstpoint toq andnarrowsall17Yedges afterdrawing withsignedshifts6/4 and4/3. Phase0/1 s7zero → no ribboncalls. Exact terminalphase3t8 skipsribbontail despitepositiveSIN. Unlikeindexed sibling no uninitializedfadeinput here; formula/order retained.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Sixteen ribbons eight spinning handles models naming acceptance (BV-03/BV-08, P2):
FF9 e17fcd780 adds three canonical ovl_13af9000 names. Catalog
4,802 unit/symbol names, 218 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-sixteen-ribbons-eight-spinning-handles-models.json and Binviz
target/ff9-names-sixteen-ribbons-eight-spinning-handles-models/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 456/5,540 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 sixargVOID B8(c,d,a,b) result r then100(&ctxINTtable[f],e,r,r,r). No tablebounds/clamping; name describes interpolation and replicated grayscale values, not specific screenfade provider identity. 7794 fiveargVOID unconditional k=0x20000000/(len*len),kind0 4096-((0x10000000/len)*t>>16),kind1 k*(len-t)^2>>17,kind2 4096-k*t*t>>17,default0; callB4(handle,extra,w),NOreturnstatement. MaindeclaresINTandCONSUMESreturn into scales, rotations andpositions; serious ABI frontier remains, notrenamedasreturnsinterpolation.

78CC descriptor288, initclear7objectslots separately THENfirst16stateINTwords descending15..0,70table32. SignedsceneX/U16Z anchor withY-512. Init16 global324byte ribbonrecords:life0,key/radiusfromtable,angleRAND&4095,stepRAND&255+64,step2=64,110resource17initialbuffer. Init8handlesresource13, per-indexrandomuniformscale range1024..1536 growing512eachindex,Yanchor+rand_n2(-512,512),XZanchorfixed,anglei1024+RAND1023,wc0,h8-1024,spinRAND511+256. No actualorbitingtranslation update; spinninghandlesnamechosen. Setupresource1handle,positionXYZ/order,scale12288,threehandlepairs9/9,8/8,19/20 androt/scales1024. Three1F8(16) vectors,Yoffsets-1024/-2048,allpadsuninitialized.

Exactframeevents0 ribboncount74/actorcounter32;34capture80(16,46,pos64)/resource14nullable512;42res15;82res7;98res5table/h12=36/hc204/512 plusres4;114w0=128;126res3;146releaseobj6 thenres18(untracked)nullable512/w4=16;154flash4;157stopw0/releaseobj2/3/4/6 AGAINwithoutnulling;158flash8/three32pulsecounts/96spincount/res11nullable512;190actorreturncounter32. Sevenseparateendedchecksafterevents preserved, obj5neverloadedhere; no missed-frame backfill/newrelease.

Counterorder w4, w0, w14,w20,w18,w10,w1C,w8,wC,w24,w28 exactly. PREdecrementpositivecounts includingarmed-eventframe; negativecounts remaintruthy. Scale/rotation/positionvalues CONSUMEVOIDhelperreturns. w0 D4(ctxp2C+84,anchor,rot)/60pose/tintfirst7 and240. Threepairedpulse groups consumedweight,pose/tintlast16/240; w18 rotation updates AFTERposes so nextupdateorientation. Eighthandle wc halfword += helper; XYscales+=helper/Z2048,pose/tint. FixedXZ/Y notorbitalmotion.

Ribbons activew1C predec74→73..0,setctxw28=128. Whilecount>=9 searchfirstfreeinEACH8recordhalf (maxTWO launches/update),2C4(16,key,record),life8. EVERYlive16recordpredec,80anchortranslation,load32byteGTE matrix;33points k0..32 withangle+k64 (2048angularspan, notfullcircle). LocalSIGNEDXYZ cosineX/sineZ/Y0,RTV0TR/fullINTstore,readFLAGignored, outputU16lowX/lowY,constant192atpoint+4,lowZat+6. Header32/0/32 byte3untouched. Lifetimefade life<4→life1024,else(8-life)1024; angleadvance usesVOIDhelperlate. Submit110resource17,clearctxw28. No restoredmatrix/FLAGuse/padfix.

w8/wC flashhelpers afterpredec; actorw24 predec usesINT-declaredVOIDhelperkind1 Y fromv108towardsv118, publish204(16,partialSIGNEDvec). w28 predec nonzero helperkind2 Yv118→v108,zero fresh1F8(16,vec) then204 (notforcedcachedoriginal). Terminal t>=254 tested LASTafterallwork, no78/newfinish/release. Preserve word/halfword conversions and original ABI/provider frontiers.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Sampled tiles wobble particle actor vectors naming acceptance (BV-03/BV-08, P2):
FF9 7fbf93b97 adds three canonical ovl_12cea000 names. Catalog
4,805 unit/symbol names, 219 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-sampled-tiles-wobble-particle-actor-vectors.json and Binviz
target/ff9-names-sampled-tiles-wobble-particle-actor-vectors/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
0 exact/3 partial, 0/5,792 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 fiveargINT initphase0,random%1000-400amplitude,THREErotationhalfwords0/speeds0;positionhc/he/h10,handle,otherfieldsnotinitializedhere. Everyupdate beforephase snapshotsXYZposition intoINTvec,scale8192 each withWuninit, addsorientationh14/h16speeds. Phase0 Ywobble cos(frame32)*amp plusalternatingframe;at>=32 phase1,halfwordBEFOREstate=-1,positionhc-=amplitude, randomangularspeeds0..7,velocitieshc/4+RAND128,h10/4+RAND128, verticalamp=RAND256-700+B0((hc/7)^2+(h10/7)^2,20)/2. FrameargNOTreset bycallback;transitiondrawusesOLDsnapshotandphase0lum.

Phase1 updatesSIGNEDhalfwordXZ usingvelocity>>2, velocities*=7>>3, Y+=amp/5,amp+=12;fade4096-frame4096/40. f60uses OLDposition snapshot takenbeforeupdates, notfreshpositions. Terminal>=40 aftermovement BEFOREpose/tint;rotationalreadyadvanced. Ifs<4096,visibility1onlys<2048,tint(s>>5)-128;identicalarmsbyframe kept. No invalidframe/clamps/padinit added.

7B30 descriptor76/particle40/count10/callbackINTaddress,reg3/4untouched;initkind3/cnt0,70table12,THREEXYZ0,handles11/27, publishctxpoint3Y-256. Actualinitialupdatekind3 runs2F4(448,256,256,256,buf,X,Z,3072),kind0/frame-1,stillcnt++. Kind0frame1 fills96SIGNEDhalfwordarray-1, f4THREE4x4rects at512/384,636/384,576/492 withdestoffset0/32/96HALFWORDS. ThirddestinationstartsBEYONDdeclared96length, preserveunknownextent; zero-selectionscanreadsONLYfirst96.

Kind0 publishedYwobble-256-(frame*2)*parity, sixposesSAMEhandle11 withprogress/cosines repeatedlookups,tint,rotation+=600eachpose. >=-4handle27fixedposition0/1454/0 withcosYscale,2681/tint/240;>=9sprites30/31. >=16 afterdrawkind1/frame-1whiteflash,scan96anyzero picksoneofTWO10resourcearrays forall10handles338mode0. No per-tile resource selection invented.

Kind1 publishedYwobble16. Exact0 TWOwordpositioncopy(c14containspositionZ+pad) thenhalfword c10Y-=384;resource26nullable256. Twenty-five184allocationATTEMPTS;successfulrecord copieshandlefromstatebyte28+i4 (only10declaredhandles, readscontinuepaststate) andTHREEpositionhalfwordswithY+1454. Failuresstilladvanceindex. Clearendedobject,first4whiteflash,first24sixposeshandle11,first16resource31/context256. >=32 switcheskind2/frame-1 releasesobj EVENNULL andleavespointer. Otherworkstilldrawtail.

Kind2 gray137,exact1resource29nullable256,publishYdiminishingwobble. Exact0 f100FOURargs(ctx+8,2,128,128),frames1..8 FOURargs(ctx+8,1,w,w), preservemissingchannel K&R ABI, notinsertblue. >=40kind3/return1 BEFOREcommon248/cnt++. Earlierkind2 fourteenarg248 withgray137,scales4128/4128/4096 andpartialrotationword0/0/-8;overlaid64bytescratch/macros preserved.

8880 descriptor268/init savesunsignedhostcount UNBOUNDEDdespitefour8entryarrays,phase0. Peractor1F8A/20CB,copyTWOWORDBtoD includingpad, three independentC halfwordsteps8192/(40*(randbit+1)), firstbitSIGNEDrand%2 (negativepossible), sharedamplitude overwrittenEVERYactor/lastwins, count0leavesuninitw108. Phase0 everyframeCOPYcachedA thenY-=frame8+RAND127,204;>=32phase1/frame-1 AFTERpublish. Phase1 copycachedA/Y+=sharedamp*cos(t4096/40>>1)>>12 then204;DTHREEhalfwords+=C then2A8. >=40 AFTERwrites restore2A8B, FRESH1F8/204 (notrestorecachedA),phase2/frame-1. Phase2 peractor128resource15 atcachedA/rotation1024/0/0/1,sine(frame4096/7>>2),terminal>=7 AFTERdraw; no78/newcountclamp/provideridentity guessed.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Pixel colored grid spiral handles paired pulse naming acceptance (BV-03/BV-08, P2):
FF9 972cf7148 adds three canonical ovl_1176a000 names. Catalog
4,808 unit/symbol names, 220 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-pixel-colored-grid-spiral-handles-paired-pulse.json and Binviz
target/ff9-names-pixel-colored-grid-spiral-handles-paired-pulse/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 812/6,660 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 fifteenargVOID. Initialize TWO fallback words zero; skip0 increments W/H and reserves W*H*36 bytes BEFORE positive-dimension checks. fFC(0,fl); load context matrix and center, rotation INPUT out (fallback zero), f14(out,mat), optional uniform f20 scale, GTE center translation. One center depth is reused for EVERY cell; pad nonzero invokes f27C. Codes38/3A; ZEROZ before XY-only loads. Each row resets pk to SAME pool, overwriting the first row storage rather than advancing pool; preserve explicit arena extent/order frontier, do not claim corrected grid allocation.

Grid edge colors zero; interior RGB from 5-5-5 pixels times c11 after TWO left shifts by2, signed >>12 and byte narrowing. Interior fourth color bytes remain partially initialized. RTPT then MAC0 read WITHOUT NCLIP; nonzero gate, RTPS fourth, f234 length8 with center depth; all cells advance36 even unlinked. Final f234 length1 with fFC result. No context GTE matrix restore. Unconditional tail a6/a5 and remainder unguarded, rectangle page coordinates and W-1/H-1, f4(rect,tex). Skip mode does not increment dimensions. Actual f4 provider read/write direction unresolved here; original upload comment not accepted as proof. No provider identity guessed.

7CA0 descriptor56. Init savesctx, f200 firstvector and1FC second,70table6, FOUR338handles resources28/27/29/30 and TWO58objects9/11, resource14 nullable table/h12=18/hc36/h22=32, resource12 nullable256; phase1/framecounter0,trackedobjectsNULL. w28 created object unused later; no cleanup added. Phase1 n0..8 ordinary nineupdates, exact4 loads18/19/20, ring13 and sine/grid weights; transition at>=8 AFTERwork so frame8 can draw grid.

Phase2 ordinary n0..46 (47updates), flashfirst4/redrandomfirst32. Pillar/model gated n<29, spiral n<36. Ring13 alwaysdrawn with v/2*2+8192 literal truncation; repeated cosine scale writes overwritten but calls retained. Events2/14/3 load16/21/22/23; trackedobjects clear ended h30 or write constant1024/0/0 into POSITION fields5C/5E/60, no rotation correction. Spiral18 iterations reuse FOUR handles i&3, not18 independently created particles. Negate-before-shift ang=-n<<7 and xdc=-n*411 remain. Random called EVERYiteration even after3sparks because left-first condition; up to3 resource15 draws with positive random XYZ offsets and partial pad. Phase2>=46 transitions AFTERwork.

Phase3 n>=16 calls78/returns1 BEFOREcommon counter. Common grid only la!=0, rotation1024/0/n256/pad1, fifteenarg caller with tpage279/U0/V48/two columns/framecnt&7/step256/inputW/H16,scale lb/color la/fl1. Builder actually increments to17x17 reservation10404; tail rectangle16x16. Phase2 from29 has la0 so no grid. Counter increments except phase3 terminal. Preserve helper VOID declaration and context-global type frontier; definitions/declarations/direct call propagated.

8DDC descriptor16 phase/counter0. Every update reads2B8(16,1,vec8) BEFOREphase/terminal. Phase0 n4096/3 cosinehalf/diminishing scale, >=3 transitions AFTERwork; phase1 n4096/9 parity256, >=9 transitions AFTERwork. Phase2 n4096/9, sinefade, cosine+4096 firstsize, c=(cosine<<1)+4096 secondsize; >=9 returns1 AFTERvector/trig BEFOREdraw/counter. Draw resources24/25 aroundctxw28=128; first NULLrot,d/3,a/2; second0/0/b/0 rotation, c DIRECTLY (not c/2), a*2/3. Unknownphase still draws both with zeros. No finish78 or release added.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Twisted textured tube spiral sprite particles naming acceptance (BV-03/BV-08, P2):
FF9 93c6b1a20 adds three canonical ovl_1297e000 names. Catalog
4,811 unit/symbol names, 221 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-twisted-textured-tube-spiral-sprite-particles.json and Binviz
target/ff9-names-twisted-textured-tube-spiral-sprite-particles/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
1 exact/2 partial, 364/9,132 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7718 twelveargVOID reserves0x3200 bytes unconditionally (320 FT4s), transforms center with context matrix, f14/f20/27C then own GTE matrix. Twenty axial bands z0/z1 spaced2150, sixteen angular segments step256 with extra per-band twist. RGB col*k>>12 narrowsbyte,code2Copaque255else2E,tpage OR semi lowbits evenopaque. Texturefour32wideblocks/16eight-highrows,lastwidth31/height7. nc initially UNINITIALIZED used in identical ternary arms and identical store guards; preserve matching branch frontier. Actual NCLIP>=0 gate; average depth computed then discarded/replaced4095. f234 length9 BEFORE fourthXY store; q advances ONLY accepted cells despite fullreservation. No GTE matrix restore or pad writes added.

7BC0 fiveargINT init clears h2/h18/w24/w20, four RANDOMcalls settwoangles and two parameters with SIGNED remainder thenU16narrow. Otherfields host supplied. Only noninit kind4 decrements U16 X at hc by128+unsignedha, then draws resource2 via114 using pointer hc and signed h4*3>>1; frame>=17 returns1 AFTERmovement/draw. Otherkinds neverterminate here. No assumed lifetime fade/velocity reuse of otherwiseunused initialization fields.

7D2C descriptor180/48particles/40record/callback. Initphase2/counter0/table6/four58handles10/7/8/9, anchorTWOwords includespad, rotateTHREEhalfwords0, capturedthree-lane vectors offsetZ9557/7168 andINTtranslation. sc/rotpads/e/pb0 untoucheduntilphaseevents. Five phases2..6, ordinary framecounts19/25/11/52/17; transitions afterphasework, terminalphase6 returns AFTERcamera/sprite/flash BEFOREcommoncounter.

Phase2 publishanchor camera fields18/1A/1C, rotatinghandle10 two poses withZ1024offset restored, shrinking8192scale/cosinetints,resource3 context256. >=18 initializeFOUR eXYZ rotations(no pad), load19nullable128. Phase3 decreasinghandle10 n12..19, growingfourposesSAMEhandle7 fromn11 withperpose9*2>>4 scaling, resources1n15..20/4n>=12/12n>=17 and13event12,blueflashthenwhiten21; >=24 copiesanchorword0 thenY+=1024 BEFOREword1, publishescamera, loads13 trackedpb0. Preserve repeatedtrig/order and unsignedrange gates.

Phase4 anchorcopy plus diminishingY=(4096-n4096/10)>>2, pairedsprites3/1context256,handle10first16althoughphaseends10. Flashfirst6 thenchannel2n>=7; >=10 releasespb0 WITHOUTNULLguard, preparesrotY1024 andscales,loads19/15nullable128,fullchannel2white. Phase5 allupdatesfourlarge sprites,resource5pairfirst32. Fulltwistedtube radius7168/color6144/flags23/clut16000/step cosine88+64, table238 colors into partiallyinitializedvector; rotateZ24 andtexture240.

Phase5 128 resource14 sprite draws fromFF0 everyupdate with per-loop angles, two64entry historyrecords halfwords decremented64 beforetwo110 submissions; initial110calls only n0. Three184allocation ATTEMPTS eachupdate n>=8, successfulkind4 TWOword position copies at hc andh10 (secondwritesfollowingpad), randomX±256/circularYZ/randomSIGNEDha%70/size4096..8191. Tablebuffers/pads/no-clamps frontier preserved. Event43load13; n>=44overlayquad grows/clampsgray255; >=51 releasesparticlepool180,loads15,phase6/frame-1 AFTERallwork. Common248 overlayonlyia (phases2/3/4), literalfourteenargcall,sharedunion/pads unchanged. Phase6 cameraanchor/resource3/sineflash n>=16 calls78 return1. No new releases/provider/resource identity guessed.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Scripted stage props orbiting handle groups ribbons naming acceptance (BV-03/BV-08, P2):
FF9 2e5ee92ec adds three canonical ovl_13a86800 names. Catalog
4,814 unit/symbol names, 222 scoped alias headers. Three full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-scripted-stage-props-orbiting-handle-groups-ribbons.json and Binviz
target/ff9-names-scripted-stage-props-orbiting-handle-groups-ribbons/. Three full native object pairs equal;
exact affected/scored namespace three. Pinned strict-relocation scores unchanged:
2 exact/1 partial, 456/11,028 code bytes, zero failures. Three
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and seven isolated committed paths audit.

7704 sixargVOID hostB8(c,d,a,b) then100(&INTtable[f],e,r,r,r), no bounds/clamping. 7794 fiveargVOID unconditional k=0x20000000/(len*len), kind0 linear-down,kind1 squared remainder,kind2 one-minus-squared progress,other0; B4(handle,extra,w), no returnstatement. Main declaresINT and CONSUMES return fourtimes in grayscale calls; mismatch preserved explicitly, name does not claim returned interpolant.

78CC descriptor296. Init writescamera0/-24576/-16384 then savesctx/70table32. Clears ONLY first64words256bytes, leaving vectors at256andabove partly untouched. Actor count UNBOUNDED D_801ea670 table capture, forceY0. f80(16,5/9) capturespos110/118, v120 X fromfirst/Zfirst-512/Ymean (notfullmean). Sixteen firstpool110initcalls; firstEIGHTribbons write SIXTEEN pos entries despite pos declaredNINE, linksj+1 andrandomlife0..15/hCE0. Preserve record extent/link-frontier; no reinterpretation as safe9entry ring.

Init creates16 stage-prop descriptors and FOUR orbiting groups of THREEhandles each, fixedinitialangles2048/3072/4096/5120,radius3072/scales4096. Exacteventframes0/45/79/95/143/159/207/255/287/319/335/399/415/423/447. Event0 loadsresource11 intoobj[1],45 loads13/14untracked plus10obj[0];95 resets firstpool thenenablesstage,143/159 armsharedfadecounts plusoppositeYresources3/4;207armsfourprops/bursts;255loads5;287loads16/17obj[2]/[3];335 movesBOTH objects via1D8WITHOUTNULLguard,setsSIGNEDY-1277 in halfwordvector andTHREEINTD_801ea600, disablesseveralcounters,armslaterburst;399resetsecondpool;415releasesbothunguarded;423armsactorbursts24. EXACT447 returns1 BEFOREeveryper-updateoperation, no>=guard/78/missedframebackfill added.

Per-update D4 derivesorientation beforeprops. FourgroupXZ trig(radius) motion andrandomXYjitter; h8/h4 sharedfadecountersw44/w3C decrement INSIDEfour-group loop (up toFOUR perupdate), notonceperframe. Flags can switch midwaybetween groups. Two verticalpropsuse rz halfword sine*w4 thenw4-=4,notry. Stagerotations andtexture240literalcalls retained. Otherprop countdowns PREdecrement, pose then gated tints; fourprops rz +=U16w2C ALWAYSafterpose, orientationappliesnextupdate. w50initial104cannotreach>=113fade-inbranch in ordinaryrun. Unarmed w10stillretained, w2C/w38constantparametersnotnormalcountdowns.

Fourflashcountdowns callINT-declaredVOIDeasing before100; w20 PREdecrement64→63, color2*w20 bytes andscale3*w20+4096,14arg248 withonlyTHREEcolorbytes/THREErotationhalfwords. w34 PREdecrement; scanall16freefirstpool records andspawnwhenremaining>=13: f14matrix,translationglobalhalfwords, TWO distinct GTEinputs/outputs, FLAGreadunused, (point0-point1)/9,life8..11,ninepoints randomXYZvelocities. Newlycreatedrecord stepssameupdate; PREdecrementlife thenonlylife<8 moves/narrows9points,weightslife<<9,ctx128aroundallsubmissions. No GTE restore.

w4C initial95 PREdecrement. UpperEIGHTribbons life8..11 thenstep9points withactor-selected matrices/translations; transformOLDposition beforeintegratingvelocity, outputpointweight64,resource12odd/6even. LowerEIGHT use SIGNED halfword link index andnine chainedreads from16 initializedpositions despitedeclared9; life takesfirstnextlink. Context128onlyaroundlowerpooldraws. w58 PREdecrement24actor bursts: UNBOUNDEDhostactorcount indexes16secondpoolrecords, zero life respawns8/ninedivergingpoints, newrecords drawnextupdate; liveonesfadeonlyw58<8,ctx64. Verticalvelocity EXACT(-rand())&7 positive0..7 (notnegativefall). Preserve allcount/pad/index frontiers. FinalstagepropYrotation+=3, return0; no releasesotherthanexact415 or inventedproper-effectidentity.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Four phase pixel grid spiral handle finale naming acceptance (BV-03/BV-08, P2):
FF9 f31529e1e adds two canonical ovl_102a6800 names. Catalog
4,816 unit/symbol names, 223 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-four-phase-pixel-grid-spiral-handle-finale.json and Binviz
target/ff9-names-four-phase-pixel-grid-spiral-handle-finale/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
0 exact/2 partial, 0/6,128 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 fifteenargVOID reserves (W+1)*(H+1)*36 up front. out serves rotation INPUT, NULLfallbackTWOwords0;f14/f20/27C,centerdepthsameforallcells. Pixel5-5-5RGB scaledwithTWOleftshiftsby2,partiallyinitializedcolorcodebytes/edgeblack. Eachrow resets pk SAMEpool despitefullreservation,notcorrectrowstride. GTEZEROZ/XYloads,RTPT thenMAC0 WITHOUTNCLIP/nonzerogate/fourthRTPS,234length8peracceptedcell andlength1end. No GTErestore. Unconditionaldivisiona6/a5+remainder andf4(rect,tex);skip keepsincomingdimssoW-1/H-1samplewindow. f4 direction/providerunknown, do not adopt historical uploadcomment.

7CA0 descriptor56. CapturesU16hostXYZ atpc38, Ynarrowminus512; TWOwordcopyorigin intoh8/ha/hc/pad thenFORCESha0 (secondposition Y=0, notcopiedloweredY). Savesctx/table6; FOUR338resources12/9/11/2, TWO58resources10/14;17atsecondpositionnullabletable/h12=18/hc36/h22=32. phase0/counter0/objectsNULL, originpad uninitialized copiedintosecondpad. Createdw28 notusedlater, no cleanup.

Phase0 ordinary0..24 (25updates) draw16context512,gridfadeprogress/3,gridsize6144thencommon2/3;>=24transition1 AFTERdraw/load5nullable256. Phase1 ordinary0..8 (9updates), RED100screen-table notsound,exact4loads20/21/22,twosinecalls,grid size(2*progress)/3+1365;transition>=8 AFTERwork. Literalformula differsfromthreephase sibling, no copied resource/formula identities.

Phase2 ordinary0..46 (47updates), first4screenflash/first32randomred. First29pillarresource19atY-1024 withcos*2+6144,sizeprogress+4096;ring16allupdatesliteral(sin/2)*3+8192. Modelw24 priorcosscale writes overwrittenbycos/2+512 and((sin*3/2+4096)*3)/2, repeatedtrig retained. Events2resource4/14resource23/3trackedresources24and25nullable256. Trackedh30 clearsendedelsePOSITION5C/5E/60 getsconstant1024/0/0, notrotation repair.

First36 phase2 updates spiral18poses reuseFOURhandles byG14Obj pointer view of INTstatehandlearray. Negate-firstshift -t<<7 and -t411, base1250/oscillation185,Ybase3584,heightcos2950first6then2600-angle; signedroundtowardszero t1250/16 correction retained. Threepositiveoffsetrandomsparkresource18drawsmaximum, RANDOMcalledall18iterations evenwhenlimitreached. Pad/partialscaleandU16narrowing unchanged. >=46transition3 AFTERallwork.

CommongridonlyspA8nonzero, rotation1024/0/t256/pad1, fifteenargcall page279/two columns/counter&7/step256/input16x16/scale(spAC*2)/3/colorfade/fl1. Actual17x17 reservation10404/tail16x16, allphase0nonzero gridupdates included. Phase2>=29fade0nogrid;phase3nodraw. >=16phase3 calls78/returns1 BEFOREcounter, otherwisecounter++. Source matching registerpins16/18, wrappers, signed shifts/overflows and parameter order retained.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Orbiting trail staged impact fan ring spread naming acceptance (BV-03/BV-08, P2):
FF9 16f59842c adds two canonical ovl_12b16000 names. Catalog
4,818 unit/symbol names, 224 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-orbiting-trail-staged-impact-fan-ring-spread.json and Binviz
target/ff9-names-orbiting-trail-staged-impact-fan-ring-spread/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
0 exact/2 partial, 0/7,640 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

C704 event1 initializes ONLYh4=24 andreturns0. Kind0 resource5 bufferinit atframe0 then signed-halfword radial/angular updates withuppercaps2000/2450. Firstform D4/E4 direction, cosine/sine jitter uses UPDATEDh10 insecondargument,DCmutatesposition,110width frame12clamped245 then*16+320. stw0>0 switchesformAFTERdraw/marks precedinghalfword-1; no explicitframe reset. Secondform frame4clamped128 angularjitter,context255around110 width h22-frame570; particleindexh6==1 publishesposition intohostpoint1 then subtracts s16(frame<<5) Y/Z. Kind0 neverterminates here.

Kind1 BOTH Y/Z +=768 everyupdate,resource9 trail width6144 or20480 ifh10==2; atframe>=7 AFTERdraw transitionskind2 andprecedingmarker-1, thenselector2whiteflash/kind3, nonzero randomgrayflash, zero loads10nullable128; all setY0. Kind2 computes one(frame4096)/24,resource9first8,128resource4nonzero else114resource8/context320; nonzero modelw88 cosinegrowth/sineTHREEtintcalls,terminal24 AFTERdraw. Kind3 resource9first8/flashfirst6,resource4/context1024,THREE60poses w90/w94/w8C,terminal32 AFTERdraw settingkind2. Partialrotpad/scalefourthwords retained.

D218 descriptor152/57particles/36record/callback. Initphase/counter/emissioncount0,1CC/table5, FIVE338handles resources133/135/136/155/145, camera68/190via nestedp4unguarded. Position/scales/rotations partly initialized, UNINITIALIZEDlocalfr testedin identicalarms (matching frontier). Capturespoint3frominitial0/-1994/0. h24/h26/h28 not initializedhere butconsumedatfirsttransition; global3halfwordvector readasTWOwords includingadjacenthalfword. Do not repair extent/initialization.

Phase0 ordinary0..65: camera64thenhandle133 pose/visibility/tint, exact31load2h22=-240/exact27load3h22=128,138resource13frames32..47,fade31..38,overlayfrom49. Three184ATTEMPTS/update from33 untiln57; countincrements BEFOREallocation sofailuresconsumeindex, buffersstartindex1*204 notzero. Kind0 allsuccessful withselector/indexmotion fields, width10240; elsebranch incrementscenterY12 evenbeforeemissionwindow/aftercap. >=65transition1 AFTERdraw,point3copiesuninitializedh24..28, resetcount/rot/scales,shiftcenterYZ8192.

Phase1 ordinary0..16 fasterrotatinghandle133; >=16transition2/camera3globalcopy THEN h24..28 global offsetYZ-20480/copyfirstorigin, D4intoTWOINTwords, point2offset512, goto sharedset10 resetsn. Phase2 exact0 releasepool then32allocationATTEMPTS; ACTUALkind0/h14=1 (oldcommentkind1wrong), index-one buffers, sine/cosine*5/2 addedWITHOUT>>12 narrowinghalfwords, h10=32bitSWnarrows16. Per-update Y/Z accumulate TWO separatecosine600 calls, publishpoint2,resource11fromn!=0. >=32transition3AFTERwork,positioncopiesbeforeY-128/pointX+2048/Y-32/Zglobal,resetn.

Phase3 ordinary0..72, exact0releasepool thenone184ATTEMPT/update untiln32, first30randomspreadselectorparity, lastTWOselector2largerspread. Countbeforeallocation, bufferindexn*204. Eachupdate basecopythenX+2048-progress/Y-32/Zglobalunsignedalias; shakeframes9..48 includesrand_n(0) at45..48 (not skipped), resource11first16,texture240always,fadefrom56. >=72 resetsstate/point3,1CC/180/78 thenreturns1 BEFOREoverlay/resource1/countertail. Commonoverlay onlyphase0positiveweight, shared40bytescratch partialcolor/rotpads,134resource1counter incrementsallnonterminalupdates. No missedframebackfill/providercleanup added.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Four phase pillar pixel grid spiral handles naming acceptance (BV-03/BV-08, P2):
FF9 f63144203 adds two canonical ovl_f987800 names. Catalog
4,820 unit/symbol names, 225 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-four-phase-pillar-pixel-grid-spiral-handles.json and Binviz
target/ff9-names-four-phase-pillar-pixel-grid-spiral-handles/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
0 exact/2 partial, 0/6,044 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 fifteenargVOID grid: skip0 increments W/H and reserves all36bytecells BEFOREpositivechecks; NULLrotationinputfallback8zero bytes,f14/optionaluniformf20/padf27C,centerdepthreused. ZEROZ/XYloads,RTPT thenMAC0WITHOUTNCLIP gateANYnonzero/fourthRTPS. Interior5-5-5RGB afterTWOc11<<2,byte3partial/blackedges. EACHrow resets pk SAMEpool ratherthanadvancing, preserve arena frontier; f234length8eachlinkedcell/length1end. Unconditional a6/a5+remainder,f4rect tail,skip keepsincomingdimsW-1/H-1. No GTErestore/providerdirectionguess.

7CA0 descriptor56/init TWO hostvectors via200/1FC,table6,FOUR338handles22/23/24/25 plusTWO58objects8/9. Resource12atsecondvectornullabletable/h12=18/hc36/h22=32. Phase0/counter0/twotrackedNULL; no hardcodedYzero or actoroffset as inotherfourphase variant. TableD801E8EF8 declaredONEINT but70count6, providerreadextentunknown. Createdw28 unusedlater; pads/partialvectors remain.

Fourphases ordinary25/9/47/17updates. Phase0 t4096/24 ring11context512,gridfadeprogress/3 andscale2730 (not6144orcommon2/3);>=24transition1/load10nullable256 AFTERdraw. Phase1 redflash,exact4loads18/17/16atfirstvector,gridfadeSIN(t128),scale(t512)/3+1365;ring11 repeatSIN;>=8transition2 AFTERwork. Scope namesdescribephasebehaviorwithoutclaimingwater/spellidentity.

Phase2 first4flash/first32randomred SIGNEDremainder%50, before29sprite15Y-1024 cos*3/2+4096/gridscaleprogress/2+2048. Sprite11 allupdates exactSIN/2*2+8192 truncateorder, repeatedSINcalls. Modelw24 firstcosXYZwriteslateroverwritten XZcos/3+384,Ysin*3/2+4096; THREE separateSINtints. Events2resource14/14resource19/3resource21and20trackednullable256; endedh30clearselsewrites1024/0/0 toPOSITION5C/5E/60, notrotation repair.

Phase2first36updates 18poses reuseFOURadjacentINThandles (&w14)[i&3], not18createdobjects. Negate-firstshift -t<<7/-t411,base700/oscillation170, verticalscale2496+(progress>>5),height2400COSfirst6else2400-angle. Trig/B4oscillation/interpolation literalorder and signed narrows retained. MaxTHREEsprite13randomoffsetsparks, RANDOMall18iterations evenwhenlimitreached. >=46transition3 AFTERwork.

Common gridonlyfade!=0,rotation1024/0/t256/pad1,fifteenargs page279/U0/V48/columns2/counter&7/step256/input16x16/scaleLAC DIRECT (notcommon2/3)/colorfade/fl1/pixelbuffer. Builder17x17reservation10404/tail16x16. Phase2from29fade0;phase3terminal16 calls78return1 BEFOREcounter. Otherwisecounter++. Preserve pins20/16,memoryword arithmetic/partialpads/APIarities and providerownershipfrontiers.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Eight phase model pair actor points loaded objects naming acceptance (BV-03/BV-08, P2):
FF9 e489130fc adds one canonical ovl_12a8c800 names. Catalog
4,821 unit/symbol names, 226 scoped alias headers. One full own-unit
body reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-eight-phase-model-pair-actor-points-loaded-objects.json and Binviz
target/ff9-names-eight-phase-model-pair-actor-points-loaded-objects/. One full native object pair equal;
exact affected/scored namespace one. Pinned strict-relocation scores unchanged:
0 exact/1 partial, 0/9,480 code bytes, zero failures. One
installed WASM token comparison agrees; current source/header/catalog/object/
review bindings and five isolated committed paths audit.

EC724 descriptor128/initphase/counter0/savedctx/1CC/table5. FOUR338handles131flags0/129flags1/130flags0/128flags0, nestedhostp4/p54unguarded68selector. ThreezeroINTposition stores/reloads have emptyasmbarriers, scales6144 thensecond2048,three-halfword rotations0; secondtranslationY=-512. Actorpoint/tempvectors/objectpointers notinitializeduntilphases. No semantic sound claim solelyfrom68comment.

Eightphases ordinary framecounts25/25/22/22/25/22/22/33. Common64 firsttransformargument is THREEINT jitteredcopyofposition whiletranslationalso&w48; preservesunresolvedvectorABI/meaning. Three rand_ncallsranges14/14/8/6/2/1/1 inphases0..6, no randominphase7. 254writesactorpoints, publishTHREEU16 toctxbyte24/16. Phase0/1 providerread Ythen+700-frame12, two-wordcopyincludingpad/qY+=768,exact0load2nullabletable/h12=16/hc204/h22=64; symmetric±X twin28Cposes onSAMEhandle130 withselectors1/2 and handle128/fadeweight. Resource131model/texture240two/screen248 THREEcolorbytes/pads retained.

Phase0>=24 next1/1CC AFTERdraw,phase1>=24 next2/1CC/cleartwopointers/68s[2]. Phase2 first10/last>=20 host1900/10, exact0pointpublicationsetsXZfromzerotranslation. TWO254reads fromselectorsD30[0]/[1]; fullscalehandle131 plus129XZquarter/Yhalf. Exact12 loadsTWOresource1 withdistincttables/h12=50/hc28/h22=32. ROTmacro writesconstant2560/0/0 toPOSITIONfields5C/5E/60 (not actual rotation despiteoldcomment), clearsendedh30. >=21 next3/pointersNULL WITHOUTreleases/Ytranslation-=100/68s[4].

Phase3 host1900first10/7from17, selectorsD2C0/1, first18 Y+=sin(frame1024/18)*41>>12/publishpoint. Pairscaleshalf;exact12TWOresource6/constantposition2560;>=21 next4/pointersNULL/68s[5]/translationY-=3096,254selector33,Y-=70,publishbyte16. Phase4 exact0 1CC,selectorsD34[0]/[1],exact6tworesource6/constant1024position,>=24 next5/1CC/pointersNULL/translationY-=900/19024.

Phase5/6 jitterrange1 makesTHREErand_n(1) calls retained. D38[0]/[1] pointsY-=170,publish24, two-wordqx→rxincludespad thenrY-=70/rZ-=128. Exact11loadresource4nullabletable/h12=50/hc28/h22=-128 thenresource14untracked, guardedrepeat h22=-128; exact0resource6atrxothertrack. Positionmacro1024/0/0, model129XZthird/Yhalf. >=21 nextphase/pointersNULL/1CC/19024 (phase5) or68s[6](phase6), selectorD38[1] or4 withY-170/-896,publish24. No automaticrelease or movementunmentionedinbody.

Phase7 no jitter,host19023 fromframe23,selector4point: beforeframe16 Y+=128-((4096-cos(frame64))>>2),elseY+=128. Publish24;qxZ+=550;exact10resource9w78nullable128. PositionmacroSIGNED-624/0/0 convertedU16 intohostposition. Modelpair/scalesandtwo240calls EVERYphase2..7. Terminal>=32 calls1CC/78 AFTERallphasework BEFOREctxflagclear/counter; otherupdatesclearf28=0/counter++ evenunknownphase. Partialvectors, overwrittentrackedreferences, K&R/providernull frontiers and no added ownership cleanup preserved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Ten curved ribbons actor vertical oscillation naming acceptance (BV-03/BV-08, P2):
FF9 a035886f7 adds two canonical ovl_1063d000 names. Catalog
4,823 unit/symbol names, 227 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-ten-curved-ribbons-actor-vertical-oscillation.json and Binviz
target/ff9-names-ten-curved-ribbons-actor-vertical-oscillation/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 312/4,036 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 fiveargVOID unconditional k=0x20000000/(len*len), kinds0linear-down/1squared-remainder/2one-minus-square/other0; B4(handle,extra,w), no return. Main uses consumedreturn for modelscales andactorY; no invented returned interpolation type or fixes to zero-denominator/overflow/clamps.

783C descriptor220/init clearsONLYfirst16words descending15..0/table4. TEN260byteribbonrecords life0/110resource13init. TargetZselectSIGNEDactorZ ±780, target/positionY-1536 butivY0,publishctxpoint0XYZ. ActorcountUNBOUNDED globalunsized28byterecords, life48..55,angularstepSIGNEDrand%511+256 (notmaskedrange),TWOseparate1F8capturedvectors. FOURhandlesresources8via58/11and12via18/9via58,threeprimaryzero scaleexceptthirdY256; fourthscale1024/0/1024. Pads/unknownextent remain.

Exact0eightarg2F4 andtwosequencecounters1. Exact1 scalegrow16/ribbon48/fourthYgrow16 andthreepersistentflags1. Exact42scalechange8/fourthYshrink8,exact49followflag0,exact51fourthposeflag0. Grow/changes PREdecrement then THREE consumedVOIDhelpercalls pergroup; fourthY+64 and-128 pulsecounts. FollowD4(target,ctxpointbyte84,secondrot), derivefirst/thirdorientations THEN secondX-=1024,poses ordersecond/third/first/268. Colorfade checks w3 AFTERitwasdecremented inearlierscaleblock so ordinaryseven samples -32..-224,zeroendpoint skipped; no extra fullblack.

Actorflag stays1. PeractorPREdecrementlife; nonzeroadvanceangle beforeYformulas. life32..47 consumedkind2param +cos*(48-life)>>10;16..31cachedY+((cos<<4)>>10)-128;1..15 consumedkind1param+cos*life>>10; life>=48 keepsinitialsecondcapturedposition. Alllive204publish, lastlife0 usesFRESH1F8/204 notforcedcachedoriginal. Angularstep maynegative dueSIGNEDrandremainder, nocountclamp/padcopyrepair.

Ribboncount PREdecrements, evenfinal0updateprocesses TENrecords. LivePREdecrementlife thenangles; generate11vertices: firstspherepoint basedonhandle0scale, sameXZlastpoint butlastY0, NINE interiorcurvedXZweights pa0/pa1/pa2 fromtt=(j+1)*190650>>12 anduu512-tt. DriftU16offXZ+=INTvel, controlpoint usescos/sin ofD4-producedangle; interiorY B8(sphereY,0,j,11). Notclaimed ordinarytwo-distinct-endpoint Bezier fromtarget/follow because literalXZ endpointscoincide. Vertexpadsrecordpad,headers10/0/10; life<4weightslife1024else4096,includingzero-lifedraw. Freewhenremaining>=7 initialize life2..6/randomangles/steps/pad64..128/nineoffsetvels±96; no drawuntilnextupdate.

Two134resource7/10 sequencecountersINCREMENT eachupdate from1, notdecrementedflashlifetimes. Terminalreturns t>=70 AFTERallwork/tails, no78/release/reset. Invalid/negativeframes andproviderarities/readextents unchanged; no original effect identity guessed.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Five phase six handle particles saved matrix fade naming acceptance (BV-03/BV-08, P2):
FF9 d319dd2ed adds two canonical ovl_13731000 names. Catalog
4,825 unit/symbol names, 228 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-five-phase-six-handle-particles-saved-matrix-fade.json and Binviz
target/ff9-names-five-phase-six-handle-particles-saved-matrix-fade/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 380/4,828 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7718 event1 initializes ONLYh0=0. Otherwise a=(frame<<7)>>2, strength*((4096-sin(a)))>>12 is subtracted ONLYfrom signed positionX; positionY/Z unchanged. Uniform INT scale=((4224-cos(a))*particleScale)>>12 and shared host handle at78 passed to60. THREEseparate cosine calls produce tint (cos>>5)-128. Terminalframe>=32 checked AFTERpose/tint. Local fourthwords and vec declared4byte versus externalrecord20/readextent frontier retained, no rotation extent or initializer repair.

7894 descriptor188/16particles/20record/callback. Initphase/count0/contextsaved/1CC/table70count7. Actual six resources13(flags0),15(flags1),5,1,2,16; not donor comment1/2/9/3/6/4. Copies TWOwords from global THREEhalfword vector, secondword includes adjacent storage frontier. Initialize rotation34XYZ0/scales4096/INTposition widened, hostpoint2 THREElanes, rotation5C0/1024/0. Target vectors, strength and matrixsave7C notinitialized here; pads remain.

Phase0 ordinary33updates0..32: pose/tint handle13,11C resource11, hostpoint2X-=46. Transition AFTERdraw DC displacement-2408 generates target54 then widened48. Phase1 ordinary33updates: poses15/5/13,tints15and5,hostpoint2X-=sin(frame32)*46>>12,strength1024. Everythirdframe literal (FR/3)*3==FR tries184; nullable sets rotationhc/he1024/h10=0,targetXYZ,scale4096. 128resource17 underctxflag512,sizeframe64; resource11 via11C. Negative multiples retain literal gate.

Phase2 ordinary25updates0..24: t=frame4096/24,poses15/5/13 plus1/2 at target withXZ(4096-cos(t>>2))/3+245/Y2048; strengtht/2+1024. Everythirdframe particle scale4096. Exact1 copies target into TWOthree-lane points then h18X-=3072; nullable resource7 loaded object gets RAWpointer+2C=&h18,h22=128, no header extent inference. Overlaygray132 withpartialcolorfourthbyte/rotpad andscales4128/4128/4096. 128resource17 size(t>>1)+2048/sinefade,11Cresource11 opaque.

Phase3 ordinary25updates: XZ((4096-cos(t>>2))*245)>>12/Y2048,strength(t>>2)+3072. EveryFOURTHframe ANDframe<24 emits fading particles scale4096-t: six ordinary attempts0/4/8/12/16/20. Exact22 captures EIGHTctxrotwords into save7C. Skipped event leaves stale/uninitialized save7C; no backfill. Overlay/11C, transition afterframe24 restores hostpoint3 from v8 butY-=512.

Phase4 ordinary21updates0..20: save EIGHTcurrentctxrotwords9C, restore captured7C into memory, posehandle13/268(1)/tint((4096-frame4096/20)>>5)-128, restorecurrent9C BEFOREoverlay/11C. No explicit GTE load added. 11Cresource11 usesflag1 versus earlier255. Terminal20 calls TWOarg180(pool,704),78,return1 BEFOREcommon tail. All nonterminal phases inclunknownphase do TWO240 calls using count4 and10000-count4,134resource3 thenincrementcount. Pool16descriptor not invented emission cap; partialscratch/unclamped arithmetic/provider arities unchanged.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Five phase saved matrix fade resource variant naming acceptance (BV-03/BV-08, P2):
FF9 133c1d8b9 adds two canonical ovl_12b92000 names. Catalog
4,827 unit/symbol names, 229 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-five-phase-saved-matrix-fade-resource-variant.json and Binviz
target/ff9-names-five-phase-saved-matrix-fade-resource-variant/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 380/4,828 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7718 event1 initializes ONLYh0=0. Otherwise angle=(frame<<7)>>2; subtract ((4096-sin(angle))*globalStrength)>>12 ONLYfrom positionX, signed widened Y/Z unchanged. Uniform scale((4224-cos(angle))*particleScale)>>12,60 usingvec4byteview andsharedhandle78. THREEindependentcosine evaluations tint(cos>>5)-128. Terminal32 AFTERpose/tint. Uninitialized fourthINTwords, vec declared4bytes versus external20byte record andproviderreadextent retained.

7894 descriptor188/16particles/20record/callback. Initphase/count0/contextsaved/1CC/tablecount7; actual six338resources1(flags0),2(flags1),9,3,6,4. TWOwordglobalTHREEhalfwordcopy readsadjacenthalfword; rotation34XYZ0,scale4096,INTposwidened,hostpoint2THREElanes,rotation5C0/1024/0. Target/globalstrength/savedmatrix7C initiallyunspecified,padsuntouched.

Phase0 ordinary33updates: pose/tint1,11Cresource16,hostpoint2X-=46 then >=32transitionandDC-2408target54 widened48. Phase1 ordinary33: poses2/9/1, tints2/9,sineXhostpointmotion,strength1024,everythirdframe184attempt via literal divisiongate. Success rotationhc/he1024/h10=0,targetXYZ thenh2scale4096; preserves ownstoreorder. 128resource5 underflag512,sizeframe64 andframe128weight;11Cresource16cosineparameter thenopaque4096.

Phase2 ordinary25updates: t=frame4096/24,poses2/9/1 plus3/6 at target withXZ(4096-cos(t>>2))/3+245/Y2048; strength(t>>1)+1024. Everythirdframe particle scale4096 with own reordered hc/he/h10 andXYZstores. Exact1 chainedcopies target54 intoh18firstandh10second, thenh18X-=3072; resource11nullableobject rawpointer+2C=&h18,h22=128. Signeds8SB stores0x84 narrowing to-124 while passingbyteaddress to248; no unsignedview conversion. Overlaypartialcolor/rotpad scales4128/4128/4096;128resource5size(t>>1)+2048/sine,11Cresource16.

Phase3 ordinary25: poses/tints2/9/1 plus3/6XZ((4096-cos(t>>2))*245)>>12/Y2048,strength(t>>2)+3072. EveryfourthframeAND<24 emits scale4096-t, ownstoreorderh10/hc/he/Z/Y/X/scale. Exact22 captures EIGHTctxrotwords7C; skippedframeleavesstaleoruninitializedmatrix. Overlay132signedbyteview/11Cresource16. Transition24AFTERwork,restorehostpoint3fromv8 withY-512.

Phase4 ordinary21: EIGHTcurrentctxrotwordssaved9C,writecaptured7C intoctxrotmemory,pose1/2681/tint((4096-frame4096/20)>>5)-128,restorecurrent9C BEFOREoverlay/11Cresource16flag1. Ownstoreorder scaleZbeforeotheroverlayfieldsretained. No explicitGTEloadorbackfill. Terminal20 TWOarg180(pool,704)/78/return1 BEFOREtail. All nonterminalincludingunknownphases TWO240count4/10000-count4,134resource7 (distinctfrom13731000resource3),count++. Externalpool16doesnotimpose inventedcap; partialscratch/readextents/arithmetic unchanged. Originaleffectandprovider meanings unresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Eight row textured swirl band paired orbiting sprite naming acceptance (BV-03/BV-08, P2):
FF9 ddeb336be adds two canonical ovl_11e90800 names. Catalog
4,829 unit/symbol names, 230 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-eight-row-textured-swirl-band-paired-orbiting-sprite.json and Binviz
target/ff9-names-eight-row-textured-swirl-band-paired-orbiting-sprite/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
0 exact/2 partial, 0/4,852 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 reserves5504primitivebytes unconditionally: GT4 poolfirst1664bytes (32maxrecords52bytes),FT4 at+1664 (96maxrecords40bytes). Eightrows/sixteensegments, first/lastrowsGT4 with black edge andcolored interior; sixmiddleFT4. Rejected nc==0 doesnotadvance localrecordpointer; colorwritesoccur BEFOREclipgate andreserved cursorstilladvancesfullsize. nc!=0 accepts BOTHsigns, notclaimed standardfrontface-only cull. No capacity/clipping/depthgate/GTErestore added.

Drawer loadsctxmatrix/position, literalRTV0TR_X then14(m1,m)/20(m,m2), storesGTEthreeINTtranslation/matrix27C/loadsderivedmatrix. Outerrotation/translationreadextents and partialmatrixpad preserved. Colors pal0 setsRGBONLY; pal!=0 masks globalindex31, calls238 TWICEwithSAMEindex into separatecolors, multipliesbycol>>7, thenincrementsindex. Blackcbword0 thencode,cf code2E/2C,cg/cb3E/3C selectedsemi!=255; flagsOR((semi&3)<<5) even255. No invented paletteadvance or lowerclamp.

Eachrow s5+=sweep/ph+=phstep BEFOREradiuscalls B4(la,lb,j512) andnextrow512. Segment then s5+=sweep andinneredgeposition r0/8 atph,outer(r0+spin)/8 atph+phstep, eachsin/cos s5 /s5+256; s5-=sweep beforeinnercoords thenloopincrement256. Rowsdepth0..1152 by144, spinr0 increments eachrow. Partial fourthhalfwordofEVERYvv leftuninitialized. UVtile32 withlasttile31 narrowing. RTPT/NCLIP/readMAC0 thenliteralG24_AVSZ4 opcode0x158002d/readregister19>>2 BEFOREloading/projectingfourthvertex; host234(09000000FT4/0C000000GT4,depth,record) BEFOREfourthscreenstore, thenpointerincrement. Retain macro labels/opcodes/order without semantic normalization.

7F28 descriptor36. Init table70count1/resource9spriteflags1/phase0/tick0, TWOwords copyhost+38/+3C intoanchor8/C includesadjacenthalfword beyondHost0c declared3lanes,angle18XYZ1024/32/0,partialpaduntouched. Perupdate radius=(tick<<9)/72, angle4096-sin((tick<<10)/72), signedorbitpointanchor+sin/cos*radius>>12,Yanchor. v60XYZ0/tick32/0 withpaduninitialized. No actualspell/vortexidentityorprovideradmission inferred.

Phase0 ordinary13updates0..12: normalizedprogressframe4096/12,outerXZ? actual scaleXYZ(fp,fp,8192); inner(8192,8192,cos(fp>>2)*2),twodraws brightness128 andfp>>5,spinfp>>6,phase tick40,step300,sweeps200/128. Sprite scale(fp,4096-sin(fp>>2),fp),tint(fp>>5)-128 AFTERpose; >=12transition1/frame-1. Phase1 ordinary13: outer(fp+4096,fp+4096,8192),inner8192all; radiiinner456-(fp>>5),spin(fp>>5)+64, sprite4096all no tint. >=12transition2/frame-1 thennullableloadresource7atliveorbit,h22=49.

Phase2 ordinary33updates0..32 (commentends31wrong): fp=frame128,uniformdrawscale(fp>>1)+8192 bothbands,outerradius200-(fp>>6),spin(fp>>5)+192,sweepouter(fp>>5)+160/inner128. Sprite4096all. Transition AFTERdraw>=32phase3/frame-1. Phase3 ordinary17updates0..16 (comment15wrong): bothbandsuniform10240,brightnesssin(frame64)>>5,spin320,outerradius136/384 andsweep288,innerradius328/640 sweep128; spriteXZ4096/Ysin(frame64),tint((4096-frame256)>>5)-128. Terminal>=16 calls78 andreturns1 AFTERdraw/tint BEFOREangle/texture/ticktail.

Nonterminaltail EVERYphase includingunknownphase subtracts16+tick2 fromSIGNEDangle18Z (commentmistakenh1C),THREE240 calls weights10000-tick12/10000-tick6/tick8 thenincrementsglobaltick. Phaseclockresetswithoutresetingglobaltick, no camera writes/release added. Partialvectors/scales fourthwords, repeatedsin/cos/phaseargumentcalculations, unclampedarithmetic andboth-viewABIfrontiers retained.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Five curved ribbons actor zero vertical vector pulse naming acceptance (BV-03/BV-08, P2):
FF9 bf2ccc620 adds two canonical ovl_10614000 names. Catalog
4,831 unit/symbol names, 231 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-five-curved-ribbons-actor-zero-vertical-vector-pulse.json and Binviz
target/ff9-names-five-curved-ribbons-actor-zero-vertical-vector-pulse/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 312/4,084 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

77F0 computes k=0x20000000/(len*len) UNCONDITIONALLY. Kind0 4096-((0x10000000/len)*t>>16),kind1 squaredremainder*k>>17,kind2 4096-(k*t*t>>17),otherweight0. CallsB4(handle,extra,weight),discardsresult andreturnsVOID. No clamp/overflow/divisionzero/prototype repair.

7928 descriptor212. Init ONLYfirst16counterwords cleareddescending15..0/table70count4/FIVE284byterecordslife0/110resource13. Captures1F8actor0sixbytevector/2A0threeINTvector/1F4selector/200derivedeightbytepoint. Publishes ONLYX/Z intoctxpoint/oINTposition (Ynotinitializedhere). FOURhandles8via58/11and12via18/10via58; firstrotation2048/0/0 allthree scales0, fourthrot0/scale585/0/585/ivY0 andcapturedSIGNEDactorX/Z. 2E4fourth32. Pads/unknownproviderreadextents retained.

Exact0eightarg2F4 atcapturedactorX/Z withweight-4096. Exact1 follow1/growth16/ribbons48/firstoscillation16. Exact9fourthpose1/Ygrow8;exact17middleoscillation25;exact42shrink16/restoreoscillation16/Yshrink8;exact58follow0/fourthpose0. Counterspredecrement; scalegroupsTHREEconsumedVOIDhelpercalls to uniformscales withextras368/323/409thenreverse. FourthY+=64or-=64. Events notbackfilled/guarded againstnegativeframe.

Firstoscillation16 predecrements; ifcapturedselector>=513 THREEINTvector pulse usesCONSUMEDhelper fromsavedvectorYtoY/2,uniformXYZ and2A4(0,iv). Actor0position X/Zcaptured, Ycaptured+CONSUMEDhelper(0,-512)+cos(frame256)*(16-remaining)>>10,204then200refreshderivedpoint. Middle25setsYcaptured+cos(frame256)*16>>10-512/204ONLY (no200refresh inthatblock). Restore16 predecrements; nonzero selector>=513 pulsesreverse vector andYhelper(-512,0)+cos*remaining>>10. Lastzero performsFRESH1F8(0,sp28) and2A4savedvector UNCONDITIONALLY regardlessselector, then204/200. No forcedcachedactorpositionreset or reconstructedcamera/fovidentity.

Everyupdate publishes derivedpointY tooctxpoint/oINTpos evenwithoutfollow; X/Zremaininitfixed. Follow D4(derivedpoint,ctxpointbyte84,secondrot),firstX=secondX+2048,thirdX=secondX+1024,sharedY; THENsecondX-=1024. Posessecond/third/first,268first1,134resource7frame. Fourthposeconditional with134resource9frame. No colorfade block unlike sibling1063D000, no allactorsloop.

Ribboncountpredecrements; FIVErecords eachlivepredecrements andstilldrawsatlife0. Header10/0/10,angles+=steps; spherepointca fromhandle0scale/liveINTpos, cbXZ=pos+(sphereoffset>>4),cbY0. Elevenvertices endpointsca/cb andNINEmiddlepoints. tt=((j+1)*190650)>>12,uu512-tt,weightsuu2>>6/(tt*uu+uu4)>>6/tt2>>6; Yweightedca/cb/(posY>>1), XZmixedsignedhalfwords andD4angle-derivedcontrol expressions with literalmultiplication placement. U16offsetsaccumulate signedvelocitynarrows, then applied TWICE: onceinside sp48 andonceagainvpXZ. Preserve doubleoffset/halfwordwrap/partialca-cbpad rather than normalize to standardBezier.

Eachlive110resource13 usesweightlife512 WITHOUTclamp, including0 onexpiry. Free record initializesonlyifremainingribboncount>=9: lifeC4(2,8),anglesC4(640,1536)/RAND&4095,stepsC4(-96,96),padC4(8,32),nineoffsetXZ0 thenvelsC4(-32,32),notdrawnuntilnextupdate. Unknownrangeproviderendpoint semantics notassumed. Finalreturnframe>=70 AFTERallwork, no78/release/explicitactorrestoreterminal. Partialvectors/storeorder/pin17/matchingwrapperand consumedVOIDresults preserved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Ten particle fan table driven three model spread naming acceptance (BV-03/BV-08, P2):
FF9 9eeeb4ac8 adds two canonical ovl_1035f000 names. Catalog
4,833 unit/symbol names, 232 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-ten-particle-fan-table-driven-three-model-spread.json and Binviz
target/ff9-names-ten-particle-fan-table-driven-three-model-spread/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
0 exact/2 partial, 0/3,700 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 mode1 sets phase/frame zero and loads resource20 at parent state+24; nullable object h22=32. It does not initialize particle index, size or centers; controller supplies some of those after allocation. Other modes index unchecked global four-U16 records. Phase0 computes frame256 angle, XY scale=(4096-sin)*size*5/2>>12, Z=cos*size>>12; copies two table words and subtracts2048 from first U16 of local v18. Loaded object tracks ORIGINAL three table lanes in this phase, not shifted local v18. Advance frame then transition at4; common draw follows after transition.

Phase1 holds XY=size*5/2/Z=size and tracks loaded object using shifted v18 XYZ. Phases0/1 clear the object pointer if h30==-1, then guard again. Parent h28 nonzero transitions to2 immediately, resets frame, copies base center into both companion centers and two table words into particle rotation. Jitter uses signed rand remainder50 and a second C0 call with TWO arguments (firstRandom/50, updatedh38), masked31 for h3A. No standard zero-argument RNG prototype repair. The transition update takes the THREE-model common draw branch with phase1 scale/tint.

Phase2 uses angle=(frame4096/20)>>2 and THREE separate sine evaluations for XY scale, tint weight and direction length. D8(table,q), D8(table,2q), D8(table,3q) produce motion vectors: centers all subtract X/Z; Y adjustments differ by frame-6, four times and eight times. Update U16 rotations by h38/h3A. Increment local frame; at20 return1 BEFORE common scale halving/poses/tints, so nineteen ordinary phase2 updates draw. Size and centers are INT, direction buffer only three signed halfwords; provider read extent/padding remain unresolved.

Common path halves XY scale only. Phase<2 calls2E4(parentw4,2), poses w4 then w14 using TABLE pointer p as rotation and particle center as position, not shifted v18. Phase>=2 poses w10/wC/w8 at three centers using particle rotation. Weight<4000 selects268(handle,1), brightnessweight>>5; otherwise268(handle,0), brightnessweight*3>>6; tint subtract128. No clamps, releases, terminal backfill or unknown-phase initialization added; unknown phases retain uninitialized v20 and weight0 behavior.

7ED8 descriptor52/10particles/68-byte records/callback. Init table70 count6, FIVE58 handles with signed-halfword narrowing: resources13/23/22/24 with154/3DC0/128/0, resource11 with186/3D40/0/128. Clear phase/index/count/trigger/textureweight. Capture host XYZ as U16, copy original XYZ to second vector then subtract512 ONLY from first Y. Load resource14 at lowered vector, nullable h22=100. Mutate TEN global table records X+=92/Y+=512 in place each init; no reset/clone added.

Phase0 attempts ONE184 allocation per update, ordinary ten frames0..9; success alone increments table index, DC(lowered anchor, indexed table,-16,v20) then widens three signed lanes into particle centers and size=(signed record fourthlane*31)>>3. Pool descriptor10 is not an extra counter guard; null attempts do not consume index. Textureweight+=32 each update. Transition at>=9 AFTER attempt/weight; phaseclock=-1. Phase1 adds sine(frame1024/14)*32>>12, ordinary fifteen updates0..14. At>=14 sets phase2/frame-1/trigger=oldphase1, loads SIX resources15/16/19/21/17/18 in that order at first vector, nullable h22=64.

Phase2 frame<5 calls100(ctx->p20+1,1,q>>2,q>>1,q), q=(4-frame)*250/4: a primitive/context fill call rather than an asserted sound fade. Exact7 loads resource25 and nullable h22=220(0,32). Terminal>=21 calls78/return1 BEFORE240/counter tail. Every nonterminal update, including unknown phase, calls240(textureweight,640,384,64,128,640,256), increments count. Parent trigger remains set; no particle cleanup or loaded-object release inferred. Negative frames, partial vector pads and extended job+16 view retained.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Five jittered trails actor zero vertical vector pulse naming acceptance (BV-03/BV-08, P2):
FF9 5c7005d90 adds two canonical ovl_10698800 names. Catalog
4,835 unit/symbol names, 233 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-five-jittered-trails-actor-zero-vertical-vector-pulse.json and Binviz
target/ff9-names-five-jittered-trails-actor-zero-vertical-vector-pulse/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 312/3,828 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

77B8 unconditionally computes 0x20000000/(len*len). Selected weights: kind0 linear down, kind1 squared remainder, kind2 one minus squared elapsed, otherwise0. B4(handle,extra,weight) result discarded, no return expression. Keep overflow/division-zero/unclamped arithmetic; no synthesized INT return even though caller consumes it.

78F0 descriptor168; init saves context, clears loaded-object global, table70count3. Clears sixteen words descending15..0 through w[10] declaration into the following padding: array extent/layout frontier retained, not changed to sixteen-element declaration. FIVE288-byte trail records initialize life=-index and submit110resource9. THREE handles resources11/10/6, rotation first2048/0/0, uniform scales92/80/102. TWO independent200 captures, 1F8 actor0 position,2A0 INT vector and1F4 selector. Initial target X/Z copied, targetY uninitialized until per-update200; INTposition signed-extends all captured lanes.

Every update starts80(16,23,global start). Exact0 starts sequence counter1/follow1/trails36/growth16; exact8 actor/vector shift16; exact16 loadresource2 at second200 capture; exact24 holdY4; exact28 shrink16/restoreY16; exact40 loadresource1 at first200 capture and nullable h22=220(0,128); exact44 F100(ctxbytebuffer,255,255,255,255), follow0. Counterw2 is cleared in init and never armed by these events; its positionY branch remains available to external state. No invented normal activation.

All active counters predecrement. w2 sets signed captured INTpos with consumed linear helperY. Growth/shrink each use THREE consumed helper returns for uniform scales, from92/80/102 to394/323/409 and then0. Shift16 conditionally pulses INTvectorXYZ using savedY tohalfY when selector>=513; actor0 Y=capturedY+consumed helper(0,-512), X/Z captured,204. Hold4 writes fixed capturedY-512. Restore16 conditionally reverses vector and actorY; zero endpoint uses FRESH1F8 and restores saved INTvector UNCONDITIONALLY, then204. No cosine shake is present in these blocks; names say vertical shift.

Every update refreshes200 target, copies THREE lanes to object-follow point; nullable loaded global h30==-1 clears pointer, but1D8(global,point) runs UNCONDITIONALLY even when NULL. Follow copies target signed lanes into INTpos and can overwrite prior w2 changes, computes second rotation from target/context+84, firstX+2048/thirdX+1024 and matchingY, then secondX-=1024; poses second/third/first,268first1. No NULL guard, release or camera/provider normalization added.

Trail countdown predecrements. FIVE records: life>0 predecrements, builds ELEVEN points starting at signed global80 vector. Increment=(INTposition+randomDirection-start)<<12 divided by11; emitted j0..10 means final sample10/11, not forced endpoint. Y offset used BEFORE yoff+=dy; each vertex pad=life*4 and header10/0/10. Weight life1024 for life<4, else4096, including zero-life submission. life<0 increments ONLY, reaches0 but does not spawn in same update. life0 spawns only remainingtrailcount>=5 and does not draw until next update.

Spawn lifeC4(4,8), two masked angles, directionY=sin*firstscale; X/Z use ABS of that Y radius with cosine/sine of second angle. No full sphere-cosine reconstruction. Eleven offset/velocity endpoints0; nine interior signed ranges grow by vertex index (offset32+16j,velocity16+8j). Padded arrays/record words untouched. Resource12 sequence counter134 increments each update from1, not a sound/flash lifetime assertion. Returns frame>=90 AFTER every block and counter; no78/release or terminal restore added.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Scheduled three particle rise spin model sprite burst naming acceptance (BV-03/BV-08, P2):
FF9 f73b648e4 adds two canonical ovl_1000c000 names. Catalog
4,837 unit/symbol names, 234 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-scheduled-three-particle-rise-spin-model-sprite-burst.json and Binviz
target/ff9-names-scheduled-three-particle-rise-spin-model-sprite-burst/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 696/2,984 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 mode1 resets ONLYphase0, loads resource3 at current particle position and nullable h22=16. Position/rotation/base words/index are not initialized by this branch; controller sets base and random rotations after184 returns. Phase0 s=frame4096/15; exact0 initializes resource4 140-byte buffer selected by unchecked particleindex. Copy two base INTwords to SVec (including pad), Y-=2*(4096-s), Z+=4096-s then copy bothwords to particle. Signed lanes widen to INTposition; rotations+=70/64/8 with narrowing. Shared handlep10 posed at uniform409 scale; fourthINTwords uninitialized.

Phase0 contextflag256 surrounds110resource4 at local offsetY+320/Z-160 withscale24576 and sine weight2/3+1365, then114resource1 at particle position. Particleindex0 publishes THREE position lanes to hostpoint. Loaded resource3 pointer clears if h30==-1; atframe>=15 AFTERdraw, phase1/precedingparticlehalfword=-1, guarded oldobject h38=UNGUARDEDnestedw24->h6. Reset positionY0, primitivefill100 atctxp20word1, replaceobjectresource9/h22=70, rotations0/random/0; returns0. No added phaseclock reset, nested-null guard or object release.

Phase1 s=frame4096/18. Firstfour frames fill grayscale70-frame70/4;114resource1 at size2s+6144 with sine fade. Local offsetvector computed but110resource4 uses ORIGINALparticleposition, not that local vector. RotateY+=sin(s>>2)>>7. Firsteight updates pose sharedhandlew14 with cosineXZ=(cos(frame128)/2+2048)/3,Y9/8 and tint(2sin-4096)>>5-128. Everyupdate pose sharedw18 withXZ=(cos(s>>2)/3+2730)*3/4,Ysin*2/3+2048,tint((4096-s)>>5)-128.

Phase1 resource13 sevenarg128 uses explicit SVec rotation1024/0/0/pad1,cosine size+2730 andremainingweight4096-s. Beforeframe12 extra114resource5 at sine(frame4096/6>>2)*3, with TWO independent sine evaluations. Terminal>=18 returns1 AFTERall applicable draws/tints. Does not update/drop-check resource9 pointer during this phase. Unknown phases return0, all negative/overflow/division/arithmetic and partial scratch initialization retained.

7FF4 descriptor28/3particles/36bytes/callback. Init flag/count0, savedcontext,1FC captured four-U16 point,table70count3. Create resource2 via58(151,3C80,0,0),resource11 via58(311,FFFF,-64,128),resource10 via18. No callback state/position initialization added to controller.

Update only ifflag0. Signedframe<24 reads external byte schedule WITHOUT lower-bound check. Nonzero byte andcount<3 increments count BEFORE184; NULL still consumes attempt. Successful record baseX=capturedX+C8(600),baseZ=capturedZ+C8(600),baseY0,indexcount-1 and THREE separate C0 calls into rotation halfwords. No centering of random600 range or invented schedule bytes/endpoint semantics. State index2 remains maximum via controller count but callback index has no independent guard.

134resource12 getsframe every flag0 update, even failed allocation. Finish requirescount>=3 ANDframe>=62, then78/return1 AFTER134. If schedule produces fewer than3 attempts it continues; externally nonzero flag bypasses work and terminal test. No completion based on live particle count, no retry afterthird failed attempt, no handle/object cleanup. External schedule/buffer extents remain unresolved and all original runtime/linker identities stable.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Ten particle fan three model spread typed resource variant naming acceptance (BV-03/BV-08, P2):
FF9 14cdcc1b0 adds two canonical ovl_f8da000 names. Catalog
4,839 unit/symbol names, 235 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-ten-particle-fan-three-model-spread-typed-resource-variant.json and Binviz
target/ff9-names-ten-particle-fan-three-model-spread-typed-resource-variant/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 1,692/3,696 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 op1 initializes phase/frame0 and loads resource14 at parent+24; nullable h22=64. Remaining state externally supplied. Other modes use unchecked Pt8 table index. Phase0 angle=frame256,XY=(4096-sin)*scale*5/2>>12,Z=cos*scale>>12,lum4096. Two-word local copy first U16 minus2048; loaded object tracks ORIGINAL tableXYZ. Clear dropped pointer h30==-1 and guard again. Advance localframe then transition>=4; common draw after transition.

Phase1 holds XYscale*5/2/Zscale, object tracks shifted localXYZ. Parenth28 triggers immediatephase2/frame0. First zero-argument RAND signed%50 adjusts U16h38 by-25; SECOND zero-argument RAND&31 adjusts U16h3A by-16 (distinct from1035F000 extra-arg C0). Companion center copies orderX/Z/Y with matching wrappers; copytable two words to SIGNEDx30/x32 andw34. Transition update draws THREEmodels using prior phase1 scale/tint.

Phase2 angle=(frame4096/20)>>2, THREE independent sine calls forXYscale/lum/directionq. D8(table,q/2q/3q) updates three INTcenters subtractXZ; Y subtractvY+6-frame, vY-4*(frame-6), vY-8*(frame-6). Signed x30/x32 +=U16 increments narrowed, directionbuffer FOURhalfwords unlike related three-halfword local. Updateframe BEFOREfinalcenterZstore; terminal>=20 AFTERallcenterupdates BEFOREcommon poses, nineteen ordinary drawupdates. No reordering or unsigned rotation-view substitution.

Common halves ONLYXYscale. Phase<2 calls2E4(handleA,2),60 with TABLE pointer asrotation/basecenter, GLOW(handleA), then60handleF withoutGLOW. Phase>=2 posesD/C/B at threecenters using particle rotation; GLOWeach. Lum<4000 visibility1/brightnesslum>>5 elsevisibility0/brightness((lum<<1)+lum)>>6; subtract128. FourthINTscale word remains uninitialized; unknown phases keep uninitializedscale/lum0. No object release, extra tint, clipping/clamps or callbacks on terminal added.

7ED8 reports56-byte state/10particles/68-byte record; declared EfSt is52 bytes, trailing requested extent remains unknown. Init context/table70count6; FIVE58 short handles resources19/21/22/23 with154/3DC0/128/0,resource11 with186/3D40/0/128. Zero phase/index/frame/trigger/scroll. TWO separate providers200(firstvec) and1FC(secondvec), not manual anchor/Y-512 copy. Loads resources13 and20 at firstvec, each nullable h22=220(0,70). Mutates TEN global table records X+=92/Y+=512 each init.

Phase0 ordinarytenupdates0..9 (commentnine wrong): one184attempt/update; only success consumesindex,DC(firstvec,table[index],-16,v20), widensSIGNEDXYZ, size=signed fourthtablelane*3 (not related31/8). Scroll+=32; transition>=9 AFTERwork/frame-1. Phase1 ordinaryfifteenupdates0..14, scroll+=sin(frame1024/14)*32>>12; transition>=14 trigger1 thenFOURloads resources17/18/16/15 atfirstvec, each nullable h22=220(0,32). Related unit six-load sequence not copied.

Phase2 firstfiveupdates F100(ctxbytebuffer+4,1,(4-frame)*250/4 shifted RGB). ExactZERO loadsresource25/h22=220(0,32), distinct fromrelatedunit frame7. Terminal>=21 calls78/return1 BEFORE240/frametail. All nonterminalupdates including unknownphase call240(scroll,640,384,64,128,640,256), frame++. No active-particle based finish, retry after phasewindow, extra cap/reset/cleanup or originalspell/light identity inferred. Uninitializedpads/providerextents and literal negative/overflow arithmetic retained.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Eight handle spinning blend actor group vertical shift naming acceptance (BV-03/BV-08, P2):
FF9 eafffec36 adds two canonical ovl_fb05800 names. Catalog
4,841 unit/symbol names, 236 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-eight-handle-spinning-blend-actor-group-vertical-shift.json and Binviz
target/ff9-names-eight-handle-spinning-blend-actor-group-vertical-shift/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 716/2,988 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor56; init flag0/context saved,200 four-halfwordpoint,1FC position startinghc,70(&declaredINTtable,8) with unknown table extent. EIGHT58 handles resources5/2/3/4/14/8/7/9, parameters57/3D00 or59/3D01 and0/128. Scale=(1F0(0)<<12)/720, lower test<1024 replaces with1700, upper>6144 caps6144; save global and state. This is not a random call, no smoothing of discontinuous lower substitution.

Update externally nonzeroflag returns0 BEFOREwork and terminal. Frames<24: position SIGNEDhc/he/h10; progressframe4096/24, basecos(progress>>2)/10+409, scale*saved>>12,XYZ(s,3s/4,s),rotationXYZ0/-frame32/0 withfourthlane uninitialized. Weight4*(4096-progress) chooses sequential blend5->2,2->3,3->4,4->14 using STRICT>12288/>8192/>4096; remainingweight passed unchanged. Pose resource9shared handle, THREE separate cosine calls tint(cos>>5)-128.

Frames>=24: base(saved*818)>>12,XYZ(s,3s/4,s),samefixedposition/spin. Pose14 and9, each THREE independent sine calls at((frame-24)*4096/26)>>2 for tint. Every update240(frame,576,384,16,64,table[frame%23+1].a/.b); no upper/lower index correction for negativeframes or table extent assumption.

Frames>=24 then second blend8->7: XZ=(saved<<11)>>12,Y=(cos(progress>>2)/4+682)*saved>>12,rotationY=-frame64,weight4096-progress. THREE independent cosine calls atprogress>>1 tintONLYhandle8, not7. Exact16 loadresource1 at capturedpoint, nullable h22=220(0,32); exact1 loadresource10 with220(0,64). Terminal>=50 calls78/return1 AFTERall poses/texture/events; does not setdoneflag or release handles. No column geometry identity assumed from resource comments.

7FE4 descriptor104; count from hostbyte36 is UNBOUNDED despite arrays8. Peractor1F8 captures eight-byteposition, w44=(-320-(RAND&31))*SHAREDglobalScale>>12 assigned EVERYiteration to a SINGLEshared field, so last actor amplitude wins for all normal updates. 80(actor,0,vec) supplies w48=-SIGNEDY. Not peractor random amplitude array; n0 leavesamplitudeunspecified but loops do not consume it. Count/array/global initialization frontier remains.

Frames<32 copy each capturedposition TWOplainINTstores includingpad then U16Y+=sharedAmplitude*cos(frame32)>>12,204(actor,vec). Frames>=32 copyagain, first U16Y+=peractorOffset*((frame-32)*1024)>>12 then U16Y+=sharedAmplitude*sin((frame-32)*256)>>12 as TWOseparate narrowing stores; do not combine or clamp. Negativeframes/unboundedcount/shift-overflow retain original semantics.

Terminal>=36 occurs AFTERthe linear/sine update has been published for EVERYactor, then SECONDloop FRESH1F8/204 each actor andreturn1. Fresh capture is not proof of restoring cached original positions; no forcedcopyfrominitcache, reset/78/release added. Originaleffect/provider identities unresolved and no whole-game or matching-gain claim.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Four handle shrink spin blend load paired sprite pulse naming acceptance (BV-03/BV-08, P2):
FF9 9063d59a2 adds two canonical ovl_1184f800 names. Catalog
4,843 unit/symbol names, 237 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-four-handle-shrink-spin-blend-load-paired-sprite-pulse.json and Binviz
target/ff9-names-four-handle-shrink-spin-blend-load-paired-sprite-pulse/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 1,940/2,772 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor40. Init context/table70count4; THREE58 resources3/4/5 narrowSIGNEDhandle with186/3DC0/0/0, fourthresource2 with184/3D40/128/128. State1/count0,200 firstsignedfour-halfwordposition and1FCsecondposition; secondvector not subsequently consumed. No assumed actor-relative tracking or cleanup.

State1 ordinaryfiveupdates0..4: sin(frame256),uniform scale((4096-sin)*3072)>>12,rotationXYZ0 andsignedINTposition. 2E4(resource3handle,2),60,2680,26C0. Transition>=4 AFTERdraw/state2/frame-1. Fourthrotationlane/INTvectorfourthwords uninitialized. No smoothing of scale jump on next state.

State2 ordinaryseventeenupdates0..16: uniform3072/rotationYframe32,pose3/2680/tint0,pose2/2681/THREE independent cosine calls atframe128 for tint(cos>>5)-128. Texture240 weight((4096-sin(frame128))>>9)+frame5. Transition>=16 AFTERdraw setsstate3/frame-1 thenFOURloads14/13/1/6 at firstposition, nullableh22=220(0,32), then100(ctxINTaddress+4,1,0,32,70). Preserve rawinteger address view and load order.

State3 ordinarythirteenupdates0..12: uniform3072/rotationY512. frame<2 weight8192-frame2048; elseweight4096-((frame-2)*4096/14). STRICTweight>4096 blends3->4 atweight-4096 andtints3=-64; otherwiseblends4->5 withweight andtints4=((weight-2048)>>5)-128. Exact1 resource16loaded/nullableh22=220(0,32). Terminal>=12 AFTERblend/tint/load calls78/return1 BEFOREcommoncountincrement. No invented completion at interpolationendpoint16. Unknown state still incrementscount, no poses.

7E98 descriptor16. Initphase/count0/contextsaved,positionv8 uninitialized untilupdate2B8(16,1,v8). Everyupdate obtainsv8 BEFOREswitch/terminal; localsx/h/w/size0. Phase0 ordinaryfourdrawupdates0..3: x=frame4096/3,size512,w=cos(x>>2)>>1,h=((4096-x)>>1)+2048. Transition>=3 AFTERparametercalculation resetsphaseclock, thencommondrawstillusesphase0values.

Phase1 ordinarytendrawupdates0..9: m=frame4096/9,h=4096+(frame&1)*256,x=(m>>1)+2048+(frame&1)*256,size=(m>>3)+512 usingidenticalifarms,w=h. Transition>=9 resetsphase2/frame-1 AFTERparameters, commondrawstillphase1. Phase2 size1024-(frame4096/9>>2),x=sin(k),w=cos(k)+4096,h=cos(k)*3/2+4096 withTWOseparatecoscalls. Terminal>=9 returns1 BEFOREvec/contextflag/sprite/counttail, while2B8alreadyran; ordinaryninephase2draws0..8.

Common pulse tail fillsALLfourrotationlanes0/0/size/0,contextflag128 around TWO128 calls: resource20 atv8 withNULLrotation,sizew/3,fade x/2; resource19 withrotationvector,sizeh/2,fade(2x)/3. Clearflag0 thenincrementcount. Originalsame-codecommentresource15/16 doesnotmatchthisunit20/19 andisnotusedasnamingauthority. Unknownphase emitszero-sizedsprites usinginitializedlocals; no clamps/phasebackfill/78/release added.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Eight handle spinning blend actor group shift resource variant naming acceptance (BV-03/BV-08, P2):
FF9 20fdd20fe adds two canonical ovl_105c3000 names. Catalog
4,845 unit/symbol names, 238 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-eight-handle-spinning-blend-actor-group-shift-resource-variant.json and Binviz
target/ff9-names-eight-handle-spinning-blend-actor-group-shift-resource-variant/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 696/2,932 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor52. Init flag0; copy actorX/Y to vectorB, captureZ, then FORCE B.Y=0 before two-word copy toA, including uninitialized B.pad; A.Y-=512 with U16 narrowing. Thus poses use signed widened B XYZ with Y0, loads use A Y-512, not capturedactorY. Table70count8. EIGHT58 resources6/4/5/3/2 with57/3D00/0/128, resources8/7 with59/3D01/0/128, and9 withfirstparameters. No provider-derived random scale unlike fb05800.

Frames<24: progressframe4096/24, cos(progress>>2), rotationXYZ0/-frame32/0, XZscale=cos/10+819,YhalfXZ. Weight4*(4096-progress) selects strict>12288/>8192/>4096 blends handle0->1/1->2/2->3/3->4. THEN XZscale multiplied5>>2 forhandle7 pose, Y unchanged; THREE independent cosine calls tint. Rotation local has THREEhalfwords (not four) and INTposition/scale threewords; provider read extent remains unresolved.

Frames>=24: XZ1228/Y614,posehandle4 and THREE sine tints at((frame-24)*4096/26)>>2; XZthen5/4 forhandle7 pose andTHREEseparate sine tints. Common XZ*=4/5 restores after rounding, retained despite later overwrite. Every frame240(frame,576,384,16,64,table[frame%23+1].a/.b). Do not normalize reverse scaling or negative-frame indices.

Frames>=24 thenblendhandle5->6 withXZ3584/Ycos(progress>>2)/4+1024,rotationY=-frame64,weight4096-progress; THREE cosine tintsONLYhandle5 atprogress>>1. Exact16 loadresource14 atA,nullableh22=220(0,32); exact1 loadresource1,nullableh22=220(0,64). Finish>=50 AFTERall work calls78/return1, no flag mutation or release. External nonzero flag bypasses work/terminal andreturns0.

7FC0 descriptor104; count fromhostbyte36 UNBOUNDED despite eight-entry arrays. Init captures1F8 position each actor; ONE sharedbase overwritten each iteration with -400-(RAND&127), final actor draw sets amplitude for all. Peractoroffset=-SIGNEDY from80(actor,0,vec). Unlike fb05800 amplitude doesnot multiply global scale. Count0 returns0 immediately without amplitude initialization; no array cap or peractor amplitude array added.

Frames<32 TWOword capturedposition copy in literal secondword-first order; cachecos function pointer then U16Y+=base*cos(frame32)>>12/204. Frames>=32 copyagain, U16Y+=peractoroffset*((frame-32)*1024)>>12 then U16Y+=base*sin((frame-32)*256)>>12 as TWO separate narrowing operations. Preserve cachedfunctionpointer and provider call order, negative/overflow arithmetic andpads.

Finish>=36 AFTERthe computed shift has been published to everyactor, then SECONDloop uses FRESH1F8/204 andreturns1. Fresh provider capture doesnot prove restoration to cached original position. No forcedcopyfrominit, camera writes, finish78, release or terminal backfill added. Semantic names match observed operations while original effect identity remains unresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Fixed negative z load twelve group inward spiral naming acceptance (BV-03/BV-08, P2):
FF9 d75624e39 adds two canonical ovl_13bd2000 names. Catalog
4,847 unit/symbol names, 239 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-fixed-negative-z-load-twelve-group-inward-spiral.json and Binviz
target/ff9-names-fixed-negative-z-load-twelve-group-inward-spiral/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 156/2,448 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor0. Init writes ONLYfirstthree lanes ofglobalSIGNEDfour-halfwordposition0/0/-1560, savescontext,loadresource30 anddiscardreturn. Fourthlane untouched. Othermodes returnframe>=64, no per-frame work/finish78/release or state allocated. Resource ID identified but original resource purpose unresolved.

77A0 descriptor0, all effect state in globals. Initcountdown0/contextsaved; FORTYEIGHT separate198 resource lookups in twelve groups: (3,18),(8,15),(21,16),(22,13),(24,14),(25,11),(26,12),(27,9),(28,10),(1,6),(4,7),(17,5). In each pair first record resolvesfirstresource and EACHofnextthree resolvessecond independently; calls not deduplicated.

Twelve groups ofFOUR20-byte records. Groupg angleorigin=-681*g; recordk angleorigin+192*k,radius16+16*k,delayg-(k-4)=g+4-k,life8. Init each actualrecord throughbyte offsets andmatchingwrappers. Capture THREE unsignedcenterlanes fromhostp2C+24,globaldeclaredTHREEhalfwords; no assumed fourthlane read extent or extra provider initializer.

Exactframe0 setscountdown32. Exact8 loadresource19 atcenter andnullableh22=128. Exact24 EACHactor countbyte drivesunboundedunsizedglobalpositionarray:200(actor,0,position),THREE loadsresources20/2/23 inthatorder, eachnullableh22=220(actor,128). Actorcount maychange whileloop runs; no snapshot/cap/release added.

While countdownNONZERO, predecrement thencontextflag512 aroundall48 records. DelayNONZERO decrements ONLY, doesnotdraw when it reaches0 thisupdate. Else lifeNONZERO angle-=64,radius-=2,life-=1 THENdraw114(resource,center,NULLrotation,(newradius<<8)+16384,0,1,newlife512,cos(newangle)*newradius>>12,sin(newangle)*newradius>>12). Literalradiusdecreases, not oldcomment outwardspiral. Life0endpoint STILLdrawn onlastactiveupdate; last recordstartsradius16andends0, othersstart32/48/64 andend16/32/48.

Draw order nestedgroup/record preserved, width dependsnewradius andfade newlife512, offsets explicitly passed aslasttwoarguments rather thanmutatingcenter. Afterallrecords clearcontextflag0. Negative externallysupplieddelay/life/countdown continue literalNONZERO arithmetic; no positiveonlyguard or extra cap. Ordinaryallrecords finishbefore32-countdown expires but globalstate remains external. Returnframe>=96 AFTERevents/countdownwork, no78 or cleanup. No missedframebackfill and sharedglobals remain unresolved lifecycle frontier.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Twenty rising pulse particles layered fade naming acceptance (BV-03/BV-08, P2):
FF9 979714da2 adds two canonical ovl_12a6c000 names. Catalog
4,849 unit/symbol names, 240 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-twenty-rising-pulse-particles-layered-fade.json and Binviz
target/ff9-names-twenty-rising-pulse-particles-layered-fade/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 376/2,300 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 seven-argument callback mode1 clears ONLY h0, returns0. Other modes compute a=frame<<6, scaleXY=((4096+(COS(a>>2)>>1))*w4)>>12, scaleZ1024; h14 subtracts unsigned h2 modulo16bits, ha increases62 modulo16bits. Position lanes sign-extend h8/ha/hc separately. Slot60 receives v10 rotation, INT position/scale, ent+74 handle. Three independent SIN(a>>1) calls yield tint=((sin>>2)+3072)>>5 minus128; no deduplication or arithmetic regrouping. Frame>=64 returns1 AFTER placement/tint, no entity cleanup; other modes/negative frames retained.

787C descriptor128 state/20 and24 outputs/callback in out5, leaves other slots untouched. Init zero phase/counter, save context, calls1CC then70(table,5), constructs FIVE handles in order resources7/11/5/10/2 through198/338. Handle resource2 is stored but not placed by this body; do not infer its future use. Main p28=(0,-8192,0), scalesXY819 Z3072, rotation(1024,0,0); second p48 initially same thenY-=20480, scales3072 andsame initial rotation.

Init v8 ONLYXYZ=0, TWOwordcopy into v18 includes uninitialized pad, then v18.Y-=4096 and stores THREE host position halfwords. v10 is NOT initialized before198(resource3)/1D4(&v10); retain that frontier. Nullable loaded object h22=128 andp7C saved, no synthesized initialization or cleanup. Twenty allocation ATTEMPTS via184(job+w10), failure skips writes only; wv409+160*i, h14=768*i, h2=16-2*i, ha=-8192-(-1024+320*i), XZ0, rotationX-1024/Y0. h2 producer is SIGNED but callback consumer UNSIGNED; preserve modular behavior including negative late values. Progression advances on failed allocations too.

Other modes read job.type as frame. ONLY phasew0==0 executes effect work: s=(t<<12)/60; secondrotationZ+=2 then60(handle7), p28.Y+=70;2E4(handle11,128), place/tint11 with((4096-s)>>5)-128. RotationZ TEMPORARILY subtractt*12 for handle5 placement/tint(s>>5)-128 thenrestore. CopyXYZ p28 to v10, placehandle10 with all scales=s/5+819. Flag256 around128(resource1,v10,NULL,2730,0,1,-1), thenflag0.

Draw two240 calls with t*10/t*7 andfixed other arguments, two11C layers resources8/9 with opposite t/3 screen shifts and final parameter -1 versus4096-s/2. col ONLYRGB132 initialized, fourthbyte untouched; sc ONLYXY=(t>>1)+4096 before248, rotationTHREE halfwords0; sc.Z assigned4096 AFTER248. Preserve uninitialized scaleZ/colourPad read frontier and all arities. Nullable object pointer first checks h30==-1 then clearsp7C, rechecksbefore writingh5C/h5E/h60 from h40/h42/h44; no position update inferred.

Frame>=60 finish AFTER all phase0 placement/draw/object work:180(job+w10),78(),return1 BEFORE common flagclear/counterincrement. Nonzero phase performs NO effect drawing/finish, onlyflag0 andw4++, returns0. Negative frame arithmetic, externally changed phase and borrowed object lifecycle remain literal. Callback termination64 is distinct from controller60. No count cap, missed-frame backfill, missing-handle release, padding cleanup, null guard change or provider replacement.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Twelve particle fan shared sine hooked object load naming acceptance (BV-03/BV-08, P2):
FF9 427bd4f61 adds two canonical ovl_12d42800 names. Catalog
4,851 unit/symbol names, 241 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-twelve-particle-fan-shared-sine-hooked-object-load.json and Binviz
target/ff9-names-twelve-particle-fan-shared-sine-hooked-object-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 684/1,852 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 ev1 phase0, loadresource4 at existing v0C, nullable h22=0, separate zeroargRAND calls set w4=(RAND&31)+39 andw8=(RAND&31)+32. Existing v0C/v14/v20 supplied by allocator/caller, no additional initializer. Other events phaseNONZERO returns0 immediately. Phase0 incrementsw4 by12 clamps only when>=321 to320; incrementsw8 by29 clamps when>=557 to557. Those values feed movement/rotation slots, not identified sprite sizes.

Phase0 t=4096 except frame<5 usesframe<<10, then(t*SHARED D_801e86CC)>>12. CallsD4(v0C,v20,TWOintlocal),E4(local,v14,w8),DC(v0C,v14,w4,v0C) in order. Drawing TWO114calls at NEW v0C usesresource14 and3, size t+2048, frame, mode1 versus2, fade-1/offset0/0. Separate198lookups retained. Negative frame/overflow frontiers preserved; no clamp on shared value or interpolation.

Callback checks nullable object h30==-1 andclears pointer AFTER motion/draw. SharedNONZERO returns0. SharedZERO still performs motion/draw first, then ifobj nonnull sets h38 fromobj.p24.h6 WITHOUTp24nullguard; phase=1,return1. Later phase1returns0 evenifsharedbecomesnonzero. Termination is shared-value-driven, not frame bounded; no object release, position write or implicit retry.

79B0 descriptor32/12particles/40recordsize/callbackout5; untouched descriptor slots preserved. Init phase0/count0, savesctx viaD_801e7E88.ctx, TWOword copieshost+38/+3C into packedp4/p8 including fourthhalfword, unsignedY-=512. 200(16,0,v0C),20C(16,v14),shared4096 thenunsignedrotationY+=2048. No invented coordinate initializer/provider semantics.

Phase0 oddframes allocate ONLYifcount<12; count incrementsBEFORE184 so failed allocation consumes anattempt. Successful particle copiesTWO v0C words includingpadding, targetXYZ storesX/Z/Y inliteralorder, headingY=v14.Y-768+((count&3)<<9), headingZ0, headingX=v14.X+((count>>2)*3<<7), halfwordnarrowingretained. Count startsat1 forfirstsuccessfulattempt, groups derivedfromattemptcount notsuccesscount. t>=62 transitionAFTERattempt tophase1/resetjob.frame=-1; continuesphase0idle until62 evenwhen12attemptscomplete. No lowerframebound/countcap added.

Phase1 t<8: b=t<<9,a4096,shared=SIN(t<<7); t>=8: a=SIN((t-8)<<7),b4096,shared0. Thena=3*a+1024. Fully initialize rotationvector(1024,0,0,1),flag32 around128(resource20,packedp4,rotation,a,0,1,b), clearflag0 then134(resource15,t). Shared isZERO atordinaryphase1frame0 aswellasfrom8 onward, so callback completion depends onhost scheduling; do not claim continuous fade or persistence throughall8frames.

Phase1 t>=16 transitionsAFTERdraw tophase2/resetframe-1 thenloadresources16/17/18 eachnullableh22=128, resource19nullablep14=globalhooktable/h12=25/h22=32/hC=84, resource21nullableh22=32. All usepackedp4 origin, no release/backfill andorder preserved. Phase2 t>=32 selfstoresphase2 andreturns1; no78 call. Unknownphasesreturn0, nocommoncounter. Job castObj16 readsresourceid+8 separatelyfromframe/allocatorarg+10. Sharedglobal context/scale and loaded-object/hook ownership unresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Four orbiting handles five staggered trails screen flash naming acceptance (BV-03/BV-08, P2):
FF9 72058d926 adds one canonical ovl_1084e000 names. Catalog
4,852 unit/symbol names, 242 scoped alias headers. One full own-unit
body reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-four-orbiting-handles-five-staggered-trails-screen-flash.json and Binviz
target/ff9-names-four-orbiting-handles-five-staggered-trails-screen-flash/. One full native object pair equal;
exact affected/scored namespace one. Pinned strict-relocation scores unchanged:
0 exact/1 partial, 0/2,868 code bytes, zero failures. One
installed WASM token comparison agrees; current source/header/catalog/object/
review bindings and five isolated committed paths audit.

Mode0 ONLYout0=452/return0. Init savecontext/70(globaltable,20), clear firstTENwords includingpad24 ONLY. loc8halfwordwrites all retained evenunused. FOUR58resource5 handles withliteralUVarguments; rg.angle=i*1024,radius256,h3C0,paduntouched;198/58 and1FC(0,v180[i]) interleavedforeach. ScaleINTthree819. FIVE58resource1handles, act0/cnt-1each. Separate1F0(0)/1F4(0),1FC(0,v160),overridev160.Y=-128,act[0]=1, onlyv140XYZ0, w13C/w1C0=0.

Initcenterh158=v160.X,h15C=v160.Z,h15A=v160.Y-w16C unsigned/signednarrowing literal. w148/w150=ORIGINALw168,w14C=w16C/3; THENw168=(old>>2)+(old+256). Trail0XYZ=center, yb=centerY. Othertrailpositions, fade, pos4/objects/targetvectors and padding uninitialized untilevents/activation; no blanketzeroing. Storedtransforms/extents and ownership remain original.

Exactframeevents0:t8=80/t0=16;8:t18=60/w13C16;16:t10=8;20:tc/t1C32, FOURseparateloadsresource2atindependentv180[0..3],t20=48;48:resources11/12/7 atv160 withEACHnullableh22=220(0,256);52:t4=16;55/56/57/58/59:cnt[4/3/2/1/0]=8. No missedframebackfill or deduplication. Allrgangles+=64 andw1C0+=16 onEVERYupdate, evenbeforeactivation/afterterminal threshold.

NONZEROtc predecrements thenyb=B4(v160.Y,centerY,tc<<7). NONZEROt0/t4 predecrement inTHATorder, maybothoverwritefade; cosine containsliteralleft/right12bitshifts. t8NONZERO predecrement thenfourposesXZ=center+cos/sin(angle)*(w168+fade)>>12, Y=yb-cos(512)*fade>>12,60(rgrecord,posrecord,scale,hnd). Hostp2C firstXYZ=pos0 truncatedhalfwords andh30=256/h32=rg0angle/h34=0. No implication thosehostfields are a camera API.

t20NONZERO predecrements, four loadedobject targetXYZ:center+cos/sin(i1024+w1C0)*(w168+fade+512)>>12, Y=pos0.Y; fourthpaduntouched. IfobjectNONZERO andh30==-1 clearhandle; ALWAYS1D8(handle,&target), INCLUDINGzero pointer afterdrop orfailedload. No extra nullguard/release. t10NONZERO predecrement then240(table8328[t10>>1]);t14NONZERO predecrement then240(table8348[t14]); ELSEframe>=21 randomcallandlow3bitzero armst14=4 WITHOUTdrawingnewflashuntilnextupdate. Randomcalls happenbeforeterminal92guard.

t1CNONZERO trail0.Y=yb, v=PREDECREMENTcount,n=v>>3; when(v&7)==0 andn>0 activateact[n] andcopytrail0XYZ. Ordinary32timer emitsindices4/3/2/1 atframes20/28/36/44, no n<5guard forexternally changedtimer. Thent1C--. t18NONZERO predecrement thenD4(ctx.p2C+84,&center,v140),forcev140X/Z0 ONLY. ForEACHactiveof5:60(transform,&trail,&w148,handle), cnt>0predecrement then26Ctint=(newcnt<<5)-256; reaches0deactivateAFTERplacement/tint. i0/w13CNONZERO additionalpredecrement tint overridescountertint evenifjustdeactivated. Negativecntnotdecremented unlikeliteralNONZEROother timers.

Terminalframe>=92 returns1 AFTERall timer/pose/random/trailwork BEFORElast240(10000-frame*4,608,256,32,240,576,256). Otherupdates drawlast240 thenreturn0. No78/release/restore callback, phase switch or semantic meteor identification. NegativeNONZEROtimers, unbounded tableindices/activation n, read-before-init/padding/provider null frontiers remain literal. Init returns0 viazero label withoutdrawing.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Six sliding curve ribbons twelve staggered sparks screen flash naming acceptance (BV-03/BV-08, P2):
FF9 2ac7ffecf adds one canonical ovl_13b8c000 names. Catalog
4,853 unit/symbol names, 243 scoped alias headers. One full own-unit
body reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-six-sliding-curve-ribbons-twelve-staggered-sparks-screen-flash.json and Binviz
target/ff9-names-six-sliding-curve-ribbons-twelve-staggered-sparks-screen-flash/. One full native object pair equal;
exact affected/scored namespace one. Pinned strict-relocation scores unchanged:
0 exact/1 partial, 0/1,964 code bytes, zero failures. One
installed WASM token comparison agrees; current source/header/catalog/object/
review bindings and five isolated committed paths audit.

Mode0 ONLYout0=20. Initw0/w4/w8=0, savecontext,200(0,0,pos). SIXglobal280-byte ribbon records:110(198(resource3),0,0,-1,4096,4096,1,record,0) thenlives4/window32/delay2*i. InitTWELVE20-byte sparks viaTWOseparateRAND_N2 calls(0,47)/(-16,16), delay3*j,lives2/frame0; exactorder preserved. Thenw0/w4=1; neverautomaticallyclearflagswhenrecordscomplete. No heapallocation/particlecallback inferred.

Update exact24 loadsresources1then2 atsavedposition, eachnullableh22=220(0,512); exact25 flashcount4. No missedframebackfill/objectcleanup. Ifw0NONZERO visitall6 records. OnlylivesNONZERO predecrementdelay thenifnegative draw; reachingzeroWAITstilldoesnotdraw, initialzero draws onfirstupdate. Negative delaykeepsdecreasing with noreset betweenlives; negative externallysuppliedlives retainsNONZERO behavior.

For eachdue ribbon build64 interpolated samples from17keyindices: segment0 uses2C4(16,*tp,vc) then2B0(vc,1024,va); segments1..15 use80(16,key[k],va); allsegments80(16,key[k+1],vb). EachsegmentfourBC(va,vb,m*1024,sample[k*4+m]), retainsfourthlane/providerreadextent. Tabletp+=17/toff+=68 afterEACHrecordincludinginactive; no hoisting/dedup/substitution.

Headerwritesrec0=rec2=32 thenrec1=0, pad3untouched. s0=4096 unlesswindow<16 thens0=window<<8. Copy33XYZ samplesstartingwindow intohalfwordrecord+4; wlane128 stored BEFOREZ; ppadvancewrapper preserved. Normalinitialwindow32 accesses sampleindex64 for j32 althoughonlyindices0..63 populatedthisbody, unsizedglobalextent/providerstate unresolved; no clamp, repair, endpoint extrapolation or initializer. Flag512 around110(resource3,0,0,jobframe,4096,s0,1,rec,0) thenflag0. AFTERdraw window-=4; ifnegative reset32/lives--. OrdinaryeachlifehasNINEwindows32,28,24,20,16,12,8,4,0, lastdraws0 fade, FOURlives=36draws perribbon; delay not rearmed.

Ifw4NONZERO resamplebothsparkcurves EVERYupdate evenwhenall12liveszero: TWOcurves ofSEVENkeys, SIXsegments each, fourBCsamples persegment, total48samples. Separate80 keycalls(vc/vd), BCm*1024. ForEACHspark livesNONZERO predecrementdelay, ifnegative flag512 then114(resource5,samples[startkey],NULL,16384,localframe,1,4096,randomoffset,0),clearflag0. Localframe++ AFTERdraw; >=6 rerandomizesstartkey/offset evenonFINALlife, resetslocalframe0,lives--. Ordinary2lives*6=12draws each; delay onlyinitial, no perlife rearm. RAND_N2 endpoint contract not newlyproven; numericalarguments preserved.

FlashNONZERO predecrement then100(ctx.p20,1,0,0,newcount<<6); ordinary4 draws192/128/64/0 beginningexact25. Returnsjobframe>=64 AFTERobjectevents/allribbon/spark/flashwork; no78/release or idle-counter update. Readextents ofprovider-writtenfourhalfwordvectors, boundarysample64/globalrecordlife andmultiplecontexts lifecycle remain unresolved andunchanged. Name recordsvisiblecurves/counts/flashbehavior only.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Four phase shrink spin blend hooked object load naming acceptance (BV-03/BV-08, P2):
FF9 63e663f20 adds one canonical ovl_10100000 names. Catalog
4,854 unit/symbol names, 244 scoped alias headers. One full own-unit
body reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-four-phase-shrink-spin-blend-hooked-object-load.json and Binviz
target/ff9-names-four-phase-shrink-spin-blend-hooked-object-load/. One full native object pair equal;
exact affected/scored namespace one. Pinned strict-relocation scores unchanged:
1 exact/0 partial, 2,084/2,084 code bytes, zero failures. One
installed WASM token comparison agrees; current source/header/catalog/object/
review bindings and five isolated committed paths audit.

Mode0 ONLYout0=40. Initglobalcontext/70(table,4), FOUR58 handlesresources3/1/25 with(186,15808,0,0), then2 with(184,15680,128,128), eachreturnSIGNEDs16->INT. Phase0/counter0, copyactorSIGNEDXYZ intop andq separately, thenp.Y-=415 with16bitnarrowing. Bothvectorfourthpadsuntouched, q isORIGINALactorposition not loweredp. No addedownedobjectstate/release.

Phase0 exactframe0 load7atp,nullablew14=globaltable/h12=10/hC=92/h22=220(0,32). Thesearedata/hookfields, no replacementABI. frame>=8 nextphase1/resetjobtype-1 AFTERloadcheck; ordinary9updates, no posework. Phase1 t=frame<<10,sine(t>>2), XYZscale=((4096-sine)*3584)>>12, rotationXYZ0, INTtranslationSIGNEDpXYZ. 2E4(handle3,2),60,268(handle3,0),26C(3,0,0,0); fourthlanes ofrotation/scale/translation remainuninitialized. frame>=4 nextphase2/reset-1 AFTERdraw;ordinary5updates shrinks3584towards0.

Phase2 fixedscale3584/rotationYframe*32/translationp; primary3 same2E4/60/268/26Csetup then60(handle2)/268(handle2,1). t=frame<<8 THEN>>1, THREE separateCOS(t) calls eachtint=(cos>>5)-128, no dedup. Screen240 firstarg=((4096-SIN(t))>>9)+frame*5, fixedrest(576,384,64,128,576,256). Otherhalfwordrotation/storeordering andparenthesization literal.

Phase2 frame>=16 transitionAFTERallposes/tint/240 tophase3/reset-1, THREEseparateloadsresources24/22/21 atLOWEREDp, eachnullableh22=220(0,32). Resource10loadedatORIGINALq,returnignored;100(ctx.w20+4,1,0,32,70). Ordinary17updates andno missedframeevent backfill. Providerobject lifecycle/value/read extents unchanged.

Phase3 fixedscale3584/rotation(0,512,0)/translationp. If frame<2:m=frame<<11,w8192;else m=((frame-2)<<12)/14,w4096 viawrapper. w_b=w-m. Strictw_b>4096 draws28C(handle3,handle1,w_b-4096),26C(handle3,-64each); ELSEdraw28C(handle1,handle25,w_b),26C(handle1,((w_b-2048)>>5)-128each). Ordinaryframe0/1 firstpair, frame2..12secondpair. Onlyexactframe1 load8atq nullableh22=220(0,32).

Phase3 terminalframe>=12 calls78/return1 AFTERblend/tint/optionalload butBEFOREcommoncounterincrement. The14-frame denominator isNOTreached tozero underordinary13updates; terminalweight4096-floor(10*4096/14)=1171 remains. Ordinaryphasecounts9/5/17/13, jobframe reset-1 reliesonhostincrement. Counter++ onotherupdates includingunknownphase; query/initnotincremented. Negative frames/overflow/sharedctx/provider arity/uninitializedfourthlanes remainliteral, no normalization/backfill/extra cleanup or inferred spellidentity.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Fixed negative z load per actor resource load naming acceptance (BV-03/BV-08, P2):
FF9 acb55711d adds two canonical ovl_13bb9800 names. Catalog
4,856 unit/symbol names, 245 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-fixed-negative-z-load-per-actor-resource-load.json and Binviz
target/ff9-names-fixed-negative-z-load-per-actor-resource-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 412/412 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor0/no state. InitglobalSIGNEDs16XYZ0/0/-1560 ONLY, fourthhalfworduntouched, savesG28context, lookupresource3/loadglobalvector/discardresult. Othermodesreturn1 ifframe>=64 else0, no perframework/78/release or objectstate. DistinctresourceID comparedto othermodules preserved.

77A0 descriptor0/no state. Init savesdifferentlocalcontextview atSAMEglobalidentity, for i0 whilei<LIVEcountbyte atctx.actors+36,200(i,0,unsizedglobalposition[i]) THEN198(job.id,1)/1D4thatposition, discardreturn. Countreloadedforeachcondition, no snapshot/dedup/nullguard/countcap/vectorinitializer or release. Globalfourhalfwordrecordproviderwriteextent unresolved, sourcekeepsallrecordrefs. Othermodesreturn1 ifframe>=48 else0; query/initreturns0. No updaterestore/78/scheduledevent inferred.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero resource load host position publish naming acceptance (BV-03/BV-08, P2):
FF9 59c57379a adds two canonical ovl_12f9f000 names. Catalog
4,858 unit/symbol names, 246 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-resource-load-host-position-publish.json and Binviz
target/ff9-names-actor-zero-resource-load-host-position-publish/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 400/400 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor12/stateobjpointer+THREEU16positionhalfwords. Init savesEfxCtxcontext,200(0,0,state.pos),198(job.id,1)/1D4atthatpos, saveobj includingNULL, ifnonnull h22=220(0,128). Positiondeclaresonly3halfwords althoughstatealigned12bytes; trailingpad/providerwrite/readextent notexpanded orinitialized. Othermodesreturnjob.type>=50; no updates/release/78/reload. Initreturns0. Actorzero behavior from literal200selector0, numericalobjectpurpose unresolved.

77EC descriptor8, query/init gotocommonzero, initONLYsaveG28context/no position initializer. Othermodes captureframeBEFOREprovider then1FC(16,stateU16pointer), copyEXACTTHREEhalfwordstoctx.p2C inXYZorder, returncapturedframe>=7 AFTERpublication. Fourthlane/padsremainprovidercontrolled, selector16 originalpurpose/cameratargetidentity notnewlyproved, nameonlyhostpositionpublication. Terminalupdate stillpublishes; no cachedframe reread, lowerbound/clamp/providerrewrite/extra nullguard or cleanup.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero resource four load host position publish naming acceptance (BV-03/BV-08, P2):
FF9 793de73dd adds two canonical ovl_12fcb800 names. Catalog
4,860 unit/symbol names, 247 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-resource-four-load-host-position-publish.json and Binviz
target/ff9-names-actor-zero-resource-four-load-host-position-publish/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 400/400 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor12; statepointerobj/THREEU16position/trailingalignmentpadding. Init savesEfxCtxcontext,200(0,0,pos),198(job.id,4)/1D4atpos, storesobjectevenNULL, nullableh22=220(0,128). Othermodesreturnjobtype>=30, DISTINCTfromresource1/50 variants. No per-framework, relookup, finish78/objectrelease/vectorinitializer/padexpansion, providerread/writeextent unresolved.

77EC descriptor8, query/initgotocommonzero, initONLYsavedG28context. Updatecapturesjobframe BEFORE1FC(16,state), copiesTHREEU16statehalfwords tohostp2C inXYZorder, returnscapturedframe>=7 AFTERpublication. No actor16/cameraidentity inference, fourthlane remainsprovidercontrolled, nullguard/bodygoto/readordering retained. Negativeframes/providerwrites/borrowedcontext ownership unchanged. Distinctselectors/framesnotnormalized.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero resource one load twelve frame position publish naming acceptance (BV-03/BV-08, P2):
FF9 ee9d43e38 adds two canonical ovl_1126d800 names. Catalog
4,862 unit/symbol names, 248 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-resource-one-load-twelve-frame-position-publish.json and Binviz
target/ff9-names-actor-zero-resource-one-load-twelve-frame-position-publish/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 400/400 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor12/stateobjpointer+THREEU16position. Init savedEfxCtxcontext,200(0,0,pos),loadresource1 via198/1D4, retainobjincludingNULL, nullableh22=220(0,128). Othermodesreturntype>=50, no updates/78/release/providerrewrite or additionalpositioninitializer. Fourthhalfword/trailingpadding providerreadextent notassumed safe or changed.

77EC descriptor8, initONLYsaveCtx2c context; no goto/revisedG28view. Updatecapturesjobtype into tBEFORE1FC(16,typedVec3hstate), thenTHREEU16x/y/zstores toctx.obj at+44, returnscapturedt>=12 AFTERpublication. Descriptor8 butdeclaredVec3hsize6/providerfourthhalfwordfrontier retained. Name identifieshostpositionpublication withoutprovingselector16target/camerameaning. Terminal12 DISTINCTfromframe7 copyvariant, query/initreturns0, no nullguard/clamp/restore/finish78 or otherstatework.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero two resource load immediate position publish naming acceptance (BV-03/BV-08, P2):
FF9 f1119a4f9 adds two canonical ovl_12ff9000 names. Catalog
4,864 unit/symbol names, 249 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-two-resource-load-immediate-position-publish.json and Binviz
target/ff9-names-actor-zero-two-resource-load-immediate-position-publish/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 496/496 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor16, stateTWOobjectpointers andTHREEU16positionhalfwords/trailingpadding. Init savecontext,200(0,0,pos),198/1D4resource4 then8 SAMEpos. BOTHloads happenBEFOREany h22 assignment; nullableobj0 gets220(0,128), thennullableobj1getsSEPARATE220(0,128), no dedup or earlier scaleassignment. BothpointersincludingNULLretained. Query/initgotozero/return0; othermodesreturnjob.frame>=0, ordinaryfirstupdateendsmodule. No providerrewrites, vectorinitializer/padexpansion/78/release/objecttracking.

784C descriptor8, query/initgotozero, initONLYsavedctx. Othermodes captureframeBEFORE1FC(16,stateU16pointer), copyTHREEhalfwordstoctx.p2C inXYZorder, returncapturedframe>=7 AFTERpublication. Fourthhalfword/providerwriteextent andselector16purpose unresolved; do not infercameratarget oractor16identity. Nullguard/order/bodygoto/signature unchanged, no cleanup/restore or lowerbound.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero resource four load twelve frame position variant naming acceptance (BV-03/BV-08, P2):
FF9 4b5e61c14 adds two canonical ovl_1129a000 names. Catalog
4,866 unit/symbol names, 250 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-resource-four-load-twelve-frame-position-variant.json and Binviz
target/ff9-names-actor-zero-resource-four-load-twelve-frame-position-variant/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 400/400 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor12, stateobjpointer+THREEU16positionhalfwords. Initcontextsaved,200(0,0,pos),loadresource4 via198/1D4, storeobjincludingNULL, nullableh22=220(0,128). Othermodesreturntype>=30 withno update/finish78/release or addedinitializers. Padding/readextent notexpanded or zeroed; resourcesnotrenumbered.

77EC descriptor8, initONLYsaveCtx2ccontext, no goto. Othermodes capturesjobtype BEFORE1FC(16,typedVec3hstate), copyexactU16x/y/ztoctx.obj at+44, returncapturedt>=12 AFTERpublication. Descriptor8/Vec3h6byteextent/frontier retained; fourthlane/providerpurpose unknown. Query/initreturns0, no addednullguard/restore/providerrewrite/78; terminal12 differs fromotherpublisher7variants.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero two resource load thirty frame twelve publish naming acceptance (BV-03/BV-08, P2):
FF9 59f8df146 adds two canonical ovl_112c7800 names. Catalog
4,868 unit/symbol names, 251 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-two-resource-load-thirty-frame-twelve-publish.json and Binviz
target/ff9-names-actor-zero-two-resource-load-thirty-frame-twelve-publish/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 496/496 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor16, stateTWOobjectpointers andFOURU16positionhalfwords. Init savedG20context/200(0,0,pos); TWOseparateloads4then8 atSAMEposition happenBEFOREany h22update; nullablea gets220(0,128), nullableb getsSEPARATE220(0,128). BothstoredpointersinclNULL, providerpositionread/writeextent notnormalized tothree. Othermodesreturntype>=30, query/init0, no tracking/finish78/release/extra fourthlane initializer.

784C descriptor8, initONLYsaveCtx2ccontext, othermodes capturejobtypeBEFORE1FC(16,typedVec3hstate), copyTHREEU16x/y/ztoctx.obj+44 inXYZorder, capturedt>=12 AFTERpublication. OriginalVec3h6extent versusdescriptor8 retained. No inferenceofcamerapurpose/actor16identity, nullguards/restore/frameclamp/finish78 or branchrewrite. Allborrowedcontext/objectfrontiers preserved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero resource seven load delayed second object naming acceptance (BV-03/BV-08, P2):
FF9 76b3b19ff adds two canonical ovl_11251000 names. Catalog
4,870 unit/symbol names, 252 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-resource-seven-load-delayed-second-object.json and Binviz
target/ff9-names-actor-zero-resource-seven-load-delayed-second-object/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 596/596 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor12, objpointer+THREEU16positionhalfwords. Init savedEfxCtxcontext/200(0,0,pos),198(job.id,7)/1D4atpos,storeobjincludingNULL, nullableh22=220(0,128). Othermodesreturntype>=120 withno update/release/78/relookup. Padding/thirdlaneproviderextent preserved; resource7/terminal120 distinctiveandunchanged.

77EC descriptor16,statea/bpointers+FOURU16pos. InitONLYsaveG20context/200(0,0,pos),load10/savea, nullableh22=220(0,128); b isUNINITIALIZEDbyinit. Othermodes capturet=jobtype, exactt5 lookup/load9 atSAMEsavedpos/saveb includingNULL/nullableh22=220(0,128). EVERYupdateAFTERoptional5load,134(198(job.id,1),t), thenreturnt>=50. Terminalupdate still134; no missedframebackfill/bnullinitializer/dedup/release/providerreplacement/78. Selector/resources/framevalues unchanged, originalframe-effect purpose unresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Slot selected two stage resource load buffer marker naming acceptance (BV-03/BV-08, P2):
FF9 4d4066a58 adds two canonical ovl_11437000 names. Catalog
4,872 unit/symbol names, 253 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-slot-selected-two-stage-resource-load-buffer-marker.json and Binviz
target/ff9-names-slot-selected-two-stage-resource-load-buffer-marker/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 516/516 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor20; initreadsSLOTfrom*out, writesstate.slot, savescontext BEFOREvalidation. Onlyslot>=LIVEcountbyte(ctx.info+36) invalid; negative slotpasses thisliteralguard. Invalid pathcallhelper(ctx,7,1) thenreturn1 BEFOREproviders/position/objectsetup. Validcallhelper(ctx,7,0), THREE2D8calls selectors4/6/7(job.id,-1) inthatorder,1FC(state.slot,statefourU16position),loadresource5 via198/1D4/storea inclNULL; nullableh22=220(slot,32). b isNOTinitializedhere; no cap/lowerbound/addedzeroing.

Othermodes capturejobtype, exact18 loadsresource8 atSAMEsavedposition/storeb inclNULL, nullableh22=220(slot,128); exact12callhelper(ctx,7,1). Returnt>=60 AFTERbotheventchecks. Descriptorquery/init0exceptinvalidinit1, no78/release/restore or missedframebackfill. Jobtype captured BEFOREpossibleproviders, sourceorderloadcheckthenmarkercheck preservedevenmutuallyexclusiveordinaryframes.

78F4 helperargumentsBuf10**/INTindex/INTvalue; dereferenceppONCE thenSTOREconvertedU8 at(*pp)->d[i], dataoffset16. Declaredd[1] doesNOTproveallocatedextent; signedindex/unboundedindex/valueconversion/nullfrontiers remainliteral. MainpassesCtxI* throughK&Rdeclaration, whichviewscontextfirstwordasbufferpointer despiteitslocalfirst12opaque bytes; no cast/typefix/layoutreconstruction/newboundarycheck. Markermeaning notnewlyproved asspecifichostflag orcompletionownership.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Slot selected short two stage load buffer marker naming acceptance (BV-03/BV-08, P2):
FF9 848662d75 adds two canonical ovl_131d3000 names. Catalog
4,874 unit/symbol names, 254 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-slot-selected-short-two-stage-load-buffer-marker.json and Binviz
target/ff9-names-slot-selected-short-two-stage-load-buffer-marker/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 516/516 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor20; initstate.slot=*out/savecontextbeforevalidation. InvalidONLYslot>=ctx.info.countbyte, helper(ctx,7,1)/return1; no lowerboundguard, negative remainsvalid. Validhelper(ctx,7,0),2D8 selectors6/7/8(job.id,-1),1FC(slot,fourhalfwordposition),load1/storea, nullableh22=220(slot,32). Thisvariants selectors6/7/8/resources1/2 differ from11437000, not normalized. Stateb remainsuninitializeduntilscheduledload.

Updatecapturesjobtype; exact15 loadsresource2 atSAMEsavedpos/storeb inclNULL/nullableh22=220(slot,32), unlikeother128scale variant. Exact10callhelper(ctx,7,1). Returnt>=40 AFTEReventchecks; loadcheckBEFOREmarkercheckinactualsource. No78/release/restore/missedframebackfill/vectorinitializer or datachange. Invalidinitreturns1 separatelyfromordinaryinit/query0.

78F4 VOIDhelper dereferencesBuf10** once, storesconvertedU8value atindirectbuffer+16+SIGNEDunboundedindex. d[1]extent/inheritedcontextfirstword-as-pointer/nullandnegativeindex/frontiers unresolved. MainCtxIfirst12bytesopaque whileK&RallowsCtxI*passed toBuf10**helper; preserve mismatch rather than invent typedcontext/header/providerABI. Sharedbuffer marker7value0/1 meaning observedonlyaswrites, no alleged allocation/resourcelifetime fix.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero delayed resource load flash position publish naming acceptance (BV-03/BV-08, P2):
FF9 6d5474db9 adds two canonical ovl_1302d000 names. Catalog
4,876 unit/symbol names, 255 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-delayed-resource-load-flash-position-publish.json and Binviz
target/ff9-names-actor-zero-delayed-resource-load-flash-position-publish/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 548/548 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor16, stateobjpointer/FOURU16position/INTlevel. Init savesCtx28context,200(0,0,pos),level128; obj isNOTinitializeduntilframe3. Othermodes capturet=jobtype, ctx.w28=128 and118(198(resource2),savedpos,t,1,-1) EVERYupdate BEFOREeventchecks. Exact3load1/saveobj inclNULL/nullableh22=220(0,128). Exact7call100(ctx.p20+4,1,level,level,level), readsCURRENTstatelevelthree times asliteralexpression. Returnt>=50 AFTERdraw/events.

Contextw28 isNOTcleared onany update includingterminal, preservepersistentflag128 frontier. No missedframebackfill, objnullinitializer, release/78, providerrewrite or assumedperframeeffectpurpose. Position4halfwordproviderextent kept, query/init0. Currentlevel mayexternallychange; no constantfold oflevel128 or snapshotintroduced.

7880 descriptor8/initONLYsavedG28Ctx; query/initgotozero. Othermodes capturejobframe BEFORE1FC(16,stateU16pointer), copyEXACTTHREEU16statehalfwordstoctx.p2C XYZ, returncapturedframe>=7 AFTERpublication. Fourthlane/providerwriteextent/selector16meaning unresolved; no cameratype/actor16identity inference, nullguard/orderchange/restore/78. Ownaddress7880 preserved viaalias.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Slot selected hooked two stage load buffer marker naming acceptance (BV-03/BV-08, P2):
FF9 a3871d7f9 adds two canonical ovl_10456000 names. Catalog
4,878 unit/symbol names, 256 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-slot-selected-hooked-two-stage-load-buffer-marker.json and Binviz
target/ff9-names-slot-selected-hooked-two-stage-load-buffer-marker/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 604/604 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor20; initreadsidx=*out/savesstate.idx/globalctx BEFOREvalidation. ONLYidx>=((Cnt79A8*)ctx.pc).b24 invalid, helper(ctx,7,1) thenreturn1; negativeindicespassliteralguard. Validhelper(ctx,7,0),2D8 calls1/5/7(job.id,-1),1FC(state.idx,TWOINTwordvectorstate8),loadresource8/saveobj inclNULL. Nullableobjw14=(INT*)(globalbytehooktable+idx*1440),h12=10,hC=144, NESTEDrepeatifobj beforeh22=220(idx,32). Preserve redundantguard/order andsignedunboundedtableoffset, no inferredtableextent.

Statep4 NOTinitializedbyinit. Othermodes capturejobtype; exact15load6 atSAMEsavedtwo-wordvector/savep4 inclNULL/nullableh22=220(idx,32). Exact10helper(ctx,7,1). Returnt>=50 AFTERloadcheck/markercheck; sourceorderliteral. No78/release/restore/missedframebackfill/vectorinitializer/hookcaller inference or automaticcleanup.

794C VOIDhelperBuf10**/SIGNEDINTi/INTv, dereferencepp thenU8store(*pp).d[i] atbuffer+16+i, no bound/nullguard. d[1]notallocatedextentproof. MainG14Ctx* passedthroughK&Rdeclaration interpretsctx.p0 asBuf10* whileheaderdeclaresG14Ctx0 first18padbytes+b12; unknownactualextent/alias/frontierretained. HeaderdoesNOTprovesafetyofbyte23/marker7orcompletebufferpurpose. No newtypecast/schema repair/provider admission; namesdescribeactualbytewrite/hookedloadbehavior.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Two derived key points anchor three load hook naming acceptance (BV-03/BV-08, P2):
FF9 4a6d7bdf7 adds two canonical ovl_1215d000 names. Catalog
4,880 unit/symbol names, 257 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-two-derived-key-points-anchor-three-load-hook.json and Binviz
target/ff9-names-two-derived-key-points-anchor-three-load-hook/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 680/680 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor32 althoughdeclaredSt20 hasobj/pad4/twofour-halfwordvectors ending24; preserveallocation/viewfrontier. Init savecontext,2C4(16,18,local32byte_m1) then2B0(m1,200,v8), SECONDindependent2C4(16,18,local32byte_m2) then2B0(m2,800,v10). v10computedbutnotusedlaterthisbody, cannotdedup/discardsecondquery. Loadresource1 atv8/saveobj inclNULL, nullableh22=220(16,128). Othermodes returntype>=40, no perframework/78/release/providerextent/extrainitializer changes.

7840 descriptor20, stateTHREEobjectpointers+THREEU16position/trailingpadding. Init copyanchorctx.p0C+56/+58/+60 intoXYZ FIRST, then savecontext. THREEseparateloads3/4/5 atSAMEpos/storeobj0/1/2includingNULL BEFOREhookconfiguration. Onlynullableobj0 cb=globalbytehooktable/h12=16/h0C=96; otherobjectsno h22setup or hook. Othermodesreturntype>=40. No addedscale, tracking/release/78, fourthlanecopy/providerextent repair/nullguard or descriptor filling.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Actor zero delayed load flash twelve frame publish naming acceptance (BV-03/BV-08, P2):
FF9 84b7c652d adds two canonical ovl_112fb800 names. Catalog
4,882 unit/symbol names, 258 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-actor-zero-delayed-load-flash-twelve-frame-publish.json and Binviz
target/ff9-names-actor-zero-delayed-load-flash-twelve-frame-publish/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 548/548 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor16/stateobj/FOURU16pos/INTlevel. Init savecontext/200(0,0,pos)/level128 ONLY; objNOTinitialized. UpdatecapturetypeBEFOREproviders,ctx.w28=128 then118(198(resource2),pos,t,1,-1) EVERYupdate. Exact3load1/saveobj inclNULL/nullableh22=220(0,128),exact7 100(ctx.p20+4,1,level,level,level); returnt>=50 AFTERallwork. w28 remains128 includingterminal, no clear/78/release/backfill/objectinitializer/dedup. Currentlevelexternalfrontier retained.

7880 descriptor8/typedVec3h6byteview; initONLYsaveCtx2c. UpdatecapturetypeBEFORE1FC(16,state), exactU16XYZstorestoctx.obj at+44, returncapturedt>=12 AFTERpublication. Fourthlane/providerextent/selector16meaning unknown, no actor16/cameratargetinference, lowerbound/nullguard/restore/providerrewrite/78. Query/init0; no goto introduced.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Slot selected delayed load shared position track marker naming acceptance (BV-03/BV-08, P2):
FF9 e3cf25683 adds two canonical ovl_103c4800 names. Catalog
4,884 unit/symbol names, 259 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-slot-selected-delayed-load-shared-position-track-marker.json and Binviz
target/ff9-names-slot-selected-delayed-load-shared-position-track-marker/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 664/664 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor28; init idx=*out/state.idx/savecontext BEFOREvalidation. ONLYidx>=ctx.pccountbyte+36 invalid, helper(ctx,7,1)/return1. Validhelper(ctx,7,0),2D8 selectors1/5/6(job.id,-1),200(state.idx,0,stateTWOintwordvec8),load7/savep0 inclNULL/nullableh22=220(idx,32). THEN20C(0,stateTWOintwordvec10),copyONLYsecondU16lane intoglobalsharedposition[1]. SharedXZ NOTinitializedhere; p4NOTinitializeduntilframe18, no addedclear/lowerbound/providerextent repair.

Updatecapturetype; exact18load8 atSAMEsavedvec8/savep4 inclNULL/nullableh22=220(idx,128). ForALLt>=18 nullablep4h30==-1 clearsreference, RECHECKp4 thenwriteshostpositionh5C/h5E/h60 fromCURRENTsharedU16XYZ, no snapshot. Atframe18 performsloadTHENtracking, terminal50stilltracks. Exact12helper(ctx,7,1) AFTERload/trackblocks. Returnt>=50; missed18mayconsumeuninitializedp4 ifhostjumpsframes, preserved no backfill/nullinitializer/78/release.

7988 VOIDhelper dereferenceBuf10**once, U8convertedvalue STOREbuffer+16+SIGNEDunboundedindex. d[1]notallocatedextentproof; mainG14Ctx* K&Rinterpretsctx.p0 asBuf10* despiteheaderG14Ctx0viewextent. Negativeindex/null/alias/bufferextentfrontiers unchanged. Marker7writes0/1 indicateobservedbytes only; sharedpositionexternalproducer/lifetimeunresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Host position resource six two origin four staged load naming acceptance (BV-03/BV-08, P2):
FF9 4f237442d adds two canonical ovl_135c8000 names. Catalog
4,886 unit/symbol names, 260 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-host-position-resource-six-two-origin-four-staged-load.json and Binviz
target/ff9-names-host-position-resource-six-two-origin-four-staged-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 728/728 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor12, objpointer+THREEU16position/trailingpad. Init saveEfxCtxcontext/200(16,0,pos),load6 via198/1D4/saveobj inclNULL/nullableh22=220(16,128). Othermodesreturntype>=60, no perframework/78/release/providerextent/vectorinitializer changes. Numericalselector16 treatedasunknownhostselector, nameonlyhostposition resource-load behavior.

77EC descriptor40/stateFOURobjectpointers/FOURU16vec/THREEU16pa+pad/THREEU16pb+trailingpad. Init saveG30Ctxcontext/1FC(16,vec), copyONLYXYZ vecintopa thenpb; pads/fourthlanes/objectpointers NOTinitialized. Twooriginsinitiallyequals butdistinctstoredcopies remainexternallymodifiable, no consolidation/snapshot/providerreadextent repair. Query/initgotozero returns0.

Updatecapturetype BEFOREswitch. Exact8 loadresource1 atpb/storeobj1,14 atpb/storeobj0,2 atpa/storeobj2; ALLTHREEloads occur BEFOREnullableh22assignments forobj1thenobj0 eachSEPARATE220(16,32). obj2getsNOh22assignment. Exact10load7 atpa/storeobj3 inclNULL, no h22assignment. Returnt>=90 AFTERevents, no78/release/backfill/nullinitializers/orderchanges/fourthlane stores. Per-object/providerborrowedlifecycle remains unresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Two resource derived vector track three origin hook naming acceptance (BV-03/BV-08, P2):
FF9 90e024ec7 adds two canonical ovl_101d3000 names. Catalog
4,888 unit/symbol names, 261 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-two-resource-derived-vector-track-three-origin-hook.json and Binviz
target/ff9-names-two-resource-derived-vector-track-three-origin-hook/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 792/792 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor32; init savedcontext/200(0,0,TWOintwordvec8)/84(16,0,TWOintwordvec10), load1 atvec10/savep0 thenload2 atvec10/savep4 inclNULL. ONLYAFTERbothloads D4(vec10,vec8,&THREEU16h18..h1C). No h22 setup/hooks or additionalvectorinit. D4fourthlane/providerread/writeextent unchanged. Othermodescachep4 intoo THENcapturejobtype; ifcachedo nonnull ando.h30==-1 clearsstatep4, RECHECKstatep4 thenwritescurrentstatep4U16position5C/5E/60 fromderivedh18/h1A/h1C. Terminalt>=40 AFTERtracking. Firstloadedobjectp0nottracked/released, no78/positionrecomputation/nullguardchange.

7890 descriptor36/stateTHREEobjectpointers/originalTHREEU16XYZ+pad/TWOwordsecondvector/THREEU16loweredorigin+tailpad. Init savedcontext/200(0,0,&hc)/1FC(0,vec14). CopytargetYfirst, Xsecond,Zthird fromoriginalXYZ thenunsignedtargetY-=200. Load3 atLOWEREDorigin,4 atSEPARATEvec14,5 atORIGINALhc-origin inthatorder; storep0/p4/p8includingNULL. AFTERallloads, nullablep0w14=globalINTtable/h12=13/hC=144. No otherhooks/scaleassignments/fourthlaneinitializer/providerextent repair. Othermodesreturntype>=40, no updates/78/release. Threeoriginsemantics fromdistinctvectors, notassertionallrepresentdifferentactors.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Hooked track delayed eased host vector offset naming acceptance (BV-03/BV-08, P2):
FF9 0bd7838ae adds two canonical ovl_ff37800 names. Catalog
4,890 unit/symbol names, 262 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-hooked-track-delayed-eased-host-vector-offset.json and Binviz
target/ff9-names-hooked-track-delayed-eased-host-vector-offset/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 920/920 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 readsAPIfromsavedglobalcontext. Computek=536870912/(len*len) UNCONDITIONALLYbeforekind switch, w0initial. Kind0 w=4096-(((268435456/len)*t)>>16);kind1=((k*(len-t))*(len-t))>>17;kind2 4096-(((k*t)*t)>>17);otherkindw0. ReturnliteralhostB4(handle,extra,w) RESULT, notrawweight. Signedmulorder/overflow/divide-byzero len0 INCLUDINGunknownkind, negative/out-of-rangeframes andnoclamp preserved. Comment0..4096 appliesonlyordinarydomain, no newrestriction/providersemanticproved.

783C descriptor48/fiveFOURU16vectors/objpointer/countdown. Init countdown0/savecontext,84(16,0,a4),200(0,0,aC),1F8(0,a14), load5ata4/saveobjinclNULL THEN D4(a4,aC,a24), nullableobjw14=globalbytehooktable/h12=46/hC=76. Positionproviderextent/fourthlanes savedas-is; shiftedvectora1CNOTinitializeduntilcountdown. No extraobjectscale/initializer/cleanup.

Updatecapturejobtype, nullableobjh30==-1 clearsreference, recheckthenSIGNEDhalfwordobject5C/5E/60 writesfromcurrentU16a24XYZ. Exact24countdown=t(24); countdownNONZEROpredecrement thenhelper(1,24,newcount,0,8192), returningHOSTB4result w. a1C.X=a14.X+(COS(SIGNEDa24.Y)*w>>12), a1C.Z=a14.Z+(SIN(SIGNEDa24.Y)*w>>12), a1C.Y=a14.Y-(w>>6), eachU16narrowing inXZ/Ystoreorder. 204(0,a1C). Fourthlanea1Cuntouched butproviderreadextent unresolved; no hand-written lerp replacement or signed-anglechange.

Ordinary24activeupdatescount23..0, finalhelper/offset/204 STILLrunatcount0 beforeinactivitynexttick. NegativeNONZEROcountdowncontinuesdecrementing, no positive-only guard. Returncapturedt>=50 AFTERobjecttrack/event/countdownwork, no78/vectorrestore/release/backfill/globalprovideradmission. Namehostvectoroffset avoidsunprovedcamera/actorparameter identity; a24derivedvectormeaning andborrowedobjectlifetime remainfrontiers.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Eased vertical shift cosine shake host vector restore naming acceptance (BV-03/BV-08, P2):
FF9 e10216147 adds two canonical ovl_12d8a800 names. Catalog
4,892 unit/symbol names, 263 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-eased-vertical-shift-cosine-shake-host-vector-restore.json and Binviz
target/ff9-names-eased-vertical-shift-cosine-shake-host-vector-restore/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 964/964 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 savedglobalcontextAPI, k=536870912/(len*len) UNCONDITIONALLYbeforekind switch/w0. Kind0 w4096-(((268435456/len)*t)>>16),kind1((k*(len-t))*(len-t))>>17,kind2 4096-(((k*t)*t)>>17,unknown0. CallsB4(handle,extra,w) withoutCreturnvalue (VOID). Mainconsumesit asINTthroughseparateTUextern; native register/result frontier preserved andverifiedobjects. No simplification toreturnweight/rawlerp, signedmulreassociation,len0guard includingunknownkind, positive-onlyrange restriction or clamp.

783C descriptor16: xU16/ySIGNEDs16/zU16/partlyopaquehalfwords/hU16at10. Init savedcontext,clearTHREEglobaltimers viaforloop,1F8(0,state),loadresources1THEN2 atstate/discardBOTHreturns; no h22setup/objectpointertracking/release. hNOTinitializedexplicitly. Query/initgotozero; providerread/writeextent beyondXYZ opaque unchanged.

Updatecaptureframe; exact0 armshakecount40 THENdropcount16,exact24 risecount16,exact40 FRESH1F8(0,state) then204(0,state). Afterevents dropNONZEROpredecrement/INT-consuminghelper(kind2,16,newcount,SIGNEDstatey,statey-800) storehU16; thenriseNONZEROpredecrement/helper(kind0,16,newcount,y-800,y) overwritesh. Sharedtimers/globalh producer andVOID/INTfrontier preserved, no assumptioncachedh alwaysinitialized ifevents skipped.

ShakeNONZEROpredecrement thenlocaldeclaredTHREEU16v: X=stateX+(COS(frame<<6)>>7),Y=stateh+(COS(frame<<8)>>5),Z=stateZ+(SEPARATECOS(frame<<6)>>7). Call204(0,v), fourthlane/providerreadextent unresolved; no dedupthirdCOScall. Ordinarydrop16 frames0..15/rise16frames24..39/shake40frames0..39; finalcount0update stillhelper/shake. Exact40 freshread/restore occursbeforetimerchecks soexternalNONZEROtimer canoverwriteit laterthatsameupdate, no unconditionalendstate claim.

Returnframe>=72 AFTERall events/timers/restore/shake; no78/backfill/extra vectorrestore/cleanup. NegativeNONZEROtimers decrementliteral andsignedy arithmetic/halfwordnarrows/order preserved. Namehostvectorrestore fromfresh1F8/204, no newlyprovedcamera/actoridentity or originalspelleffect.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Index selected offset paired load actor zero hook naming acceptance (BV-03/BV-08, P2):
FF9 2b42958ff adds two canonical ovl_122fb000 names. Catalog
4,894 unit/symbol names, 264 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-index-selected-offset-paired-load-actor-zero-hook.json and Binviz
target/ff9-names-index-selected-offset-paired-load-actor-zero-hook/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 808/808 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor28, TWOobjpointers/FOURU16origin/FOURU16offsetorigin/INTn. Initn=*out/savecontext/state.n BEFOREguard; ONLYn>=3 returns1. Negativeindices passguard andswitchNONEcase, leavingp0UNINITIALIZEDbeforelaternonnullcheck. Valid0/1/2:1FC(0,v8), copyONLYXYZtov10/fourthpadNOTcopied. Case0offsetX+100/Y-400/load2,case1X-50/Y-50/load3,case2Y-200/load4 atv10, U16narrowingretained. Allcases includingnegative thenload5atv10/storep4 inclNULL.

Afterload5, ifp0nonnull h22=220(0,128), thenifp4nonnull separate220(0,128). No NULLinitializer/nlowerbound/resource-arithmetic switchreplacement/vectorpadcopy/additionalguards or dedup. Othermodes returnjobtype>=40 withno per-framework/78/release/backfill; invalidinitreturns1, ordinaryquery/init0. Sourceownership/uninitializednegativeindexfrontier documented notfixed.

7910 descriptor12, objpointer+FOURU16position. Init savecontext/200(0,0,v),load1/savep0 inclNULL. Nullablep0p14=globalbytehooktable/h12=13/hC=80, REDUNDANTnestedifp0 beforeh22=220(0,128); explicitreturn0insideinit retained. Othermodesreturntype>=40. No impliedcallbackprototype/newproviderpurpose/objecttracking/78/release or stateinitializer, fourthlaneproviderextent unchanged. Actualhookfields/gates/storeorder preserved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Saved vector paired track anchor staged six load naming acceptance (BV-03/BV-08, P2):
FF9 a7a3e0f4c adds two canonical ovl_10df7800 names. Catalog
4,896 unit/symbol names, 265 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-saved-vector-paired-track-anchor-staged-six-load.json and Binviz
target/ff9-names-saved-vector-paired-track-anchor-staged-six-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,052/1,052 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor24; two object pointers and TWO FOUR-U16 vectors a(+8)/pos(+16). Init saves D_801E7B68, 200(16,0,a),20C(16,pos), loads3 then5 BOTH at a includingNULL. AFTERBOTHloads each nullable object gets separate220(16,128). Update capturesframe, checks each nullable h30==-1/clears/rechecks then writes current savedposXYZ into U16px/py/pz. Returnframe>=40 AFTERbothtracking. Fourthlane untouched; no provider requery, release/78, extent repair or dedup.

7918 descriptor40; EIGHT pointer slots with p14/p1C unused, THREEU16pos(+32)/tailpadding. Init copies ctx.posXYZ FIRST then savescontext, loads6->p0/7->p4/12->p8. AFTERallthree, nullablep4 p14=bytehooktableD_801E7B78,h12=6,hC=96. Exact8 loads1->pC/15->p10/16->p18 at same cachedposition includingNULL; return>=40 AFTERevents. No h22 assignment/tracking/release/backfill/initializer for untouched or later slots, no new callback prototype.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Derived key point binding live host vector track naming acceptance (BV-03/BV-08, P2):
FF9 72742c0fd adds two canonical ovl_128b7800 names. Catalog
4,898 unit/symbol names, 266 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-derived-key-point-binding-live-host-vector-track.json and Binviz
target/ff9-names-derived-key-point-binding-live-host-vector-track/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 636/636 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor20: obj, THREEU16a4/pad/THREEU16a0C/tailalignment. Init savecontext,200(0,0,a4),2C4(16,2,local32byte s32 matrix),2B0(matrix,400,a0C),load1 at a4 inclNULL; no h22. Update captures type, nullableobj h30==-1 clears/rechecks then POINTER BINDS obj.p2C=(Vec3h*)state.a0C, not an XYZ copy. Return!(type<40). Vec3h is THREEU16, lifetime/provider extent unresolved; no78/release/new initializer.

7830 descriptor20: object+THREEU16h4/h6/h8/pad+FOURU16vecc. Init savescontext,80(16,15,vecc),load3 there inclNULL, nullableh22=220(16,32). Update captures type BEFORE20C(16,&h4) EVERYupdate includingNULLobject/terminal; nullableobject h30==-1 clears/rechecks then copies REFRESHEDU16XYZ to h5C/E/60. Return>=40 AFTERprovider/tracking. No cached substitution, provider-suppressing nullguard, release/78 or extent repair.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Indexed random target handle motion shared target load naming acceptance (BV-03/BV-08, P2):
FF9 e3b682f5b adds two canonical ovl_12ccd800 names. Catalog
4,900 unit/symbol names, 267 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-indexed-random-target-handle-motion-shared-target-load.json and Binviz
target/ff9-names-indexed-random-target-handle-motion-shared-target-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 904/904 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor64. Init saves n=*out/context/state.n BEFOREONLYn>=16 reject; negative passes. Onlyn==0 calls70(globalbyteTable,16). 200(16,0,state), signedstartY=-2000; copy ctx.pos U16XYZ into signedtargeth8/ha/hC then separate rand_n(2000) additions to signedX/Z with narrowing. PublishTHREEtargetU16 to SINGLEsharedD_801e8764[3], overwritten eachinit. D4(start,target,&h10) writes opaque record, overwriteh10 with40(targetY-startY,startZ-targetZ) U16. ScaleZ/Y/X4096, INTposition signedstartXYZ, U16handle=338(198(job+8,11),1).

7704 update capturesframe; ift<6, BC(start,target,t<<10,signeds16v[4]), copyXYZ intoINTposition; no lowerbound/clamp. 320(handle,currentY) then31C(&h10,&INTposition,&scale,handle) EVERYupdate. t>=4 returns1 AFTERdraw, ONLYn==15 calls78. Ordinary0..4 weights0/1024/2048/3072/4096; continued5 uses5120; >=6 reusesposition. No discardedD4 opaque fields/tableinit for otherindices/release/newguard/provider arithmetic rewrite.

79B0 descriptor12, Ob22*+THREEU16vec/tailpad. Init copies sharedD8764XYZ FIRST then saves Ctx16context,load1 atcopy inclNULL/nullableh22=220(0,128). Othermodesreturnframe>=30 with no per-framework/78/release. Producer is own7704; shared latest-init target/ownership/lifetime remains frontier, no per-index table, padding repair or backfill.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Fixed negative z load per actor two scalar fades naming acceptance (BV-03/BV-08, P2):
FF9 cf8090b18 adds two canonical ovl_13be6000 names. Catalog
4,902 unit/symbol names, 268 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-fixed-negative-z-load-per-actor-two-scalar-fades.json and Binviz
target/ff9-names-fixed-negative-z-load-per-actor-two-scalar-fades/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,064/1,064 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor0/no state. Init ONLY globalSIGNEDs16XYZ=0/0/-1560 with fourthpad untouched, saveG28contextD7B74,load1 atD7B7C/discardobject. Othermodes return>=64; no78/release or new initialization.

77A0 descriptor0/globalFade2a,b and unsized HP8positions; actorcountbyte+36 LIVEreloaded eachloop test. Init a=b=0/savecontext; peractor200(i,0,cachedposition[i]),load4 there; nullableh22=220(i,512),h12=30,p14=first2040bytehooktable[i],hC=156 inthatorder. No caps/snapshot/padinitializers. Update captureframe; exact16 eachLIVEactor uses CACHEDposition,load5 nullable220(i,512)/h12=30/secondtable[i]/hC156 THENload6 nullableh22=220(0,512), constantZEROactor acrossloop deliberately retained.

Exact24 arma16/exact40 b16. Each NONZERO counter PREDECREMENTS then EACHLIVEactor2BC(i,a*8) or2BC(i,128-b*8). Both independently run/secondwins if externally overlap. Ordinarya24..39 sends120->0,b40..55 sends8->128. NegativeNONZERO arithmetic remains literal. Return>=56 AFTERevents/fades. No count snapshot/dedup/clamp/positionrefresh/78/release/flag/provider purpose inference.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Cosine dipped two history trail actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 c6cb7a7bb adds two canonical ovl_12250000 names. Catalog
4,904 unit/symbol names, 269 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-cosine-dipped-two-history-trail-actor-zero-load.json and Binviz
target/ff9-names-cosine-dipped-two-history-trail-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,064/1,064 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor116; signedp0/p8/m10 FOURlanes, U16h18 FOURlanes, INTv20 FOURlanes/scalethree, TWO U16rotation/INTposition history records eachwithunusedfourthlane, TWO U16handles. Init context,70(table,2),200(16,0,state),200(0,0,p8),80(16,23,m10),D4(m10,p8,h18),scales4096,h18X+=2048 U16narrowing,initialINTXYZ=m10signedXYZ,handles338(resource5,1) then58(resource12,151,15680,0,0). History/fourthlanes not initialized; no repair.

Update capturesframe; ifframe<8 (no lowerbound) ang=(frame<<12)/6,BC(state,p8,ang,locals16t[3]),INTXYZ=tXYZ, then Y-=cos(ang>>1)*200>>12. DrawHEAD60(h18,v20,scale,h70), then i0..1 ifframe>=i+1 draw cachedhs/hv with SAMEtrailhandleh72,268(h72,3),26C(h72,-(i<<4) repeatedRGB). OnlyXYZ shift olderhistory0->1 then current->0 AFTERdraw, fourthlanes retaincontents. frame>=6 calls78/return1 AFTERhistory. Ordinaryframe6 endpointweight4096, continuing7 uses4778 no clamp; no historybackfill/providerextent/release/change to provider argument order.

7A44 descriptor12; object+THREEU16pos/tailpad. Init context,200(0,0,pos),load4 inclNULL,nullableh22=220(0,128). Othermodesreturnframe>=30; no per-framework/78/release/initializer. Name reused for same observed actor-zero load behavior only.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Phase gated staged load three phase paired draw naming acceptance (BV-03/BV-08, P2):
FF9 6194bd9c9 adds two canonical ovl_116f6000 names. Catalog
4,906 unit/symbol names, 270 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-phase-gated-staged-load-three-phase-paired-draw.json and Binviz
target/ff9-names-phase-gated-staged-load-three-phase-paired-draw/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,248/1,248 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor24, phase/object/TWO signedFOURhalfwordpositions. Init savescontext/phase0,200(0,0,vec8),1FC(0,vec10),load1 atvec8 inclNULL/nullableh22=220(0,32),THENload9 atvec10 OVERWRITESsameobjectpointer with no h22. Update capturesframe; ANYnonzero phase returns0 immediately includingterminal. phase0 exact2 load2 atvec8 overwritespointer/nullable220(0,32); frame>=60 return1. Phase never advances here, retained externalphasegate/uninitializedpads/missedframe behavior. No release/backfill/78.

78A4 descriptor16; phase,counter,FOURs16vec. Init savescontext/phase0/counter0, no vecinit. Update initializes a/d/c/b ZERO,capturesn,2B8(16,1,vec) BEFOREphasework eventerminal/unknownphase. Phase0 a=(n<<12)/3,b512,d=cos(a>>2)>>1,c=((4096-a)>>1)+2048; n>=3 writesphase1/job.type=-1 THEN draws oldphase sample. Phase1 q=(n<<12)/9,c=((n&1)<<8)+4096,u=(q>>1)+2048,a=parity256+u,b=(q>>3)+512,d=c; >=9 phase2/job.type=-1 then draws. Phase2 q=(n<<12)/9,t=q>>2,b1024-t,a=sin(t),d=cos(t)+4096,c=SEPARATEcos(t)*3/2+4096; n>=9 returns1 AFTERquery/trig but BEFOREdraw/flag/counter.

Nonterminal/unknownphase rotXYZ=0/0/b with pad0/s16narrowing; ctx.w28=128 around128(resource11,vec,NULL,d/3,0,1,a/2) THEN128(resource12,vec,rot,c/2,0,1,a*2/3); clearflag0/counter++. Counter independent of job.type neverusedhere. Unknownphase drawszeros thenincrements; no trig dedup, clamp/phase repair/78/release/preterminal flagclear.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Same origin paired load variant delayed cosine swing naming acceptance (BV-03/BV-08, P2):
FF9 28e7493de adds two canonical ovl_10029000 names. Catalog
4,908 unit/symbol names, 271 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-same-origin-paired-load-variant-delayed-cosine-swing.json and Binviz
target/ff9-names-same-origin-paired-load-variant-delayed-cosine-swing/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,108/1,108 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor24 twoobjectpointers/TWO FOURU16vectors. Init savescontext,1FC(0,a8),200(0,0,a10); loads7 then8 BOTH at a8 inclNULL. a10 queriedbutunusedlater; nullableSECONDobjecth22=220(0,128),firstno h22. Othermodesreturn>=40, no tracking/release/78/providerquerydedup.

7828 descriptor36 twoobjects/THREEFOURU16vectors/SIGNEDvariant. Init statevariant=*out/savecontext BEFOREONLYvariant>=3 reject; negativepasses and laterindexes arrays negatively. 210(16) selects84(16,0,a8) else200(16,0,a8);1FC(0,a10),load10 ata10 inclNULL/nullable220(0,128). p0 and a18 NOTinitializeduntilframe10. Exact10load9 atGLOBALrowD_A0D0[variant],nullablew14=byteTable+variant*3168,h12=18,hC176,nestedredundantnullguard/h22=220(0,16).

Updates10..25 ONLY(t>=10 andt-10<16) w=((t-10)<<12)/15,BC(a10,a8,w,a18). Variant1 X-=cos(w>>1)*1200>>12 elsevariant2 X+=same; allvariants Y-=SEPARATEcos(w>>1)*600>>12, U16narrowing each. Nullablep0 h30==-1 clears;1D8(p0,a18) UNCONDITIONAL evenNULL AFTERclear. Name does not claim nullsafeprovider. Return>=40 AFTERevents. Retain separatecoscalls/negativevariant/raw hook extents/no eventbackfill/nullinitializers/release/78/clamp.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Derived vector y offset track four offset staggered load naming acceptance (BV-03/BV-08, P2):
FF9 e7c71f3c5 adds two canonical ovl_11f1a000 names. Catalog
4,910 unit/symbol names, 272 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-derived-vector-y-offset-track-four-offset-staggered-load.json and Binviz
target/ff9-names-derived-vector-y-offset-track-four-offset-staggered-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
1 exact/1 partial, 396/1,140 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor28 obj+FOURU16v4+THREEU16anchor/pad+THREEU16b/tailpad. Init savescontext,200(16,0,v4),copyhostanchorXYZ,load3 atv4 inclNULL THEN D4(v4,a,b)/bY-=1024 U16narrowing. Nullableobject updates h30==-1 clears/rechecks then currentbXYZ to h5C/E/60. Returncapturedtype>=70 AFTERtracking. No provider semantic reinterpretation/binding instead ofcopy/scaleassignment/requery/release/78/extentrepair.

7890 descriptor80 tenobjectpointers/THREEU16anchor+pad/FOUR FOURU16offsetpositions. Init copiesanchorXYZ BEFOREsavingcontext; fori0..3 copyONLYXYZ to b[i], mutateGLOBALRec4.frame=i<<2 (U16),localTHREEU16rotation0/(i<<10)/0,DC(anchor,rotation,rand()%400+1400,b[i]) with SIGNEDremainder/no normalization. Globalschedule.ids unchanged, fourthlanes/pointersuninitialized, own table data not functions. Preserve registerpin and unknown DC/provider extents.

Update capturesframe. Exact15 loads14 then13 atanchor/storeobj8/9inclNULL. THEN loopFOURrecords each readsCURRENTwritableframe/id, whenequals capturedframe loadsCURRENTid then13 at ownsavedb[i]/storeobj[i]/obj2[i]. Everyupdate134(198(resource1),frame) then134(198(resource2),frame), includingterminal. Return>=45 AFTERallloads/updates. Nomissedframebackfill/schedule snapshot/padinit/scales/tracking/release/78/newnullguard. Shared schedule externally mutable/ownership unresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Two history point trail discarded cosine actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 891d787dc adds two canonical ovl_12262000 names. Catalog
4,912 unit/symbol names, 273 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-two-history-point-trail-discarded-cosine-actor-zero-load.json and Binviz
target/ff9-names-two-history-point-trail-discarded-cosine-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,028/1,028 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor116: signedFOUR-halfwordp0/p8/m10, U16h18[4], INTv20[4], THREEscaleINT+pad, TWO U16hs[4]/INThv[4], U16handles. Init context,70(table,2),200(16,0,state),200(0,0,p8),80(16,23,m10),D4(m10,p8,h18),scales4096,h18X+=2048 U16,INTXYZ=m10signedXYZ,338(resource5,1)/58(resource12,151,15680,0,0). No history/fourthlane init.

Update capturedframe; frame<8 includingnegative ang=(frame<<12)/6,BC(state,p8,ang,locals16t[3]),INTXYZ=tXYZ THENcos(ang>>1) RESULTDISCARDED. Drawhead60(h18,v20,scale,h70); i0..1 frame>=i+1 draws cachedhs/hv with SAMEtrailhandleh72,268(h72,3),26C(h72,-(i<<4) RGB). ShiftONLYXYZhistory0->1 then current->0 AFTERdraw, fourthlanes unchanged. frame>=6 calls78/return1 AFTERhistory; ordinary6weight4096,continued7weight4778. No removingunusedcosquery/clamp/backfill/extentrepair/release/historyinitializers.

7A20 descriptor12 object+THREEU16pos/tailpad. Init savesEfxcontext,200(0,0,pos),load4 inclNULL/nullableh22=220(0,128). Othermodesreturn>=30 with no per-framework/78/release. Own copied body fullyread; actor-zero load name reused based on actual behavior.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Spinning cosine dipped three history trail actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 c7e9e25a4 adds two canonical ovl_12297800 names. Catalog
4,914 unit/symbol names, 274 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-spinning-cosine-dipped-three-history-trail-actor-zero-load.json and Binviz
target/ff9-names-spinning-cosine-dipped-three-history-trail-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,120/1,120 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor140; THREErotationU16fourlane/positionINTfourlane historyrecords. Init context,70(table,2),200(16,0,state),200(0,0,p8),80(16,23,m10),D4(m10,p8,h18),scales4096,h18X+=2048 U16,INTpositionXYZ=m10signedXYZ. U16head338(resource2,1),U16trail58(resource12,151,15680,0,0). Fourthlanes/history untouched/uninitialized.

Update frame<10 includingnegative ang=frame<<9,BC(state,p8,ang,locals16t[3]),INTXYZ=tXYZ,Y-=cos(ang>>1)*400>>12,h18X+=512 U16. Drawhead then i0..2 ifframe>=i+1 drawhs/hv sameh8A,268(handle,3),26C(handle,-(i<<4) RGB). ShiftONLYXYZ 1->2 then0->1 thencurrent->0 AFTERdraw, unusedfourthlanes retained. frame>=8 calls78/return1 AFTERallwork. Ordinary8weight4096 withNINTHspin increment; continuing9weight4608/tenthincrement. No clamp/historyinit/providerextentrepair/release/dedup.

7A7C descriptor12 object+THREEU16pos/tailpad. Init saveEfxcontext,200(0,0,pos),load1 inclNULL/nullableh22=220(0,128). Othermodesreturn>=30, no per-framework/78/release/backfill/padinit. Name describes actual actor-zero load, not resource/spell identity.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Directional vertical swing callback derived follower naming acceptance (BV-03/BV-08, P2):
FF9 e097eadae adds two canonical ovl_1086a000 names. Catalog
4,916 unit/symbol names, 275 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-directional-vertical-swing-callback-derived-follower.json and Binviz
target/ff9-names-directional-vertical-swing-callback-derived-follower/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,092/1,092 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 uses savedcontext; s=n<<8,BC(globalstart,globalend,s,signeds16out),s>>=1, outputY-=cos(s)*384>>12 then directionbyte+31==1 subtractsSEPARATEcos(s)<<9>>12, ==3 addsit. BOTHextra adjustments Y only (notX); signed-halfwordnarrowing, exactshiftorder unchanged. Writesobject INTXYZ=outSIGNEDXYZ<<12 BEFOREn<16?0:-1. Terminal16stillinterpolates at4096/offsets/writes; >16extrapolates; no lowerclamp/trigdedup/returntype or pin repair. Endpoint refreshed bymain updates, not assumedfixed.

785C descriptor36; phase,TWO two-INTwordvectors,THREEU16derivedpos/pad,TWOobjectpointers. Init savescontext/phase0/globaljob,200(0,0,v4),copiesBOTH32bitwords into globalstart. No object/derivedposinit. Everyupdatecapturesframe then210(16)==0 selects200(16,1,vc) else84(16,0,vc),copiesBOTHwords globalend BEFOREswitch/terminal. Phase0 writesphase1/job.frame=-1 thenload2 atglobalstart inclNULL; nullableh22=16/nestednullguard/unguardedinnerp0->hook=callback, gotozero. No p0nullguard added.

Phase1 ifframe<16 return0 afterendpointquery; otherwisephase2/job.frame=-1,load6 atvc inclNULL/nullableh22=220(16,24),THEND4(vc,v4,pos),return0. Phase2 nullablefollowerh30==-1 clears/rechecks/writecurrentderivedXYZ toobjectU16positions; exact6 loads1 atCURRENTvc inclNULL/nullable220(16,24),terminal>=44 AFTERtrack/event. Unknownphase query/publication then0. No78/release/backfill/requeryderivedpos/callbacksignature normalization.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Fixed negative z load per actor offset model tint naming acceptance (BV-03/BV-08, P2):
FF9 17366d09b adds two canonical ovl_13baa000 names. Catalog
4,918 unit/symbol names, 276 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-fixed-negative-z-load-per-actor-offset-model-tint.json and Binviz
target/ff9-names-fixed-negative-z-load-per-actor-offset-model-tint/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,048/1,048 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor0/no state. Init onlyglobalSIGNEDXYZ0/0/-1560/fourthpaduntouched,savecontext,load2 atglobalposition/discardobject. Othermodesreturn>=64,no78/release/newpadinitialization.

77A0 descriptor0/globalcounter/modelhandles/rotTHREEs16/scaleTHREEINT/wordpositions16byte/halfpositions8byte. Init counter0/savecontext/70(scratch,4); EACHLIVEcountbyte+36actor, INThandle=58(resource12,375,65535,0,128),200(i,0,halfposition), copySIGNEDXYZ toINTwordposition with Z+256 FIRST then U16alias halfZ+=512. No padsinitialized/cap/handlenullguard/countsnapshot. AFTERactorloop globalrot0/2048/0,scale6144each.

Updatecapturedframe; exact0 sets counter32 (do-while0 wrapper), EACHLIVEactor load10 THEN5 atOFFSETcachedhalfposition/discardboth. NONZEROcounter predecrements; EACHLIVEactor60(globalrot,wordposition,scale,INThandle). Ifcounter>=25 separate26C eachRGB=(24-counter)*16; ifcounter<8 separate26C eachRGB=(counter-8)*16; no calls in8..24. Ordinaryt0..6 firstvalues-112..-16, t24..31 lastvalues-16..-128, NONEatmiddlecounter24..8; no addedzero/reset, negativecounters retainliteralbehavior. 134(resource11,capturedframe) ONLYinsideNONZEROgate AFTERactorloop evenzeroactors. Return>=48 afterallwork, no78/release/refreshpositions/flag/clamp/dedup/omittedpadrepair.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Large scale cosine dipped three history trail actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 dfeb3645b adds two canonical ovl_12273800 names. Catalog
4,920 unit/symbol names, 277 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-large-scale-cosine-dipped-three-history-trail-actor-zero-load.json and Binviz
target/ff9-names-large-scale-cosine-dipped-three-history-trail-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,096/1,096 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor140 THREE U16fourlane rotation/INTfourlane position histories. Init savesD7B94,70(D7BA4,2),200(16,0,state),200(0,0,p8),80(16,23,m10),D4(p8,m10,h18) REVERSEDargumentorder versus otherfamily variants. Scale6144each, no h18Xoffset/spin. InitialINTXYZ=SIGNEDm10XYZ. U16head338(resource1,1),trail58(resource12,151,15680,0,0). Full signedp0/p8/m10/U16h18/INTv20 fourlaneviews retainunusedlanes.

Update capturedframe; frame<10 includingnegative ang=frame<<9,BC(state,p8,ang,locals16t[3]),INTXYZ=tXYZ then Y-=cos(ang>>1)*400>>12. THREEhistorysamples tint-(i<<4); terminalframe>=8 AFTERallwork. Ordinary8endpoint4096; continued9weight4608; no additional rotationincrement. Head60 precedes historyloop; for each history i ifframe>=i+1,60(rotation[i],position[i],scale,SAMEtrailhandle),268(trailhandle,3),26C(trailhandle,negative repeatedRGB). ONLYXYZ history shifts from oldest highindex down then currentXYZ->history0 AFTERdraw; FOURTHlanes/pads/history not initialized byinit or shifted. Terminal78/return1 occurs AFTERhead/historydraw/shift; no release/clamp/eventbackfill/padinitializer/historyrepair/trig/providerquery dedup.

Secondhandler descriptor12 objectpointer+THREEU16position/tailpad; init savesEfxCtxcontext,200(0,0,pos),loadresource5 inclNULL/nullableh22=220(0,128). Othermodesreturncapturedjobtype>=30 with no tracking/release/78/fourthlaneinit. Originalfilename/commentaliases remain; own copied body fullyread.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Six frame cosine dipped three history trail actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 478efd9af adds two canonical ovl_1223e000 names. Catalog
4,922 unit/symbol names, 278 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-six-frame-cosine-dipped-three-history-trail-actor-zero-load.json and Binviz
target/ff9-names-six-frame-cosine-dipped-three-history-trail-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,128/1,128 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor140 THREE U16fourlane rotation/INTfourlane position histories. Init savesD7BB4,70(D7BC4,2),200(16,0,state),200(0,0,p8),80(16,23,m10),D4(m10,p8,h18),scale4096each,h18X+=2048 U16narrowing. InitialINTXYZ=SIGNEDm10XYZ. U16head338(resource1,1),trail58(resource12,151,15680,0,0). Full signedp0/p8/m10/U16h18/INTv20 fourlaneviews retainunusedlanes.

Update capturedframe; frame<8 includingnegative ang=(frame<<12)/6,BC(state,p8,ang,locals16t[3]),INTXYZ=tXYZ then Y-=cos(ang>>1)*200>>12. THREEhistorysamples tint-(i<<4); terminalframe>=6 AFTERallwork. Ordinary6endpoint4096; continued7weight4778; no per-update spin. Head60 precedes historyloop; for each history i ifframe>=i+1,60(rotation[i],position[i],scale,SAMEtrailhandle),268(trailhandle,3),26C(trailhandle,negative repeatedRGB). ONLYXYZ history shifts from oldest highindex down then currentXYZ->history0 AFTERdraw; FOURTHlanes/pads/history not initialized byinit or shifted. Terminal78/return1 occurs AFTERhead/historydraw/shift; no release/clamp/eventbackfill/padinitializer/historyrepair/trig/providerquery dedup.

Secondhandler descriptor12 objectpointer+THREEU16position/tailpad; init savesEfxCtxcontext,200(0,0,pos),loadresource5 inclNULL/nullableh22=220(0,128). Othermodesreturncapturedjobtype>=30 with no tracking/release/78/fourthlaneinit. Originalfilename/commentaliases remain; own copied body fullyread.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Spinning resource four three history trail actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 c6760b349 adds two canonical ovl_12285800 names. Catalog
4,924 unit/symbol names, 279 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-spinning-resource-four-three-history-trail-actor-zero-load.json and Binviz
target/ff9-names-spinning-resource-four-three-history-trail-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,120/1,120 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor140 THREE U16fourlane rotation/INTfourlane position histories. Init savesD7BAC,70(D7BBC,2),200(16,0,state),200(0,0,p8),80(16,23,m10),D4(m10,p8,h18),scale4096each,h18X+=2048 U16narrowing. InitialINTXYZ=SIGNEDm10XYZ. U16head338(resource4,1),trail58(resource12,151,15680,0,0). Full signedp0/p8/m10/U16h18/INTv20 fourlaneviews retainunusedlanes.

Update capturedframe; frame<10 includingnegative ang=frame<<9,BC(state,p8,ang,locals16t[3]),INTXYZ=tXYZ then Y-=cos(ang>>1)*400>>12 andh18X+=512 U16. THREEhistorysamples tint-(i<<4); terminal>=8 AFTERallwork. Ordinary8endpoint4096/NINTHspin increment; continued9weight4608/tenthincrement. Head60 precedes historyloop; for each history i ifframe>=i+1,60(rotation[i],position[i],scale,SAMEtrailhandle),268(trailhandle,3),26C(trailhandle,negative repeatedRGB). ONLYXYZ history shifts from oldest highindex down then currentXYZ->history0 AFTERdraw; FOURTHlanes/pads/history not initialized byinit or shifted. Terminal78/return1 occurs AFTERhead/historydraw/shift; no release/clamp/eventbackfill/padinitializer/historyrepair/trig/providerquery dedup.

Secondhandler descriptor12 objectpointer+THREEU16position/tailpad; init savesEfxCtxcontext,200(0,0,pos),loadresource5 inclNULL/nullableh22=220(0,128). Othermodesreturncapturedjobtype>=30 with no tracking/release/78/fourthlaneinit. Originalfilename/commentaliases remain; own copied body fullyread.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Small scale four history point trail actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 e36e7f3ce adds two canonical ovl_12396800 names. Catalog
4,926 unit/symbol names, 280 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-small-scale-four-history-point-trail-actor-zero-load.json and Binviz
target/ff9-names-small-scale-four-history-point-trail-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,132/1,132 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor164; all Vec3hFOURU16 includingpad/Vec3wFOURINT includingpad; FOURhistoryrotations/positions. Init savesD7BB8,70(D7BC8,2),200(16,0,v0),200(0,0,v8),80(16,23,v10),D4(v10,v8,v18),scaleZ/Y/X=256,v18X+=2048 U16. InitialINTXYZ usesEXPLICITsigneds16casts fromU16v10XYZ. TWOSEPARATE338(198(resource5),1) create headha0/trailha2, no dedup even same resource/service.

Update capturedtype; type<8 includingnegative s=(type<<12)/6,BC(state,v8,s,locals16l18[4]),INTXYZ=l18XYZ thencos(s>>1) RESULTDISCARDED. No Ydip or spin. FOURhistorysamples use -(i*20) RGB (not16). Terminaltype>=6 AFTERallwork despitecomment5steps, ordinary6endpoint4096/continued7weight4778. Head60 precedes historyloop; for each history i ifframe>=i+1,60(rotation[i],position[i],scale,SAMEtrailhandle),268(trailhandle,3),26C(trailhandle,negative repeatedRGB). ONLYXYZ history shifts from oldest highindex down then currentXYZ->history0 AFTERdraw; FOURTHlanes/pads/history not initialized byinit or shifted. Terminal78/return1 occurs AFTERhead/historydraw/shift; no release/clamp/eventbackfill/padinitializer/historyrepair/trig/providerquery dedup.

7A88 descriptor12 objectpointer+THREEU16position/tailpad. Init saveEfxcontext,200(0,0,pos),load4 inclNULL/nullableh22=220(0,128). Othermodesreturn>=30; no tracking/release/78/newpadinit. Own copied body reviewed directly.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Spinning cosine dipped four history trail actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 b4bc9a796 adds two canonical ovl_1222c000 names. Catalog
4,928 unit/symbol names, 281 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-spinning-cosine-dipped-four-history-trail-actor-zero-load.json and Binviz
target/ff9-names-spinning-cosine-dipped-four-history-trail-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,160/1,160 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor164; FOUR U16fourlane rotations/INTfourlane positions inhistory. Init context,70(D7BE4,2),200(16,0,state),200(0,0,p8),80(16,23,m10),D4(m10,p8,h18),scale4096each,h18X+=2048 U16,initialINTXYZ=SIGNEDm10XYZ. U16head338(resource1,1),trail58(resource12,151,15680,0,0); no history/pad/fourthlane initialization.

Update frame<10 includingnegative ang=frame<<9,BC(state,p8,ang,locals16t[3]),INTXYZ=tXYZ,Y-=cos(ang>>1)*400>>12,h18X+=512 U16. Head60 then FOURhistorysamples ifframe>=i+1:60(hs/hv,scale,SAMEtrailhandle),268(handle,3),26C(handle,-(i<<4) RGB). ONLYXYZ shifts2->3,1->2,0->1,current->0 AFTERdraw. frame>=8 calls78/return1 AFTERdraw/history, ordinary8weight4096/NINTHspin increment, continued9weight4608/tenthincrement. No clamp/extentrepair/release/historybackfill.

7AA4 descriptor12 obj+THREEU16pos/tailpad. Init saveEfxcontext,200(0,0,pos),load5 inclNULL/nullableh22=220(0,128). Othermodesreturn>=30, no tracking/release/78/padinit. Own canonical representative fullyread; original resource identity unresolved.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Resource six four history point trail actor zero load naming acceptance (BV-03/BV-08, P2):
FF9 3c420638f adds two canonical ovl_122a9000 names. Catalog
4,930 unit/symbol names, 282 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-resource-six-four-history-point-trail-actor-zero-load.json and Binviz
target/ff9-names-resource-six-four-history-point-trail-actor-zero-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,124/1,124 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor164; FOURU16rotation/INTposition historyrecords. Init context,70(D7BC0,2),200(16,0,v0),200(0,0,v8),80(16,23,v10),D4(v10,v8,v18),scaleZ/Y/X4096,v18X+=2048 U16,initialINTXYZ=EXPLICITs16casts fromU16v10. U16head338(resource6,1),trail58(resource12,151,15680,0,0), no history/fourthlaneinit.

Update type<10 includingnegative s=type<<9,BC(state,v8,s,locals16l18[4]),INTXYZ=l18XYZ,cos(s>>1) RESULTDISCARDED. No Ydip/spin. Head60 then FOURhistorysamples ifi+1<=type:60(rot/pos,scale,SAMEtrailhandle),268(handle,3),26C(handle,-(i*20) RGB). ONLYXYZshift2->3,1->2,0->1,current->0 AFTERdraw. type>=8 calls78/return1 AFTERallwork, ordinary8weight4096/continued9weight4608. No removingunusedcosquery/clamp/padinit/release/historybackfill.

7A80 descriptor12 object+THREEU16pos/tailpad. Init saveEfxcontext,200(0,0,pos),load5 inclNULL/nullableh22=220(0,128). Othermodesreturn>=30; no per-framework/78/release/newinitializer. Own transplanted body fullyread.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Five staged load hooks saved binding three load naming acceptance (BV-03/BV-08, P2):
FF9 f8443915b adds two canonical ovl_11dc5000 names. Catalog
4,932 unit/symbol names, 283 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-five-staged-load-hooks-saved-binding-three-load.json and Binviz
target/ff9-names-five-staged-load-hooks-saved-binding-three-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,236/1,236 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor44 fiveobjects/THREEsignedFOURhalfwordvectors. Init ONLYcontext/200(0,0,v14); objslots/v1C/v24 not initialized. Exact0 84(16,0,v1C),load1->obj0 inclNULL/noscale. Exact38 84(16,0,v24),load6/7/5/2 intoobj1/2/3/4 ALLbefore FOURnullable separate220(16,128) h22assignments in1/2/3/4order. THENnullableobj1 firstbytehooktable/h12=28/hC156;obj2 secondtable/same28/156;obj4 thirdtable/h12=58/hC112. No hookonobj0/3.

ForEVERYframe>=38, includingexact38/terminal80, nullableobj4 h30==-1 clears/rechecks then POINTERBINDSw2C=state.v14, no XYZ copy/requery. Returncapturedframe>=80 AFTERevents/binding. Missed38 may consume uninitializedobj4, no backfill/newnullinitializer/78/release/providerextent repair.

7A60 descriptor20 THREEobjectpointers+FOURsignedhalfwordvec. Init context/1FC(0,vec),load3->obj0 THEN4->obj2 inclNULL BOTHbefore nullableh22assignments0then2 separate220(0,128). obj1 untoucheduntil exact4 load21->obj1 atsamevec inclNULL/noh22. Return>=60 AFTERevent. No tracking/release/78/providerclearinference/newpads/backfill.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Staged hooked live binding derived track indexed load naming acceptance (BV-03/BV-08, P2):
FF9 651aab83b adds two canonical ovl_1030e800 names. Catalog
4,934 unit/symbol names, 284 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-staged-hooked-live-binding-derived-track-indexed-load.json and Binviz
target/ff9-names-staged-hooked-live-binding-derived-track-indexed-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,096/1,096 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor40 obj0/4+THREEU16v8/pad+THREEU16v10/pad+TWOINTvec18+THREEU16v20/tailpad. Init copiesanchorX/Y fromctx.pc+56/58, SAVEScontext BETWEENY/Z, thenZ+60. Noobj/vectorinit. Exact10load2 atv8 inclNULL/nullablew14=firstINTtable/h12=130/hC24. Exact40 84(16,0,v10),publishTHREEU16to sharedD7B9C,load3 at PREVIOUSvec18 BEFORElaterrefresh (includingunknowninit ifframesmissed),nullablew14=secondtable/h12=160/hC24,THEND4(v10,v8,v20).

EVERYt>=10 84(16,0,vec18) AFTERswitch includingNULLobj/terminal; nullablep0 h30==-1 clears/rechecks POINTERBINDS p2C=vec18. EVERYt>=40 nullablep4 h30==-1 clears/rechecks COPYScurrentderivedU16XYZ to h5C/E/60. Returncapturedt>=100 AFTERproviders/events/tracking. No h22/78/release/querydedup/earlyrefresh/providerextent/backfill/newinitializer; sharedXYZ publishedONLYexact40.

79B8 descriptor24 obj/TWOINTvec4/THREEU16derivedhc/he/h10+pad/SIGNEDidx. Init idx=*out/storeidx/savecontext BEFOREONLYidx>=LIVEcountbytectx.pc+36 reject; negativepasses.1FC(idx,vec4),U16aliasY-=200,load1 inclNULL/nullableh22=220(STOREDidx,128),THEND4(sharedINTdeclD7B9C,vec4,&hc). Sharedsource maypreexist/load40producerordering external; no new scheduling guarantee/fourthlaneinit/extentrepair. Update cachesobjBEFOREcapturesjobtype, nullablecachedobj h30==-1 clearsstate/rechecksstate thenCOPYXYZ; return>=60 AFTERtrack. No requery/78/release/invalidinitclear.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.

Typed staged live binding shared derived indexed load naming acceptance (BV-03/BV-08, P2):
FF9 31827d1e9 adds two canonical ovl_11186000 names. Catalog
4,936 unit/symbol names, 285 scoped alias headers. Two full own-unit
bodies reviewed and bound, zero deferrals; other units pending. Included headers
and any reused prior header-review report hashes bound. Evidence:
docs/function-names-typed-staged-live-binding-shared-derived-indexed-load.json and Binviz
target/ff9-names-typed-staged-live-binding-shared-derived-indexed-load/. Two full native object pairs equal;
exact affected/scored namespace two. Pinned strict-relocation scores unchanged:
2 exact/0 partial, 1,096/1,096 code bytes, zero failures. Two
installed WASM token comparisons agree; current source/header/catalog/object/
review bindings and six isolated committed paths audit.

7704 descriptor40 TWOobjects/FOUR THREEU16vectors withpads. Initcopyctx.sys.v38XYZ ALLbeforecontextsave, noobj/othervectorinit. Exact10load2 atv8 inclNULL/nullablehookbyteD7BA4/h12=130/hC24. Exact40 84(16,0,v10),sharedXYZpublish,load1 atPREVIOUSv18 BEFORElaterrefresh,nullableD87D4/h12=160/hC24,THEND4(v10,v8,v20). Everyt>=10 84(16,0,v18) AFTERswitch/nullableobj0 h30==-1 clears/rechecks POINTERBINDw2C=&v18. Everyt>=40 nullableobj1 clears/rechecks thenCOPYderivedXYZ toh5C/E/60. Return>=100 AFTERwork, no h22/78/release/earlyrefresh/querydedup/backfill/providerextent repair.

79B8 descriptor24 object/THREEU16pos+pad/THREEU16derivedv+pad/SIGNEDn. Init n=*out/storen/savecontext BEFOREONLYn>=LIVEcountbyte reject; negativepasses.1FC(n,pos),U16Y-=200,load3 inclNULL/nullableh22=220(STOREDn,128),THEND4(sharedbyteDeclD7B9C,pos,v). Updatecapturedtype,nullableobjecth30==-1 clear/recheck COPYcurrentvXYZ,return>=60 AFTERtrack. Sharedpoint exact40producer/lifetime/ordering external; no extent/initialization repair/requery/release/78/invalidinitclear.

Definitions, declarations and applicable direct calls/callback references
propagate; scoped aliases preserve linker/address/runtime identities. Original
effect/resource/provider identities unresolved. All types/layouts/bodies/pads/
flags stay. Maintained naming/farm/scorer/preprocessor and thin adapters reused;
existing BV-03 reviewed deferral prioritization/BV-06 provider proposals retained.
No new tooling implementation/private ownership/type/CFG walker, matching gain,
provider admission, linked-image/gameplay/workspace acceptance. Refresh source-
bound evidence, pin historical tools/reports and preserve foreign work/index;
full-tree naming goal active.
