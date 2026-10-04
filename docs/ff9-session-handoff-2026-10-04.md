# FF9 disc-1 decompilation handoff — 2026-10-04

The user requested this session stop so work can resume in a handoff session.
All three agents report no live processes, exec sessions, Docker jobs or Node
slots. Root reproduction session43984 ended with exit0; the cold reproduction
and shared linked inspection also ended with exit0. The goal is paused for the
handoff, not achieved. Scope remains disc1: recover the complete game and build
a playable WASM browser version. Do not substitute selected-module compilation
or a bounded compatibility proof for whole-game completion.

## Read first and workspace state

- Game: `C:/Users/elkin/dev/ff9-decomp`; read this file, `CLAUDE.md`, then
  `docs/binviz-runtime-adoption-checkpoint.md`.
- Binviz: `C:/Users/elkin/dev/binviz`; inspect
  `target/ff9-next-integration.json` and `target/ff9-session-handoff-seal.json`.
- User explicitly authorized parallel agents, SDK reuse and use of rebuilt
  Binviz. Use shared batch/workspace/linked/campaign capabilities first.
- `docs/ff9-sdk-reuse-audit.md` and `docs/decompilation-tooling-handoff.md` in
  Binviz preserve public-source references and reusable feature findings.
- Existing Binviz changes include another session's Rust/UI work. Do not
  overwrite executables, rebuild over pinned tools, or commit the entire tree.
- Game dirty state unrelated to this handoff includes tracked
  `wasm/__pycache__/mkmod.cpython-311.pyc`, untracked caches, and four file11
  unit-init files: `wasm/runtime/build-file11-unit-init.py`,
  `check-file11-unit-init.mjs`, `file11-unit-init-policy.json`,
  `file11-unit-init.py`. Preserve those other-session files.

## What now works

The first actual selected world module is linked: `binviz/target/ff9-sr12`.
All295 selected callers compile and link with the actual registry and one RAM;
0 refused. This excludes native B7098, so it is not the complete296-entry world
module. Module is `actual-linked/boot-g0_file12-resident.wasm`,1372548bytes:

`3c3da705d6f7d71a8d34721eeba969cbda69dcd9b86dc8ec1d400c594237d400`

Stage freeze `9cb7acf66d99b9b6a45b8b70392e358783867a156cb9e72c7bfe0ecedc5882bf`.
Cold202.1064s, producer133.0866s; warm0.39054s/0subprocesses. All8 new shared
artifact identities verify. No need to repeat this full build on resume.

Root independently reproduced actual linked host composition, all295 registry
mappings, string success/missing-binding paths and12 linked metadata refusals.
Root freeze `target/ff9-root-linked-host-composition-review/root-frozen.json`:
`124b03addf77b6e2d4a693ed8f13c1bf7b1ce1cf3710089fce009ce59429922f`.
The actual final module also passes2160 completed BOOT dispatch comparisons
and80 retained CD frontiers; rootfreeze
`target/ff9-root-linked-transit-review/root-frozen.json`:
`98b316a1c442080d8cf3cafa8dd137374c9122e217fa0a188010687ec04b34f1`.

The original cold load stopped at BIOS FlushCache after copying all190464
world-image bytes correctly. That failure is retained unchanged in
`target/ff9-file12-cold-load`, freeze2a1640af…bde178. A separate guarded private
software contract now allows the genuine BOOT12900(3) return through its own
critical exit, followed by full physical-image/all295 member checks and actual
registry activation. Exact module/owner/image, completed CD, IRQ/callback state
and single-use invocation are required; no global BIOS no-op was installed.
Four discarded software words0/256/DEADBEEF/FFFFFFFF give identical meaningful
effects.14 refusal cases plus missing-profile cold attempt pass. No ROM result,
cache timing or nine other FlushCache callers are certified.

- Candidate `target/ff9-file12-flush-cache-prototype/frozen.json`:
  `02acf6c59a01797225e3c1c890412e9effb2af3f735ab981fb73a1fb7940f7b3`.
- Root independently reproduced it;
  `target/ff9-root-file12-flush-cache-review/root-frozen.json`:
  `a0cfd58151b8860f1a8ab728d81f0a8c43cb8ed9769152b3935ae7d1f27f3ed4`.
- Offline command, from Binviz:
  `python -B target/ff9-file12-flush-cache-prototype/reproduce.py --out-dir target/FRESH-FLUSH-REVIEW`.

## Exact next runtime action

`target/ff9-file12-world-init/handoff.md`, `checkpoint.json` and
`executed-world-init.json` preserve the latest genuine scene-initializer attempt.
Freeze273files:
`3d1122769ca102c571e49e3e5a9d724fdc5768f1b55f68a2f4a6310eb46d7800`.
This is agent-executed evidence, not an additional root runtime reproduction.

After real cold load/registry activation, the actual resolver invokes A8B14
void0. Native A8860's first call has no omitted semantic prewrites: only its
stack/saved-register prologue precedes it. No harness page/context values were
written. A8B14 initializes genuine screen3/context/function-pointer slots, then
reaches real A8FF0→217C4 CD streaming. Actual selector0 chooses resource2712;
page remains genuine2331.

It stops at the existing `cd-host.mjs` fixed cumulative256-busy-poll guard on
poll257. CD is still progressing: loaderstate3, readingtrue, sector callbacks
through LBA2677/destination80101800. Counts2364hostcalls/244events/234archive
reads/727526bytes/15026traceevents are diagnostic only. Initializer has not
returned; resource completion is unproven. Worker was discarded after the
exception; never resume that thrown compiled stack.

Resume by inspecting resource2712's actual archive extent and advancing trace.
Review an explicit bounded/configurable busy budget in a fresh private runner
instead of treating this harness limit as a game/platform blocker. Current
maxHostCalls does not configure the fixed256busy limit. Preserve the baseline
failure, do not fake CD completion, and do not invoke A88C4/A924C/page-loop or
B7098 until genuine initialization/preconditions are established. Existing
`node target/ff9-file12-world-init/check-world-init.mjs --word 0` still fails at
the retained budget and need not be repeated unchanged.

## Scratch stack and built Binviz feature correction

B7098 remains excluded. Historical366 execution checks/114 refusals are a
bounded proof, not production transitive closure. Read
`target/ff9-file12-scratch-closure/checkpoint.md`;4file freeze:
`48bae10c7c5a5dead6d27d1364af576627f6a40aede1ca9152a44502d85a6ec4`.
Join actual native and linked call graphs, capture current excluded-driver and
callee inputs, establish the real mutable compiler-stack global using existing
helpers, and bound indirect/host/callback/IRQ/exception effects before admission.
Do not install a no-op SP service or silently advance old source-bound proofs.

**Existing Binviz `linked <wasm> --json` already exposes the read-only import/type
inventory.** Earlier missing-API/fnptr-trace proposal is historical/superseded.
Debug executable SHA
`9e105a035c7449808ebe646e849048cdf5bfe3e52130c8a4f2d7ed8d6891ab3e`
reports functions[].index/imported/importIdentity/parameters/results/calls/
indirectCalls/stackPointerOperations. All286 ordered function imports and four
duplicate-name/signature pairs match the frozen legacy capture. Root reran the
shared command, producing identical report97d42a2d…c1f339; rootfreeze
`target/ff9-root-linked-inventory-review/root-frozen.json`:
`35934a154dd3f66daee31ed634bd0264230513da9cb73eb0dd1a47be486cc57b`.
Empty stack observations do not prove zero-frame bounds: actual sr12 does not
establish the exported mutable __stack_pointer authority the reader uses.

## SDK sound reuse checkpoints

PSY-Z pinned21cbe23573f2b1c1b778ebad9729327744af628b provides recovered SDK C
and portable implementations. Version compatibility is function-specific;
FF9-specific AKAO/engine code remains game work. Its24voice mixer was already
root-reproduced, retaining existing512KiBSPURAM/device authority.14groups,
48device regressions,120nativeENVX/95native mask histories pass. Actual CD-enabled
mixing still refuses without PCM; successful diagnostic quantum explicitly
uses synthetic zero CD. Native sample timing, sweep/negative gain, END+mute,
reverbwrap, noise/PMON/IRQ remain unproved. Public renderAudio stays refused.

Native envelope getter5931C truevoid2 plus real588B0/58830 caller closure has
140 original/actual-C comparisons and separately140 maintained typed-fallback
comparisons, with47controls (30positive/17refusal). Root independently rebuilds
both identical modules and repeats those campaigns. Root90file freeze:
`target/ff9-spu-envelope-root-review/root-frozen.json`:
`2e985d2450c98f18a28cf5871e2c33bb3cfa024486411a4b3843cf80c1269646`.
Basis120file freeze0ccdc821…97eeb; portable-v2 launcher6file freeze22e00be1…ec56
fixes only a missing mkdir; old failed reproduction retained.
Offline command:
`python -B target/ff9-spu-envelope-portable-v2/reproduce.py --out-dir target/FRESH-ENVELOPE-REVIEW`.

Shared planner recognizes eligible void2 binding but its emitted C conflicts
with an existing signedness/pointer declaration. That compile failure is retained;
the existing game resident-call-abi declaration-replacement/shim fallback works.
Do not credit the shared candidate with compilation success. Reusable fix/acceptance
is in the tooling handoff. Pinned release545beb…44b6e also refuses current facts'
calleeSpan field; debug9e above accepts it. Preserve both tool identities/schema
refusals; don't strip facts or overwrite release/scorer.

Stopped CD reuse research is `target/ff9-psyz-cd-pcm/CHECKPOINT.md`,8file freeze
`17503ea1938904c13d5b04a9b0b7309bdd7b4d8639d1109401de0becfe14cda8`.
Genuine Psyz_CdPullSamples/4bit XA decoder/resampler is pinned MPL2
`psyz/src/psyz/libcd.c`, source0f6f2b10…f2685. No sector decoded, native audio
command proved, or PCM prototype compiled. Rawdisc1 has one MODE2/2352 track,
no declared AUDIO. Existing50frame AKAO history has no CD commands. First capture
an actual command/mode/filter/sector history, then select licensed decoder spans
with authored interface types and independent arithmetic/pull tests. Upstream
silently substitutes unsupported18900/8bit coding; a bounded adapter must refuse
that. Keep one existing CD controller/volume/IRQ authority and mixer unchanged.

## Efficient resume and data

- Avoid rerunning unchanged full builds/proofs. Reproduce only newly changed
  runtime paths or evidence needed for the next admission.
- Use maintained Binviz batch/workspace/linked/campaign tools; thin adapters
  only for configuration/capture, never new C/opcode/ownership parsers.
- Keep max2 heavy2GiBNode jobs; full compiler stage uses pinned offline image
  sha256:395aa6ea420f04cf93f8891a7fbd22bb92a5723d70322eca9084a22ef52f2c2b.
- Fixed busy budget is the latest immediate runner gap. Other handoff findings:
  generated-C declaration conflict, preflight all proof dependencies, RCS code
  string omission, and measured capture/staging overhead. Linux volume staging
  is an unmeasured hypothesis; do not claim savings.
- Assets `C:/Users/elkin/.local/share/binviz-validation/ff9`: SLUS_012.51,
  FF9.IMG, overlays-proposed/ovl_08c000.bin, and
  `Final Fantasy IX (Disc 1) (v1.1).bin/.cue`. Do not commit game assets.
- On Windows use rg filenames/exact paths and preserve cleanup warnings.
  Game writes/Git operations outside Binviz need the normal escalation path;
  authorization for decompilation persists. Stage only owned named notes/files.

No fresh whole-game matching percentage, playable scene, browser integration,
battle-module completion or complete disc1 coverage is established. Continue
the full objective from the actual initializer/resource frontier above.
