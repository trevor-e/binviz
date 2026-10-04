# Compiler facts and evidence lineage

Follow the [recommended workflow](decompilation-workspaces.md#recommended-workflow)
for command order and native build profiles. This page describes extraction,
cache identity and the compiler observations available to workspace audits.

Binviz ships a maintained Clang adapter in `tools/compiler_facts.py`. Clang
extracts the C observations; Rust validates identities and audits every extracted
direct call. The existing `contracts` reader also accepts legacy finding reports.
No adapter edits source or grants ABI exceptions.

## Produce and inspect a report

Configure an installed Clang, explicit target, flags and translation units:

```json
{
  "schemaVersion": 1,
  "compiler": "clang",
  "target": "mipsel-none-elf",
  "flags": ["-std=gnu89"],
  "units": [
    {"id": "caller", "source": "caller.c"},
    {"id": "provider", "source": "provider.c"}
  ]
}
```

Source paths and include flags resolve relative to the configuration directory.
Unit IDs must be distinct and contain only letters, digits, underscores, dots or
dashes. Python uses its standard library; Clang remains an external prerequisite.
The initial producer handles C, not C++ overloads or member calls.

```text
python tools/compiler_facts.py tests/fixtures/contracts/config.json target/contracts-demo/facts.json
binviz contracts target/contracts-demo/facts.json
binviz contracts target/contracts-demo/facts.json --caller caller --json
binviz evidence target/contracts-demo/facts.json --json
```

The tracked synthetic example produces eleven direct calls and six findings:
one count mismatch, two consumed void results, argument and result narrowing,
and one missing definition. It also retains a stored callback reference, K&R
parameter types, typedef/pointer spellings and grouped function declarations.
The snapshot `tests/fixtures/contracts/facts.json` tests importing observations
without requiring a local compiler. Regenerate live evidence with your compiler;
the snapshot's external compiler/recipe files are not shipped or verified.

JSONL progress on stderr reports preparation/extraction stages, unit counts,
elapsed time and gaps. Failed compiler setup or translation units yield explicit
gaps and exit 1 while preserving successfully extracted calls. Cancellation
inside a unit preserves partial coverage and labels subsequent units unexamined.
Set `"cache": ".binviz-compiler-cache"` to reuse typed AST extraction. The cache
directory must stay inside the configuration directory. Keys bind the unit and
prepared span identities, prepared bytes, selected source/header bytes, actual
compiler and adapter hashes, compiler flags/target layout and environment hashes.
Cached payload digests are checked, corrupt entries are rebuilt, and concurrent
identical keys share an atomic writer lock. Unknown flags that may load plugins or
response files disable reuse with an explicit reason. Failed extractions are not
cached as successful facts.

The cache retains optional named intrinsic and LLVM storage/IR payloads, with
their digests and paired storage fields validated. `-w`, common/wrapv and
strict-aliasing/delete-null-pointer-checks semantic flags are explicitly supported
without stripping them from the compiler recipe. Plugin/response-file inputs
still disable reuse. Regression coverage includes real Clang cold/warm trap and
storage extraction with unchanged flags, identical facts/IR and zero warm
extraction subprocesses. Preparation and target discovery still run.

Preprocessing and target discovery always run. This detects a new header earlier
in the search path even when the formerly selected header has not changed.
Policy-only decisions reuse the raw facts without calling Clang. JSONL events
and `facts.artifacts/cache-report.json` report hit/miss reasons, elapsed time
and separate bootstrap/preparation/extraction subprocess counts. Tests demonstrate
two cold extraction processes, zero warm extraction processes, leaf-only
invalidation, corrupted output recovery, header shadowing and environment changes.
No game image is needed by these fixtures.

## Interchange and audit scope

`format: "binviz-compiler-facts"`, `schemaVersion: 1` identifies the shared schema.
It contains compiler configuration/data layout/target widths, translation units,
body definitions and separate prototypes, direct calls with supplied/promoted
argument types and result use, address-only references, extraction gaps, and an
artifact/stage manifest. Declaration IDs use unit, prepared byte span and symbol;
call IDs use unit and prepared span. Grouped declarators retain a common group
identity and separate per-symbol ranges; no semicolon boundary is assumed.

Clang preprocesses once into a retained `.prepared.i` file, then parses it as
`cpp-output`. Source spans are exact UTF-8 byte offsets in that prepared artifact,
not guessed offsets in the original source. This slice does not map expanded
macro references back to editable canonical source. Complex return declarators,
floating point, aggregates, varargs and wider words remain explicit unsupported
profiles. Basic typedef expansion comes from Clang typedef nodes; spellings are
retained separately. `oldStyle` labels an unspecified compiler function type;
K&R definitions still retain their actual compiler parameter types.
`parameters` retains the formal variable types, while `abiParameters` retains
Clang's callable function type. For a K&R `short` formal, the incoming contract
can be promoted `int`; the audit uses that incoming contract rather than treating
the local variable's narrowing as a caller mismatch.

Audit joins prefer same-TU bodies and preserve internal linkage. Multiple external
bodies require explicit ownership resolution. A prototype alone is a missing
definition, never provider evidence. Native physical ownership is not established
by a translation-unit ID. The initial `c32-scalar` audit compares integer/pointer
word width and signedness; it does not prove pointer compatibility, native O32
lowering, source correctness or gameplay behavior. Matching wire representations
can still contain unresolved source or native behavior.

Each finding has a stable SHA-256 ID, call-site/declaration/provider identities,
source span, argument evidence and reason. All findings are reported, rather
than stopping after a caller's first mismatch. Address references stay separate;
indirect calls produce an extraction gap. Both void casts and discarded comma
operands retain discarded-result observations. No native/C call correspondence
is inferred from list order.

## Verification

Import starts unverified. Artifact records keep raw SHA-256 separately from
optional LF-normalized text SHA-256. Raw LF/CRLF differences remain visible.
Recipes bind compiler version, flags, target properties, working directory and
adapter version/hash. Prepared stages depend on the original source, actually
selected include files, configuration, compiler, adapter and recipe; extracted facts depend on
the prepared input and tool identities. Imported compiler metadata and per-unit
records must match their recipe/fact artifact hashes before auditing.

`evidence` reads actual artifact bytes once per distinct path, compares digests
and propagates changed/missing dependencies through the stage DAG. Cycles,
unknown references and duplicate producers are refused. Output corruption is
checked independently of inputs or timestamps. The command reports every reason
and exits nonzero if any identity is not verified. Paths resolve relative to the
report's directory, or an explicit `--root`. Evidence produced inside a container
should be verified in that filesystem context, or with matching supplied bytes;
a host without the recorded compiler file correctly reports it missing.

Verified means the supplied artifacts and lineage are current against the
imported manifest. It does not make a supplied policy eligible, establish
native behavior or prove that a report was produced by a trusted compiler.
The byte validator does not rerun include resolution: a newly introduced header
that shadows a recorded dependency requires fresh extraction. Detecting such
build-environment changes is handled by fresh preprocessing in the extraction
adapter and by include-tree snapshots in the incremental build adapter.

The library also introduces separate physical `UnitId`, `FunctionId` and native
analysis/matching extent records. Their address fields use exact `0x` hex strings,
including 64-bit values. Boundary consistency checks reject functions crossing a
member. Physical inventory, collision reports and native/compiler joins are
described in [Decompilation workspaces](decompilation-workspaces.md).

## Browser and MCP

Choose **Inspect compiler call contracts** on the landing page, or **Call
contracts** with a binary open. Import facts or legacy findings, filter caller,
callee, unit, kind and identity, and export selected evidence. **Verify local
artifact** associates selected bytes with an explicit artifact ID; repeat for
dependencies. Changed bytes show stale reasons. `contentState` records whether
the bytes match, separately from current lineage; matching prepared bytes allow
source excerpts even when missing dependencies leave the stage stale. The
excerpt labels that lineage state. Full stage checks remain inspectable.

**Native register audit paths** runs a [register-use batch](register-use.md)
against the currently loaded PS1 binary. Read, endpoint and frontier paths show
individual original PCs, words and delay-slot markers, alongside the complete
policy and witnesses. This does not establish a paired caller/provider allowance.

Persistent MCP sessions expose:

- `compiler_facts`: validate/read facts from `report` or `report_file`; retain
  their audit under `report_id` (default `current`).
- `call_contracts`: import facts/legacy reports or query a retained `report_id`,
  with exact optional `caller` and `callee` filters. No binary required.
- `verify_evidence`: hash local artifacts from an imported or retained report;
  use `root` for inline/session reports. Returns statuses even when stale/missing.
- `register_use`: one strict batch-schema `request` on a loaded binary.
- `register_use_batch`: the complete versioned `batch` on a loaded binary.

Structured JSON is returned without the text-tool truncation limit. Failed imports
leave the retained report intact. The CLI, MCP and WASM use the same policy/batch
parser and native auditor; a discarded return remains distinct from a kill.

Physical ownership, paired scoped policies, edit plans, campaigns, incremental
batches and storage observations are now documented in
[Decompilation workspaces](decompilation-workspaces.md). The
[tooling spec](decompilation-tooling-spec.md) records proof frontiers and migration
work that remains open.

Optional compiler configuration `storageFacts: true` emits actual LLVM IR with
the same compiler/flags and records allocation and lifetime instructions, exact
IR artifact/line identity and explicit unsupported-size or dynamic frontiers.
The IR is a content-addressed extraction output. These observations describe
compiler storage decisions; they do not infer complete C lifetimes or a native
stack upper bound. Warm extraction caches include this configuration flag.

Compiler intrinsics retain stable names, caller/unit identities, argument types
and prepared source spans. `__builtin_trap`, `__builtin_debugtrap` and
`__builtin_unreachable` have explicit effects; unsupported builtins retain named
records and gaps. They are not invented native provider addresses.

Tag names occupy a separate C namespace during typedef expansion: `RECT` may
expand to `struct RECT`, while `struct RECT *` stays unchanged. Canonicalization
is idempotent, and written spellings remain available for declarations. Case and
default labels retain statement result-use context. Definitions additionally carry
`nameSpan` for individual function declarators, and direct calls carry `calleeSpan`;
both refer to the exact prepared UTF-8 buffer. Comma siblings and mixed object/
function declarations can be edited through these compiler locations without a
lexical C parser. Raw-source edits require an explicit reviewed source mapping
when prepared bytes differ; blanket line-number shifts are not inferred.
