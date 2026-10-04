# Binviz decompilation tooling: implementation handoff

Requested outcome: move repeated analysis out of per-game scripts and into
Binviz, with the same inspectable results available to people and agents.
Detailed requirements and observed FF9 cases are in
[the full spec](decompilation-tooling-spec.md).

For current usage, start with the
[recommended workflow](decompilation-workspaces.md#recommended-workflow).
It covers executable/profile selection, cached stages, proof reinspection,
publication, progress export and focused validation. The delivery sections below
retain the implementation history; use the latest checkpoint for current scope.

## First delivery: inspect a caller and its blockers

Start with existing xrefs, contracts, compiler facts, evidence, register audits
and queue APIs. Some extensions are already in the working tree; inspect and
validate them before adding another implementation. The full spec's checkpoint
describes that work, but does not establish that FF9 collectors have migrated.

Start by inspecting `workspace.rs`, `inventory.rs`, `adapters.rs`,
`campaign.rs` and `linkevidence.rs`, plus the corresponding MCP operations.
These now contain additional work beyond the original compiler-facts slice.
Their presence is an implementation lead, not a verified migration result.

For a selected caller, show one work package containing:

- Its original load unit/extent, recovered source, prepared source and revisions.
- Every call finding, including additional findings hidden behind a compiler's
  first error, plus missing or unexamined inputs.
- The actual selected provider and compiler definition ABI; show competing
  definitions and why each was selected, shadowed, excluded or unresolved.
- Native call/delay-slot instructions and compiler source spans. Unestablished
  native/source correspondence stays visible.
- Register read/overwrite witnesses, unresolved paths, and the exact caller/site
  scope of any policy. Explain both eligibility and refusal.
- Preparation, compile, link and proof outcomes, with stale dependencies and
  reproduction links.

Use the same IDs and records in the existing UI, CLI and MCP. A user should be
able to click a blocker and inspect the evidence an agent used to decide it.

## Features to move into Binviz

| Order | Feature | Work it replaces | Acceptance |
| --- | --- | --- | --- |
| 1 | Verified ownership and complete compiler call audit (BV-01/02/05) | Per-game "owned caller" joins, definition maps and first-error reports. | Join native xrefs to physical units and actual compiled providers; expose all findings. A resident definition and host surrogate with the same name cannot silently overwrite one another. |
| 2 | Paired native audits and scoped policy consumption (BV-04) | Repeated provider/post-call checks and manually filtered argument exceptions. | Show witnesses; enforce caller, site, provider, count and revision in the shared consumer. An unlisted caller/site stays refused. A surviving unread return is distinct from an overwritten register. |
| 3 | Evidence lineage and promotion plans (BV-03) | Hash-check loops, frozen manifests and hand-seeded accepted artifacts. | Explain stale inputs and invalidate only dependents. Reproduce baseline/candidate stages before proposing an exact promotion; preserve historical evidence. |
| 4 | Differential proof campaigns (BV-07) | Repeated harness builders, trace observers and result comparators. | Configure existing native/WASM runners; compare return words, ordered events and relevant memory. Show genuine versus controlled providers and explicit unknown frontiers. Keep fixture values game-owned. |
| 5 | Incremental batch execution (BV-08/05) | Repeated file scans, separate compiler processes and whole-unit replays. | Share binary loads/facts, deduplicate identical jobs, bound memory/workers, and show cache miss reasons. Measure cold/warm time and subprocess count before claiming improvement. |
| 6 | Reviewable bridge generation (BV-06) | Ad hoc source rewrites, callback wrappers and argument conversions. | Produce exact before/after plans; preserve evaluation and full return words. No automatic padding, truncation or widened exception scope. |
| 7 | Object/access extent and closure reports (BV-09) | Manual stack-frame, LLVM lifetime and provider-copy reviews. | Distinguish storage size, initialized bytes and provider access range. A matching two-byte object passed to a four-byte writer must be flagged; unknown callback/IRQ paths remain incomplete. |

Python can remain a maintained Clang adapter or game runner. Move duplicated
analysis and decisions into shared Binviz records; translating every script to
Rust is not the objective.

## Concrete gaps from the latest decompilation batch

- **Caller ownership:** native xrefs exist, but the game still has to join them
  to physical members, prepared C and selected provider definitions. The caller
  inspector must show the join, competing providers and every unresolved edge.
- **Evidence promotion:** adding file-12 preparation changed eleven file-11
  discovery stamp identities while their source, CPP, raw objects and dependency
  outputs stayed identical. A shared promotion plan should explain and validate
  this transition, preserving old evidence and exact policy scope.
- **Execution observation:** BBD3C passes a meaningful unit pointer in a MIPS
  delay slot. A provider-entry observation before that slot sees the wrong value.
  The configured runner needs a supported event after the slot executes, with
  its timing explicit in the proof record.
- **Complete, incremental reports:** a compiler's first error hides later
  blockers. Keep all call findings and reuse their verified raw-fact inputs;
  policy changes should recompute decisions without repeating unchanged scans.

The full spec records acceptance and refusal cases for these examples. Treat
them as reusable requirements; the particular addresses and fixtures stay in
FF9's configuration.

## Next migration targets from the current batch

Prioritize replacing duplicated decisions, then reduce orchestration. These
targets extend the existing implementation; they do not require another parser,
call graph or queue.

| Target | Temporary game logic | Required shared result |
| --- | --- | --- |
| Complete caller package | `typed-abi.py`, `resident-call-abi.py`, owned-prototype collectors | Join existing native xrefs, compiler facts, physical ownership and actual linked provider. Show every finding, source span, instruction witness and unresolved join. |
| Provider and service selection | CD/GPU/formatter scope collectors and host maps | Show the native definition, excluded/substituted provider, wrapper, import signature and software-service obligations together. The same import field with different signatures remains distinct. |
| Precise cache invalidation | Discovery-key reconciliation and repeated hash checks | An unrelated new header reuses unchanged facts; a header shadowing a requested include invalidates its consumers. Explain the dependency and measure subprocess reuse. |
| Intrinsic and exception outcomes | Special handling for implicit Clang declarations and trap comparisons | Give `__builtin_trap` a stable named effect and source span. Compare visible state before an exception; transient AST IDs and merely observing two traps cannot establish equivalence. |
| Configured campaigns and batches | Private native/WASM harness builders and replay recipes | Preserve failing baseline cases, compare ordered effects and full return words, expose runner authority, and rebuild only affected stages. |
| Concurrent naming and runtime work | Repeated whole-unit source/hash reconciliation while another session edits names | Run against one captured source/header/tool/recipe namespace, distinguish valid historical results from current publication eligibility, and explain which live edits require a new transition. Reuse `build-batch` frozen inputs and stable compiler paths. |

For each target, the implementation session should record the exact legacy
helper retired, the game configuration retained, positive and refusal parity,
and cold/warm elapsed time plus compiler/runner process counts. A shared API
existing in the checkout is an implementation lead; migration is complete only
when the game consumes it and duplicated analysis can be removed.

The first user-visible acceptance is simple: selecting a caller explains who
owns each call, which provider will run, why it is blocked or accepted, and which
source, instruction or stale input supports that decision. Agents must receive
the identical record through MCP.

## Delivery sequence and definition of done

1. Validate existing compiler-facts/evidence tooling on tracked synthetic inputs.
   Then deliver ownership plus the caller inspector through UI/CLI/MCP.
2. Add the shared scoped-policy consumer and evidence promotion plans. Retain
   negative cases for ambiguous ownership, stale inputs and unlisted sites.
3. Add campaign/batch adapters, bridge plans and object/closure evidence as
   independently usable slices. Preserve the current conservative outcomes
   until stronger analysis is demonstrated.

The first demonstration needs no game image: two callers, an overlay/provider
collision, multiple contract findings, one scoped policy and one changed input.
Show the same results on all surfaces and the changed input invalidating only
its dependents. Then compare one frozen, user-local FF9 collector with the new
path, including refusals, before retiring duplicated logic.

Keep archive formats, load-unit configuration, recovered source, platform/device
semantics and game fixtures in the game/runtime project. Binviz owns shared
analysis, identity, evidence, explanations and orchestration interfaces.

## Requirement for continuing decompilation sessions

Flag a reusable gap when it appears, before creating another analysis helper.
Report the repeated work, existing capability, missing connection, temporary
adapter and proposed feature. Update the full spec at each coherent batch with
evidence paths, acceptance/refusal cases and migration status. Clearly separate
proposed, implemented, demonstrated and migrated work. Do not wait for the user
to ask for another tooling review.

Use this brief flag in progress updates: **Tooling gap:** repeated task;
existing Binviz entry point; missing capability; temporary adapter; spec ID.
At the batch checkpoint, add an evidence-backed backlog record and state whether
the adapter can be retired. Another session owns feature implementation unless
the user assigns it to the decompilation session.

## Prompt for the implementation session

> Implement the first delivery in this handoff using the detailed spec. Inspect
> existing and in-progress code first. Extend verified ownership and deliver a
> complete caller work package through shared core, CLI, UI and MCP, followed by
> exact scoped-policy consumption. Use the synthetic demonstration above and
> preserve ambiguous, stale and unsupported outcomes. Compare a frozen legacy
> collector before migration. Do not change FF9 source to obtain a tooling pass.

## Additional implementation checkpoint: 2026-10-03

The working tree now includes physical inventory and actual object/link joins,
caller packages, exact scoped-policy eligibility/application, queue blocker joins,
reviewable scalar extra-word bridges, configured build/campaign adapters, storage
observations and before/candidate promotion plans. CLI, MCP and browser/WASM share
those records; the browser also exports progress SVG/PNG. Usage and boundaries:
[Decompilation workspaces](decompilation-workspaces.md).

Synthetic tests demonstrate two same-address overlay callers, complete findings,
one exact policy scope, changed-input refusals, actual WASM signatures/calls/data,
separate matching/analysis extents, applied compile/link gates, object-size hazards,
promotion recipe/consumer refusals and preserved queue filters. Actual Windows
native DLL/Node-WASM tests exercise seven baseline cases plus wrong-word,
wrong-memory and extra-device negative controls. Configured build tests cover
cold/warm subprocess counts, leaf-only invalidation, header shadowing, output
corruption and concurrent job deduplication. These are demonstrations, not FF9
collector migration or PS1 instruction campaigns.

General callback used-return closure, cross-register callee contracts,
externally reviewed loop bounds and complete stack upper bounds
remain open. Compiler facts now cache typed extraction with verified prepared,
header/tool/environment keys and measured cold/warm subprocess counts; fresh
preprocessing detects header shadowing. Observable-bit mode handles
locally established constant counters and keeps unknown paths unresolved. Storage
facts are current reviewed descriptions, with their authority explicit; stack
observations do not claim an upper bound. Promotion plans preserve both namespaces
and report prerequisites; they do not publish accepted evidence or approve policies.
No game-owned source or collector was modified or retired.

Single-GPR no-consumption certificates now recompute exact native audits, retain
separate killed/discarded endpoints and compose current acyclic child evidence.
Mixed kill/preserve callees conservatively retain the word in caller continuation;
unknown calls, stale dependencies and recursive proof cycles refuse. CLI/MCP/UI
expose identical certificates and dependency/witness records. No game closure or
provider scheduling equivalence is inferred from those certificates.

## Remaining-work implementation checkpoint: 2026-10-03

The next batch implements scoped matching-note plans/publication, separate
baseline/candidate readability acceptance, configurable two-WASM comparison
roles, exception-point state/effect comparison, selected service chains,
shared-memory initialization and restoration order, finite callback/stack models,
scalar variadic homes, missing-input candidate validation and shared-tail physical
ownership. Cross-register child certificates and external native-byte-pinned loop
bounds reuse the existing native auditor. Optional Clang LLVM IR supplies actual
allocation/lifetime instructions; intrinsics retain stable names and trap effects.
CLI, MCP and workspace/WASM reports expose these workflows. Usage and exact
assumptions are in [Decompilation workspaces](decompilation-workspaces.md).

Actual compiler acceptance fixtures compare complete objects at identical
compiler-visible paths and recompute original-code scores. Native DLL/Node
campaigns include wrong-return, wrong-memory and extra-device negative controls.
Additional two-WASM and existing independent FF9 CPU/Node fixtures compare a
zero-divisor trap checkpoint and reject an observable store before the trap.
The synthetic native instruction fixture executes BEQ, its delay slot and BREAK
through the existing CPU model; it is not hardware or scheduler equivalence.

`tools/ff9_collectors.py` now compares frozen game reports with actual shared
backends. All 13 checks agree: six native register audits, six frozen module
hashes and the CD service's zero/six-word signatures. Current reports and CLI/input
digests are under `target/ff9-shared-migration/`. No game source was changed and no
legacy collector was retired. Historical scheduler/gameplay observations remain
historical. Retirement is a separate game-owned integration decision.

The stack certificate covers its finite reviewed call/interrupt model and
constant straight-line frames with verified operands/restoration. Branching or
dynamic frames, incomplete callback targets and unknown host effects refuse.
Shared initialization requires current reviewed layout artifacts and exact
restoration observations for live memory; a checkpoint name alone is insufficient.
Readability grants no matching credit for an unchanged partial score and no
linked/gameplay proof. General whole-program hardware bounds remain unsupported.

Validation uses targeted batch checks and one full Rust suite (361 passing,
one optional game test ignored), followed by the focused stack-operand/restoration
regression. No browser automation was used for this batch.
The final CLI/MCP binaries and optimized WASM, TypeScript and Vite build also
pass. Executed campaign coverage is 32 pairs; compiler/cache regressions pass
six/four tests respectively. Logs are `target/validation-final-rust.log`,
`target/stack-final.log`, `target/campaign-final.log`,
`target/compiler-batch.log`, `target/build-cache-batch.log`,
`target/validation-cli-mcp.log` and `target/validation-wasm-release.log`.
The native release CLI and MCP server were subsequently rebuilt together with
`cargo build --release -p binviz-cli -p binviz-mcp --offline`. Direct release
smoke checks verify CLI commands/binary inspection and MCP initialization/tool
discovery. Build diagnostics are in `target/validation-native-release.log`;
executable SHA-256 identities and smoke results are in
`target/native-release-check.json`.

## Game-session adoption checkpoint: 2026-10-03

The disc-1 session now invokes the rebuilt release CLI directly. Its independent
`target/ff9-shared-migration-root-20261003/migration-report.json` agrees on all
13 captured register/module/signature checks. Actual workspace blocker/caller
inspection and linked VSync signatures are recorded under
`target/ff9-new-runtime-inspection/`. The frozen ranking workspace still has
unresolved bindings; inspection does not promote them.

The shared adapter already supports distinct original callee and selected
definition/linkage identities. An observed remaining gap is generating eligible
discarded-result plans: the workspace policy consumer supports that policy, but
the planner refuses its kind and unequal return representations. See the updated
BV-06 acceptance in the full spec. The game keeps its frozen compatibility
adapter while testing real shared consumers; no new private analyzer is needed.
Equal-arity VSync sites additionally need a verified direct binding plan: thirteen
one-word calls have no argument exception, so extra-word policies correctly
refuse them. The fourteenth call needs discarded-result lowering as well. These
are concrete generation gaps, while selected-provider identity and inspection
are already present.

The game also exercised the shared exception campaign with the genuine B7F20
fixture: the old cursor byte and exception-state digest fail, and the corrected
candidate passes; root release-CLI execution and reinspection reproduce both.
Only that one exception comparator is replaced. Ordinary executed void entries
need a verified void result profile because the current record validator requires
return bits. The remaining 314 histories and 266 policy controls retain their
legacy consumers. The BV-07 acceptance is recorded in the full spec; do not
serialize invented zero returns to force a migration.

Actual runtime adoption subsequently reproduced the full immutable file12 strict
diagnostic pass: 295 attempted, 246 accepted and 49 rejected; no registry/link.
The shared analyze stage caches those verified reports, while its game outcome
stays refused. Cold214.15s/warm0.388s with zero warm subprocesses apply to this
diagnostic batch. A Windows cleanup failure originally hid the producer cause;
the maintained batch adapter now preserves primary diagnostics and separate
cleanup warnings, with all seven focused tests passing. The release CLI was
rebuilt to embed that fix; current and preserved-before tool identities/smokes
are in `target/ff9-rebuilt-adoption/`.

Real game adoption also exposed and repaired compiler-cache rejection of optional
intrinsic/storage payloads. Production semantic flags remain in the recipe and
are explicitly cacheable; hidden plugin/response inputs still refuse. All nine
real-Clang tests pass. The genuine B7F20 before/candidate producer now reuses both
typed extractions on a warm run, retaining fresh preparation/target discovery.
The earlier frozen disabled-cache lane is preserved separately. These are
game-session observations, with no new matching or playable-scene credit.

## LoadImage adoption follow-up

The game now exercises optional LLVM storage extraction on seven actual frozen
file12 caller bodies. `target/ff9-file12-loadimage-next/compiler/` contains twelve
Clang call expressions, seven storage records and 46 verified artifact identities.
Native Binviz references identify eleven physical JALs: B8DC0's five C expressions
share one native tail, yielding four JALs. Keep that non-bijective correspondence
explicit when selecting linked providers; ordinal pairing is insufficient.

The actual compiler facts expose a reusable type-normalization defect: typedef
`RECT` expanding to `struct RECT` repeatedly expands the tag token again, producing
`struct struct ... RECT *`. Fix canonicalization using compiler type/tag identity
or a tag-aware cycle-safe expansion; retain the original spelling. Acceptance
must include same-named typedef/tag, chained aliases, qualifiers and nested pointers,
with idempotent canonical output and unchanged target-width/category observations.
The game does not add a private type normalizer to work around it.

Extraction keys include environment digests. Fresh Docker containers changed
HOSTNAME and caused seven legitimate key misses despite identical generated facts.
Pinning the game runner hostname preserves that evidence and yields seven hits,
zero AST/LLVM subprocesses, with byte-identical facts. Default environment capture
must remain conservative; any narrower configured environment profile needs explicit
validation and a distinct recipe, rather than silently dropping key inputs.

The next real naming-transition batch additionally instruments cold staging:
169.70s total, 113.31s input staging, 41.98s producer comparison; maintained
preparation15.11s, discovery14.44s and ABI4.34s are nested producer measurements.
It verifies equality for295 prepared CPP/objects,295 discovery CPP/raw objects
and311 resident/private-core definition ABIs. This is evidence for prioritizing
the already-flagged verified Linux execution-root/sharded input reuse over adding
compiler workers. Warm0.412s is unchanged-result reuse, not the cost of a new
candidate. Game report: `../ff9-decomp/build/file12-committed-transition/` pinned
to3f13bcbc; subsequent live naming commits need distinct source transitions.

The rebuilt release also executes the genuine GPU body campaign:44 original-
instruction-translated-WASM/C-WASM pairs pass, with12 expected frontiers and189
verified artifact identities. Root's independent fresh run verifies196 identities
(including its configured runner/tool artifacts), using `campaign` and
`campaign-compare`; full RAM, VRAM and ordered device effects are observed.
Actual original MIPS interpreter comparisons remain a distinct controlled queue
boundary proof. Debug printing, unknown Ops/workers, stalled device progress and
unsafe deferred local-object lifetime remain explicit frontiers. Reproduction is
in `target/ff9-root-gpu-review/`, with the authored configuration in
`target/ff9-gpu-real-bodies/campaign-final/`.

This fixture caught undefined DATA symbols that `wasm-ld --allow-undefined`
resolved to address zero. The normal linked inspector reported no type problem;
that alone does not certify a complete data-provider closure. Preserve the failed
tentative-register specimen as an acceptance case: object DATA references must
have real definitions/linked addresses or an explicit reviewed external-memory
provider. Distinct required register backing locations cannot silently alias zero.
This is an additional BV-04/BV-07 integration requirement, not a request to change
the game's canonical register map.

The shared full295 diagnostic batch with the scoped BIOS rand service verifies
251 accepted/44 refused, removing exactly five caller failures from246/49.
Cold217.80s/warm0.390s apply to that unchanged candidate; warm reuse launches zero
subprocesses. Nine batch artifact/stage identities are verified by `workspace`.
The producer exit remains1 and no whole registry/link is emitted. The final rand
guard revision has separate97 refusal controls and five identical rewritten CPP,
plain and LTO outputs; preserve the earlier full replay's distinct collector hash.
Do not turn compiler acceptance, a software BIOS profile or shared capture success
into native matching/cold-boot/gameplay credit. Captures:
`target/ff9-root-rand-strict/` and `target/ff9-root-rand-review/`.

The next actual combined shared batch verifies265 accepted/30 refused of295,
up from251/44:14 caller failures disappear, three advance to later contracts.
It contains final f3 rand, genuine GPU bodies, scalar VSync and two reviewed
menu source corrections. All17660 input identities remain unchanged; workspace
verifies8 declared artifacts and imports the actual stage. Cold221.9869s,
warm0.3849s/zero subprocesses/identical published outputs. The game producer
still exits1 before registry/link. Root capture/freeze:
`target/ff9-root-combined-strict-v2/`,8ad049f1…94a1da.

Actual root production-RAM GPU campaign/reinspection passes44 pairs with12
expected frontiers,207 verified identities and bound observations. Root final
scalar VSync independently passes42 original comparisons,4 fullword queries,
60 timer histories,18 legacy pairs and12 refusals. Two public menu source fixes
retain4/4 exact native instruction+relocation matches (whole ELF bytes differ)
and309 original/C histories/105 controls. These remain distinct scopes, not
whole-game/native matching progress. Game-owned commitc3f2726d6 binds only the
two source fixes and checkpoint notes.

Source/header naming comparisons now demonstrate narrow declared input staging:
2954 files,67.10s versus169.69s previously. Discovery and strict compilation have
different input closures: strict enumerates all retained BOOT plain/LTO objects,
classifies HLE from BOOT sources and links with no section garbage collection.
An exact-record dependency projection proposes9231 strict files from17660;
a parity run is pending in `target/ff9-root-combined-strict-subset/`. Do not
claim that projection verified until actual producer/count/object equality.
Use these specimens when implementing declared glob/object collections and
verified reusable staging roots; do not remove dependencies from cache recipes
merely because a diagnostic build stops before the final link.

The first9231 strict projection is now a concrete refused dependency specimen,
not a timing improvement: maintained formatter generation loses global register17
when unrelated resident sources are omitted. Existing asmgen.pinned_registers
scans the entire src/include corpus for file-scope register annotations, independently
of the builder name_table. Actual counted output changes b9fc3e8… to b2c3afe8…;
the exact generator seal refuses before caller replay. Keep this negative input/
output/diagnostic package at target/ff9-root-combined-strict-subset. A conservative
9912-file revision restores every omitted source containing literal register;
actual full parity remains pending. No formatter hash reseal or private type parser.

A separate Windows host path-length refusal occurs before any compiler starts
when the configuration directory itself is long. Attempted parent cache escape is
correctly refused by workspace ownership. A short owned configuration root with
absolute declared inputs avoids that setup issue while retaining exactly the same
input/recipe cache key and compiler-visible /work paths. Actual short-root run is
in target/ff9-sr2; failed setup reports remain separate under the longer root.
Shared BV-01 acceptance should cover host-path-length diagnostics and safe short
owned staging roots, plus explicit transitive directory-scan/register-map inputs
for generated code. Native/C/ABI facts cannot certify omitted producer metadata.

Actual9912 strict projection now passes full parity:265/30, all1135 emitted
artifacts byte-equal and170 BOOT definition reports equal. Cold147.6728s vs221.9869s
(33.48% less elapsed); warm0.33665s/0 subprocesses,8 workspace identities verified.
Frozen target/ff9-sr2 manifest2911465c…55f7a contains the actual reports. The9231
register17 dependency omission remains a refused negative specimen, not a speedup.
This validates short owned Windows staging roots plus conservative declared scan
dependencies without altering producer code or generated-code seals. Shared tools
should expose producer directory-scan inputs and explicit collection closure so
the next game can construct this projection without bespoke dependency scripts.
Both actual ResetGraph callers pass1, so passing body modes0/3/5 cannot admit them;
that exact mismatch is a useful source/call-facts scope-refusal specimen.


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

### Adoption findings: statement context and proof-derived explanations

Two real compiler-facts specimens report `resultConsumed:true` for discarded
void expression statements beneath switch labels. Preserve the raw findings;
fix the maintained Clang extractor rather than adding a game-specific classifier.
The default-label example is retained in
`target/ff9-file12-geometry-boot/BV06-default-void-counterexample.json`:
`sub_800a95a0:call:6427:6452`, genuine void `sub_8002eb64(0x39,0x194)`.
The case-label example is in
`target/ff9-file12-resident-helpers/compiled/facts.json`:
`sub_800b73f8:call:4506:4521`, genuine void C4710 expression in case32.
Maintained typed caller records independently mark both discarded. Acceptance
needs case/default/nested labels, ordinary statements, and meaningful non-void
uses; keep declaration identities and source-buffer spans exact. Existing facts
must remain immutable and corrected extraction must invalidate dependents.

Scope explanations should derive from referenced shared certificate/audit IDs,
not copied prose. A reviewed A9C1C queue scope carried a copied 21E5C A1-kill
sentence even though its actual 1C750 proof correctly said incoming A1 is unread
and may be written. Its separate v2 fixes only that explanation. The new queue
family also distinguishes A9C1C's physical post-call kill from A8FF0's explicit
discarded-register return boundary; without that policy A8FF0 stays unresolved.
Expose those distinct structured witnesses and transitive native/review inputs
through the same CLI/MCP/UI record. A missing child or changed witness must
refuse the explanation as well as the eligibility decision. This extends the
existing certificates; it does not call for another native analyzer.

### Adoption findings: computed branch targets and composed edits

`register-use` accepts reviewed `indirectTargets`, but callee-certificate Request
currently rejects that field. The retained sound specimen is
`target/ff9-file12-sound-family/indirect-target-gap/`: shared sine alternate
8004BB80..8004BBF0 belongs to the cosine owner BB7C/116B. JR8004BBA4 computes
BBAC + ((A0 >> 6) & 0x30), capturing its target before the BBA8 delay-slot write.
Its four targets are BBAC/BBBC/BBCC/BBDC. Default inspection remains unresolved;
the reviewed CLI audit plus explicit discarded boundary is unread, not killed.
Extend first-class certificates to consume content-bound target-review artifacts
and complete dependency closure, recompute the same audit, and retain surviving
return/caller obligations. Acceptance includes missing/changed targets, wrong
ownership/extent, changed delay slot, stale target evidence and schema parity.
Do not infer workspace acceptance from a standalone CLI policy.

The actual sr6 candidate also exposes composition ordering: exact A95A0 terminal
rewrite succeeds independently, but applying it before the GPU pristine-CPP gate
correctly rejects `GPU scoped raw CPP drift`. Preserve both source buffers and
the immutable failed integration; no policy hash refresh is justified. A reusable
multi-policy plan should validate all scopes against one original buffer, detect
overlap, merge applicable typed-call plans, then emit reviewed transformations
and terminal guards in explicit stages. Expose mapped identities after each
stage and refuse incompatible plans. Validate the actual selected caller through
its full production chain before launching all295; a generic typed-only compile
misses these consumer interactions. Game-specific terminal/debug behavior remains
configured by the game. This extends existing edit plans and buffer mapping.

### Adoption findings: existing services, profiles and SDK reuse

The next grouped candidate, sr7, stopped before caller compilation because its
music change edited the original menu session source pinned by the GPU scope.
The failed stage remains immutable. Sr8 restores the original source and adds a
separate, explicitly owned file12 session/profile for the qualified music
frontier; all eight new build identities verify and 287/295 callers compile.
Do not solve independent profile ownership by refreshing an unrelated policy
digest. Expose the host dependency and owning profile in the same caller package
and composition plan so this conflict is visible before launching a full batch.

An isolated AD39C diagnostic omitted the existing SDK missing-argument policy
and falsely suggested that its 1D898 memset call needed a new admission. Actual
completed sr8 already admits that call through the existing software BIOS
memory contract. Retain the additional 162-pair validation without duplicating
the host. A focused caller check must compose every applicable existing service,
provider selection and edit stage; a partial generic ABI check cannot establish
the production caller's next blocker. Reports should name omitted stages.

Public SDK recovery and portable providers are now catalogued in
[ff9-sdk-reuse-audit.md](ff9-sdk-reuse-audit.md). Check existing cross-project
store/signature functionality before adding another matcher. A reusable catalog
should connect versioned native identification, recovered source, true ABI and
optional platform replacement, retaining separate evidence for each. PSY-Z and
PsyCross provide useful reference/backend candidates; their API coverage is not
FF9 compatibility proof. Marker emitters are game code, and concrete PsyCross
return/queue/environment behavior differs from some already reviewed FF9 SDK
contracts. Preserve those distinctions in CLI/MCP/UI selection and progress.

### Adoption findings: SDK metadata joins and hidden RCS IDs

The SDK follow-up confirms that masked signatures/release ambiguity and the
matched-source store already exist. Extend their evidence relation to an exact
upstream revision/file/license, optional portable provider, dependency closure,
existing runtime service and compatibility campaign. Keep recovered C,
assembly placeholders and compatible-but-different implementations distinct.
Acceptance must preserve ambiguous release matches, report the already admitted
AD39C memset service before recommending duplicate work, and expose missing
provider dependencies. Pinned examples are in `ff9-sdk-reuse-audit.md`; PSY-Z's
nested recovered-decomp MIT notice is separately captured and supersedes the
initial incomplete root-license inventory. No new SDK matcher is requested.

`libraries` misses the literal
`$Id: sys.c,v 1.140 1998/01/12 07:52:27 noda Exp yos $` at file0x860 /80010060
in SLUS_012.51 SHAe30e40745d079aed7071c130785fb42406d8857bdf5484a101ec9baed143ee1c.
Its output lists only intr.c1.75 and bios.c1.86. `rcs.rs::parse_id` already has
the exact successful parsing test; `library_sources` discovers through the
general string index, whose console-code suppression hides this embedded ID.
Use a targeted RCS discovery scan independent of that suppression, retaining
file offset/address and format mapping. Validate embedded-code IDs, ordinary
data IDs, malformed/truncated text and deterministic deduplication; general
string-browsing suppression need not change. Agent specimen
`target/ff9-sdk-reuse-next/native-id-defect.json` uses debug88f7d5...;
`target/ff9-root-sdk-library-defect.txt` independently reproduces with pinned
release545beb01f1e1e8fd6dccd2c2f66b80224bc152e23b12a2ebcbe1a93822644b6e.

### Adoption findings: combined declarations and build measurements

The genuine B54EC source has a comma-separated extern declaration at line10.
The maintained caller adapter hits its declaration-span assertion when asked
to adapt32120's corrected true2 signature. Preserve the failing source,
`current-review/compiled/sub_800b54ec.prepared.i`, full compiler facts and
`consumer-adapter-review.json` under `target/ff9-file12-transit-wrapper`.
Support separate declarators within one declaration using exact maintained
Clang spans; do not silently split source text in a game-specific parser.
Acceptance needs two/three declarators, mixed function/nonfunction declarators,
unchanged unrelated declarations, precise buffer mappings and ambiguous-span
refusal. Four other translation units/five calls already adapt normally; the
two extra-three-word callers remain separate proof obligations.

Measured cache experiment `target/ff9-sr9-lto-carry-v2` seeded290 validated LTO
triples; all290 resulting prepared/object/stamp bytes and five refusal details
remain identical. Total elapsed increased160.84157s→164.70668s; producer elapsed
changed91.78199s→89.79032s. No speed gain was established, and sr10 did not adopt
the larger input archive. Failed/refused producer reports currently omit their
existing discovery/LTO hit counters because those are emitted only after final
link. Preserve those counters and phase elapsed times on refusal so downstream
Binviz reports can show where work actually repeats. Batch-stage success only
means verified outputs were published; it does not mean the game producer
linked. A future optimization should target the measured phase and retain the
same source/profile/output checks. Direct references to published archives
also avoid redundant completed-tar copies.

### Adoption findings: reusable native GTE provider

The renderer campaign reproduces192 actual three-body executions against the
same software GTE contract. Its native producer reuses the maintained MIPS
executor but substitutes seven existing recording-only COP2 callback sites
with a pinned host-compiled `gte.c` provider. This follows the earlier script-ID
hook pattern; it adds no decoder or CFG walker, but duplicates provider wiring.
Specimen: `target/ff9-file12-render-boundary/native-gte-proof.py`, corresponding
compiler/check/reproduction records and immutable freeze
d7a68b2f96781d800e9e3a2387472d241345426a6b1b4d373a410ed7ae323757.

Expose an injectable native-execution provider through the existing proof
campaign producer contract, with explicit COP2 data/control reads/writes,
commands and memory load/store callbacks. Pin backend source/compiler/profile
identities and expose ordered trace, final state, unsupported commands and
instruction-budget frontiers in shared reports. Reuse the existing executor;
eliminate callback-text rewriting. A software backend comparison and an
independent hardware oracle must remain different proof statuses. Acceptance
should replay these192 cases, including actual zero-vertex GTE work, complete
scratch state and the retained zero-polygon budget frontier. Missing/mismatched
providers and unknown commands must refuse. This does not call for new SDK,
instruction or caller parsers, or certify hardware timing.

### Adoption findings: verified dependency transitions

The canonical32120 correction adds `include/fade-transition-names.h`. The game
builder's `typed-abi._fingerprint` hashes every header in `include` and `wasm`,
so this addition changes discovery keys for callers that do not include it.
The full295 normally regenerated records change only their keys; CPP, objects,
prototypes and dependency contents remain identical. This is verified under
`target/ff9-fade-header-discovery-v2`, using the actual completed sr10 outputs.
The first replay's stale historical expected hashes caused an explicit A8B14
refusal, preserved under `target/ff9-fade-header-discovery`; v2 corrected the
fixture baseline without changing caller sources or the maintained producer.
Cold report total94.4898s includes producer26.4946s/discovery12.9850s; warm0.3562s
launches zero subprocesses. All8 new shared artifact identities verify. The
large total/producer gap is measured orchestration work; profile capture and
frozen-file staging before increasing compiler workers.

More than20 inherited file12 scopes pin completed discovery-stamp identities.
Several SDK scopes also pin the entire oldBOOT module and HLE source. The
reviewed production transition changes only32120 plain/LTO objects and the
HLE plain/LTO objects;169/170 selected prepared/LTO providers are identical.
The HLE source changes two32120 declarations and two forwarding calls. Exact
evidence is under `target/ff9-current-boot-provider-review` and
`target/ff9-file12-transit-b6/transition`, with separate actual-module execution
evidence in `target/ff9-root-transit-source-review`.

Extend shared artifact/dependency transitions to join an immutable completed
proof basis to normally regenerated current inputs. Record each changed field,
the actual producer/recipe, unchanged semantic inputs and outputs, affected
proof consumers, and any behavior-changing provider. Do not silently replace
old hashes, rewrite current stamps to historical identities, or treat unchanged
caller objects as proof that a changed callee is equivalent. A changed callee
needs its separate execution evidence and caller-contract review. Keep module
identification, scoped runtime compatibility and hardware fidelity distinct.

Reuse Binviz's existing include-search indexes and stage model for precise
invalidation; no new header scanner or cache-key algorithm is needed. A game
adapter's blanket header fingerprint should not force the shared system to
invent a second invalidation engine. Acceptance should retain original and
current records, admit the single-header/identical-object case, reject changed
CPP/object/dependency/prototype or an unverified current key, reject a shadowing
header, and preserve unrelated changed-provider refusals. The actual final-link
capture also needs the game producer exit and module digest/single-RAM result;
successful publication of a batch artifact is a separate status.

### Adoption finding: preflight every declared proof dependency

The first combined final-three-callers stage (`target/ff9-sr11`) stopped before
caller compilation after76.8191s of producer work. The transition scope pinned
`file11-boot-abi.py` from its proof snapshot, while the combined snapshot carried
the earlier-stage helper:204 CRLF lines and one unrelated357c0 caller-source
identity differed. The source is not silently normalized or the pin relaxed;
the five selected definitions and exact scope must be reproduced with that
captured helper before deriving its metadata pin. No295/0 result was produced.

Expose a shared preflight over the existing artifact/proof-consumer dependency
records before launching preparation/compiler work. Report all mismatched
dependencies together, their owners and old/current identities. An explicit
derived transition still requires its producer/output evidence; preflight does
not authorize a digest replacement. Acceptance should detect this helper drift
before compilation, preserve the existing gate refusal, and reuse verified
dependency results without introducing a second source parser or matcher.

The preserved sr11 stage measures146.1368s total versus76.8191s producer time,
with10386 verified inputs; this is not evidence that more compiler workers will
help. A separate unmeasured optimization candidate is Linux-side archive staging
for Docker execution. The current adapter extracts thousands of files onto
Windows before mounting them at canonical `/work`. Evaluate an immutable
Linux-volume input snapshot, a separate writable output volume, and streamed
archive publication through the existing batch artifact model. Preserve exact
compiler-visible paths, input identities/read-only boundaries, pinned image,
resource limits, producer exit and all outputs. Acceptance requires identical
CPP/object/ABI/module outputs and all refusal behavior, plus measured total and
per-phase elapsed time; do not claim a speed gain from the hypothesis alone.

### Adoption finding: read-only linked-module import/type inventory

The actual first linked file12 module (`target/ff9-sr12`, module3c3da705…7d400)
contains286 function imports. Four qualified JSnames occur twice with different
WASM signatures: DrawSync, StoreImage, ClearOTagR and ff9_cd_busy. The initial
checker's unique-name assumption refused this legitimate representation. Three
anticipated display-worker imports also disappear because the final link resolves
their selected implementations internally. These observed link results require
an explicit module-bound checker transition, not a blanket duplicate allowance.

The maintained game `wasm/runtime/fnptr.py` already reads exact function types
and imports, but exposes only its rewrite entrypoint; `mkmod` supplies global
inspection only. The temporary adapter stops that existing reader before any
rewrite and captures its parsed records. Expose a read-only inventory API/report
from the maintained reader, including module/name/kind, function/type indices,
parameter/result representations, multiplicity and selected host authority.
Bind module and reader identities. Reuse the parser; no separate WASM grammar
or game-specific import parser. Acceptance should retain this286-entry sequence,
the four distinct pairs, missing/mismatched-binding refusals, internally resolved
endpoints and unchanged input bytes. Inventory approval does not establish game
startup, BIOS fidelity or hardware timing.

Handoff correction (2026-10-04): the existing maintained command
`target/debug/binviz.exe linked <module> --json` already provides the required
read-only inventory, direct/indirect calls and stack-operation observations.
The captured debug9e105a…91ab3e report has exact parity for all286 ordered imports
and the four pairs; see `target/ff9-file12-scratch-closure/import-parity.json`.
The earlier fnptr trace/missing-API proposal is historical and superseded.
Adopt this built feature instead of adding another reader. Stack observations
depend on identifying the actual mutable compiler stack global; absent exported
authority and empty observations cannot certify zero scratch frames.

### Adoption finding: planner eligibility must survive generated C declarations

The selected sound envelope getter has a genuine void two-input contract.
Its existing caller declares unsigned/short-pointer arguments; the selected
provider uses int/unsigned-short-pointer arguments. These have the same WASM
representations, but emitting both declarations under one C identifier fails
compilation. The current shared direct-binding planner admits the call correctly,
then its emitted candidate fails with that exact declaration conflict. Retain
planner eligibility and emitted-candidate compile status separately.

The private `target/ff9-spu-envelope` lane uses the existing maintained game
`resident-call-abi.rewrite` fallback: remove the stale external declaration,
declare the real provider, and emit the established caller-width shim. No new
C parser or source normalization was added. Actual getter stores and genuine
refresh/release caller histories pass140 native/shared comparisons;47 host
controls pass. This fallback does not credit the shared candidate with success.

Extend the shared adapter generator to consume its existing compiler declaration
spans, reconcile the selected declaration, and reuse the established typed shim
or asm-name alias strategy as appropriate. Acceptance must compile the exact
caller/provider declarations with fatal ABI warnings, link the actual provider,
replay the140 comparisons, and retain incompatible-ABI/provenance refusals.
Do not invent an extra-word policy for an equal-arity void binding.

This lane also exposes an explicit tool transition: pinned release545beb…44b6e
refuses the new compiler-facts `calleeSpan` field; existing debug9e105a…91ab3e
consumes it and generates the eligible candidate above. Preserve the old schema
refusal and both executable identities. Shared workspace provenance should bind
the executable and compiler-facts schema and report incompatibility before
adapter execution; silently stripping compiler observations is not an upgrade.

### Shared-tooling adoption implementation — 2026-10-04

Implemented the compiler tag-typedef normalization, statement-label result-use
fix and exact individual declarator/callee spans. Added typed discarded-result
and equal-arity selected-service planning, immutable plan composition and reviewed
source-buffer maps. The lowering fixtures compile actual provider/caller WASM,
execute the selected one-argument service and retain independent native V0
continuation scope; no extra-word exception is introduced for these paths.

Void campaign profiles now verify actual compiler void bodies, result-free linked
functions and prepared-buffer build ancestry. Actual WASM execution covers normal
and wrapping inputs; changed RAM, extra events, fabricated zero and integer-getter
profiles refuse/fail as appropriate. COP2 injection now exposes all seven existing
executor callback operations through a pinned ctypes backend, with source/compiler/
profile pins, ordered traces, complete data/control banks, command and operation
bounds, and explicit software/hardware authority. The actual compiled callback ABI
fixture is not a GTE arithmetic oracle. The game executor still needs its own
complete injection hook; its existing temporary text-rewrite adapter is not retired.

Build batches support repository-relative frozen layouts, complete directory scan
roots, read-only input/writable output mounts, explicitly owned short Windows
staging, producer measurements on rejection and phase-specific errors. Shared
workspace preflight aggregates mismatches before preparation/compiler execution;
its drift control launches zero producers. Reviewed immutable overlays and exact
producer/consumer dependency transitions preserve historical records and policy
pins. Changed semantic caller inputs require new proof; changed provider pairs
require their own selected-module campaigns and caller review.

Callee certificates accept reviewed finite indirect targets using the existing
register auditor and derive explanations from recomputed witnesses. DATA closure
reads actual object symbols and linked backing, refusing unresolved/zero/overlapping
storage. SDK catalogs join the existing matcher and compiler ABI to pinned recovered
source, exact revision/file/license records and optional executed portable providers,
retaining ambiguity and already scoped service policies.

The targeted RCS scan finds the actual original FF9 sys.c ID at file offset 0x860,
address 0x80010060; the general string index remains unchanged. The read-only typed
import inventory preserves the actual sr12 module 3c3da705…7d400's 286 function
imports and distinct DrawSync, StoreImage, ClearOTagR and ff9_cd_busy pairs. Typed
host authority reviews refuse missing/ambiguous bindings and require actual bodies
for internally resolved endpoints. Native smoke records pin the executing CLI/MCP
binary alongside module identity. No game source, runtime status, hardware-fidelity
claim or legacy collector retirement is changed by these tooling results.

Validation: full workspace Rust 373 passed, 1 optional ignored; offline Clang
regressions 10 passed; host Python suite ran 22 tests: 14 passed, 8 compiler-dependent
skips (covered by the offline Clang run). Production WASM, TypeScript and Vite
build passed. Debug CLI/MCP smoke passed for preflight, source plans and adoption
APIs. Reports are under target/adoption-*. Linux-volume archive staging remains
an unmeasured optimization hypothesis; no throughput gain is claimed.

Release completion: both native CLI and MCP optimized executables were rebuilt
and smoke-tested after the production WASM build. The Cargo release command took
8m39s including its wait for the WASM build directory lock. Release preflight,
reviewed source-map adoption, individual declarator planning and retained MCP
workspace APIs passed; debug/release inventories agree on the actual 286-entry
module sequence. Exact executable hashes and sizes are recorded in
`target/adoption-release-smoke.json`. Native release paths are
`target/release/binviz.exe` and `target/release/binviz-mcp.exe`; restart retained MCP
sessions to load the new executable. Updated usage is in the workspace, compiler
facts, build batch, campaign and reference documentation.

### Usage and overlap review — 2026-10-04

The workspace recommended workflow now starts with identity preflight, recommends
release CLI/MCP for routine inspection and debug builds for tool development, and
includes a command-selection table. README, reference and adapter guides link to
that workflow. Record fields, MCP lists and strict-mode guidance include adoption
requests; the repeated preflight paragraph was removed from the adoption appendix.

`docs/feature-overlap-review.md` records inspected shared engines, consolidation
candidates and acceptance boundaries. Producer ancestry traversal is the first
internal cleanup candidate. Legacy FF9 collectors need broader consumer/equivalence
migration before retirement. No public APIs or schemas were removed in this pass.

Validation: seven documentation files, 51 local links and balanced fenced blocks;
`git diff --check` passed. These are documentation-only changes. The previously
rebuilt debug/release CLI/MCP and production web outputs remain current; no tests
or builds were repeated for this pass.
