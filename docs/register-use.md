# PS1 register-use audits from the CLI

`register-use` follows one incoming GPR word through an explicitly supplied
little-endian PS1 MIPS extent. It reports `consumed`, `dead`, or `unresolved`,
with original instruction paths for reads, ends, and unresolved frontiers.
It does not infer a C prototype, function boundary, or ABI call clobbers.
See the [library model and limits](mips-register-audit.md).

```text
binviz register-use game.exe 0x80031718 1108 0x80031718 a3 --json
binviz register-use game.exe 0x800548e8 312 0x800548e8 a2
binviz register-use game.exe --batch requests.json --json
```

The required single-request inputs are the loaded address, exact size in bytes,
entry address within that extent, and GPR. Numbers accept decimal or `0x` hex;
registers accept names (with optional `$`), numbers 1..31, or `s8` for `fp`.
Addresses and size must be aligned to four bytes and fit the PS1 32-bit address
space. The selected binary must actually map every requested byte; inferred
function names and annotations do not clip or enlarge the requested extent.
An overlay may use the existing `--overlay-at` loader option. Malformed arguments,
unsupported architecture, missing bytes, and invalid policy produce a nonzero
exit status, never an empty success. An `unresolved` analysis is a valid report
and exits successfully; consumers must inspect the outcome and its witnesses.

With the original US FF9 executable `SLUS_012.51` (SHA-256
`e30e40745d079aed7071c130785fb42406d8857bdf5484a101ec9baed143ee1c`),
the first command finds A3 killed at `0x80031738`. The second remains unresolved:
A2 survives to the return. For `0x800548e8`, A0 and A1 are consumed; A2 and A3
are not consumed inside the extent, but their callers' use after return requires
an explicit review. These statements apply to those exact supplied extents and
that input identity, not every binary at those addresses.

## Explicit reviewed policies

The default policy has `returnUse: "unresolved"`, no callee or indirect-target
summaries, and a state budget of 16384. `--policy reviewed.json` supplies a policy
for a single request. Only the documented fields are accepted:

```json
{
  "returnUse": "unresolved",
  "maxStates": 16384,
  "callees": {
    "0x80020000": {
      "register": 2,
      "effect": "killed",
      "evidence": "reviewed native first-write proof identifier",
      "instructionPath": [{"pc": "0x80020000", "word": "0x3c020001"}]
    }
  },
  "indirectTargets": {
    "0x80010020": {
      "targets": ["0x80010040", "0x80010060"],
      "evidence": "reviewed complete jump-table proof identifier"
    }
  }
}
```

`returnUse` accepts `unresolved`, `discarded`, or `consumed`. A discarded return
is an explicit caller assumption. An end under this policy does **not** prove an
instruction killed the word, and cannot automatically establish a callee kill
summary. For the FF9 A2 example, supplying `{"returnUse":"discarded"}` changes
the report to `dead` because the reviewed boundary discards the surviving word.

Callee effects accept `consumed`, `killed`, `preserved`, or `unresolved`. The
address-keyed callee summary applies only to its declared register. Indirect
targets are keyed by the jump instruction PC and must list a reviewed complete
successor set. Evidence must be nonempty; reviewed instruction paths are optional.
Address keys, PCs, words, and indirect targets accept decimal or `0x` hex. Numeric
GPRs in policies remain integers. Unknown fields, duplicate normalized address
keys, empty target sets, and invalid alignments are refused. Policies are
assertions supplied by the reviewer: the CLI embeds them in its output but does
not independently verify a supplied summary, its evidence, or a target table.
Unknown calls and indirect control remain unresolved without applicable evidence.

## Batches load the binary once

The strict batch schema contains version 1 and 1..4096 requests. Every request
requires a unique, nonempty string ID and all four exact audit inputs. An
optional inline policy applies only to that request; no batch-wide return or
callee assumptions are inherited.

```json
{
  "schemaVersion": 1,
  "requests": [
    {"id": "matrix-a3", "address": "0x80031718", "bytes": 1108,
     "entry": "0x80031718", "register": "a3"},
    {"id": "helper-a2-default", "address": "0x800548e8", "bytes": 312,
     "entry": "0x800548e8", "register": 6},
    {"id": "helper-a2-reviewed", "address": "0x800548e8", "bytes": 312,
     "entry": "0x800548e8", "register": "a2",
     "policy": {"returnUse": "discarded"}}
  ]
}
```

JSON output is `{"schemaVersion":1,"requests":[{"id":"...","report":{...}}]}`,
in request order. The single-request JSON remains the library audit report itself.
Readable output includes each request ID, disassembly, delay-slot marks, policy,
and all witnesses. All requests are parsed and their loaded extents audited
before publishing any output. A refused request fails the whole batch with its
index or ID and no partial JSON; unknown behavior produces an explicit
`unresolved` report with frontier paths. This makes batches suitable for replacing
repeated per-function CFG scripts without silently creating favorable findings.

`cargo test -p binviz-cli` covers strict inputs/policies, batch refusals and a
synthetic loaded MIPS return showing separate default/reviewed policies. Optional
original FF9 checks in the library require the user's executable; no game
instructions or assets are included in these examples or fixtures.
