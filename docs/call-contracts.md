# Inspect compiler call-contract findings

`binviz contracts` displays imported findings about calls in reconstructed C.
It complements the binary's existing callers/callees and call graph. It does
not load the original binary, parse C itself, check the reported hashes, grant
ABI exceptions or establish native behavioral equivalence.

```
binviz contracts all-caller-contracts.json
binviz contracts all-caller-contracts.json --callee sub_800ab6f0
binviz contracts all-caller-contracts.json --caller sub_800bbf5c --json
```

The text view shows caller and callee names, source path, the caller declaration,
actual supplied argument counts, actual definition signature, mismatch reason,
reported typed call shapes and reported source/native extent hashes. Unspecified
C parameter lists are labeled. Filters match exact names, preserving overlay
namespaces and case distinctions. Text output defaults to30 findings; `--top N`
changes the limit. JSON output contains every matching finding, preserves
additional adapter evidence and reports updated filtered totals. Uncached
callers remain visible after filtering. These are unknowns, not clean calls.

The report format contains `contracts`, `scope` and optional `cacheMisses`,
`contractCount` and `distinctCallees`. Each finding contains `caller`, `callee`,
`source`, `kind` (`argument-count` or `void-result`), `definitionAbi`,
`declaredCallerAbi`, `actualCallCounts` and `allResultsDiscarded`. Signatures have
`returnType`, `parameterTypes` and optional `variadic`. Hashes and `oldStyle` are
optional. Additional evidence fields survive JSON export. Contradictory counts
and mismatch descriptions are refused; this checks internal report consistency,
not whether its observations are true or current.

FF9's current `build/wasm/resident-battle-full/all-caller-contracts.json` is one
compatible producer. Refresh its compiler audit after source/definition changes;
an old imported report still describes old inputs. Already-reviewed policy cases
may appear in an audit report, so its finding count is not the strict link-blocker
count. The adapter's scope is displayed verbatim in text.

## Browser inspection

Open a binary in binviz, select **Call contracts**, then **Import compiler
report**. The browser uses the same Rust report reader as the CLI. Filter by
exact caller/callee names and expand a finding to see its declarations,
definition and full reported evidence. Uncached callers and report metadata
remain visible. A rejected import retains the previous report. Imports are
local, independent observations; opening a binary does not verify their source
or native identities. Reports up to16MiB are accepted, and untrusted strings
are displayed as text. Large integer metadata is shown without losing digits.

Native register dataflow is now in the shared library and batched CLI
[register-use](register-use.md). Clang remains the compiler-specific type
adapter. Further work can attach verified source/assembly locations to graph
edges and bind imported reports to immutable input identities.

Validation: library tests cover contradictory count/result reports, exact-name
filtering, retained uncached callers and retained adapter metadata. The CLI is
also exercised against FF9's actual imported compiler report.
Ten Chrome checks cover the real145-finding import, filtering to three caller
findings, expanded evidence, retained results after a refused import, safe text
rendering and exact64-bit metadata display. Type checking and frontend bundling
also pass.
