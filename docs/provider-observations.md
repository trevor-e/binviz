# Optional provider observations

Binviz accepts normalized pseudocode or dataflow from an optional analysis provider,
including Ghidra. This is an interchange seam; Binviz does not install, launch or
automatically export from Ghidra. Existing analysis and acceptance remain in the
shared Rust core.

```text
binviz analysis target.exe observation.json
```

MCP `analysis_observations` accepts `report` (JSON text) or `report_file` and the
loaded `binary` ID. With neither import argument it returns retained observations;
`at` selects an exact entry. Rejected imports preserve prior state. Repeated
byte-identical imports are deduplicated. Closing the binary releases its observations.
`decomp_context` includes observations for the same entry in a separate advisory
section. Query the import tool for complete JSON if the normal context is truncated.
WASM exposes `inspectAnalysisObservation` for the currently loaded binary.

The normalized v1 input has these fields (the digest markers must be replaced):

```json
{
  "format": "binviz-analysis-observation",
  "schemaVersion": 1,
  "provider": {
    "name": "ghidra",
    "version": "12.1.4",
    "profileSha256": "<SHA-256 of the retained provider configuration>"
  },
  "targetSha256": "<SHA-256 of the exact loaded binary/member bytes>",
  "architecture": "<exact arch string from binviz info JSON>",
  "addressSpace": "default",
  "entry": "0x401000",
  "start": "0x401000",
  "bytes": "0x20",
  "nativeSha256": "<SHA-256 of those 32 native bytes>",
  "pseudocode": "int example(void) { return 1; }",
  "dataflow": null,
  "evidenceIds": [],
  "unknowns": ["Provider-recovered prototype; not independently verified"]
}
```

Retain the actual provider version and configuration with the export. IDs and
unknowns preserve its source evidence; Binviz does not authenticate those IDs or
validate the provider-specific dataflow schema. Do not relabel original provider
results as runtime observations. The `profileSha256` is a caller-supplied commitment,
not evidence that the provider executed with that profile.

The reader verifies the whole target digest, exact architecture/default address
space, entry containment, every address-to-file mapping in the selected extent,
and the extent digest. Digests are lowercase SHA-256; addresses/counts use exact
`0x` strings. Input is limited to 32 MiB and an extent to 16 MiB. Missing native
bytes, internal mapping holes, changed load addresses or stale contents refuse.
Only contiguous file-backed extents are supported in v1; split functions require
a future explicit multi-range schema. The extent never replaces recovered or
reviewed function boundaries.

`identityState: verified` means supplied artifact/extent bytes agree. Provider
execution, recovered semantics and profile authenticity remain unverified.
Import creates no notes, native proof, policy applications or matching credit.
