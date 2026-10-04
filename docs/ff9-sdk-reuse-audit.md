# FF9 SDK reuse audit — 2026-10-03

Use recovered SDK source and portable implementations before spending time
recovering another standard library body. Keep FF9-specific wrappers and game
systems separate from the SDK service underneath them.

## Public references verified

| Project | Material available | Intended use |
| --- | --- | --- |
| [PSY-Z](https://github.com/Xeeynamo/psyz) | Portable C replacement for Psy-Q, including WebAssembly/WebGL2 via Emscripten; separate matching SDK decompilation | First candidate for reusable browser runtime services and recovered source |
| [PSY-Z matching decomp](https://github.com/Xeeynamo/psyz/tree/main/decomp) | Ongoing function-matching recovery, currently targeting Psy-Q 4.0; libapi, libcd, libgpu, libgte, libspu and other source directories | Compare actual library bodies and contracts; directory presence does not prove complete recovery |
| [Sozud Psy-Q decomp](https://github.com/sozud/psy-q-decomp) | Matching original SDK objects; README currently targets 3.5 | Additional implementations and object/signature references where the version agrees |
| [PsyCross](https://github.com/OpenDriver2/PsyCross) | High-level GPU/GTE/SPU/CD implementation, compatible headers and Emscripten support | Alternative browser backend/reference; README still lists MDEC, CD/XA audio and other gaps |
| [NFS High Stakes methodology](https://github.com/Caesar0007/NFSHS-PSX-decomp/blob/main/METHODOLOGY.md) | Another PS1 project's account of reusing Psy-Z SDK recovery | Version differences matter: source can guide recovery without matching a different linked SDK verbatim |
| [FFVIII decomp](https://github.com/roengstrom/ff8-decomp) | Related Squaresoft PS1 game recovery | Clues to shared conventions; do not assume FF9 engine code is identical |

Publicly hosted SDK headers, manuals and compiled libraries are not evidence
that the complete original Sony library C source is available. The projects
above explicitly recover or replace it. Preserve the exact upstream revision,
file attribution and applicable license when incorporating implementation code.
This audit has not installed upstream code in the production runtime or selected
a final backend. A later private voice-only prototype is recorded below.

## FF9 priorities

The existing game-owned `docs/library-map.md` / `config/library.tsv` census
reports 370 library/startup members, 163 called by game code, and 1,310 direct
call sites across its boot/confirmed-overlay scope. These are historical census
figures, not a fresh whole-disc measurement or a count of removable game units.
Several original spellings/prototypes were remembered rather than SDK-verified.
Check them against recovered source before treating them as authoritative.

| Area | Reuse opportunity | FF9 work retained |
| --- | --- | --- |
| CD loading | Prioritize PSY-Z/PsyCross libcd APIs, ISO lookup and callback behavior; the boot test previously stopped at CdInit | FF9.IMG request queues, async scheduling and overlay loading |
| GPU/GTE | Compare portable packet rasterizer/GTE with existing validated FF9 software services; avoid rebuilding standard APIs independently | FF9's hand-written display-list renderer, custom packets, scratch layout and ordering-table semantics |
| SPU | Reuse decoding/mixing/register machinery where supported | AKAO driver and its direct SPU register writes; it is game code, not interchangeable libspu |
| BIOS memory/string | Reuse established software service contracts and verify edge behavior | Guest-address translation, aliases, return/clobber obligations and exact call-site bridges |
| Pad/card/timing | Reuse device API implementations once validated | FF9 pad management, save format and frame/callback ordering |
| Movies | Audit libpress/MDEC in the movie overlay separately | STR streaming and FF9 movie coordination; backend coverage must be checked |

Concrete duplicate avoided today: completed sr8 already admits AD39C's
1D898 memset call through the existing file12 SDK policy. An isolated diagnostic
omitted that policy and falsely made it look like a new blocker. The extra
memory-service campaign is retained as validation, with no duplicate host or
new admission. Run the complete selected caller composition for each diagnosis.

## Reusable Binviz workflow

Batch candidates by SDK library/member/version, using Binviz's maintained native
facts, caller records and comparison tools. Match linked code against SDK
objects with relocation-aware evidence, not just a guessed function name. Record
body extent, symbols, prototypes, data dependencies and indirect entry points.
Distinguish an exact original-code identification from a compatible portable
replacement; neither implies the other.

For runtime reuse, test the actual FF9 call path and guest memory/device effects,
including callbacks, packet tags, aliases and error behavior. Keep the native
recovery unchanged while evaluating a backend behind explicit SDK boundaries.
An upstream API-compatible backend does not prove FF9 compatibility. Original
MIPS library objects cannot be linked directly into a WebAssembly executable.

Candidate reusable feature for Binviz capability review: an SDK catalog that
links versioned native signatures to recovered source and optional portable
providers. Its report should expose exact/ambiguous matches, provenance,
transitive dependencies, caller impact, existing service coverage and remaining
game-owned work through the same CLI/MCP/UI records. First check current shared
capabilities; extend them rather than adding a game-specific Python matcher.
Keep code identification, ABI validation and runtime compatibility as separate
statuses. No new parser or signature matcher was added for this audit.

## Pinned follow-up and actual coverage

The follow-up inspected PSY-Z revision
`21cbe23573f2b1c1b778ebad9729327744af628b`, PsyCross
`e56e4cde1c2b8a15e0d4e38b26cdd9202e0d17e6`, and Sozud
`6edf9b24721ba02a53eb423b8a7497b82ba0768c`. Exact selected file identities,
current imports and comparisons are preserved in
`../target/ff9-sdk-reuse-next/review-freeze.json`
(`7bbe42a85959e27111907e906492374bb7b2dc1c65f17eb81d7d3c14f8b49648`).
The later license addendum is authoritative for the recovered source:
`../target/ff9-sdk-reuse-license-addendum/frozen-files.json`
(`2591c2a8ad1dc0707f2efc9f482e03c80ed83c01a979ca380b8c61f89db385fc`).

PSY-Z's pinned [decomp/LICENSE](https://github.com/Xeeynamo/psyz/blob/21cbe23573f2b1c1b778ebad9729327744af628b/decomp/LICENSE)
assigns MIT to the recovered decompilation, with contributor attribution. Its
portable backend has a separate MPL2.0 notice. The initial root-license-only
inventory missed the nested decomp notice; that frozen inventory is retained
alongside the correction. No upstream implementation has been installed.

Prioritize two concrete candidates:

- Evaluate PSY-Z's portable SPU sample/voice backend against the existing FF9
  device's uploaded ADPCM, pitch, key-on, envelope progression and sample output.
  Preserve one register/RAM authority and existing DMA/event ordering. The
  selected backend explicitly lacks bit15 voice-volume mode; inventory FF9's
  use before admitting it. AKAO remains game code.
- Use current recovered [CD read C](https://github.com/Xeeynamo/psyz/blob/21cbe23573f2b1c1b778ebad9729327744af628b/decomp/src/libcd/cdread.c)
  and ISO search as references for missing standard loading behavior. Exact
  `cdread.c` is 5782 bytes, SHA256
  `c866b060b7f5f1e729eaa00c4847d73374e5bbdd330edf80d4d8c20962a67438`;
  an older cached page showed assembly placeholders. FF9 additionally waits
  for a previous read and has 120-frame cleanup, so preserve that difference.

FF9's GPU literal identifies sys.c1.140 (1998-01-12); selected PSY-Z recovered
GPU source identifies1.129 (1996-12-25). Do not assign FF9 an exact SDK release
from this alone. PsyCross's selected StoreImage bypasses queue/DMA, PutDispEnv
returns zero, and GetDrawEnv/reverb APIs include unfinished implementations.
Existing tested FF9 hosts remain comparison fixtures for any replacement.

The current BOOT rebuild changed module identity from `f97cc48f...022f7` to
`3c836890...2cc75` while retaining its exact265 imports. The separate full-menu
snapshot has282. These are import inventories, not executed call counts or a
whole-game coverage measurement. Current world-map sr10 compiles292/295;
remaining transition/renderer bodies are game code. CBF18's63108 is the standard
BIOS strcat exception and needs an explicit guest-memory software service.

Existing Binviz `sdk`/`identify_sdk` already supports relocation-masked library
signatures and release ambiguity. `store` already records matched C/compiler/
flags/SDK provenance. Extend their shared evidence with an upstream revision,
file/license, optional portable provider, dependencies, compatibility campaign
and existing host coverage. A second signature matcher is unnecessary.

Concrete RCS-discovery defect: `libraries` omits the sys.c ID at file0x860 /
guest80010060 even though `rcs.rs` parses that literal in a unit test. The
general ROM string index suppresses strings inside inferred code. Both the
agent's debug executable and root's pinned release545beb...22644b6e reproduce
the omission; root output is `../target/ff9-root-sdk-library-defect.txt`.
The fix and acceptance examples are in the tooling handoff.

## Reuse experiment reproduced by root

The private `target/ff9-psyz-spu-feasibility` prototype compiles the pinned
upstream voice decoder, pitch/interpolation and envelope implementation into
WASM. It reads uploaded ADPCM from the existing FF9 device's RAM, without a
second persistent SPU RAM or a replacement DMA/event scheduler. Its source and
license notices remain captured; the derived voice header retains its MPL2
notice and recorded modifications.

Root independently ran the captured offline reproduction into
`target/ff9-root-psyz-spu-review`: **22 checks passed**, producing the identical
module SHA256 `2c5032101ad22bda4b01c2a9266f61f0e95bea0468c361f6091c0d3fde5837db`.
Checks include the actual FF9 DMA upload and preserved RAM/transport traces,
independent filter0 decode expectations, pitch/envelope behavior, and explicit
unsupported-profile refusals. This establishes a useful reuse candidate; it
does not establish complete SPU compatibility, a hardware oracle, or playable
game audio. Production `renderAudio` still refuses. ENVX/ENDX publication,
sample scheduling, all-voice mixing and a real AKAO instrument history remain
integration work.

The next reuse step should integrate the voice slice through a reviewed device
attachment interface. Importing the complete upstream mixer unchanged would
introduce another RAM/register authority and its own timer/CD side effects.
Use the prototype's existing device/RAM and ordered key-edge contracts as the
integration basis.

The subsequent private attachment trial now uses an actual22-frame AKAO
upload/register history. Root reproduced **11 attachment checks and48 existing
device regressions**, including unchanged original transfer/status/IRQ behavior.
The real voice23 instrument produces896 nonzero dry stereo sample values over
512 explicitly requested diagnostic ticks, with actual ENVX/ENDX publication
through the existing device and no second RAM authority. This reuses the pinned
upstream voice algorithm; native sample timing remains unknown. Actual control
C081 is preserved, and full CD/reverb mixing still refuses. Capture, notices,
API, exact histories and limitations are under
`target/ff9-psyz-spu-attachment`; root reproduction is under
`target/ff9-root-psyz-attachment-review`. No public audio backend was enabled.

The subsequent all-voice prototype reuses the pinned upstream24-voice
accumulation, reverb and capture code through the same device/RAM authority.
Root independently rebuilt `target/ff9-psyz-spu-mixer` into
`target/ff9-root-psyz-mixer-review`:14 grouped mixer checks,48 unchanged device
regressions,120 genuine ENVX queries and95 genuine key/mode histories pass.
The identical module is
`de0bf9fc048339441b5370b08213bfa0b5c94839538e9564210a0bc3d87de5fc`.
Its real50-frame capture supplies voices22/23 and native key edges. The actual
CD-enabled request refuses without a CD PCM source; a separately labelled
synthetic zero-CD quantum produces914 nonzero sample values from the genuine
instrument slice. This does not establish native CD silence or playback timing.
Negative/sweep main gain, END+mute envelope behavior, a reproduced upstream
reverb-address escape, PMON/noise/IRQ and native sample scheduling remain
explicit frontiers. Public `renderAudio` remains refused. Reuse has replaced
algorithm recovery work; runtime composition and unsupported behavior still
require game-specific evidence.

## Current linked runtime and next reuse boundary

The earlier sr10 count above is historical. The subsequent actual sr12 world
module links all295 selected callers with one RAM; its genuine BOOT cold loader
copies the entire190464-byte world image before the preserved FlushCache
frontier. Neither result supplies a playable world scene. SDK reuse now focuses
on standard platform/device services that block that real runtime path, while
game-owned initialization, archive/resource interpretation and scratch-stack
closure remain separate work.

The private envelope-getter bridge compiles the original void two-input getter
and its genuine refresh/release callers, lowering only the actual hardware read
to the existing typed device service. All140 native/shared comparisons and47
host controls pass. The shared direct-binding planner recognizes eligibility,
but its emitted C has an incompatible duplicate declaration; the maintained
game adapter provides a separately validated fallback. The exact reusable fix
and compiler-facts/executable transition are in the tooling handoff. This lane
is not a broad SDK replacement or complete SPU compatibility claim.

## Source availability recheck — 2026-10-04

The user requested finding Psy-Q source for identified SDK functions to avoid
duplicating SDK recovery. Fresh GitHub metadata and source archives confirm the
same three revisions already pinned above. Source captures, per-file SHA-256,
upstream URLs, notices and an FF9 census cross-reference are retained in
`target/ff9-sdk-source-catalog-2026-10-04/`. `reviewed-candidates.json` records
25 manually inspected C-body candidates. No game source, runtime policy or
matching status was changed; no compiler or runtime campaign ran.

| Source | Useful recovered/portable material | Limits established by inspecting the actual source |
| --- | --- | --- |
| PSY-Z `decomp/src/libcd` | `CdInit` in `event.c`; `CdRead`/`CdReadSync` in `cdread.c`; `CdSearchFile` in `iso9660.c`; control/sector wrappers in `sys.c` | Public C wrappers still depend on lower-level services. `CD_datasync`, `CD_getsector`, `CD_getsector2` and `callback` in `bios.c` are assembly placeholders. |
| PSY-Z `decomp/src/libgpu` | Actual C for ResetGraph, image/ordering-table APIs, draw/display environments in `sys.c`; primitive constructors in `prim.c` | `_addque2` and `_exeque` are assembly placeholders. Primitive wrappers depend on header macros. FF9's sys.c1.140 differs from upstream1.129; matching requires per-member evidence. |
| PSY-Z `decomp/src/libspu` | Actual transfer, IRQ, allocation, key and reverb C, including `SpuWrite`, `SpuSetTransferStartAddr`, `SpuSetReverbModeParam` | State, private helpers, device access and version differences remain dependencies. This is separate from the portable SPU mixer already prototyped. |
| Sozud `src/spu` | Additional recovered SPU C, including `SpuGetVoiceEnvelope` in `s_gvex.c`, which is still an assembly placeholder in PSY-Z | README targets3.5, but some files also contain explicit `VERSION == 40` branches; select the correct branch per function. |
| PSY-Z `decomp/src/libetc` / `libgte` | Actual VSync C and some scalar geometry/trigonometry C | Most GTE entries still use assembly placeholders; a directory or `.c` extension does not establish recovered C. |
| PsyCross `src/psx` | Portable GPU/GTE/CD/SPU/pad source, useful for runtime integration/reference | Its basic pad API exists, but advanced pad operations include `PSYX_UNIMPLEMENTED` and constant returns. It is not the original matching DualShock driver. |

PSY-Z has1137 `.c` files in the captured decomp source tree,1002 containing
`INCLUDE_ASM` (including mixed C/assembly files). Sozud has166 `.c` files under
`src/`,108 under`src/spu/`; PsyCross has11 under`src/psx/`. These are file
inventory counts, not recovered-function counts. The375 physical census rows
include duplicate/head entries;236 have upstream text references, which can be
calls, declarations or placeholders. Neither number supplies decompilation or
exact matching credit. Use the25 body-inspected candidates as concrete starting
points and the broader catalog only for discovery.

License correction discovered in this recheck: Sozud's
[`src/press/libpress.c`](https://github.com/sozud/psy-q-decomp/blob/6edf9b24721ba02a53eb423b8a7497b82ba0768c/src/press/libpress.c)
contains real DecDCT/MDEC C but explicitly declares
`SPDX-License-Identifier: AGPL-3.0-or-later`. Do not classify that file as MIT
from the repository's root notice. PSY-Z's corresponding DecDCT file remains
assembly placeholders. The recovered PSY-Z decomp notice is MIT, its portable
backend has MPL2.0 notices, and its root explicitly lists some original-header
paths as unlicensed. Captured decomp private headers have MIT notices; original
SDK headers were not imported for this audit.

Recommended order: reuse recovered CD and SPU source for standard SDK bodies,
then GPU public APIs and primitive helpers. For every candidate, use the existing
Binviz SDK signatures/native scoring to establish the actual FF9 member/version,
review its state/data/header/helper closure, then compare real caller/device
effects before runtime admission. Reuse already validated software services
where present. Keep GPU queues, late DualShock internals, movie decoding,
FF9-specific AKAO and game resource scheduling as separately unresolved work.
The latest world-initializer stop is a runner busy-poll budget, so finding SDK
source alone does not resolve that immediate runtime frontier.
