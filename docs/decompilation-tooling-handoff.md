# Binviz decompilation tooling: implementation handoff

Requested outcome: move repeated analysis out of per-game scripts and into
Binviz, with the same inspectable results available to people and agents.
Detailed requirements and observed FF9 cases are in
[the full spec](decompilation-tooling-spec.md).

## First delivery: inspect a caller and its blockers

Start with existing xrefs, contracts, compiler facts, evidence, register audits
and queue APIs. Some extensions are already in the working tree; inspect and
validate them before adding another implementation. The full spec's checkpoint
describes that work, but does not establish that FF9 collectors have migrated.

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

## Prompt for the implementation session

> Implement the first delivery in this handoff using the detailed spec. Inspect
> existing and in-progress code first. Extend verified ownership and deliver a
> complete caller work package through shared core, CLI, UI and MCP, followed by
> exact scoped-policy consumption. Use the synthetic demonstration above and
> preserve ambiguous, stale and unsupported outcomes. Compare a frozen legacy
> collector before migration. Do not change FF9 source to obtain a tooling pass.
