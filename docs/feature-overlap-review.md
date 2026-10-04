# Feature overlap review

Reviewed 2026-10-04 against the shared Rust modules, CLI/MCP entry points and
maintained Python adapters. Simplify the recommended workflow first, consolidate
internal dependency traversal next, and retire compatibility interfaces only after
their consumers migrate. Most apparent feature overlap already shares an engine.

This pass centralizes usage advice in the workspace
[recommended workflow](decompilation-workspaces.md#recommended-workflow) and adds
a [command guide](decompilation-workspaces.md#choosing-an-entry-point). README,
reference and adapter docs link there. The repeated preflight guidance was removed
from the adoption appendix. No public command or evidence schema was removed.

## Consolidation candidates

| Candidate | Evidence and proposed trim | Required before changing behavior |
| --- | --- | --- |
| Producer ancestry helpers | `EvidenceManifest::dependency_closure`, adoption `producer_closure` and publication `ancestors` each traverse producer lineage. Move traversal into the evidence module; let callers add their own checks and stage/artifact projections. | Preserve completed-producer checks, exact review pins, missing/ambiguous/cyclic lineage refusals, and each helper's different return set. Adoption needs producer stage IDs; publication currently starts from a stage and includes stage IDs. These are not drop-in replacements. |
| Legacy FF9 collectors | `tools/ff9_collectors.py` compares frozen game outputs with shared backends. Prefer shared workspace/report queries once the game consumers use their schemas. | Existing comparisons cover six native audits, six module hashes and CD service signatures. Replay broader caller, scheduler/device and negative-control cases before retiring collectors. The current comparison does not establish full migration equivalence. |
| Legacy CLI aliases | The reference retains aliases such as `at`, `xrefs`, `callgraph` and `dwarf-*` alongside the grouped commands. Present the grouped commands in new examples; keep aliases as compatibility shims. | Locate script/MCP consumers and migrate them before removal. An alias does not require a second analysis engine, so deletion alone has little performance benefit. |
| Python launch paths | CLI `build-batch`/`campaign` and direct Python invocation launch the same maintained adapters. Recommend the CLI path for ordinary use and retain direct invocation for adapter development. | Preserve interpreter selection, argv, report paths and exit/refusal behavior. Removing a launcher would reduce surface area, not producer work. |

These are review findings, not completed refactors or measured speed improvements.
The dependency helpers are the clearest code cleanup candidate; benchmark the
repeated graph scans before treating consolidation as a throughput fix.

## Shared engines to keep

| Apparent overlap | Why keep the entry points |
| --- | --- |
| CLI, MCP and Contracts UI | Different interaction modes over the shared core. MCP keeps binary sessions open; CLI suits scripts; UI supports visual inspection. Prefer native queries for repeated work. |
| Compiler `contracts` and joined workspace caller reports | The standalone reader diagnoses extracted observations; the workspace joins physical ownership, actual linked providers, native audits and scoped policies. Raw findings remain available after policy application. |
| `linked`, `linked --imports` and adoption `import-inventory` | The first two project the existing WASM reader's module report. The adoption claim additionally checks reviewed typed host authority. An import list alone cannot replace that check. |
| `register-use`, callee certificates and workspace paired audits | All use the existing MIPS auditor. Certificates bind reviewed scope and dependencies; paired audits additionally inspect the caller continuation. Preserve unread surviving values separately from definite kills. |
| `adapters` and `source-plans` | Adapters derive eligible typed call edits; source plans compose, map or rename reviewed bytes and reuse the edit application routines. Keep these phases separate so composition cannot grant new policy eligibility. |
| SDK matching and adoption SDK catalogs | Catalogs reuse the existing signature matcher and add physical ownership, source provenance, license and optional portable-provider evidence. Preserve release and alias ambiguity. |
| Text/JSON progress, SVG export and the Progress UI | These share the recorded progress inventory and SVG renderer. PNG is a browser export. Keep one status source and exact-match byte accounting; export does not execute a matching build. |

## Distinct acceptance scopes

Do not merge operations solely because each compares a baseline and candidate:

- Promotion reports replacements and downstream invalidation while retaining history.
- Readability acceptance requires complete object equality and preserves original
  matching scores; it publishes candidate source without new matching credit.
- Matching publication recomputes exact selected matches and writes the reviewed
  note scope only when explicitly published.
- Snapshot overlays export reviewed immutable inputs and completed replacements
  to a fresh directory; historical evidence remains recorded.
- Dependency transitions verify permitted provenance changes and affected consumer
  closure. They do not silently refresh policy hashes or replace behavioral proof.

Likewise, compiler extraction caching and build-stage caching have different keys
and validity boundaries. Extraction still preprocesses and discovers target inputs;
stage reuse depends on declared input/tool/include identities. Share utility code
only where semantics agree. Software provider comparisons, hardware observations
and callback-interface tests retain their separate authority.

## Applying a trim

Choose one internal candidate per batch. Record existing consumers and observable
report/refusal behavior, move the common implementation behind existing entry
points, and run focused positive and stale/ambiguous-input controls. Rebuild the
affected delivery binaries after a runtime change. Public removal needs a documented
replacement and consumer migration; neither API count nor code deletion demonstrates
a speed gain. Documentation-only consolidation needs no binary rebuild.
