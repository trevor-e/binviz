# Decompilation workspaces

The tracked implementation brief is [decompilation-tooling-handoff.md](decompilation-tooling-handoff.md),
with requirements in [decompilation-tooling-spec.md](decompilation-tooling-spec.md).
The shared workspace combines physical ownership, Clang facts, actual linked
WASM providers, native correspondence, scoped policies and build evidence.
It can be inspected without opening a binary.

## Recommended workflow

Use the release executables for routine binary inspection and decompilation work.
Build both native tools once from the repository root, then reuse them:

```sh
cargo build --release -p binviz-cli -p binviz-mcp
```

Run `target/release/binviz` (`binviz.exe` on Windows) directly for subsequent
inspections. Here is the Windows setup from the repository root:

```powershell
$binvizExe = (Resolve-Path .\target\release\binviz.exe).Path
& $binvizExe workspace path\to\workspace.json --preflight --json
```

The `binviz` examples below assume that selected executable is on PATH. Configure
MCP with the absolute path to `target/release/binviz-mcp.exe` and restart the server
after rebuilding. When developing Binviz itself, use
`cargo build -p binviz-cli -p binviz-mcp` and the corresponding `target/debug`
executables for faster compile cycles. Debug and release are independent outputs.
Rebuild release at the delivery boundary after native Rust changes. A web/WASM build
updates neither native executable. Check the executable path your script or MCP
client actually uses when new commands appear to be missing.

1. Run `workspace --preflight --json` to collect identity mismatches and producer
   owners before expensive preparation. Repair or regenerate the affected inputs;
   preflight verifies identities, not behavioral correctness. Then inspect
   `workspace --blockers` and `--caller ID` for the selected work. These read-only
   queries need no browser session.
2. Generate current [compiler facts](compiler-facts.md) with the project's
   compiler/target configuration. Use its extraction cache and retain exact
   source/header/tool identities. Investigate stale reasons before editing policy.
3. Generate typed edits with `adapters`, and use `source-plans` when composing
   several plans, mapping reviewed spans or renaming one compiler declarator.
   Review the plan and apply it to a separate output with `apply-adapters`.
   Run one configured [build batch](build-batches.md) for the affected candidate
   stages. Reuse its cache and import its report with `--build-report`; preserve
   distinct baseline/candidate identities.
4. Run a [proof campaign](proof-campaigns.md) when the change needs behavioral
   evidence. Use persistent configured runners, exact participant roles and the
   memory/events/checkpoints relevant to the claim. Use `campaign-compare` to
   reinspect existing observations without executing runners again.
5. Inspect `--closures`, `--readability`, `--publications` and `--adoption` as applicable.
   Review candidate edits and publication plans before explicitly applying them.
   Readability acceptance preserves original-code scores and grants no new
   matching credit. Scoped matching publication recomputes selected exact matches.
6. Export the [progress treemap](progress-images.md) from the recorded notes after
   a milestone: `binviz progress game.exe --svg progress.svg`. PNG export is in
   the Progress UI. The image itself does not run a new matching build.

During implementation, run focused checks for the affected behavior. For a runtime
change batch, run the full Rust suite and build the affected native/web delivery
outputs once at the batch boundary; repeat checks
when a subsequent correction warrants it. Prefer CLI/MCP for repeated inspection.
Use the browser when the UI itself needs validation or an interactive view helps.
For local UI edits, use `npm run wasm:debug` followed by `npx vite` in `web/`;
rebuild debug WASM after Rust edits and keep Vite running for TypeScript/CSS edits.
`npm run build` makes the optimized production site and uses the slower release
compiler settings.

Documentation-only changes need link and example checks, not a native rebuild or
full test run. Reinspection through `campaign-compare` avoids rerunning runners;
it still verifies current evidence. Use cache diagnostics and staging/producer
timings to locate slow preparation before adding another build or browser pass.

## Choosing an entry point

The CLI, persistent MCP and Contracts UI expose the same core decisions. Choose
one interface for repeated work and switch when another view helps inspection.
Use the workspace as the joined view; standalone reports help diagnose one input.

| Need | Start here | Follow up when needed |
| --- | --- | --- |
| Check stale inputs | `workspace --preflight --json` | Producer owners identify what to regenerate |
| Select work or diagnose a caller | `workspace --blockers` / `--caller ID` | `--inventory` for physical ownership; `--callees` for certificates |
| Inspect compiler observations alone | `contracts FACTS.json` | Workspace joins selected providers, native audits and policies |
| Inspect actual linked ABI | `linked MODULE.wasm` | `--imports` narrows the report; adoption `import-inventory` also verifies reviewed host authority |
| Prepare source edits | `adapters WORKSPACE` | `source-plans` composes/maps/renames; `apply-adapters` writes the reviewed candidate |
| Build or prove the candidate | `build-batch CONFIG --out REPORT` / `campaign CONFIG --out REPORT` | Import build evidence; use `campaign-compare` for saved observations |
| Accept or publish work | Workspace `--promotions`, `--readability`, `--publications` | Each has a distinct acceptance scope; inspect its reasons before applying |
| Track progress | `progress BINARY --svg IMAGE.svg` | Progress UI for PNG; MCP `export_progress` for an open binary |

See the [feature overlap review](feature-overlap-review.md) for consolidation
candidates and the distinctions to preserve when simplifying these interfaces.

```text
binviz workspace workspace.json --json
binviz workspace workspace.json --preflight --json
binviz workspace workspace.json --caller a:decl:23:68:caller_a --json
binviz workspace workspace.json --inventory --json
binviz workspace workspace.json --blockers --json
binviz workspace workspace.json --storage --json
binviz workspace workspace.json --promotions --json
binviz workspace workspace.json --callees --json
binviz workspace workspace.json --closures --json
binviz workspace workspace.json --publications --json
binviz workspace workspace.json --readability --json
binviz workspace workspace.json --adoption --json
binviz workspace workspace.json --build-report build-report.json --json
binviz linked linked.wasm
binviz linked linked.wasm --imports
```

Artifacts resolve relative to the manifest, or `--root DIR`. `--strict` exits
unsuccessfully if caller packages remain unresolved or requested proof closures,
matching publications, readability batches or adoption requests are refused. JSON contains all findings;
the text summary separates raw observations, currently rejected callers and
unresolved packages. Raw compiler observations survive applied policies.

## Records and scope

`binviz-workspace`, schemaVersion 1, contains `evidence`, `inventory`,
`compilerFacts`, `bindings`, `policies`, `applications`, `builds`, `layouts`,
and optional `storage`, `promotions`, `calleeCertificates`, `proofClosures`,
`matchingPublications`, `readabilityBatches` and `adoptionRequests`. See the tracked
[synthetic workspace](../tests/fixtures/workspace/workspace.json) and its
[fixture builder](../tests/tools/build_workspace_fixture.py).

Inventory IDs identify asset, member offset/size, load address and execution
context. Same-address overlays remain different units. Primary functions,
shared fragments, assembly, exclusions and unknown/data gaps remain separate.
Analysis and matching extents are independent. Native audits consume the
configured exact analysis extent; inferred symbols cannot extend it. Native
member/slice bytes are verified against the actual asset. Collisions and alias
conflicts remain visible; retirement needs a current reviewed decision artifact.

Every binding identifies the compiler call, physical caller/provider, true
compiler definition, original JAL and delay slot, object, linked module, actual
exports and the exact linked call offset. Its reviewed correspondence artifact
must contain exactly the binding. Clang declaration availability does not select
a provider: Binviz checks actual object bodies/signatures and the linked call
target using its existing WASM reader. The reader is not a full runtime validator;
configure a validation stage when runtime validation is needed.

Policies pin allowed caller and site IDs, provider/definition, supplied argument
counts, registers and dependencies. `expectedCounts` means the supplied argument
count at each exact site. Extra-word nonuse requires both conservative provider
and post-call audits. Preserved unread return endpoints can be discarded only
when the independent continuation proves nonuse; they are never relabeled kills.
Pending loads at the native/C boundary, unknown calls and unsupported ABI cases
remain refusals. Equivalent policies retain independent scopes; conflicts refuse.

Eligibility does not consume a finding. An application needs the current edit
stage, a successful compile using its output and a current link into the selected
module. Stage identity proves declared lineage/content, not that an arbitrary
imported command record actually ran; maintained adapters retain their execution
diagnostics and recipes for reproduction.

## Browser and MCP

Open **Contracts**, import a workspace, and select a caller or translation unit.
The inspector includes all calls, provider selection, original call/slot words,
paired read/endpoint/frontier paths, policy decisions, build stages, address
references, gaps, storage and layout reports. Import a build report to merge
additional stages; existing identities cannot silently be replaced. Select
artifact files or a folder to verify actual bytes. Ambiguous basenames require an
individual file selection. Native navigation checks actual loaded bytes and
address mapping; an equal overlay address is insufficient. Export/reimport keeps
the input manifest and recomputes decisions rather than trusting status labels.

MCP uses the same records:

- `workspace_import` with `manifest` or `manifest_file`, `project_id`, optional
  `root` and `build_report_file`.
- `unit_inventory`, `caller_package` (`caller`), `contract_blockers`,
  `paired_register_audits` (`call`), `explain_policy` (`policy`).
- `plan_adapters`, `storage_evidence`, `promotion_plan`, `callee_certificates`.
- `proof_closures`, `matching_publications`, `readability_batches` inspect
  reviewed requests; `publish_matches` explicitly publishes a selected note scope.
- `workspace_preflight`, `adoption_evidence` and `source_plans` expose identity
  checks, reviewed adoption claims and immutable edit composition.
- Existing `next_functions` accepts `project_id` and an exact physical `unit`;
  current blocker/rejection counts supplement its existing readiness ranking.
  Completed, skipped and claim filters remain in effect.

Queries rehash current local artifacts. Inline imports without an artifact root
remain unverified. Reports are not truncated and malformed imports preserve the
previous retained workspace. Browser/WASM and native interfaces use shared core.

## Reviewable direct-call bridges

```text
binviz adapters workspace.json --policy a-only --out edit-plan.json
binviz apply-adapters edit-plan.json --artifact a:prepared --input caller.prepared.i --out candidate.c
```

The generator emits a deterministic typed wrapper only for an eligible scalar
extra-word site. Original expressions are passed once to wrapper parameters;
the provider gets its true formal arguments. Discarded extra expressions still
execute. It preserves function-pointer references and full return words. No
canonical input is overwritten. Before/after hashes and byte spans detect drift;
application is idempotent, and publication of the separate candidate is atomic.
Inspect and download the same plan/prepared C in the browser.

General signed-short conversions, return repair, aggregates, varargs, callbacks,
complex declarators and incomplete used-return closure remain refused. Planning
does not establish a successful compile or proof campaign.

## Storage and promotion

Optional callee certificates pin a physical provider, register, explicit
`nativeSha256` / `extent`, reviewed request artifact and native dependencies.
Replacing a manifest digest cannot reuse a review of different native bytes.
Binviz recomputes the complete exact-extent
audit with discarded-return endpoints distinct from kills. An all-killed result
can supply a kill summary; a mixed kill/preserve result supplies
`not-consumed-may-write`, keeping the incoming word live across the caller's
continuation. Certificates can compose a declared acyclic set of independently
verified child certificates. Child review/native dependency closure is required;
same-address ambiguity, stale inputs, pending loads, unknown calls and unresolved
cycles refuse. Optional observable-bit mode uses the existing auditor and retains
alias/frontier rules. These single-GPR facts do not establish callback closure.

A scoped policy names allowed `calleeCertificates` and retains all their reviewed
dependencies. The paired audits use only those current summaries. The browser
shows every certificate's native hash/extent, full audit and killed/discarded
endpoint lineage; CLI `--callees` and MCP `callee_certificates` return the same
records. The fixture includes a real native overwrite; regression tests add mixed
returns, composition, post-call consumption and stale/cyclic refusal cases.

Storage descriptions pin exact caller/site/provider, a reviewed JSON artifact
equal to the description and current dependencies. Objects describe size,
alignment, reservation, lifetime and initialized ranges independently. Provider
accesses describe offset, byte count (or unknown), read/write kind and rationale.
The report flags accesses outside the object and reads outside initialized ranges.
A sixteen-byte reservation does not make a two-byte C object safe for a four-byte
writer. Reviewed content identity is explicit; it does not prove access completeness.

Native SP transitions, saved registers and stack slots reuse the existing
address-order `mipsflow` observations. Linked WASM stack-pointer operations and
their instruction contexts come from actual module bytes. Neither report claims
a whole-program stack upper bound. Dynamic frames, recursion, unknown indirect
calls, IRQ/reentrancy and provider-copy closure remain visible limits.

Promotion requests identify accepted/candidate artifacts, separate producer stage
namespaces, pinned baseline/candidate recipe digests, prerequisites, exact
dependent consumers and explicit source/storage/proof-scope changes. Both stages
and their outputs must be independently current. A changed recipe, missing
baseline/candidate or incomplete consumer list refuses the plan. Use
[build batches](build-batches.md) to reproduce each namespace. Ready plans are
review artifacts: publication and regeneration of accepted policies are separate
steps. Historical artifacts remain unchanged and interrupted candidates are
separately recoverable.

## Scoped acceptance and proof closures

Optional `proofClosures`, `matchingPublications` and `readabilityBatches` are
reviewed requests in the workspace manifest. Every request pins dependencies by
artifact ID and SHA-256 and names a current `reviewArtifact` containing the exact
request. Missing, changed or unverified inputs produce a refusal with reasons.
Inspect the same records in Contracts, CLI `workspace --closures`,
`--publications`, `--readability`, or MCP `proof_closures`,
`matching_publications`, `readability_batches`.

Proof closure claims use these `kind` values:

- `service-chain`: selected module/root, ordered actual call offsets/targets,
  exact service module/field/parameter/result identity, and execution obligations.
- `shared-initialization`: actual module instantiation order, memory import,
  instance minimum/maximum, active segment writes, live regions and stack
  reservations. Each module needs one actual checked layout retained as an exact
  JSON artifact in the pinned reviewed dependencies. Writes into live regions require exact before/after byte
  restoration and ordered save/instantiate/restore execution observations.
- `callback-stack`: roots, finite reviewed indirect targets and return use,
  host stack/reentrancy profiles, recursion bounds and interrupt nesting. Actual
  call types and table initializers constrain targets; constant prologue frames
  are checked against linked instructions. Allocating frames require a verified
  stack-pointer operand and straight-line restoration before every return;
  branching frames require a separate path-sensitive certificate. Unknown targets, dynamic frames or
  unsupported stack transitions refuse. This is an upper bound for the reviewed
  finite model, with its assumptions retained, not a universal hardware bound.
- `scalar-varargs`: compiler-promoted scalar arguments, native argument-home
  prefix, reservation and executed obligations. Native home stores remain part
  of the exact provider span. Aggregate or unsupported variadic ABIs refuse.
- `missing-inputs`: a separate compiler-checked candidate call retains the
  original arguments and supplies the selected provider's missing inputs;
  execution obligations establish the reviewed fixture scope.

An execution obligation names a campaign artifact, exact case IDs and required
checkpoint. Binviz recomputes comparisons and requires current bound observations
and actual execution. An imported `passed` label is insufficient. Policies must
explicitly select applicable closure certificates and all their dependencies.
Call bindings can select a `physicalCallOwner` for a reviewed shared tail;
compiler caller identity remains visible and physical PC counts are deduplicated.

Matching publication requests select one unit and explicit physical functions,
compiler definitions, source artifacts, objects, producers and symbols. Binviz
recomputes strict original-code scores from current objects; all selected scores
must be exact. It preserves notes outside the selection and reports distinct
sources, unique physical functions and union coverage independently. Cached
matches outside the selection are retained without claiming revalidation.

```sh
binviz publish-matches workspace.json --publication selected-batch --out plan.json
binviz publish-matches workspace.json --publication selected-batch --publish
```

The second command rechecks current inputs and notes immediately before writing
through the existing notes journal. Newer journal changes require a new request.
MCP `publish_matches` provides the same explicitly requested publication action.

Readability batches pair separate baseline/candidate definitions, objects and
producer stages. Canonical compiler identities after preprocessing, ABI, frozen
inputs, compiler-visible paths and recipes must agree. Complete native objects
must be byte-identical; original-code scores are recomputed and partial baseline
scores retained without new matching credit. Every affected caller belongs in
the selected pair scope. The report is ready for source review and requires
regenerating downstream source-bound evidence; it grants no linked/gameplay proof.
`python tools/readability_batch.py CONFIG.json OUTPUT_DIR` runs the
configured build batch and shared acceptance inspection.

## Validation and migration

`cargo test --workspace` includes same-address overlays, complete findings,
exact policy scope, stale inputs, actual linked instructions and data exports,
applied-stage gates, edit drift, storage corrections and promotion refusals.
Fixtures are synthetic and distributable. The tracked fixture records its actual
Linux Clang identity; on another host that executable is missing until supplied.
Rust-only tests explicitly substitute a test compiler identity for join tests.

`python tools/ff9_collectors.py GAME_ROOT BINVIZ_EXE OUTPUT_DIR` compares frozen
game reports through shared Binviz backends without changing the game checkout.
The local FF9 comparison agrees on six native register audits, six frozen module
hashes and the zero/six-word CD service signatures. Its output pins the CLI and
all read inputs. No collector has been retired, and historical gameplay/scheduler
execution observations are not replayed or promoted by that comparison.

Cross-register child contracts and externally reviewed, native-byte-pinned loop
bounds now compose through the existing register auditor. Conservative alias,
pending-load and frontier rules remain. Compiler allocation/lifetime observations
and finite callback/stack certificates are available, while unsupported dynamic
or incomplete whole-program models continue to refuse.

The adoption follow-ups are available through `workspace --adoption --json`, MCP
`adoption_evidence`, and the Contracts workspace evidence panel. Add reviewed
`adoptionRequests` with distinct IDs, exact `dependencies` hashes, an identical
`reviewArtifact` JSON record and one `claim`:

- `snapshot-overlay`: a pinned JSON inventory of every relative member path and
  `members` containing `path`, `input`, and optional `completedOutput`. Completed
  replacements must have successful, fully pinned producer ancestry. Historical
  inputs remain recorded. `python tools/snapshot_overlay.py WORKSPACE ID OUT_DIR`
  recomputes the report and exports only to a fresh, nonoverlapping directory.
- `dependency-transition`: `before`/`after` completed proof artifacts,
  `changedFields` (exact JSON Pointer paths), byte-identical `semanticInputs`
  pairs and the complete affected `consumers` stage closure. Only reviewed
  dependency/provenance stamp fields can change. Changed providers require
  separate selected `providers` pairs, actual unchanged linked ABI, campaigns
  executing both configured module identities and exact `callerReview`.
  Changed caller CPP, prototypes, compiler recipes or objects require a new proof.
  This report does not replace an existing policy's hashes.
- `sdk-catalog`: a physical `unit`, `libraries` mapping artifact IDs to SDK paths,
  and source entries joining `native`, `recoveredSource`, `compilerFacts`,
  `definition`, immutable `upstreamRevision`, `upstreamFile`, `upstreamSource`
  and `license`. The existing SDK matcher recomputes matches and keeps aliases
  and release ambiguity. Optional `portable` providers require actual linked
  ABI and executed obligations; existing scoped policies are listed for reuse.
  Pinned revision labels are reviewed provenance, not a network attestation.
- `source-maps`: exact compiler `spans` and reviewed `maps` with source/target
  artifact hashes, `reviewArtifact`, and byte-equal nonoverlapping ranges
  (`fromStart`, `fromEnd`, `toStart`, `toEnd`). A removed blank line can be mapped
  explicitly; changed or ambiguous spans require compiler re-extraction.
- `import-inventory`: a `module`, exact typed `bindings` (`identity`, `authority`,
  `profileArtifact`) and `internallyResolved` endpoint names. Same-name imports
  retain their distinct signatures and multiplicity. Missing, unused or ambiguous
  host bindings refuse. `linked MODULE.wasm --imports` reads the same shared
  parser without a review or modification and reports module hash and type indices.

`source-plans` accepts a JSON request with `action: "compose"` and `plans`,
`action: "map"` with `plan`/`mapping`, or `action: "rename-declarator"` with
`definition`/`name`. Composed edits validate against one immutable buffer before
application, combine distinct helper insertions, and refuse conflicting edits.
Individual renames use Clang's identifier span, preserving comma siblings and
mixed object/function declarations. The output is an ordinary edit plan, applied
with `apply-adapters` to a separate file. MCP exposes `source_plans`.

`discarded-result` lowering retains true argument arity and evaluates each argument
once, while discarding the selected provider's result. It still requires actual
compiler non-use and independent native V0 continuation evidence. `direct-binding`
selects an equal-arity service with a verified service-chain certificate and true
ABI; it is a distinct policy from extra-word non-use. Plans retain the reference
inventory, compiler-buffer identity and selected native/linked correspondence.

Callee requests accept reviewed `indirectTargets` through the existing register
auditor. Target reviews pin provider, native hash, extent, PC and complete targets;
missing/drifted reviews refuse. Reports derive explanations from the recomputed
certificate's witnesses, preserving unread surviving values versus definite kills.
`data-closure` proof claims inspect actual object DATA symbols and real linked
backing, require complete producer pins, reject absent/zero allocations and
distinct backing overlap, and allow only explicit, exact aliases or reviewed
external providers with execution obligations. General import type inspection
alone does not establish DATA backing.
