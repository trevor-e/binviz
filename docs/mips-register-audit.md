# PS1 incoming-register audit

binviz::mipsaudit provides a shared, bounded first-read analysis for original R3000A instructions. It replaces the small native-tail and discarded-result CFG walkers without inferring behavior from reconstructed C prototypes.

Call audit(ExactExtent { start, words }, entry, register, &policy). The extent is contiguous original code and words use the caller's selected byte order. The entry must be inside it and the tracked GPR must be1..31. The caller verifies physical file identity, exact extent and any original hashes; the library does not infer them from names or neighboring functions.

Binary::audit_ps1_register(address, size, entry, register, policy) obtains the original words from verified loaded-address mapping. It requires supported little-endian MIPS input and checked32-bit aligned bounds. The requested size is exact: unavailable bytes, missing mapping and partial extents are errors, and discovered function symbols never clip or extend it. The method name explicitly chooses the PS1 instruction profile, including for a little-endian MIPS object whose ISA revision was not independently proved.

The report has three outcomes:

- consumed: at least one reachable instruction reads the incoming word, or an explicit reviewed return/callee contract consumes it.
- dead: every explored path kills the word or reaches an explicitly reviewed discarded return.
- unresolved: no witnessed consumption, and at least one unknown instruction, call, control destination, surviving return, live cycle or exhausted budget prevents a dead proof.

All read/end/frontier witnesses contain original instruction addresses and words, delay-slot annotations and a concrete entry path. A consumed report can also contain unresolved paths; it establishes possible consumption, not a complete classification of every path. The report embeds supplied policies so preserved-callee assumptions remain inspectable on continuation paths.

Immediate writes kill only after that instruction's operand reads. Loads and MFC0/MFC2/CFC2 leave the old GPR word visible to the following instruction. LWL/LWR consume the architectural base but use pending-load forwarding for their merge source when applicable. The default conservative mode explores both conditional successors without constant-condition or branch-feasibility inference. Jump/call/return slots run before transfers, with target operands read before the slot. Control in a delay slot is an explicit unpredictable frontier.

Unknown calls have no ABI clobber assumption. Reviewed callee summaries are keyed by exact destination and register, carry an evidence identifier and optionally a native instruction path, and distinguish consumed, killed, preserved, not-consumed-may-write and unresolved. Preserved and may-write summaries retain the word for examining a direct call's continuation; may-write does not assert that every path returns the register unchanged. Neither is a generic caller-saved register rule. A delayed load in a call slot needs a first-callee-instruction contract, so the whole-callee summary interface conservatively leaves it unresolved.

Reviewed indirect target sets must contain every possible destination and carry evidence. The library follows local destinations and accepts reviewed external tail-callee effects. JALR with rd=0 is a tail jump; it does not invent a return to pc+8. Non-RA links require a further continuation contract and remain unresolved. Empty target sets and unaligned external targets are refused as frontiers. Conditional-link and branch-likely encodings are outside this bounded PS1 control model.

Return use is explicit and defaults to unresolved. A true-void discarded return endpoint is labeled 'not a kill': it must never be promoted to a native callee kill summary. A load in the return slot remains unresolved because the caller's first instruction may read its old value. Graph cycles are termination frontiers; a convergent diamond is not a cycle. State limits produce a frontier instead of a favorable answer.

## Validation

Run:

    cargo test -p binviz mipsaudit --lib

The synthetic tests construct instruction words from fields. They include all12 boundary cases from the prior file11 battle audit: stores, conditional operands/slots, same-register read/write, old load values, overwrite, true-void return slots, unknown calls and branch/jump load delays. Additional groups cover native A2/A3 hazards, merge forwarding, delayed coprocessor reads, both branch paths, exact-extent exits, illegal slots, cycles/joins, reviewed summaries, architectural RA writes, indirect tail continuation and budget/input refusals.

An ignored assets-based test reads the user's original FF9 executable. Set BINVIZ_MIPSAUDIT_PSX_EXE and optionally BINVIZ_MIPSAUDIT_REPORT to an ignored output path, then run:

    cargo test -p binviz mipsaudit::tests::local_ff9_native_tail_contracts --lib -- --ignored

The selected authoritative extents are31718/1108bytes and548E8/312bytes. Original31718 kills incoming A3 at80031738 before a branch/read. Original548E8 never consumes A2/A3 and does consume A0/A1. Five reports agree with the earlier native evidence. The recorded validation separately checks the executable SHA256e30e40745d079aed7071c130785fb42406d8857bdf5484a101ec9baed143ee1c; original slice hashes are03a2dbecd1ae0edf481985d33c4c6a9e604e402523da9b22d6824b9b29354e07 ande46b832bb0db9eb5884fb7d1087f8bc967a71ee155deb4e37d1342ac6b345479. Reports and original words remain under ignored target paths.

The conservative analysis concerns normal valid instruction execution and architectural GPR reads. It does not prove exceptions/interrupts, memory safety, implicit values already copied outside the starting register, GTE arithmetic, whole-callee control from an ABI signature, or silicon timing. It neither emulates the game nor awards decompilation matching credit. The optional bit analysis and shared interfaces are described below.
## Observable bits mode

`mode: "observable-bits"` is optional; the default remains `conservative`.
`incomingMask` selects an exact nonzero 32-bit mask (hex strings are accepted by
CLI/MCP/WASM policy decoding). The existing state transfer and CFG now track
copies, masks, shifts, simple arithmetic, HI/LO aliases, delayed loads and store
byte lanes. Witnesses retain `bitsBefore` masks. Branches with locally established
constants can be pruned, so a locally initialized fixed counter can terminate;
unproved cycles and exhausted budgets remain unresolved. Signed arithmetic with
possible overflow is an observable exception frontier. Unknown operations retain
frontiers rather than dropping dependence.

Calls forget local constants. A live alias outside the starting GPR prevents a
single-register callee/return summary from proving it dead. A discarded endpoint
is still not a kill summary. `not-consumed-may-write` retains the incoming word
across continuation instead of claiming a kill or exact preservation. Workspaces
recompute content-bound single-GPR certificates, including mixed killed/discarded
endpoints and acyclic child composition; see [workspaces](decompilation-workspaces.md).
Used-return closure across callbacks, cross-register callee contracts and externally
supplied loop-bound proofs remain separate unsupported cases.

Use `register-use`, `register-use-batch`, the browser Contracts audit panel or MCP
`register_use` / `register_use_batch`; all use the same strict decoder. Workspaces
also show paired provider and caller-continuation witnesses with exact site and
physical-member scope. The workspace allowance consumer currently uses the
conservative mode, preserving its previous approval boundary.
