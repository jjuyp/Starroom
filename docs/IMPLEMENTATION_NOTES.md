# Implementation Notes

## 2026-10-10 — Independent dual DNG calibration interpolation

Actual production counterexample: interpolating endpoint CC*CM products gave inverse red
0.31250003 instead of independent oracle 0.33333334 (original 1e-6 threshold). DNG chapter 6
defines interpolated CameraCalibration and ColorMatrix separately. Candidates now retain
original CM and effective CC; the dual no-ForwardMatrix path interpolates each, multiplies
CC*CM, then inverts through the existing validated Matrix3 provider. Single/Forward/mixed paths
are unchanged. Resolver identity advances to `starroom-camera-profile-v4-calibration-interpolation`,
already bound by Native/thumbnail/export recipe keys. LibRaw decoder, ABI and source files are untouched.

Three new unit regressions cover diagonal arithmetic, noncommuting order and singular explicit
Generic status; a third 24-patch ColorChecker test uses unchanged BSD data and strict 2e-5
threshold. This chart uses the existing midpoint fallback when LibRaw CCT metadata is absent,
not proof of a complete iterative dual-white solver. 17 RAW units / 3 charts and six real licensed
sensor fixtures pass. RAW/shared/photo milestone, format, workspace warning-denied Clippy,
frontend lint/types/build pass in `.starroom-reports/test-timing-1791622744836.json`.

Reference: [official DNG 1.7.1 chapter 6](https://helpx.adobe.com/content/dam/help/en/photoshop/pdf/DNG_Spec_1_7_1_0.pdf).
Independent implementation of published equations using existing providers; no Adobe SDK/source
or PDF is copied/bundled, and existing notices remain. AnalogBalance, calibration signatures,
mixed Forward/Color, iterative white solving, >3-channel reduction and physical editable RAW WB
remain open. No complete DNG/camera IQ or final production acceptance claim is warranted.

Current local Full Rust format / warning-denied workspace Clippy / all 399 ordinary tests /
doc-test runners pass (`.starroom-reports/test-timing-1791623250565.json`). The registered
list is 403; four existing model/heavy-image/scale opt-in gates remain unaccepted by this run.
This is a correctness repair, not a claimed performance optimization. Decoder/provider pins,
original fixtures and chart tolerances are unchanged.

## 2026-10-10 — GPU diagnostics recovery and shared-output validity

All production `GpuRenderer` statistics locks previously used `expect`. A poisoned diagnostics
mutex could panic even though the device and pixel buffers were healthy. Statistics access now
recovers only non-authoritative counters, clears that mutex poison, and exposes the lifetime
`statisticsRecoveryCount`. Pipeline/device/buffer identities are not replaced; reset preserves
the recovery count. Actual buffer sizes remain derived from live resources. Real OOM/device
failure flags remain authoritative, typed and visible; recovery never masks them. Missing
allocated resources now return typed Validation instead of a release-path panic. The GPU module
has a source-sweep regression disallowing panic-based lock/resource access before its test section.

A second real defect shared output/uniform buffers between standalone Exposure and fused
creative operations without invalidating the previous creative result. The cache fingerprint
now uses explicit `Option<u64>` validity, and Exposure invalidates it before writing those
buffers. Identity creative -> Exposure +1 -> identity creative is tested against exact Native
float pixels, including HDR and alpha, without another source upload. This is correct cache
ownership, not a new algorithm, hash policy, precision reduction or output clamp.

Three new regressions cover actual GPU poisoned-statistics recovery/resident source + real OOM
classification, alternating kernels and release-source panic sweep. Actual adapter tests pass;
GPU tests remain conditional on adapter availability in environments without one. Shared graph/
photographic/RAW/ICC regression tolerances remain unchanged. This does not qualify physical
driver failure, the entire repository panic audit, direct GPU presentation or final release.
No dependency, upstream source/license, image recipe or original-photo changes. No performance
speed claim is made for this reliability repair.

Local GPU milestone passes: render 35/35, shared pipeline 73 plus 18 integration cases,
Native frontend contract 45/45, Golden/RAW manifests, workspace warning-denied Clippy, format,
frontend lint/types/build. Report `.starroom-reports/test-timing-1791621747223.json`.

## 2026-10-10 — Remove retired Browser creative engine, preserve tests natively

Removed `src/imagePipeline.ts` and its separate Browser colour/tone/detail/canvas-processing
functions, with no production import or alternate engine left. Eight original pixel-behavior
assertions are migrated unchanged to real Rust Preview/Export integration tests; all eight pass.
Two UI-only assertions remain frontend tests. Frozen reference JSON and migration tolerances
are byte-identical (canonical LF SHA in doc 47); the existing Rust migration comparison passes.
Frontend checks now validate historical data rather than execute a retired engine. Recursive
source ownership and hash checks are isolated in Node test tooling, so production React did not
gain Node runtime/types. Relevant targeted gates invoke the Native pixel cases explicitly.

Full frontend 198/198 (29 files), lint, TypeScript/build and infrastructure validation pass.
The count change moves eight image tests to Rust and adds integrity/ownership cases, not lower
coverage. Targeted Rust warning-denied Clippy and nine Native intent/migration tests pass.
The colour milestone/shared photographic/RAW regressions also pass (local report
`.starroom-reports/test-timing-1791620612991.json`); detailed mapping is in
`47_NATIVE_ENGINE_MIGRATION_COVERAGE.md`. This source-only cleanup has no runtime pixel-math
change or claimed performance benefit: the old engine was already tree-shaken from production.
No dependency, license, fixture/tolerance or original-photo changes. Final product acceptance
and the other colour/cache/tile/Windows gates remain open.

## 2026-10-10 — Actual prepared source pixels in Native preview

Decoded-tier reuse alone still reran input conversion/source WB/lens/geometry for creative
sliders. A bounded `SourcePreparationCache` now retains the real immutable f32 stage image
and semantic map. Native CPU/GPU preview call this production path, not an unused provider.
Immutable decoded allocation identity plus exact actual upstream settings prevents stale
pixel/profile/metadata, WB, optics, crop/rotate and region reuse; weak refs do not retain decoded
buffers. Construction is outside the lock. Four entries / 128 MiB retained RGB capacities;
oversized frames execute the original owned graph without a cached image clone. The working
copy remains mutable for downstream operations; source/export image ownership is unchanged.

Visible Gray Picker is factored AFTER reusable source/lens/geometry and remains the same shared
measured-white CAT. Changed picker rectangles reuse upstream pixels but recompute correction.
Other source WB modes invalidate preparation. Disabled optics and unused sample fields are
projected away. Cold/warm/oversized paths share the identical processing order and kernels.
Profiler adds backward-defaulted `cacheHits` to stage measurements; skips never manufacture
time/execution counts, and RAW does not invent a camera transform at this boundary.

Four new unit regressions, two actual licensed portrait/RAW/Lensfun integration regressions,
one actual Native binary-preview regression and one profiler regression cover real reuse,
identity/invalidation, picker/region/rotation, LRU/budget/weak ownership and original protection.
CPU/export output remains exact and GPU tolerance stays <=1 RGB8 code. Dedicated same-binary
before/after timings and limitations are in `46_SOURCE_PREPARATION_CACHE_BENCHMARK.md`.
42 requests build 2 preparations and reuse 40; retained buffer capacities are 6,291,456 bytes.
This is not end-to-end <100ms or full 100MP/Phase 9 acceptance. No dependency/source license change.

Local Full Rust format, warning-denied workspace Clippy, all ordinary tests and doc-test runners
passed in `.starroom-reports/test-timing-1791619349861.json`; the final RAW profiler clarification
and added profiler regression also passed subsequent targeted render/pipeline/real-source tests
and workspace Clippy. Web regression 203/203 (28 files) passed in
`.starroom-reports/test-timing-1791619091313.json`; full Golden/RAW manifest validation passed.
The four existing opt-in model/heavy-image/scale gates are not accepted by these ordinary runs.

## 2026-10-07 — Native spline owns curve-panel geometry

`ToneCurveEditor` previously evaluated a second JS cubic spline from `previewPresentation.ts`.
It is removed from the production presentation module. The registered `native_curve_preview`
command and test probe delegate to `sample_native_curve_preview`, which prepares the very same
`starroom-color::PreparedCurve` used by the Native pixel graph. React only maps the returned
coordinates to SVG. The bounded request contains x/y points and a count, not photo pixels;
the panel requests 129 float samples. Native accepts 2-257 samples and at most 4096 points,
rejects non-finite input/output and retains unbounded finite y/HDR/endpoint extrapolation.
Existing renderer/edit-state limits and all image-stage algorithms are unchanged.

One-running/one-pending strict latest-only queue plus effect cleanup prevents old drag/channel
responses from replacing current geometry. Pending/error/unavailable states are explicit; no
Browser spline fallback runs when Native is unavailable. Curve errors are actionable Traditional
Chinese and retain diagnostics; the existing LittleCMS cache error is classified as Color too.

Two actual desktop-command regressions prove equality to the shared spline for identity, S/fade,
negative/HDR and nonuniform/endpoint points plus bounded typed invalid data. Four frontend
contract/queue regressions preserve float samples, small request shape and latest-only behavior.
All frontend 203/203 (28 files), lint/TypeScript/build and infrastructure validation pass.
Desktop 36 ordinary tests pass (2 opt-in not accepted here). Curve milestone/shared pipeline
69 + 8 photographic/RAW integration, render 31, Golden manifests, format and workspace
warning-denied Clippy pass; local report `.starroom-reports/test-timing-1791302277100.json`.
The subsequent complete frontend/desktop runs cover the final error UX and public probe helper.

Playwright CLI drove actual React Master/Red/Green/Blue tabs, S curve/black fade, add/drag/delete
and numeric editing. The curve-specific test bridge invoked the compiled production Rust
implementation for every request and compared the exact SVG path to all 129 returned coordinates:
18 requests passed, max debug Native command time 0.2317ms. The timing excludes process start,
IPC, UI presentation and photo rendering. The photo/history fixture remains explicitly synthetic;
this is not installed photo pipeline or end-to-end latency acceptance. Temporary loopback audit
tooling is isolated under `output/playwright`, not imported/bundled by production and is closed
after the audit. Screenshot/scripts: `native-curve-ui-final.png`, `native-curve-ui-audit.js` and
`native-curve-bridge.mjs`; reusable Native probe is `src-tauri/examples/native_curve_preview_probe.rs`.

The deprecated JS image engine still used by historical migration tests is not yet removed.
Whole Phase 23, Native residency/stage caches, RAW/alpha/gamut/monitor and final Windows gates
remain open. No new dependency or upstream curve source was copied.

## 2026-10-06 — Reuse mature LittleCMS transforms at real graph boundaries

Both encoded input and output recreated profiles and LCMS transforms on every frame. They now
reuse actual pinned `ParallelLcmsTransform` objects in a bounded process-local LRU: direction,
exact optional ICC bytes, intent and BPC are part of the key; maximum eight retained transforms
and 2 MiB retained key bytes. Invalid profiles remain role-specific errors; oversized valid ICCs
still execute with the same engine/flags but are not retained. Poisoned cache access is typed,
not a release-path panic or invented sRGB fallback. Execution occurs outside the lock using
the existing safe wrapper's NO_CACHE/DisallowCache thread-sharing contract; no Starroom unsafe
Send/Sync, mutable one-pixel LCMS cache or math replacement is added.

23 CMM tests (four new) pass for exact cached/uncached concurrent float output, full-key
separation, cold-build race deduplication and real LCMS invalid/oversized/LRU budget checks.
Profile creation and pixel execution are outside the shared lock, so a cold export ICC does not
hold the object cache lock needed by a warm preview. Color milestone, real photographic
Golden, RAW/shared graph, workspace warning-denied Clippy, format, frontend lint/types/build pass:
`.starroom-reports/test-timing-1791300268882.json`. The benchmark's dedicated process records
two real transform builds and 166 reuses; see `45_ICC_TRANSFORM_CACHE_BENCHMARK.md` for measured
before/after values and limitations. No pixel policy/profile recipe version is changed because
the original transform algorithm and parameters are retained. Final code also passes local
Full Rust format, warning-denied workspace Clippy, all 374 ordinary tests and doc-test runners
(`.starroom-reports/test-timing-1791301139879.json`). The registered test list is 378; four
explicit model/heavy-image/scale gates remain opt-in, not newly accepted by this run. Desktop
34 ordinary / 2 opt-in, export 20 + 7 workflow/recovery ordinary tests pass. Full input/output pixel-stage
result caching, Native direct display and the final production acceptance are not complete.

## 2026-10-06 — Physical History gesture boundaries

The actual App History effect previously persisted after 220ms of inactivity, including while
the mouse remained down. A long slider drag therefore produced several semantic commands.
`HistoryGestureBoundary` now records physical pointer/key lifetime separately from preview
quality and numeric-editor focus. Window capture observes input before React edit callbacks;
only an editing begin attaches the latest input, so unrelated clicks/held navigation keys do
not block persistence. Intermediate changes replace the pending state without starting an idle
commit while held. Release/cancel resumes persistence; window blur and photo switches release
the hold. Starting the next gesture flushes its predecessor even before the 220ms idle deadline.
Explicit close/undo/snapshot/export flushes retain the existing serialized Native command path.
No second persisted History database, Native state schema or creative math is introduced.

Six new boundary regressions (14 History queue tests total) cover 1000 changes plus a ten-second
paused hold, matched pointer release, unrelated input, overlapping/repeated keyboard input,
lost/cancelled input and focus-only behavior. Full frontend 195/195 (27 files), lint and
TypeScript/production build pass. Native History 16/16, Session 4/4, pipeline 69 + 8 integration,
render 31, Golden/RAW manifests and workspace warning-denied Clippy pass in the History
milestone report `.starroom-reports/test-timing-1791298829003.json`. The last extra boundary
case is covered by the subsequent full frontend run, not retroactively by the earlier report.

Playwright CLI drove the actual App slider in an isolated Chromium session with explicitly
synthetic IPC, not a mock renderer accepted as Native production evidence. Two 1200ms held
pauses produced zero commits; three separate drags produced exactly three commands, including
the next drag starting before its predecessor's idle deadline. Before/after exposure values
were .5 -> 2.06 -> 2.48 -> 2.89. UI Undo/Redo restored 2.48/2.89 without phantom commits.
Held keyboard repeat added one command to 2.99; a numeric draft stayed uncommitted until Enter
and then added one command to 4.2. The unchanged scheduling audit still published 20 frames
during continuous dragging, zero catalog scans and latest final -0.7. These are interaction
ordering tests, not measured Native input-to-present latency or installed recovery acceptance.
Local reproducible scripts/screenshot: `output/playwright/history-gesture-audit.js`,
`history-gesture-final.png` and the existing slider scheduling setup/drag scripts.

Complete mask/AI/style/delete/recovery installed workflows and the same-SHA final Windows
release gate remain open. No dependency or third-party license change is involved.

## 2026-10-06 — Remove unused persistent GPU allocations

Production `GpuBufferResources` reserved a full RGBA f32 working buffer and a full single-channel
mask buffer in both exposure and creative allocation paths, but neither was ever bound, read or
written. Remove those reservations, retain source/output/readback/uniform/LUT precision and the
same shader and cache fingerprints. This is memory ownership cleanup, not a new image algorithm
or dependency. Masks continue through their existing shared graph; no mask capability is removed.

`GpuResourceStats::allocated_buffer_bytes` sums actual live wgpu `Buffer::size()` values. The
production render profiler also records `gpu_buffer_bytes` (maximum observed during that render,
backward-compatible default on deserialization). Neither counter claims physical VRAM, texture
memory or process peak. Actual adapter tests cover empty allocation, creative reuse, resize,
reuse of a larger capacity by a smaller frame, statistics reset and captured production memory.
The exposure identity output is exact, including alpha.

For the existing 512px square NASA portrait on RTX 3050 Laptop/DX12, old declared buffer sizes
sum to 17,859,072 bytes; current actual buffer sizes sum to 12,616,192 bytes (5,242,880 bytes
removed, 29.36%). One isolated current release-engine run measures median/p95 milliseconds:
Exposure 45.474/68.266, Temperature 45.824/56.015, Tint 42.098/44.375, Saturation 43.113/48.327,
Vibrance 42.914/45.930, Auto WB 44.773/48.037, Picker 43.910/49.319. Five timed samples follow
warm-up; Auto intent is unchanged across samples. CPU/GPU max RGB8 difference remains one code
(Auto/Picker zero). Prior concurrent-build samples varied substantially; they do not demonstrate
a causal latency gain. This optimization proves reduced owned buffer storage, NOT full UI
latency, 24/100MP acceptance, physical memory improvement or completed GPU-resident presentation.

Targeted GPU tests 10/10, GPU milestone/shared graph tests (69 pipeline + 8 integration + 31
render), frontend Native contract 45/45, Golden/RAW manifest validation, workspace warning-denied
Clippy, format, frontend lint/TypeScript/build pass. Local milestone timing report:
`.starroom-reports/test-timing-1791298106940.json`; the final telemetry assertion also passed in
a subsequent targeted GPU rerun. No lockfile/provenance dependency change. All full production
phases remain active; installer and final same-SHA release gates are not satisfied by this group.

## 2026-10-06 DNG ColorMatrix neutral and inversion boundary

Native authored D65 ColorMatrix counterexample produced working RGB [.915141,1.012765,1.330013]
for a known neutral. The resolver treated inverse ColorMatrix like ForwardMatrix, always adapting
from D50 even though the former maps unbalanced camera coordinates and the LibRaw bridge already
bakes Camera WB. Foundation decision: implement the public standard boundary equations using
the existing validated matrix/Bradford provider, not another demosaic/ICC engine or copied SDK.
[Adobe DNG 1.7.1 chapter 6](https://helpx.adobe.com/content/dam/help/en/photoshop/pdf/DNG_Spec_1_7_1_0.pdf)
distinguishes these input domains and uses the measured neutral white for the no-ForwardMatrix path.

No-ForwardMatrix profiles now undo baked diagonal WB, derive/normalize the neutral illuminant,
adapt it to D65 and retain scene-linear range. Two ColorMatrix candidates interpolate their original
XYZ-to-camera matrices before inversion, not inverse matrices. Invalid neutral/singular transform
produces the existing explicitly Generic descriptor; it never keeps a Resolved label with substituted
matrix values. The same D65 probe now maps XYZ to D65 within float precision (working RGB
[1.000082,.999987,.999786], with the established XYZ/Rec.2020 matrix rounding). ForwardMatrix's
D50 path remains unchanged. Profile resolver version advances to v3 and is bound into Native
transform, thumbnail and export recipe identities, not source/photo/History identities.

14 raw unit tests, two ColorChecker tests, six immutable real RAW sensor fixtures, full/half
controlled headroom oracle, three RAW shared-graph cases and 20 export unit / seven workflow
recovery tests pass. New D65/invalid-neutral/pre-inversion interpolation and independent 24-patch
ColorChecker tests keep strict numerical bounds. The old inversion test now supplies WB-balanced
input and measured-neutral expectation matching the actual boundary; its 1e-4 bound is unchanged,
not removed or widened to hide a regression. Diagnostic example data is authored matrices, not
an added camera photograph or new lighting/manufacturer coverage.

This closes the demonstrated no-ForwardMatrix white-point/baked-WB error. It does not qualify all
DNG modes: independent CameraCalibration interpolation, AnalogBalance extraction, calibration
signatures, mixed Forward/Color profile semantics, iterative dual-white solving, n>3 reduction,
physical editable RAW WB and actual scene-quality validation still need complete source audit and
tests. No Final/camera-quality-complete claim follows from these numerical and decode regressions.

This group local Full Rust Acceptance passed: 370 ordinary Rust tests and four separately
gated tests, all doc-test runners, format and warning-denied workspace/all-target Clippy.
Report `test-timing-1791288166325.json`; targeted color milestone report
`test-timing-1791287673929.json`. Existing frontend source/lockfile are unchanged from its
189-test Full web gate; JSON, Golden/RAW/generator hashes pass. Final phase acceptance remains open.

## 2026-10-06 canonical execution declarations and Native stage identities

The declared graph put lens/geometry after Detail and claimed nearly every operation had a
wgpu implementation. Actual preparation runs source-WB intent/Auto, lens/geometry, visible
Neutral Picker and AI denoise before relative color/global creative processing; local layers,
Skin/Healing/Detail and finishing follow. Split SourceWhiteBalance, visible WhiteBalance,
RelativeColor and Finishing, declare the actual order and mark wgpu support only for the fused
global kernels. DirectML availability is separate, not a wgpu stage. RAW Camera WB remains baked
by the mature decoder; SourceWhiteBalance here denotes source-intent/Auto processing, not proof
of a separate editable sensor-WB implementation. Static tile flags are conservative; conditional
Native ROI eligibility is still validated by the existing viewport path.

The production `preview_stage_identity` uses this chain, not a second settings hash. Exposure
does not alter prepared Geometry/WB keys; geometry/optics changes invalidate all later creative
work; measured Picker changes do not invalidate prior geometry; Auto ignores irrelevant picker
rectangles; relative controls do not invalidate source/geometry/AI preparation; Grain/Vignette
bind the real finishing stage instead of ColorGrading. Existing GPU pixel/parameter fingerprints
still guard hybrid/vignette fusion. No processing order, pixels, source/History identity, graph
algorithm, codec, model, dependency or durable schema is changed in this correction.

31 render unit tests, 69 pipeline unit tests/photographic/RAW integrations, 34 ordinary desktop
tests (two opt-in cases counted separately), format and warning-denied workspace Clippy pass
through the GPU milestone and desktop suite. Three new tests assert stage order/capability truth,
upstream/downstream keys and actual Native settings projection. This repairs declarations and
invalidation correctness, not missing stage-result caches, dirty/prefetch execution, GPU residency,
100MP memory or end-to-end latency. All Phase 0-30 requirements remain active.

## 2026-10-06 bounded GPU readback and real failure callbacks

The active renderer formerly ignored `Device::poll` errors, waited indefinitely for all/latest
submissions and then blocked on an unbounded callback receive. Device loss was only a manual bool
hook. Use existing wgpu official `PollType::Wait` with the exact owned submission and a shared
five-second fault deadline, followed by remaining-deadline callback receipt. This is an error
ceiling, not acceptable normal interaction latency. The staging mapping is unconditionally
cancelled/unmapped through RAII; mapped byte alignment/casting failures are typed, not panics.
Real device-loss and uncaptured OOM/validation callbacks latch the first failure. Exposure,
creative and texture entry points reject failed devices explicitly; OOM is no longer mislabeled
as InvalidPixels/DeviceLost. No normal image arithmetic, precision, cache key or provider changes.

The actual desktop preview retires only critical GPU loss/OOM/readback/validation failures so
the same bad device does not incur repeated waits on every edit. Oversized/capability and source
errors do not permanently disable a healthy GPU. Existing binary CPU-fallback labeling remains;
no Browser or cloud renderer is used. A real isolated `Device::destroy` triggers the official
callback and rejects subsequent work. Production wait-helper tests cover expired deadline,
ready callback and disconnected callback; explicit OOM and Native retirement classifier tests
cover every affected operation. Physical driver-removal/hang/OOM stress remains a final hardware
qualification item, not inferred from this device-destroy test.

Registry/lock inspection also found stale provenance claiming wgpu 30.0.0: the already locked
and license-reported version is 30.0.1, packaged commit 40f4a34/checksum recorded in the inventory.
Only the inventory is corrected; no dependency/lockfile/model/license change occurs. Header docs
now honestly distinguish production f32 buffers from planned Float16 textures. Full GPU residency,
direct presentation, remaining release-path expects and complete error/performance gates remain open.

GPU milestone validation (`test-timing-1791254401581.json`) passes 29 render unit tests, 69
pipeline unit tests, photographic/RAW/shared integrations and warning-denied workspace Clippy;
all 33 ordinary desktop tests pass (two opt-in desktop gates remain separately counted).
Locked license validation still reports 561 Rust / six production npm packages and 269 notices.
Real RTX 3050/DX12 512px probe after liveness changes: Exposure median/p95 31.200/31.831ms,
Temperature 31.107/31.398ms, Tint 31.352/33.445ms, Saturation 31.602/32.642ms, Vibrance 30.175/31.533ms,
Auto WB 32.219/32.741ms, Picker 30.717/31.170ms; unchanged one-code parity (Auto/Picker zero).
This is the same small engine-only diagnostic, not a <100ms full-desktop or 100MP acceptance.

## 2026-10-06 relative Temperature/Tint CAT and exact neutral stages

Foundation decision: reuse LittleCMS `cmsWhitePointFromTemp` daylight locus and the prepared
`cmsAdaptToIlluminant` adapter. Starroom maps relative -100..100 controls to reciprocal-temperature
displacement around an internal D65 anchor and a perpendicular CIE 1960 u/v tint direction.
6504K is a mapping anchor, never a claim about an encoded source's physical temperature. No
upstream polynomial/CAT code is copied. The CPU global graph and prepared local layers apply
the same matrix; the fused GPU receives the three Native matrix rows. No per-pixel FFI, browser
color math or second WB engine. HDR and negative working values stay unbounded, black stays black,
and the color recipe policy advances to v4. Editable RAW camera gains/profile interpolation
remain a separate requirement: this working-space relative correction does not qualify them.

New regressions check warm/cool and green/magenta direction, exact neutral/black, measured gray
luminance, 603 extreme/reversible matrix cases, the daylight-piecewise boundary, invalid controls,
real fused f32 GPU reference (2e-5, preserving alpha at this kernel boundary), full-opacity local
versus global byte equality, and real portrait/RAW Preview/Export/GPU parity with immutable sources.
The strict local/global test initially found one-code drift from neutral local Color Mixer doing
an unnecessary OKLab round trip. Repair the operator's exact identity path instead of weakening
the assertion. Neutral Tone also preserves finite negative/HDR pixels exactly, matching the global
stage bypass. Nonfinite input protection remains active. Existing historical Browser fixture
values/tolerances are unchanged. Full alpha handling remains open despite kernel-alpha checks.

The GPU skips unused perceptual conversions and neutral tone-LUT application; remove the unused
old WGSL `tone` routine while retaining the authoritative darktable-derived Native tone LUT.
Same 512px portrait / RTX 3050 Laptop DX12 engine measurements: Exposure median/p95 32.448/32.874ms,
Temperature 31.146/33.019ms, Tint 30.065/32.463ms, Saturation 32.216/33.883ms, Vibrance 30.883/32.436ms,
Auto WB 32.141/33.096ms, Picker 31.393/32.515ms. Compared with the earlier same-machine v3 samples,
Saturation 34.720 -> 32.216ms, Vibrance 33.380 -> 30.883ms and Picker 35.051 -> 31.393ms median.
Five samples are diagnostic evidence, not comprehensive benchmark statistics. Native engine
timing excludes IPC/presentation; 24MP/100MP end-to-end latency and all named controls remain open.

The preceding RAW/neutral/chroma group `6fa51eb` Full Check passed at run 37399874663, while
Draft PR #2 remained Open/CLEAN. This result does not qualify subsequent v4 source or a Final
installer. Full current-group acceptance and every stronger Phase 30 gate remain mandatory.

Current v4 local Full Rust passes: 360 ordinary tests, four opt-in tests counted separately,
all doc-test runners, format and warning-denied workspace/all-target Clippy. Report:
`test-timing-1791252390430.json`; the preceding targeted color milestone report is
`test-timing-1791252027947.json`. JSON/Golden/generator validation remains green. Frontend
source/lockfile are unchanged from the 189-test Full web gate. A new same-SHA CI run qualifies
this source group only; the stronger Final/Windows/100MP/end-to-end gates remain open.

## 2026-10-06 LibRaw sensor-white / WB headroom repair

The authored controlled DNG probe executes real sensor unpack/demosaic, not a simulated RAW
renderer. On the baseline, known camera gains [2,1,4] force blue working output to plateau near
1.0 for still-unsaturated sensor steps. LibRaw also replaces the declared white level with the
observed maximum via its default `adjust_maximum_thr`, so metadata's declared 16383 and actual
normalization differ. The bridge now integrates existing LibRaw `highlight=1` (maximum-WB
normalization; no reconstructed/blended highlights) and disables content-dependent maximum
replacement. Rust validates/restores effective max/min WB normalization in f32 before the
unchanged Camera Profile -> XYZ -> Linear Rec.2020 stage. The 16-bit mature demosaic boundary
is explicit; this is not 8-bit upconversion or a new floating-point demosaic implementation.

Identical synthetic input now has monotonic blue values .2314, .4628, .9256, 1.3884, 1.8512,
2.3140, 2.7769, 3.2397 instead of the former premature plateau. New full/half-size real LibRaw
test independently uses declared white level and known gains, retains a 4e-4 normalization
bound, source immutability and finite output. All six licensed NEF/ARW/CR2/CR3/DNG/RAF decodes,
three RAW shared-graph cases and 20 export unit / seven workflow-recovery cases pass. Restricted
models and source files are untouched. RAW decode policy enters thumbnail, preview-transform
and export recipe identities without changing source/AI/history identities or deleting caches.
The generated manifest registers the canonical generator hash/license, not a fictitious camera
photo or expanded manufacturer/scene coverage. No dependency or vendored upstream code changes.

The previous `790345a` Full CI run 37398183002 failed only on three new-test `chunks_exact` usages
under CI Rust/Clippy 1.99 (local 1.97); use equivalent `as_chunks::<3>().0`, not a lint allowance
or removed assertion. New full acceptance must verify the corrected successor SHA. Final RAW
quality/precision/latency, monitor/gamut/alpha, editable WB and the stronger Phase 30 release
qualification remain open; this group is not a Production-ready declaration.

After the RAW repair, local Full Rust Acceptance passes again: 354 ordinary tests, four
separately gated tests, all doc-test runners, format and warning-denied workspace/all-target
Clippy (`test-timing-1791250391225.json`). The unchanged frontend has 189 passing tests,
production types/build/lint and validated JSON/Golden/generator hashes. This remains ordinary
Full Acceptance; heavy 100MP/installed Windows and all new Phase 30 gates are still required.

## 2026-10-06 measured-neutral LittleCMS adaptation

Foundation decision: integrate the already pinned LittleCMS `cmsAdaptToIlluminant` through the
safe `CIEXYZExt` provider; do not invent another CAT or use RGB diagonal gains as adaptation.
Auto WB estimates a measured Linear Rec.2020 neutral. The Gray Picker still samples the actually
visible post-lens/post-geometry rectangle. Both prepare one full working-space adaptation matrix
from three XYZ basis vectors, normalize white-point chromaticity and preserve measured neutral
luminance. Per-pixel processing uses the prepared float matrix, no per-pixel FFI or 0-1 clamp.
Black/nonfinite/invalid white points remain typed errors with actionable Traditional Chinese UX.
Camera/As-Shot modes and their RAW-versus-encoded rejection semantics are unchanged.

Seventeen CMM tests pass, including three new tests comparing the prepared LCMS adapter with
the independent existing Bradford reference, reversible D50/D65, HDR/negative intermediates,
true measured-neutral luminance and invalid white points. All 67 pipeline unit tests and three
real RAW shared-graph/visible-picker tests pass. A new real portrait/mixed-light/neon integration
test verifies repeatability, CPU Preview/Export equality, unchanged sources and available real
GPU at the unchanged one-code bound. Recipe color policy advances to v3. No new dependency,
model, licensed material or upstream revision is introduced.

`color_latency_probe` measures the production Native pipeline on the existing 512px NASA portrait
using RTX 3050 Laptop / DX12. Five warmed samples: Saturation median/p95 34.720/36.701ms, Vibrance
33.380/34.721ms, Auto WB 32.735/35.015ms, Picker 35.051/35.542ms; all max CPU/GPU differences are
one RGB8 code. Saturation/Vibrance/Picker change parameters each sample; Auto WB repeats its fixed
intent and is labeled accordingly. These are engine-only measurements, not 24MP pointer-to-paint,
JPEG/WebView presentation, all-controls latency or new Final performance acceptance. Prior
512px before-values were not measured; do not claim a before/after speedup from these numbers.

Remaining WB work is explicit: Temperature/Tint still use the previous perceptual offsets;
editable RAW camera gains, robust picker/outlier qualification, physical illuminant/camera-profile
tests and full photographic quality metrics are not closed by this measured-neutral group.

The batched protected-chroma/measured-neutral local Full Gate passed: 353 ordinary Rust tests,
four opt-in release/private-model tests separately counted, all doc-test runners, workspace
warning-denied Clippy, format, 189 frontend tests / 27 files, lint, TypeScript, production build,
JSON/schema and immutable Golden/RAW validation. Reports are the local Full Rust run
`test-timing-1791249198022.json` and Full web run `test-timing-1791248988177.json` in the ignored
reports directory. This is group acceptance, not the new Phase 30 release gate or an installer claim.

## 2026-10-06 protected Native chroma controls

The old additive saturation/vibrance scale retained 15% chroma at Saturation -100, or more with
positive Vibrance. Native color now has a true zero-chroma saturation endpoint, multiplicative
Vibrance, exact neutral identity in the perceptual operator and continuous protection based on
OKLCh hue, chroma and lightness. This is an authored Starroom OKLCh workflow heuristic, not face
detection or an upstream foundation replacement. It must not be described as recognizing every
person/skin tone. The existing NASA portrait fixture is unchanged; no new dependency/model or
external source code is introduced. LibRaw, LittleCMS and the shared graph remain authoritative.

The same operator is implemented in the production fused WGSL and CPU reference, preserving
OKLab lightness/hue before output gamut conversion. Three numerical operator tests, two shared
CPU/GPU tests and a real licensed portrait integration test cover identity, grayscale despite
positive Vibrance, smooth skin-like weights, high-chroma protection, extreme controls, repeated
deterministic output and unchanged source bytes. CPU preview/export are exact; available real GPU
tests retain the existing one-code RGB8 bound. No existing regression tolerance was relaxed.
Export recipe identity now includes `COLOR_POLICY_VERSION`, avoiding reuse of an old color recipe
after semantics change. This does not yet bind every durable project/cache/profile version.

This closes the demonstrated endpoint/protection defects, not full Phase 3 acceptance. Relative
Temperature/Tint still use the previous OKLab offsets and require formal Native chromatic
adaptation; editable RAW WB, destination-gamut clipping, complete perceptual photographic baselines,
end-to-end latency and final same-SHA installed Windows acceptance remain open in the matrix.

## 2026-10-06 Display P3 named-transfer correctness

Keep LittleCMS as the input/working/output provider. Generated Display P3 formerly used a generic
2.2 power curve, not its named IEC sRGB transfer. Use LCMS analytic type-4 TRC for all three channels;
Adobe RGB retains exact 563/256 and existing Rec.2020 photographic output remains explicitly D65/
gamma-2.2 SDR, now labeled accordingly rather than claiming CSS display BT.1886, video OETF, PQ or
HLG. The [Display P3 definition](https://www.w3.org/TR/css-color-4/#predefined-display-p3) specifies
sRGB transfer. No new dependency, weak custom ICC engine or browser image math was added.

Independent stored-TRC and transform-toe/shoulder tests cover black, deep shadows, the piecewise
boundary, midtones and white, not only self round-trip. ICC colorant/chad values are fixed-point;
the transform TRC isolation normalizes measured profile white while directly checking each stored
TRC against the standard formula at the original 2e-5 bound. No existing Golden/parity tolerance
was relaxed. The real .02 encoded-gray probe improves from ~.00018293 to ~.00154786 linear, close
to standard .00154799; camera/monitor matrices are separate requirements.

Export engine/recipe now binds actual canonical generated output-profile bytes even if embedding
is disabled. A corrected resource cannot silently retain an old output identity. Four-profile
embedding, real RGB16 round trips, precision, metadata, atomic/cancel/batch and recovery regressions
remain passing. Fourteen color-management unit tests, 20 export unit tests, seven workflow/recovery
cases and warning-denied affected Clippy pass. Full final graph/Windows/performance acceptance
still remains separate, and no v1.0 Final is declared.

## 2026-10-06 canonical finishing-vignette order

The real RTX 3050/DX12 counterexample combined vignette with a local contrast/shadow layer and
showed 21 RGB8-code max difference between GPU preview and CPU export. Fusing a finishing effect
before non-commuting layers/Skin/Healing/spatial detail/grain is invalid even when isolated shaders
match. `gpu_can_fuse_vignette` now permits the existing fused kernel only with an identity tail;
otherwise the unchanged mature shared CPU finishing stage owns vignette in the canonical order.
Creative GPU acceleration remains active, source settings are unchanged and vignette is never
applied twice. Completing a later GPU finishing/spatial stage must preserve this same order.

The identical real probe now measures max difference zero. New tests verify every nonidentity-tail
family disables early fusion and six actual CPU/GPU combinations (local tone, local relative color,
sharpen, Texture/Clarity/Dehaze, classical NR, deterministic grain) stay within the existing one-code
bound. All 64 pipeline unit tests, historical migration fixture, photographic Golden test, three
real RAW shared-graph tests, doc-test runner and warning-denied pipeline Clippy passed. No tolerance
was increased, source photo altered, provider replaced or weaker algorithm introduced. Final
same-SHA full acceptance and end-to-end performance remain separate requirements.

The combined safety/finishing/P3 local Full Acceptance passed: 342 ordinary Rust tests, four
explicit opt-in gates separately counted, all doc-test runners, warning-denied workspace Clippy,
format, 185 frontend tests/27 files, lint, TypeScript, production build, JSON/schema, photographic
Golden and real LibRaw decoder/shared-graph regressions. This qualifies the repair group locally,
not every stronger production phase, GPU latency target, alpha/monitor/RAW-headroom or Final release.

## 2026-10-06 Production audit / P0 source and atomic-output safety

The new user master prompt requires a complete Phase 0-30 production/quality/performance pass,
not another RC-ready claim. `docs/44_PRODUCTION_COMPLETION_MATRIX.md` retains the full scope and
distinguishes inspected implementation, incomplete paths and actual new-target acceptance.
No algorithm was blindly rewritten during initial audit. The real Native diagnostic example
`production_audit_probe` uses exclusively created/generated temporary sources, not user photos.

Baseline `b36692f`: a valid literal name plus professional Overwrite accepted the same source
path and changed its bytes. The older single-JPEG IPC guard did not protect this actual batch
path. The writer deleted the existing destination before rename and reused a predictable PID
temporary via truncating creation. Core export now rejects source equality/canonical aliases
before decoding and immediately before persistence. Desktop batch adds indexed Library identity
checks protecting every registered source, including another selected/unselected source and lost
folders/files, without copying all catalog records into the render worker.

Reuse the already integrated `tempfile 3.27.0` safe public API: exclusive randomized same-directory
creation, chunked cancel-aware output write, fsync, guarded persist/persist_noclobber. Windows uses
the mature provider's MoveFileEx replacement without delete-first; replacement failure keeps the
previous complete file. Fail/AutoRename no-clobber races cannot overwrite another process's output;
AutoRename retries using the same encoded bytes. Cleanup touches only the owned temporary file.
SourceOverwriteForbidden has an actionable Traditional Chinese error while retaining diagnostics.
The dependency package set remains 561 crates / six production npm / 269 notice texts; adding
the existing tempfile as a direct export dependency changes only the lock/report identity and
its documented runtime usage, not model/license policy.

Targeted checks: 19 export unit tests (five new), seven export workflow/recovery integration tests,
new Library destination-identity test, new desktop protected-other-original adapter test, and
185 frontend tests across 27 files passed. Source cases cover real JPEG8/PNG8/PNG16/TIFF8/TIFF16,
Unicode/parent aliases, other/missing Library sources, raced destinations, cancellation, legitimate
replacement and Windows lock-denied replacement preserving prior bytes. Strict affected-crate
Clippy, format, frontend lint/types/build, JSON/schema and refreshed license validation pass.
The same real probe now returns SourceOverwriteForbidden and unchanged source bytes.

Other probe evidence remains unresolved: on NVIDIA RTX 3050 Laptop / DX12, vignette plus local
contrast/shadows differs by up to 21 RGB8 code values between GPU preview and CPU export. Generated
Display P3 decodes encoded gray .02 to ~.00018293 instead of the sRGB-TRC value ~.00154799; [the
named Display P3 definition](https://www.w3.org/TR/css-color-4/#predefined-display-p3) uses sRGB's
transfer curve. A transparent red source pixel exports opaque RGB [255,0,0]. These are concrete
counterexamples, not a claim that the diagnostic, matrix or existing green RC has qualified Final.

## 2026-10-05 continuous-drag / catalog-count performance repair

The user's installed `64f005f` screenshot explicitly reports Native GPU, not CPU fallback.
It shows source dimensions 6032x4032; that label alone does not establish the actual preview
payload resolution or identify a full-resolution render on every slider input. Same-test CI
medians for `64f005f` and `b17e4b8` are exposure 276.621/276.038 ms and shadows 244.586/238.995 ms.
These isolated Native samples do not establish desktop end-to-end latency or Lightroom parity.

Two real scheduling costs were found in the production click path. LatestPreviewQueue cancelled
every active frame as soon as a newer slider value arrived. Sustained input faster than rendering
could therefore prevent any intermediate publication. The checked-in baseline was executed directly
and its first same-source intermediate frame returned PreviewSuperseded. Interactive requests now
finish/publish one active frame while replacing the single pending state; final quality and source
switches retain strict latest-only cancellation. Unmounted photo/comparison surfaces cancel their
active and pending work instead of consuming Native worker capacity in the background. A second generation guard covers asynchronous JPEG
decode and deferred canvas/histogram callbacks, preventing a late intermediate frame from replacing
a newer or final frame. Native processing, output precision and final graph semantics are unchanged.

`64f005f` also refreshed whole-catalog counts on every History result. Rust now reports photographic
edit membership using the exact existing persisted-album classifier, including neutral curves and
execution-provider equivalence. React only invalidates counts on membership edges (including
Undo/Redo/snapshot restore and outgoing assets), Library changes or explicit context refresh. It
defers scans during dragging and does not rescan just because dragging ended with unchanged
membership. Unknown classification still requests an explicit count/error, never a fabricated zero.
No second durable edit-state database or stale pixel cache is introduced.

Interaction evidence: a deterministic one-second/10-ms-input/100-ms-render schedule publishes
frames during the drag and retains the latest final state; this is a scheduling test, not a claimed
100-ms measurement on the user's machine. 1000 pending updates stay bounded; JPEG reorder/final
guards, source changes and existing strict cancellation cases pass. Membership tests cover 10000
unchanged edited commits without count invalidation, Undo/Redo/off-page edges and unknown states.
184 frontend tests/27 files, lint, TypeScript and production build pass. Native warning-denied
Clippy and persisted-album equivalence tests passed; the previous `b17e4b8` full/release
success does not qualify these new edits. No dependency, license or image-algorithm change.

## 2026-10-05 M24/M25/M26 cross-page export closure

The previous React batch adapter intersected Select All IDs with the currently loaded photo page.
Off-page selections silently disappeared, and unvisited photos carried neutral in-memory settings
instead of their persisted edits. The Library path now sends compact asset IDs plus at most one
captured active workspace override. Rust resolves one catalog record/History file at a time on a
blocking worker, validates durable settings, derives the recipe-state hash from the actual settings,
and feeds the existing full-resolution shared graph/encoder/atomic writer. No pixel JSON, new
renderer, schema/dependency/license change or secondary edit-state database is introduced.

Only missing History means neutral. Missing assets, inaccessible files and corrupt/invalid History
produce per-item failures; even an active override cannot conceal corrupt saved edits. The entire
selection contributes to progress/cancellation. A batch guard prevents overlapping queues from
resetting cancellation and clears running state on worker errors. UTC capture naming uses SQLite's
calendar conversion; absent dates stay absent. The active edit is captured before the directory
picker and pending History writes are drained. Other assets are resolved from durable state when
their queue item is prepared, not from an invented page-local snapshot.

Targeted checks: 176 frontend tests across 26 files, including three new compact-selection tests
covering 500 IDs, active overrides and invalid identities; export regression/Golden/RAW validators,
lint, TypeScript, production build and warning-denied desktop/Library Clippy passed. A real Nikon
sensor fixture exercises saved exposure, neutral vs edited output, repeat-byte determinism,
metadata/date preservation, explicit missing/corrupt state and source immutability. The installed
executable self-test now restores canonical Native edit state and real Library metadata through the
same Library-export adapter after reopening the database, rather than constructing export edits
separately. Final same-SHA full/release qualification is required before delivering this change.

Final local validation passed: 330 ordinary Rust tests (four opt-in release/private-model tests
remain separately gated), all doc-test runners, warning-denied workspace Clippy, format and
photographic Golden/real LibRaw regressions; 176 Vitest tests, lint, TypeScript, production build,
JSON/schema, 561-package Rust / six-production-package npm license audit and release-resource /
offline-source validators. The last UI diagnostic addition was rechecked with the complete frontend
suite/build after the Rust full run began. Remote same-SHA and installed-runtime gates remain
distinct from these local results; no Lightroom performance/completeness claim is inferred.

## 2026-10-04 M24 whole-catalog sidebar counts

Replace page-local totals with `Library::count` using the same SQL predicates as queries,
without fetching photo records or pixels. `library_counts` runs off the UI thread and returns
All / latest import batch / five-star / persisted-edited counts. Deleted History identities
cannot inflate the count. Missing/error results show an unknown marker and diagnostic, never
a fabricated zero. Refresh is debounced after persisted changes; successful rating writes also
invalidate the totals even if slower than the optimistic UI update.

Targeted evidence: 17 Library tests passed (one unrelated release-only test ignored), 57 frontend
tests passed, 100k metadata count assertions, scoped normal/smart collection counts, empty and
stale ID counts; warning-denied desktop/Library Clippy, lint and TypeScript passed. Playwright
checked production App presentation with synthetic IPC: one visible row retains totals 100000
and 16666 instead of reporting page size. The earlier session/recovery cases still pass.
No imaging, dependency, model-license or redistribution changes. Persisted History scanning
remains proportional to saved history files; this batch does not claim a new performance parity
result against Lightroom. A new same-SHA release package remains required for delivery.
Full local candidate validation passed with 173 frontend tests, the complete Rust workspace
and doc-test runners, warning-denied Clippy, format, JSON/schema, production build, photographic
Golden and real LibRaw sensor regressions. License and release-resource validation also passed.

## 2026-10-04 M24/M29 Library session closure

Continues from qualified `095926b`, without changing imaging math or model distribution.
`SessionState.libraryBrowser` is an additive optional field: existing version-1 filter-only files
still load. Native persistence validates collection identity, bounded search and page/offset
arithmetic before atomic write. Invalid data cannot replace the prior recovery file.

The actual App waits for Library initialization before scoped restore, loads selected off-page
catalog records separately, then enables autosave. History hydration is reopened after replacing
restored editor records. Corrupt sessions and missing collections retain their recovery data;
users can explicitly discard only the session, not photos/History. Closing during incomplete
restore asks first and never marks the partial state as the saved clean session. Import resets
the stale collection/search scope and invalidates older queries.

Evidence: seven frontend restore/query tests; four Native session tests; the real Native
Library/History/Snapshot/Session/Export integration now restores an actual smart collection and
searches it before deterministic re-export. The production release self-test compares every
restored Session field, not merely the clean flag. A Playwright audit ran the production App with
synthetic IPC responses, delayed Library initialization, collection 4/search 京都/page 2 and a
selected off-page asset 700. UI scope and the first autosave matched the saved state. This is UI
ordering evidence only; Native processing is proven by Rust integration, not the browser fixture.
The missing-collection and corrupt-session UI cases also passed: neither wrote an autosave
across the actual debounce boundary, and both presented recovery choices. Full local validation
passed (172 frontend tests, workspace Rust tests/doc-tests, warning-denied Clippy, format,
lint, TypeScript, production build, JSON and Golden/RAW manifest validation). Release packaging
and remote acceptance for this change are not implied by the previous qualified installer.
The browser audit additionally exposed incorrect page-local sidebar counts; the follow-up above
repairs that defect separately.


## 2026-10-04 color interaction, whole-person and collection repairs

This batch continues from `aa5ed01`; its existing installer/CI are baseline evidence only.
No dependency or private-model redistribution decision changes.

- M7/M8: native color-band sampling now selects the nearest circular center instead of a
  flat-top weight tie. Signed grading chroma displays its real vector; captured drags preserve
  angle beyond the circle, neutral drags preserve hue, and keyboard changes wrap hue. The
  production wheel is separately testable and accessible. The legacy hue-lock flag is retained
  in state, but the inert toggle is replaced by a truthful always-hue-preserving description.
- M20: integrate (not replace) the existing pinned SegFormer ADE20K Person class 12. The UI
  dispatches `person`, shares scene-model availability with Sky, and no longer creates a face-only
  mask under a Person label. Restore verifies exact model/source identity and regenerates the
  same raster. Actual NASA fixture body probability 0.99277, background 0.00002235; shared
  Preview/Export and cold-restored output are exact. The expanded real-model release test passed
  in 82.12 s; this includes a new cold scene-session startup and is not an interaction-latency claim.
- M24: collection scope is applied in native SQL before search/pagination/Select All, including
  intersection with smart rules. The UI retains selected scope between pages, rejects stale query
  completions, and keeps existing editor state/thumbnails. Smart predicates now use Rust's
  internally tagged `field` contract; a shared JSON fixture validates both languages.
- M24/M25: Edited album reads validated persisted History state rather than only the current
  in-memory page, normalizes neutral two-endpoint curves and execution backend, and applies
  selected IDs through SQLite `json_each` before paging/Select All. Both query handlers run on
  background workers. No schema migration, stale secondary index, pixel load or source mutation.
  Tests cover reopen, Undo/Redo, empty subsets, identity curves and visible corruption errors.

- M19/M29: repeatable Advisor preview and Apply both use the immutable pre-preview baseline;
  changing suggestions replaces the staged delta instead of accumulating it. Pick/Reject in Edit
  targets the displayed photo, while Library keeps explicit multi-selection. Added UI intent tests.

Local batch evidence: 327 ordinary Rust tests (331 registered, four opt-in heavy/private tests),
165 frontend tests, warning-denied workspace Clippy, rustfmt, doc tests, lint, TypeScript/production
build, Golden 11/11 manifest and photographic regression, six RAW manifest entries and two actual
sensor regressions passed. The private actual-model opt-in test separately passed. Full local run:
`.starroom-reports/test-timing-1791107713790.json`; subsequent frontend-only repairs passed the
complete 165-test frontend suite. Remote/installer qualification must still match this batch's
commit, not earlier green CI. Original photos and public model policy remain
unchanged. All M1–M30 requirements remain in scope; no M31 work is started.
## 2026-09-15 GPU-resident creative preview and production dirty regions

- `GpuRenderer` now owns a persistent wgpu Device/Queue, two compiled pipelines and reusable
  source/working/output/staging/uniform/curve/mask allocations. Same-size slider frames update only
  the compact creative uniform and curve LUT; the immutable source upload is fingerprinted and
  reused. Production counters assert pipeline/resource reuse, source upload and one final readback.
- The Native shared preview graph fuses encoded relative Temperature/Tint, Exposure, scene-linear
  Tone, Master/R/G/B curves, OKLab/OKLCh Color Mixer, four-way Color Grading and HDR-safe Vignette
  into one compute pass. Vignette follows the existing CPU finishing reference exactly and is
  removed from the subsequent CPU detail call only when GPU execution succeeded, preventing a
  double application. Export continues to use the authoritative shared CPU reference graph.
- No parallel cache identity was introduced. `GpuStageCacheKeys` is a projection of the existing
  canonical `StageStateIdentity` boundaries for source, input/WB, global creative, local composite,
  geometry and display. Focused tests prove downstream edits preserve their upstream identities.
- High-resolution Native viewport rendering now permits local mask layers. Full-source normalized
  mask coordinates are evaluated against the decoded source region. Manual Clone/Heal with an
  explicit source expands the region by target, source, radius, feather, scale and graph halo, then
  remaps both points into the cropped working buffer. Auto-source and AI-inpaint Heal remain on the
  explicit full-frame/typed-unavailable paths because a cropped search would change semantics.
- The profiler now reports upload, final readback, fused creative duration, source/working texture
  hits/misses and tile-cache hits/misses alongside existing queue/cancel/decode/ICC/encode stages.
  The release fixture measured: cold JPEG Fit 2921.717 ms (baseline 1059.724), cached reopen 0.249
  ms (0.395, -37.0%), RAW first Fit 125.716 ms (119.397, +5.3%), RAW cached 0.264 ms (0.316,
  -16.5%), interactive 1024 106.170 ms (105.430, +0.7%), final refinement 416.602 ms (395.065,
  +5.5%), 100% viewport 284.144 ms (220.163, +29.1%) and 200% viewport 128.208 ms (118.216,
  +8.5%). These are honest single-run machine results; cold adapter/shader initialization and the
  still-CPU CameraTransform, LittleCMS output transform and JPEG encode dominate the remaining
  latency, so the aspirational 50/100/200 ms gates are not claimed as met.
- The 24/45/60/100 MP production workflow remains green after the source-region changes. Measured
  Fit/viewport/export at 100 MP were 2731.35/346.77/60964.02 ms with a 6,455,803,904-byte process
  peak. MSVC produced the release executable and NSIS installer; clean install, hidden launch,
  deterministic core self-test, real offline BiRefNet inference, legal-resource hashes and silent
  uninstall all pass. The smoke harness now uses an explicitly waited GUI process with redirected
  output files, avoiding PowerShell closing the GUI-subsystem self-test stdout pipe prematurely.

## 2026-09-10 rc.3 field image-quality, preview and workspace repair

- Exact-file picker/drop now enters the same transactional Native Library import path as folder
  import. The UI reloads persisted assets and progressive Native thumbnails; RAW paths are never
  assigned directly to an HTML image element, and the Library count is derived from Library rows
  rather than the Browser demo photo.
- LibRaw 0.22.2 exposes `cam_xyz` as XYZ-to-camera coefficients. The M3 adapter previously
  transposed that value as though it were camera-to-XYZ, producing the reported severe red/green
  Nikon cast. Resolver v2 now follows the pinned LibRaw `cam_xyz_coeff` white-normalization and
  inversion semantics before the existing D65/Rec.2020 stage. The private D750 field file was used
  only for local visual/timing verification and was not copied into the repository or CI.
- The four-channel monotone Hermite curve now prepares sorted points, secants and tangents once per
  render. Previously this allocation/sort ran once per pixel/channel even for identity curves.
  Known Hermite/HDR vectors and the existing Preview/Export/Golden regressions protect semantics.
- Preview demand now comes from actual canvas CSS pixels times device-pixel ratio. A 116% wheel
  zoom no longer forces a full 24 MP decode; pyramid tiers remain authoritative through 4096 display
  pixels, with source viewport tiles for true 1:1/deep zoom. Interactive quality is 1024-edge, the
  active frame is allowed to publish while only one newest request waits, and eligible full RAW
  decodes use a source/tier-keyed 512 MiB LRU bound. Canvas CSS geometry is independent of proxy
  raster dimensions, preventing the photo from shrinking/growing during slider refinement.
- The field workspace uses translucent blue glass panels, blur, edge highlights and larger filmstrip
  targets. Basic Color is now the first visible color group with blue-to-amber Temperature,
  green-to-magenta Tint and chroma gradients; advanced native OKLCh tools remain below it.

## 2026-09-10 v1.0.0-rc.3 close-request field hotfix

- The installed rc.2 close handler correctly prevented the native close request until Session state
  was marked clean, but its final `window.destroy()` call was outside the Tauri 2 capability ACL.
  The resulting `plugin:window|destroy not allowed by ACL` error kept the window open even after a
  successful clean-session write.
- The desktop capability now grants only `core:window:allow-destroy` and only to the existing `main`
  window. The save/confirmation behavior is unchanged: transient Browser-fallback edits still need
  explicit discard confirmation, clean Session persistence happens before destruction, and a real
  persistence failure still leaves the recovery envelope and window intact.
- Release validation reads the actual capability JSON and fails packaging if the permission is
  removed. No imaging, Library, Export, AI, network, privacy or source-file behavior changed.
- rc.2 artifacts remain immutable. The repaired build is versioned `1.0.0-rc.3` and requires a new
  same-SHA Blueprint plus Windows MSVC/NSIS install/runtime gate before publication.

## 2026-09-06 v1.0.0-rc.2 field validation

- Tauri asset URLs now grant only the exact generated thumbnail-cache file. Library thumbnails are
  generated outside the SQLite mutex and delivered by a bounded progressive queue, so one corrupt or
  unsupported asset cannot hold back the rest of the grid.
- Library schema v2 stores a monotonic import-batch identity. Registration uses filesystem identity
  and cheap raster header dimensions; full encoded/RAW metadata enrichment runs on a blocking worker
  without holding the catalog lock during decode. Recent Imports therefore means the exact newest
  batch, and filtered Ctrl/Cmd+A uses a native ID-only query that is independent of pagination.
- Removal is one Native SQLite transaction with foreign-key cascades and a retired-ID high-water
  mark. It never deletes or moves source files. Stable-ID Shift/Ctrl/Meta/Ctrl-Shift selection,
  shortcuts 0-5, visible thumbnail rating and immediate Five Stars filtering use this catalog state.
- Native Preview runs off the WebView thread. Per-surface latest-wins scheduling keeps one active and
  one newest pending request, cooperatively cancels superseded work between shared-graph stages and
  rejects stale results before publish. A bounded source/tier decode cache and process-wide GPU
  device reuse accelerate repeated sliders and A-B-A switching without caching creative output.
- Asset switching now paints the already generated Native Library thumbnail immediately and labels
  it as a refining state. The authoritative Native Fit or high-resolution tile replaces it when
  ready; the thumbnail is never treated as final 1:1 data or passed through Browser color math.
- The field UI uses Library/Develop/Retouch/Compare navigation. Develop owns Navigator plus
  Presets/Layers/History, including full Snapshot lifecycle; Export is a floating glass panel and
  Portrait/Layers/Looks are context-disclosed. Image math remains entirely in the Rust graph.
- Per the explicit user decision, BiSeNet remains an ignored local-only model and is not included in
  public rc.2 packaging. A clean install must state Model not installed for Face/Skin; no download,
  cloud, API or synthetic provider is permitted.
- Subject/Background is the one bundled rc.2 AI capability. The release job downloads the exact
  official BiRefNet v1 asset (id `186739942`, 224,005,088 bytes), verifies SHA-256 before Tauri
  packaging, and retains the upstream MIT license. Installed discovery prefers the executable's
  `models/local` resource directory while an explicit `STARROOM_LOCAL_MODELS` override remains
  authoritative. The production self-test requires Subject/Background Ready and Face/Skin Model not
  installed; a second installed-binary self-test initializes the CPU ONNX provider and executes a
  real finite Subject mask inference. Sky and AI Denoise remain explicit optional local states.
- AI mask sessions are now capability-scoped. A missing optional SegFormer Sky model can no longer
  prevent the independently verified BiRefNet Subject/Background provider from initializing; when
  all reviewed local models exist the combined provider path remains available.
- Native model availability is now queried before tool activation and verifies both file presence
  and the pinned hash for Face/Skin, Subject/Background, Sky and AI Denoise. The four UI states are
  Ready, Model not installed, Invalid and Error; model paths are not exposed in the normal status UI.
- Fit and high-resolution preview requests have distinct contracts. Slider interaction always uses
  the bounded tier; the settled 1:1/high-zoom request decodes/render the immutable source at its real
  dimensions, so it cannot publish an enlarged 1800/4096 frame. Viewport-only rendering/caching is
  now uses an SRP3 binary tile contract carrying source dimensions and tile origin. The WebView
  keeps its bounded Fit canvas and overlays the high-resolution tile at source coordinates, so a
  100 MP backing canvas and a whole-frame JPEG are avoided. Rust preserves f32/RAW profile metadata,
  expands tile-safe graphs by the declared halo, and caches encoded tiles in a bounded 64 MiB LRU.
  Global-coordinate stages are rendered through an explicit full-frame compatibility path and then
  cropped for transport; no incorrect local result or silent optimization claim is allowed.
  The real 24/45/60/100 MP release gate passed with fixed 4,718,592-byte viewport transport; measured
  previews were 0.85/1.40/1.28/2.08 s and peak process memory 1.59/2.93/3.89/6.45 GB.
- Professional batch preparation no longer decodes the source when AI Denoise is disabled. The
  FullResolutionRenderer performs the single required decode. AI Denoise still performs its model
  residual preparation and final graph decode, preserving existing output semantics.
- The manual rc.2 Release Gate records a deterministic 200-image Library registration, first
  thumbnail and cached-restart corpus plus 24 MP cold Fit, cached reopen, interactive Exposure,
  final refine and 100%/200% viewport timings. These runner values become the RC1-vs-RC2 report;
  ordinary source commits do not run the heavy corpus. Implementation-freeze Release Gate
  `34305888090` passed: 200 registrations 65.952 ms, first thumbnails 487.647 ms, cached restart
  57.610 ms, RAW Fit 463.501 ms, interactive response 93.010 ms and final refine 1460.835 ms.
- The shared CPU graph uses pinned Rayon 1.12.0 only for independent per-pixel stages. LittleCMS
  input/output transforms use `NO_CACHE` thread-local contexts and chunked parallel execution;
  a 32,777-pixel HDR regression compares against the serial transform within `1e-6`. Render order,
  ICC embedding and Preview/Export parity are unchanged.
- Real 24 MP export measured 17.447 s, at least 85.5% faster than the conservative rc.1 field lower
  bound of two minutes. The complete first JPEG Fit measured 2.151 s; Starroom exposes immediate
  Native thumbnail feedback while it refines rather than misreporting the thumbnail as final Fit.

## 2026-08-26 M30 release-candidate qualification

- M30 begins under feature freeze after M28 acceptance `94a9ccc` / `32942892981` and M29 acceptance `d83fd9f` / `32943720530`. Official `@tauri-apps/cli` 2.11.4 is an exact build-time pin. A separate Windows gate now builds the real MSVC release executable and NSIS installer, launches the unpacked and clean-installed executable, performs silent uninstall, records SHA-256 values and uploads the artifacts.
- The RC identity is synchronized as `1.0.0-rc.1` in Cargo workspace and all 23 locked Starroom path packages, npm package/lock and Tauri bundle configuration. Release validation rejects any stale lock entry; the dependency-license report was regenerated solely because both lockfile identities changed and retains the same 556 Rust / six npm / 268 notice closure.
- `scripts/validate-release.mjs` enforces synchronized package/Cargo/Tauri identity, release icon/notices, exclusion of tracked ONNX/checkpoint weights and absence of executable browser/Rust network-client APIs. XMP namespace/schema/source-reference URLs are data or comments and do not constitute a network client.
- The repository now carries the canonical GNU GPLv3 license text and the Windows bundle includes it alongside third-party notices, model provenance and the detailed dependency inventory. A cross-crate M30 recovery suite calls the production Session, History, Library thumbnail and Native Export APIs for corrupt state, missing assets and incomplete temporary output; corrupt user state is left untouched and surfaced as a typed error.
- `scripts/validate-licenses.mjs` resolves the locked Cargo graph and production npm closure, rejects missing/unreviewed license identifiers and compares a deterministic report against both lockfile hashes. The generated report currently records 556 registry crates and six production npm packages and is itself bundled with the installer notices.
- The same license gate extracts top-level upstream LICENSE/LICENCE/COPYING/UNLICENSE/NOTICE files, normalizes and content-hash-deduplicates 268 texts into `THIRD_PARTY_LICENSES.txt`. NSIS packages that file and the installer smoke compares every legal resource against the repository SHA-256 before launch and again confirms uninstall cleanup.
- Library startup now classifies SQLite corruption as `CorruptDatabase` and refuses future schema versions without downgrading them. A corrupt cached JPEG is decoded, rejected and regenerated from the immutable source instead of being returned to the UI as if valid.
- The production desktop binary has a bounded `--release-self-test <empty-directory>` diagnostic used by the installer gate. It imports a generated encoded source into the real SQLite Library, persists/reloads History and Snapshot identity, verifies crash/clean Session state, runs the same Native shared Export twice with byte parity, rehashes the immutable source and reports absent unbundled models as explicit `typed-unavailable`. The diagnostic refuses non-empty targets.
- Project sidecars now expose a production `read_sidecar` schema boundary, accept the legacy schema-1 state used before layers, validate current schema 2, reject future schemas and corrupt JSON without mutation, and use a same-directory fsync-backed temporary file for atomic replacement. The existing storage-independent non-destructive project representation remains unchanged.
- The registered Release Candidate workflow includes an explicit high-memory job rather than a plan-only proxy. It generates real 24/45/60/100 MP JPEG sources, opens a bounded Native preview, applies a radial adjustment Mask and production Healing operation, performs full-resolution Native shared-graph JPEG export, verifies dimensions/source immutability and emits elapsed/OS peak-memory metrics. This gate must actually pass before the earlier M28 100 MP limitation can be closed.
- Identity Geometry now transfers its owned working buffer without an identity resample, and an identity Denoise/Local Detail/Sharpen/Grain/Vignette group transfers the same buffer without the former chain of full-frame clones. The small regression compares against the former production calls, checks floating-point tolerance/exact detail output, asserts pointer reuse and relies on the existing profiled graph regression to retain Geometry/Detail stage telemetry. Non-identity processing is unchanged.
- Golden schema 3 activates five immutable photographs: NASA/scikit-image public-domain astronaut, Dgmsaikat CC0 iPhone 15 night city, JINDONG H CC0 Nikon D7100 ISO-2500 neon streets, and Luc Viatour/Benjamin Gimmel CC-BY-SA-3.0 Google Gallery derivatives. The manifest records source page/file, immutable upstream revision where available, author, license, retained license text, camera, format, dimensions, bit depth, EXIF/ICC state, byte length and SHA-256. Ten photographic cases share only visually applicable sources; ColorChecker remains the existing BSD numerical oracle rather than being mislabeled as a chart photograph.
- `m30_photographic_golden` decodes each unique active photograph through the production image boundary, proves identity Native Preview/Export byte parity, checks high-precision output for NaN/Inf, runs a deterministic multi-stage extreme edit twice, requires a material pixel change and rehashes source bytes after rendering. This is executable engineering regression evidence; final human visual/competitor A/B remains post-RC field validation.
- A same-SHA push/PR CI split exposed wall-clock creation metadata in dynamically generated LittleCMS built-in ICC profiles: one Export job crossed a one-second boundary and one did not. Starroom now canonicalizes only the 12-byte ICC creation-date header of its generated sRGB/P3/Adobe RGB/Rec.2020 profiles to a fixed release epoch, preserving all color tags/TRCs/primaries while making transforms and embedded profile identities byte-deterministic. Tests require stable bytes and successful LittleCMS reparsing.
- Targeted Windows Blueprint runs `33034036657` and `33034039310` passed on commit `96feaf1`: the previously intermittent Export profile test is stable, the five-asset photographic shared-graph regression executes, and Geometry/Detail/GPU/RAW/AI/dependent jobs remain green. The next commit is the unique immutable RC candidate and intentionally requests Full Acceptance once.
- M30 validation now follows Fast -> Targeted -> Full -> Release. The expensive Release Candidate workflow is manual-only, pins Rust 1.97.1 and has separate lock/target/profile-aware test and release caches. `docs/37_M30_RELEASE_BLOCKERS.md` is the closed-status ledger; GitHub zero-job startup failures are recorded as external infrastructure failures, not repaired with product changes.
- Level-4 remains deliberately unaccepted while exact-candidate Golden execution, the true 100 MP pixel workflow, MSVC installer/runtime, and physical multi-monitor qualification are pending. Non-Exposure GPU stages and unbundled AI weights remain explicit capability states rather than false release claims. `docs/35_M30_LEVEL4_RELEASE_ACCEPTANCE.md` records the remaining release and field-validation decisions.

## 2026-08-26 M28-M29 performance and desktop hardening candidate

- M28 instruments the Native render/export graph with per-stage CPU duration, local process current/peak working set and exact cache hit/miss counters. The M13 scheduler now exercises 24/45/60/100 MP plans, visible priority, generation supersession, dirty-region identity and bounded RAM/VRAM accounting. Scale regressions execute 100,000 metadata-only Library rows, 10,000 History commands and bounded 100/500-item export queues without loading a corresponding pixel corpus into memory.
- The GPU contract remains deliberately honest: wgpu Exposure is the currently migrated production node and is compared to the CPU oracle; Tone, Curve, Mixer, Grading, Detail, Masks, Skin, Healing and Geometry continue through the complete CPU reference rather than being labelled GPU-complete. Final preview state and Export stay deterministic shared-graph results.
- M29 introduces a single `CommandId` catalog used by both shortcuts and `Ctrl/Cmd+K`, atomic pixel-free session autosave, explicit Recover/Discard crash recovery, clean session restore, typed error presentation, and native Tauri file drop. Unsupported dropped files are visibly rejected; supported encoded/RAW paths enter Native preview and never silently choose Browser Canvas.
- Image pointer coordinates now share a CSS-pixel normalized mapper with zero-size guards and 100%/200% scale regression. The Command Palette traps/restores keyboard focus, existing semantic regions/ARIA/focus styling remain active, and reduced-motion/responsive rules are preserved. A controlled Playwright audit verified command search -> real Geometry action, modal focus containment, 1280x720 layout and local-origin-only browser requests.
- The M24-M26 workflow integration now persists/reopens the M29 session and compares the second Native export byte-for-byte with the first. Installer execution, clean-machine startup and physical multi-monitor qualification remain explicit M30 gates, not claims inferred from browser automation.
- M28 acceptance commit `94a9ccc` passed Windows Full Acceptance `32942892981`. Its measured report retains both runs, reports 64 full-frame versus four dirty tiles for the same 4096-square preview, and explicitly does not reinterpret plan-only 100 MP scheduling or compact fixtures as a full-resolution render benchmark. M29 production code passed in that same run; its dedicated acceptance commit follows without changing the verified behavior.

## 2026-08-20 M21-M23 native intelligence / look candidate

- M21 adds `starroom-ai-denoise` around the exact NAFNet-SIDD width-32 upstream checkpoint. `scripts/export-nafnet-sidd-width32.py` verifies upstream commit/checkpoint SHA, exports deterministic FP32 static 512x512 opset-20 ONNX and fixes only output shape metadata. Both checkpoint and ONNX remain ignored local files. Linear Rec.2020 D65 is robustly normalized, converted to linear sRGB, range-compressed and sRGB-encoded for inference; output is inverted back to working space as a residual. 512 tiles overlap 64 pixels with raised-cosine blending and visible-first ordering. Model inference and Amount/Detail/Color Noise/Preserve Skin adjustment identities are separately cached.
- The M21 adjustment stage is part of `starroom-pipeline` after optics/geometry and before relative color, tone, curves, mixer, grading and detail. Tauri resolves the local model and native residual before either Preview or full Export. Missing/hash/export/runtime/DirectML/inference/OOM/tensor/output/cancel states are explicit; an enabled edit without the exact residual fails instead of silently selecting classic denoise or Browser Canvas. Superseded previews cancel through a request token, while the M13 scheduler still rejects stale completion. A conservative 2 GiB working-buffer preflight prevents unbounded full-frame allocation.
- DirectML remains the preferred provider. A classified provider/runtime/inference failure reopens the same pinned ONNX on CPU and records both the active provider and fallback reason in the Native model-status command; invalid model hashes, invalid exports and malformed outputs never become a silent fallback. Inference caching excludes Amount/Detail/Color Noise/Preserve Skin, so those slider rerenders reuse the residual. Timing is hardware-dependent because the 117 MB local model is deliberately absent from CI; acceptance records deterministic tile/domain/cache behavior, and actual DirectML/CPU wall-clock measurement remains a release-machine qualification item rather than a fabricated CI number.
- M22 adds `starroom-reference`. It computes luminance quantiles, OKLab mean/covariance, eight OKLCh bands and three tone-region means in Rust, then fits bounded existing Tone/WB/Curve/Mixer/Grading semantics with a monotonic curve, skin protection and confidence. The editor exposes Select, Analyze, Preview, Apply, Reset and Save as Look with Amount/Tone/Color/Grading/Protect Skin. Category/global interpolation reuses M23 semantic curve and circular-hue policies; Amount zero is exact. TypeScript only carries paths, controls and compact adjustment state.
- M23 adds `starroom-look` and strict `.srlook` v1 schema with schema version, portable metadata and unknown-field/future/corrupt/range rejection. Portable state deliberately excludes crop/geometry, masks, healing, face identities and camera profiles. Amount and normalized A/B file weights interpolate semantic parameters, circular hue and sampled monotonic curves. Deterministic identity-seeded Grain includes Amount/Size/Roughness/Color; signed Vignette includes Midpoint/Roundness/Feather/Highlight Protect. Both execute after detail in the same Preview/Export graph. Local frontend acceptance is 33/33 Vitest, ESLint and Windows-safe production build. Authoritative warning-denied Rust compilation is recorded by the final Windows Level 3 CI because this workstation has no MSVC `link.exe`.
- M21, M22 and M23 targeted acceptance push runs are `32378734268`, `32379178627` and `32379573766`. The final acceptance commit is deliberately marked `[full-acceptance]` so GitHub runs warning-denied Clippy, rustfmt, the complete Rust workspace, frontend tests/lint/build, JSON/schema, Golden/RAW manifest and packaging validation rather than relying on targeted-path success.
- Cross-milestone regression covers AI Denoise → Detail → Native portrait mask retouch and Reference Match → Save Look → reload equivalence. A normalized A70/B30 Style Mixer recipe is also rendered through a Native adjustment Layer, radial Mask and deterministic Grain/Vignette with byte-identical Preview/Export output. DirectML runtime/inference fallback remains explicit and separately tested; hash/model/output failures never take that path.

## 2026-08-20 M17-M20 local portrait/editing acceptance

- M17 consumes M16 source-space soft Skin and protected Eyes/Brows/Lips/Mouth/Hair rasters inside `starroom-portrait`. A multi-scale low/high-frequency split drives smoothing, texture preservation and tone evenness; hue, chroma and face exposure are evaluated in Native working-space color. Neutral controls are exact identity, protected features are weighted out, and malformed/non-finite masks fail explicitly. The identical stage is called by Preview, Before/After and Export.
- M18 adds a serialized `HealingOperation` graph in `starroom-heal`: Clone and Heal support manual or deterministic auto source, source/target patch geometry, feather, opacity, rotation, scale, tone adaptation and texture adaptation. AI inpainting remains an explicit unsupported mode. The editor records continuous source-normalized freehand points with interpolated spacing; the same compact brush grammar is reusable by M15 masks and future AI-mask refinement. Rendering remains non-destructive and cache identity changes with operation state.
- M19 keeps recommendation logic local and deterministic. `starroom-advisor` computes histogram, ordered percentiles, clipping, contrast, chroma, white-balance estimates and optional M16 skin-weighted portrait statistics. Rules emit bounded parameter deltas, confidence, reason and priority. React stages a Native preview result before accepting it and supports Apply, Ignore, Dismiss and safe Apply All; there is no LLM, GPT, cloud call or telemetry.
- M20 reuses the M16 `ort 2.0.0-rc.10` session infrastructure. The fixed BiRefNet model produces a soft Subject probability and exact Background complement; the fixed SegFormer-B0 ADE20K model validates the 150-class output and extracts config-verified Sky class 2. Person/Skin/Hair continue through the fixed M16 portrait provider. DirectML is requested as one coherent provider and deliberately reopens on CPU if initialization fails; model absence, hash mismatch, runtime/init/inference/tensor/output/OOM/cancellation states are typed and never become a synthetic mask.
- Generated masks serialize provider/model/version/hash, semantic class, threshold, feather, invert, cache identity and metadata only. Rasters remain Native-memory values keyed by immutable source hash, semantic/provider identity, model hash and inference contract; global Exposure/WB/Tone/Curve/Mixer/Grading/Detail changes therefore do not invalidate inference. M15 Add/Subtract/Intersect/Invert and layer opacity evaluate the generated R16Float-compatible soft raster in the shared Preview/Export graph.
- The two M20 weights remain under ignored `models/local/` and are neither committed, packaged nor sent to CI. BiRefNet is recorded as MIT; the NVIDIA SegFormer weight is non-commercial research/evaluation only and blocks commercial distribution unless relicensed. CI validates contracts with deterministic fixtures and does not silently substitute absent weights.
- Local frontend gates pass without adding Browser color or creative math. This workstation still lacks the installed Visual Studio toolchain, so an isolated pinned Windows sysroot was used for focused M17-M20 Rust tests and Clippy; the final warning-denied workspace compile/tests and Level 3 acceptance remain authoritative on GitHub's Windows runners.

## 2026-08-13 M15 Native mask tree / layer compositing acceptance

- M15 is Starroom-owned local-edit architecture. `starroom-project::MaskTree` is persisted as a composable expression instead of a destructive raster: None, Radial, Linear, Brush/Eraser, Luminance and Color Range leaves combine through Add/Subtract/Intersect/Invert.
- `starroom-pipeline` evaluates the tree against finite post-geometry normalized image coordinates and linear working RGB. The resulting `0..1` mask multiplies the M14 layer opacity before Normal compositing; no intermediate RGB clamp or browser image processing is introduced. Unsupported Provider leaves are explicit typed errors, never guessed subject masks.
- The existing wgpu R16Float mask resource remains the GPU precision/resource contract. The current production reference is CPU-native and shares the same compact Tauri settings for Preview, Before/After and Export; future GPU execution must satisfy this reference's mask regressions.
- The layer inspector exposes serialized Native mask families and editable geometry/range/brush-point values, including eraser state. It transports intent and snapshot history only; Rust remains the sole evaluator/compositor. GitHub Windows Level 3 Full Acceptance `31713812877` passed both Web and Rust after a clippy-only correction in `7b9a5ed`.

## 2026-08-13 M14 non-destructive adjustment layers candidate

- The M14 layer stack is Starroom-owned document/render architecture, not a new image-science replacement. `NativeAdjustmentLayer` carries only serializable intent; the Rust shared graph applies enabled Normal layers in order in linear Rec.2020 D65 and blends their own native creative result at finite `0..1` opacity before Detail/output.
- Preview, Before/After and Export use the same Tauri `NativeEditSettings.layers` contract. Browser UI only mutates immutable layer intent and snapshot history; it does not evaluate an image or silently replace an unsupported blend mode.
- Sidecar `AdjustmentLayer` records keep their established stable order/mask references and now reject duplicate identity/order, non-finite adjustment values and invalid opacity before write. M15 will connect the persisted mask tree to this layer evaluator; M14 intentionally does not pretend that an unimplemented spatial mask is active.
- Windows Level 3 Full Acceptance run `31710577180` passed all Web and Rust checks after the M14 formatting and clippy corrections.

## 2026-08-13 M13 preview pyramid / scheduler candidate

- `starroom-render::scheduler` is Starroom-owned orchestration, not replacement image math: it selects fixed 512/1024/2048/4096 preview levels, scales image-space viewports, builds halo-aware tiles from the existing graph declaration and orders visible viewport work before its neighborhood and the remaining image.
- Every scheduled tile carries source-version identity, serialized graph state identity, pyramid level, output region and generation. A new request supersedes the prior generation; completion of old work is rejected rather than silently drawn. The bounded LRU accounts for RAM-derived frames and RGBA16Float-equivalent VRAM reservations independently.
- Tauri Native Preview now picks the requested pyramid level before decode, schedules the Native shared graph and only reuses a matching encoded frame. The full-resolution export command still decodes the immutable source and never reads preview cache pixels. A status command exposes cache/stale counters without transferring pixels through JSON.
- Windows Level 3 Full Acceptance run `31707532900` passed the Web and Rust jobs, including format, warning-denied Clippy, scheduler regressions, the Rust workspace suite and frontend validation. The earlier M12 GPU Clippy warning and the desktop `Cargo.lock` drift were repaired before this acceptance run.
- Regressions cover 24/45/60/100 MP deterministic plans, tile halo coverage, visible-first priority, generation cancellation, graph-key isolation and RAM/VRAM LRU eviction. The target has no new third-party dependency; pinned wgpu remains the resource backend. Windows CI acceptance is pending after a Clippy repair to the M12 GPU buffer-length expression.

## 2026-08-12 M12 GPU acceptance

- `starroom-render::gpu` integrates official wgpu 30.0.0 as a Windows DX12-first adapter. `GpuRenderer` owns instance, adapter/device/queue, shader module, pipeline/bind-group layout, buffers and explicit RGBA16Float/R16Float resource allocation. The only migrated production node is scene-linear Exposure; the CPU Tone/Curve/Mixer/Grading/Detail sequence stays the reference implementation rather than introducing a second colour-science path.
- Native Preview selects the parity-checked GPU Exposure path when available. DX12-unavailable, adapter/device, shader, unsupported-resource, OOM and device-loss states are typed and produce an explicit Native CPU-fallback flag/status; no browser fallback is selected. Export stays on the same shared CPU semantic graph for deterministic output.
- GPU regressions cover neutral, portrait/skin, landscape shadow, neon/high saturation, HDR, deep shadow, finite guards and the Native shared preview parity boundary. Windows push run `31620924649` passed GPU Check plus Web, RAW, Color, Detail, Geometry, Optics and AI gates. The local machine passes format/metadata, lint, Vitest, Golden validation and build; it lacks `link.exe`, so CI remains the native compiler authority.

## 2026-08-12 M7-M11 final acceptance

- M7 through M11 are production implementations in the Rust Native shared render graph. Preview, Before/After and Export share the same mixer, grading, detail, Lensfun optics and geometry stages; the TypeScript layer transports typed parameters and interaction state rather than image-processing math or full-frame JSON pixels.
- Authoritative Level 3 push run `31613588937` passed on Windows: rustfmt, warning-denied workspace/all-target Clippy, 123 Rust tests, immutable RAW/Golden validation, 26 Vitest tests across four files, ESLint, TypeScript and the production/package build. The workstation independently passed rustfmt, metadata/lockfile checks, JSON manifests, Vitest 26/26, lint and build; native linking remains delegated to CI because local MSVC `link.exe` is not installed.
- Acceptance commits are `9b99c3c` (M7), `7054f03` (M8), `12c31af` (M9), `b8b8d16` (M10) and `a6f2aad` (M11), followed by warning-denied CI compatibility fixes through `13c25ec`. Draft PR #2 remains open and unmerged. Work stops at M11; M12 has not started.

## 2026-08-12 M11 Geometry acceptance candidate

- `starroom-geometry` now owns normalized crop, original/common/custom aspect constraints, arbitrary/fine rotation, flips, scale/offset, horizontal/vertical keystone and a solved four-point projective homography. A single inverse-mapped bilinear resampler preserves finite scene-linear values and returns explicit invalid-buffer/singular/non-finite errors; the source file remains immutable.
- Upright is image-derived rather than a mode preset: Sobel line gradients estimate horizon roll and position-correlated vertical/horizontal convergence, with confidence-gated Auto plus Level, Vertical and Full policies. The implementation is Starroom-authored after studying the pinned darktable `ashift.c` architecture; no darktable code was copied, so no new GPL-derived provenance entry is needed beyond the existing behavioral-reference pin.
- Lens correction feeds Geometry before creative processing in the Native shared graph. Crop changes the authoritative rendered dimensions, and Preview/Before-After/Export use the same graph. `CoordinateMapper` explicitly names SourceSensor, OrientedImage, PostLens, PostGeometry, Viewport and Normalized spaces so later masks/layers/pyramids/GPU work cannot silently reuse coordinates in the wrong stage.
- React transports typed geometry parameters only. It provides direct numeric values, Free/Original/common/custom crop ratios, grid/crop overlay, Upright selection, rotation/flip controls and draggable plus numeric four-point guides. Edit snapshots provide undo/redo; project JSON stores geometry with backward-compatible defaults.
- Regressions cover identity and finite resampling, crop/original/common ratio, homography corner rectification, image-derived horizon analysis, coordinate composition, compact Native IPC and Preview/Export dimension/pixel parity. Local Vitest, TypeScript, ESLint and production build pass; authoritative warning-denied Rust/Native tests run on Windows CI because this workstation lacks MSVC `link.exe`.

## 2026-08-12 M10 Lensfun optics acceptance candidate

- M10 is a real pinned Lensfun provider rather than a trait/stub. The complete upstream v0.3.4 XML database at commit `101c745e847a5de4a1e569a94368ce2027198598` is embedded unchanged. Rust streams it through pinned `quick-xml` 0.41.0, resolves camera mount and lens, interpolates focal calibration, selects aperture/distance vignetting calibration, and adapts Lensfun's exact Poly3/Poly5/PTLens modifier equations.
- RAW camera/lens/focal/aperture/focus metadata is now copied through the LibRaw C ABI. JPEG/TIFF metadata is extracted by pinned `kamadak-exif` 0.6.1. Auto and manual matching both report typed `autoMatched`, `manualMatched`, `missingMetadata`, `unknownCamera`, `unknownLens`, `mountMismatch` or `ambiguous`; enabling correction without a usable profile is an error and never a silent generic fallback.
- Distortion, lateral TCA, vignetting and auto-scale switches execute on linear RGB before M7/M8 creative color, tone and detail. A deterministic bilinear CPU reference reports crop fraction and finite failures. Preview and Export call this same shared stage for RAW and encoded inputs. Before remains profile-disabled source processing.
- The UI exposes all switches, Auto/Manual identity entry and an explicit profile-status resolver. Project schema persists switches, matching intent, selected profile/status/version and manual identity. Profile data and adapted model source are retained under CC-BY-SA-3.0/LGPL-3.0 with NOTICE/provenance updates; no unverified system Lensfun DLL is loaded.
- Regressions cover official-database Nikon match/mount/profile ID, missing/unknown status, exact model finite/crop behavior, unknown-profile rejection, compact IPC state and Preview/Export parity. Local frontend acceptance passed with Vitest 25/25, TypeScript, ESLint and production build; authoritative C++/Rust compile/tests run on Windows CI because local MSVC `link.exe` is unavailable.

## 2026-08-12 M9 Detail Engine acceptance candidate

- Replaced the prototype one-radius Gaussian behavior with three distinct Native operators. Sharpen combines fine/coarse residuals with Amount/Radius/Detail, edge Masking and local-range Halo Protection. Denoise is an edge-aware range/spatial filter with separate Luminance/Chroma, Detail Protection and High ISO strength. Local Detail separates Texture (fine residual), Clarity (mid-frequency residual) and Dehaze (broad veil/local contrast); these are not aliases for one filter.
- The detail graph is ordered denoise -> local detail -> sharpen after Native creative color/tone and before the existing final output-gamut/display transform. Broad Dehaze support uses a summed-area O(n) blur rather than an impractical large separable kernel. No stage clamps scene-linear RGB to 0..1.
- Parameters have typed IPC range/finite validation, direct numeric UI, non-destructive snapshot undo/redo and project persistence. Browser Canvas is no longer used as a detail reference or silent fallback. Preview and Export share the same `render_working_graph` calls.
- Regression coverage includes sharpen identity/edge gain/masking/halo bounds, chroma/luma/high-ISO denoise and step-edge preservation, non-alias Texture/Clarity/Dehaze behavior, finite output and Native Preview/Export parity. Algorithm families and control semantics are compared against the already-pinned darktable quality foundation; no new third-party runtime was added in M9.
- Local frontend acceptance passed with Vitest 24/24, ESLint and production build; Rust formatting passed. Native warning-denied compile/tests remain authoritative in Windows CI because local MSVC `link.exe` is unavailable.

## 2026-08-12 M8 Color Grading acceptance candidate

- `starroom-grading` now owns four independent Global/Shadows/Midtones/Highlights Hue/Chroma/Lightness vectors plus Balance, Blending and master Amount. Smooth normalized tonal masks and crossover semantics follow the established design principles of darktable Color Balance RGB; the compact OKLab implementation is Starroom-authored and runs in the existing linear Rec.2020 D65 Native graph.
- Default controls return exact identity before conversion. Zone edits are finite for negative/wide-gamut and scene-linear HDR samples; no intermediate 0..1 clamp was added. Preview and Export invoke the identical grading call immediately after the M7 mixer.
- React exposes numeric four-zone controls and transports typed state only. Existing photo snapshots provide undo/redo; project schema now persists all four wheels and crossover controls with backward-compatible defaults. IPC validation rejects non-finite/out-of-range values rather than silently substituting them.
- Added shadow-vs-highlight isolation, balance/blending distribution, neutral identity, skin/neutral/neon/wide-gamut vectors and Preview/Export parity regressions. Local frontend acceptance passed with Vitest 23/23, ESLint and production build; Rust formatting passed, with authoritative native compile/tests reserved for Windows CI because this workstation lacks `link.exe`.

## 2026-08-12 M7 Color Mixer acceptance candidate

- The production Color Mixer is now an eight-band Rust Native stage in linear Rec.2020 D65 -> OKLab/OKLCh -> circular overlapping band weights -> independent Hue/Chroma/Lightness -> Rec.2020. Red wrap-around is circular, achromatic pixels remain identity, scene-linear HDR inputs remain finite, and display/output gamut compression stays at the existing final output boundary.
- Red, Orange, Yellow, Green, Cyan, Blue, Purple and Magenta each expose typed numeric controls. The React editor only serializes controls; it performs no color-science math. Native Preview and Export already share `apply_creative_graph`, so both invoke the identical mixer stage.
- The targeted tool sends only normalized coordinates and compact edit state to Rust. Rust decodes the source through the same RAW/encoded working graph and returns an enum band; neutral samples return no selection. Mixer state is covered by the existing non-destructive snapshot undo/redo and the project schema now persists all eight bands, hue-lock intent and band width.
- Added numerical regressions for hue lock, chroma/lightness independence, red circular overlap, neutral identity, scene-linear extremes, finite output and project/IPC round-trip. No new third-party executable dependency was introduced: conversions use the already-recorded Bjorn Ottosson OKLab reference and Starroom's existing Rec.2020 matrices.
- Local frontend acceptance passed (Vitest 22/22, ESLint, TypeScript and Vite production build). Rust formatting passed; local native linking remains unavailable because MSVC `link.exe` is not installed, so warning-denied Clippy/workspace tests remain gated by the Windows GitHub runner.

Record deviations, dependency-version changes, GPU/backend issues, camera exceptions, model substitutions, benchmarks and unresolved quality tradeoffs. Do not rewrite specification history to hide compromises.

## 2026-08-14 M16 Portrait Detection acceptance

- M16 adopts a real local `ort 2.0.0-rc.10` ONNX Runtime adapter rather than a trait-only face provider. It verifies exact SHA-256 values before opening the fixed OpenCV Zoo YuNet detector and yakhyo Face Parsing **BiSeNet ResNet18** parser. DirectML is selected only when both sessions initialize there; otherwise both are deliberately re-created on CPU and the actual provider is reported to the UI. There is no cloud, telemetry, browser inference or synthetic output substitution.
- YuNet output decoding validates named class/objectness/bbox/keypoint tensors, performs confidence/NMS multi-face selection and records normalized source bounds plus five landmarks. Stable IDs intentionally hash only this source identity and same-photo geometry; no biometric embedding or cross-photo recognition is created. The parser uses the required 512 RGB float `/255`, ImageNet mean/std and HWC-to-CHW contract, then turns logits into probabilities before semantic mapping.
- Semantic masks are projected back through the stored square 1.4 crop and eye-line rotation into source image coordinates. `Skin` subtracts eyes/brows/lips/mouth/hair/eyeglass probability; all masks remain finite soft coverage. A compact M15 `PortraitSemantic` leaf records face ID, region, threshold/feather, parser id/version/hash and cache identity, while native runtime cache holds the source raster. Preview and export attach the same cache values to `RenderSettings`; no parser raster travels in Tauri JSON.
- `MODEL_PROVENANCE.md` records YuNet as approved MIT and BiSeNet as **NON_COMMERCIAL_ONLY / REVIEW_REQUIRED_BEFORE_PUBLIC_RELEASE** due to its trained-model/data provenance. Both ONNX binaries remain in ignored `models/local/`, are not committed or CI-provisioned, and a no-model application exposes a typed unavailable state. Real model integration is therefore verified only on a deliberately provisioned local machine; public CI validates compiled adapter, mocks/semantic contracts and absence of model binaries.
- Local frontend checks passed: ESLint, Vitest 28/28 and production TypeScript/Vite build. `cargo fmt --check` and locked metadata pass. This workstation still lacks MSVC `link.exe`, so Rust acceptance ran on the authoritative GitHub Windows runner. Focused run `31724891922` passed Classify, Web and Detail Check; Level 3 Full Acceptance `31727256150` passed full JSON/Golden/lint/Vitest/build/packaging plus Windows rustfmt, warning-denied Clippy and the full Rust workspace for SHA `c508793`. M16 is accepted; PR #2 remains Draft and `main` remains unmerged.

## 2026-08-12 Development Acceleration Pass

- Added executable Level 1/2/3 routing through `scripts/test-target.mjs`. The eleven named targets own explicit Rust commands, related Vitest files and Golden tags; Level 2 adds shared pipeline/render regressions plus lint/format/build, while Level 3 retains the complete former acceptance surface and JSON/package checks.
- Golden schema v2 gives every scene canonical tags and validates them against one registry. `select-golden-fixtures.mjs` supports union or all-tag intersection without silently changing `planned` fixtures to `active`. Full Acceptance still validates every Golden and all immutable CC0 RAW bytes/hashes.
- Replaced unconditional duplicate full jobs with dependency-aware classification. Leaf changes run relevant checks; pipeline/render/project/workspace/CI changes fan out. A `[full-acceptance]` push, release tag or explicit workflow input runs authoritative Full Rust and Full Web. Action revisions are immutable; Cargo/native keys contain OS, architecture, complete compiler hash and lockfile hash. The npm cache accelerates verified `npm ci`; cached `node_modules` is deliberately rejected as stale-tree risk.
- Baseline before this pass: run `31597151940` took 8m28s wall time (Rust 8m25s: fmt 9s, Clippy 3m29s, tests 4m23s; web 22s), and the duplicate PR run `31597154881` took 8m20s (Rust 8m15s; web 34s). Every push paid both full native builds because there was no Cargo target cache.
- Local post-pass frontend baseline on the same workstation: Level 1 `test:web` 3.76s measured command time (Vitest 21/21 plus Golden contract); Level 3 Web 9.21s (JSON 0.05s, Golden/RAW hash validation 0.14s, lint 3.60s, Vitest 1.35s, build 4.07s). Local native timing remains unavailable because this workstation still lacks MSVC `link.exe`; the runner records that typed toolchain failure and Windows CI remains authoritative for Rust/LibRaw timing.
- Remaining performance risks: the first Windows cache population must still compile static LibRaw/LittleCMS; RAW sensor regressions remain intrinsically I/O/CPU heavy; path fan-out is intentionally broad for shared graph/contracts. Uploaded timing JSON and explicit cache-hit output make cold/warm comparison visible instead of guessing. Quality, Preview/Export parity and Full Acceptance were not relaxed.

## 2026-08-12 M6 tone curve candidate

- The Native shared graph now owns `ToneCurveSet` with Master, Red, Green and Blue curves. Every curve uses the existing tested monotone cubic Hermite mapper; endpoint tangents extrapolate scene-linear values outside 0..1 instead of clipping HDR data. The legacy single curve remains a backward-compatible Master fallback.
- Preview and Export invoke exactly the same curve stage. Added native channel-curve parity regression; UI transport serializes the curve set without transmitting pixels. Remaining UI channel controls and project-side persistence are tracked until acceptance.
- The M6 editor exposes Master/Red/Green/Blue tabs, per-channel direct point creation/drag/right-click deletion/numeric editing, endpoint X anchoring, Identity/S-curve/Black-fade presets and histogram background from the actual rendered preview. All operations use the existing non-destructive history snapshot and IPC submits a compact typed curve set to the Native graph.
- M6 implementation acceptance passed on 2026-08-12: Push run `31594724778` and Draft PR run `31594728996` completed green with format, warning-denied clippy, full Rust workspace including RAW tests, Golden-manifest validation, lint, Vitest and production build. The following acceptance-record commit remains Draft and does not merge `main`.
- Final coverage audit added an explicit production `AutoWhiteBalanceProvider` boundary backed by the active gray-world implementation, skin/mixed-light regression, custom four-channel preset save/load, dedicated identity/S-curve/HDR-extreme/RGB tests, a real Nikon RAW curve Preview/Export parity test, and synthetic portrait/gradient Golden vectors. Synthetic vectors are deterministic and redistributable but do not replace the still-planned license-reviewed photographic Golden set.
- Final coverage acceptance passed after correcting the S-curve assertion to operate in the actual linear Rec.2020 working domain: push run `31597151940` and Draft PR run `31597154881` completed green. This is the M6 stop point; M7 was not started.

## 2026-08-12 M5 white balance / calibration candidate

- M5 introduces a typed `WhiteBalanceMode` in the Rust Native shared graph: `SourceDefault`, `AsShot`, `Camera`, `Auto`, `NeutralPicker` and `Relative`. LibRaw continues to apply the recorded Camera Neutral / As-Shot multipliers before its linear camera-RGB output reaches the M3 camera-profile stage. `AsShot` and `Camera` therefore retain real RAW semantics; encoded JPEG/PNG/TIFF only accepts `SourceDefault`/`Relative`, and asking it for a camera WB returns a typed error instead of silently inventing one.
- Auto uses a bounded, deterministic gray-world provider that excludes near-black and clipped samples. Neutral Picker accepts a small normalized source-space ROI, estimates a green-referenced diagonal correction from finite pixels, and is executed before native creative colour/tone. The existing M3 Bradford D50/D65 adaptation remains the formal camera/profile chromatic-adaptation stage; picker/auto act in the resulting linear Rec.2020 D65 working space.
- Relative Temperature/Tint retain their explicitly non-Kelvin encoded-image meaning. RAW values remain downstream fine corrections to the true LibRaw Camera/As-Shot basis, never a claim that an encoded image has physical camera Kelvin metadata. `Project.whiteBalance` serializes intent, controls and optional ROI with a backward-compatible `sourceDefault` default, ready for UI undo/copy-paste state.
- Added Native graph numerical regressions for neutral gray picker, Auto WB with clipped/HDR extremes, typed RAW/encoded semantic rejection and invalid ROI rejection. The implementation uses existing LibRaw and the already-proven LittleCMS/Bradford working-space stage; no new third-party executable dependency was added. Acceptance/CI result will be added only after the M5 acceptance commit is green.
- M5 acceptance passed on 2026-08-12: Push run `31591507530` and Draft PR run `31591511674` completed green, including format, warning-denied clippy, the complete Rust workspace/full RAW fixtures, manifest validation, lint, Vitest and production build. Draft PR #2 remains unmerged.

## 2026-08-12 M4 tone / light candidate

- M4 selects a **direct GPL-derived / private-use adaptation** of darktable's stable generalized-loglogistic sigmoid response, rather than retaining the former simple highlight division. The source is `darktable-org/darktable` `release-5.6.0`, tag object `f89bf9231fb21db0a53b3c279ff164caef48cef8`, commit `3c17b2976793303c186a5f64e8c9635ecf8b15d3`, `src/iop/sigmoid.c`, GPL-3.0-or-later. The adapted function is isolated in `crates/starroom-color/src/lib.rs`, carries a source marker, and the project workspace now declares GPL-3.0-or-later. `NOTICE.md` and provenance identify the corresponding-source duty before any external binary release.
- Exposure remains scene-linear EV multiplication. Contrast operates around 18% middle gray in stops. Highlights use the adapted film/paper shoulder only in the bright zone, then luminance-ratio RGB scaling preserves hue/chroma; Shadows have a zero-at-black influence weight so `Shadows +50` cannot introduce a white veil at true black. Whites/Blacks retain deliberately narrower zone weights than Highlights/Shadows. No creative stage clamps to 0..1; gamut bounding remains solely at the declared output boundary.
- The same `apply_tone` function is used by rendered images and LibRaw camera-profile output in `starroom-pipeline`; Preview, Before/After and Export remain the same native graph. M4 tests are numerical Golden vectors while redistributable portrait/backlight/night photographs are still honestly `planned` in the fixture manifest.
- M4 acceptance passed on 2026-08-12: GitHub Actions push run `31589000442` and Draft PR run `31589004929` completed green, including format, warning-denied clippy, native tone tests, full RAW regressions, manifest validation, lint, Vitest and production build. The Draft PR was not merged.

## 2026-08-12 — M3 camera profile / RAW color candidate

- The LibRaw bridge still owns sensor parsing, black/white normalization, As-Shot WB and mature demosaic, but no longer performs the final camera-to-working transform. It emits 16-bit linear, white-balanced camera RGB with an identity `rgb_cam`; Rust converts to authoritative `f32`, resolves a typed camera profile, executes camera RGB -> XYZ D65 -> linear Rec.2020 D65, and only then enters the shared creative graph. Preview, Before/After and Export therefore use the same explicit profile stage.
- `CameraProfileResolver` consumes LibRaw's public `cam_xyz` plus both public `dng_color` records. Embedded DNG ForwardMatrix is preferred; ColorMatrix is combined with CameraCalibration and inverted when ForwardMatrix is absent. Two calibration sets are selected/interpolated in reciprocal-temperature (mired) space using the As-Shot neutral estimate, and DNG's D50 PCS result is adapted to D65 with the tested Bradford stage.
- Native Nikon, Canon, Sony and Fujifilm inputs use LibRaw's identified camera matrix through an extensible family resolver. A DNG with valid embedded matrices is resolved independently of make. An unknown camera or invalid/missing matrix enters a visibly reported `Generic RAW Profile` using the documented linear-sRGB generic basis; it is never labeled as a known profile or silently substituted.
- Every descriptor carries stable ID, resolver version and SHA-256 over the exact matrix/policy fields. `Project.cameraProfile` persists ID/version/hash/status with backward-compatible optional deserialization. Native SRP2 preview transport appends only the small UTF-8 profile ID before the JPEG payload; pixels remain binary, and export reports the profile ID/hash.
- Added the `colour-science/colour` v0.4.7 `DATA_BABELCOLOR_AVERAGE` 24-patch xyY reference under its retained BSD-3-Clause license. Its source URL, tag object, source blob SHA-1, illuminant, observer and published precision are recorded. The Golden image scene remains `planned` because no chart photograph/baseline has been accepted; only the numerical oracle is active.
- M3 limitations: the active CC0 Apple iPhone SE DNG contains no valid embedded DNG matrix exposed by LibRaw, so its actual fixture regression requires the visible `Generic RAW Profile` state rather than inventing a camera profile. The current production bridge supports three-channel camera RGB, which covers the active NEF/ARW/CR2/CR3/DNG/RAF fixtures. Unusual four-channel sensors must receive an explicit matrix/demosaic contract before being claimed. No proprietary DCP profiles or silent make/model substitutions are bundled. GPU parity remains a later milestone.
- M3 acceptance passed on 2026-08-12: GitHub Actions push run `31561022046` and Draft PR run `31561024876` both completed green. The Windows run passed format, warning-denied clippy and the complete Rust workspace including the native ColorChecker and six-camera RAW regressions; web passed manifest validation, lint, Vitest and production build. The PR remains Draft and `main` was not merged.

## 2026-08-12 — M2 LibRaw sensor pipeline candidate

- Integrated the real LibRaw 0.22.2 source at peeled commit `b93f6e45c194f5df9b02a43b1af9a54b4f41f33f` (annotated tag object `24fa7e5463cbf8b8615dbd2b16c933a294d52400`) under the selected CDDL-1.0 path. Upstream source and notices are vendored unchanged; `starroom-raw` owns an original narrow C ABI bridge instead of leaking LibRaw structs through the Rust workspace.
- Develop uses `open_buffer` -> `unpack` -> sensor-buffer validation -> `dcraw_process` -> 16-bit memory image. The bridge never calls `unpack_thumb` or any embedded-JPEG API. Library thumbnails remain a separate future optimization.
- LibRaw owns format parsing, black subtraction/scaling, camera As-Shot WB and mature AHD/X-Trans demosaic. It emits linear Rec.2020/D65 (`output_color = 8`, unity gamma, no auto-brightening) at 16-bit precision; Rust converts those samples to authoritative `f32` without an 8-bit intermediate. Preview may request LibRaw half-size sensor processing and then a Lanczos3 bound; export reopens and fully develops the immutable source.
- `RawMetadata` records format, make/model, decoder, RAW/active sizes and margins, orientation, Bayer filter or 6x6 X-Trans layout, black/channel-black/white levels, As-Shot multipliers, derived green-normalized Camera Neutral, pre-multipliers, demosaic provider and LibRaw version. Errors distinguish unsupported extension/input, corrupt/invalid sensor data, decoder failure, invalid RGB and non-finite output.
- Native Tauri Preview, Before/After and Export now dispatch through `DecodedSourceImage` and the same Rust shared processing graph for rendered or RAW files. RAW is explicitly reported as `LibRaw camera matrix`; a Native RAW failure is surfaced and never triggers Browser Canvas fallback.
- Added six byte-for-byte CC0 files from raw.pixls.us commit `6f997cac925e9fe7dbf2a41d8e242398d8c9d4d4`: Nikon D1 NEF, Sony DSLR-A100 ARW, Canon PowerShot S2 IS CR2, Canon PowerShot G5 X Mark II CR3, Apple iPhone SE native DNG and Fujifilm X-Pro1 X-Trans RAF. `fixtures/raw/manifest.json` records source, archive/project attribution, CC0 license, camera, format, size and SHA-256. JavaScript validation and Rust regressions reject missing/mutated/mislabeled fixtures.
- RAW regressions perform full sensor decode/demosaic for all six sources, require finite non-empty Rec.2020 output, active-area and level sanity, positive WB/Camera Neutral, correct CFA family, source immutability and pinned binary version. A shared-graph integration test measures decode time, first preview render and slider-only rerender independently; thresholds are deliberately generous CI safety ceilings, while emitted `RAW_METRIC` lines provide actual measurements.
- M2 acceptance passed on 2026-08-12: GitHub Actions push run `31513488590` and Draft PR run `31513493542` both completed green. The Windows PR run passed format, warning-denied clippy and 79 Rust tests; the full six-camera sensor regression took 48.94 seconds and the Native preview/shared-graph regression took 2.64 seconds. Web acceptance passed manifest validation, lint, 20 Vitest tests and the production build. The timing tests independently measure decoder, first-preview and slider-rerender intervals and enforce 120 s / 30 s / 30 s CI ceilings; these are safety limits rather than performance targets.
- Deliberate M2 limitation: camera color currently uses LibRaw's identified camera matrix and As-Shot path. DNG dual illuminants, ForwardMatrix, explicit D50/D65 profile resolution, Generic Profile state and persisted profile fingerprint belong to M3 and are not claimed here. The CC0 decoder fixtures validate formats, not portrait/HDR/night visual quality; Golden scene entries therefore remain honestly `planned` until matching license-cleared scenes and reviewed baselines exist.

## 2026-08-11 — M1C Native preview vertical slice

- Desktop JPEG/PNG/TIFF imports now retain the dialog-selected source path and use Rust for the actual preview: bounded decode in `starroom-imageio`, LittleCMS input transform, linear Rec.2020 D65 working graph, relative WB, exposure/tone, curve/color/detail stages, sRGB output, then JPEG encoding. Export reopens the full-resolution source and enters the same `render_shared_graph`; it never promotes preview pixels into export input.
- The Tauri 2 IPC contract deliberately keeps pixel arrays out of JSON. `native_preview` receives a small serializable request (`sourcePath`, `maxEdge`, edit state) and returns a versioned `SRP1` binary frame through `tauri::ipc::Response` (20-byte header plus JPEG payload). `native_export_jpeg` receives source/output paths and writes the encoded result directly. Tauri channels are reserved for future progressive/tiled streaming; cross-platform shared memory was rejected for this slice because ownership and lifecycle complexity are not justified before profiling.
- React/TypeScript only maps serializable UI values, parses the binary envelope and displays the returned JPEG. No new color-science or creative pixel math was added. Existing `src/imagePipeline.ts` math is marked deprecated and remains solely as an explicit `Browser fallback` for browser-hosted imports and the bundled SVG demo.
- Native failures never invoke Browser Canvas. The visible status and preview badge identify `Native CPU` or `Browser fallback`; unsupported M1C edits (Masks, Optics, Geometry, Clarity, and signed-negative detail modes) produce an explicit error instead of being ignored. Those tools remain subsequent native-graph work, not completed foundation claims.
- Before/After uses two requests to the Native graph: neutral serializable settings for Original and current settings for Edited. Native JPEG export also checks that its destination is not the source path.
- Added a shared frozen fixture at `tests/fixtures/m1c/browser-native-reference.json`. Vitest regenerates the Browser reference, while a Rust integration test compares Native CPU output against documented per-case thresholds for neutral identity, Exposure, relative WB, Tone and Curve. These tolerances describe the migration gap and must tighten rather than disappear as mature tone/WB foundations replace temporary references.
- Added official `@tauri-apps/api 2.11.1`, `@tauri-apps/plugin-dialog 2.7.2` and Rust `tauri-plugin-dialog 2.7.2` for binary IPC and scoped desktop path selection. Local Rust link tests remain blocked by the missing MSVC `link.exe`; Windows CI is authoritative for native compilation/tests.

## 2026-08-11 — F0 provenance and Golden Image specification

- Added the first reviewed third-party inventory in `docs/17_THIRD_PARTY_PROVENANCE.md`. It separates reference-only foundations from binary dependencies and records purpose, immutable upstream version/SHA, license, derivation status, binary inclusion and external-distribution risk.
- Added the Golden Image contract in `docs/18_GOLDEN_IMAGE_SPEC.md` and the machine-validated required-case manifest in `fixtures/golden/manifest.json`. All eleven required scenes carry identity, extreme-control, finite-number and tone/color-regression obligations; CPU/GPU parity is reserved as a mandatory future assertion when GPU stages arrive.
- Golden source photographs are deliberately not fabricated or downloaded without redistribution review. Entries remain `planned` until their hashes, licenses, ICC/EXIF metadata, ROIs, settings vectors and reviewed baseline artifacts exist.
- CI now validates the Golden manifest structure and required case IDs before frontend checks.

## 2026-08-11 — M1B LittleCMS provider and shared color graph

- Added the production `LittleCmsProvider` using pinned `lcms2 6.1.1`, `lcms2-sys 4.0.7` and its statically compiled LittleCMS 2.19 source (`LCMS_VERSION 2190`). The binary version has a regression assertion tied to the provenance inventory.
- Input pixels now use an embedded RGB ICC profile when present; missing profiles take the explicit, reported `assumedSrgb` fallback. Invalid embedded profiles are typed errors and do not fall through to sRGB.
- LittleCMS converts encoded input RGB through ICC PCS into a generated linear Rec.2020/D65 working profile. After shared creative/detail stages, the same graph converts working RGB to either the sRGB fallback, a supplied display ICC or a supplied export ICC.
- Native preview and export have named entry points over one `render_shared_graph` implementation. Their transform report records actual input/output profile sources and the working space.
- Tests cover Bradford D50/D65 mapping and round-trip, all four ICC rendering intents, embedded ICC equivalence, missing-profile fallback, invalid input/output profile errors, NaN/Inf rejection, display-profile preview, supplied export profile and preview/export graph identity.
- The browser Canvas pipeline remains a temporary interactive slice; connecting file import/export UI to the native Tauri graph is still M1C work and is not claimed by this provider milestone.
- Local Rust linking remains blocked by the workstation's missing Visual C++ linker. Formatting is checked locally; Windows GitHub Actions is the authoritative Clippy/compile/test gate for this native change.

## 2026-08-11 — v0.2 native workspace quality gate

- Repaired the `starroom-heal` test indexing rejected by Rust 1.97 Clippy and retained explicit `(width, x, y)` coordinate semantics through a shared test helper.
- Corrected the OKLab inverse XYZ matrix sign for the S contribution to X. The previous negative sign caused measurable lightness and chroma drift after hue rotation; a Rec.2020 RGB -> OKLab -> Rec.2020 RGB round-trip regression now protects the conversion chain.
- Truncated two color-matrix literals only beyond meaningful `f32` precision, as required by the current Clippy `excessive_precision` lint.
- Regenerated `Cargo.lock` after activating rendered-image I/O dependencies. Rust CI now uses `--locked` for Clippy and tests, and checks formatting without mutating the checkout.
- Local native compilation remains unavailable because this workstation has no Visual C++ linker. Two independent Windows GitHub Actions runs passed workspace format, Clippy and tests before the stricter reproducibility gate was enabled; the updated gate remains the authoritative Windows validation.

## 2026-08-09 — M0 workspace and interactive shell

- Added the React/TypeScript/Vite application shell, theme tokens, persisted Dark/Gray/Light theme, persisted Simple/Pro mode, collapsible Library/Filmstrip, tool rail, inspector controls, local image import, Before state, and reversible UI adjustment history.
- Added a Rust workspace with `starroom-core`, `starroom-project`, and a narrow Tauri 2 command boundary.
- Added source-identity SHA-256 verification and a test proving identity reads do not mutate source bytes.
- Added frontend and Windows Rust CI gates.
- The current canvas uses a non-color-managed browser preview. Adjustment controls intentionally do not use CSS image filters; decoded-image color management and shared CPU/GPU render output remain M1 work.
- Pinned TypeScript to the current 6.0 series because `typescript-eslint` 8.66 does not yet accept TypeScript 7. Frontend validation used React 19.2.8, Vite 8.2.1 and Vitest 4.1.10.
- Vite/Rollup exits after transform when the repository is addressed through its Unicode Windows path. `scripts/windows-safe-build.mjs` creates a temporary ASCII `subst` alias for production builds and removes it afterward; Linux/ASCII paths run directly.
- Installed Rust 1.97.1 through official rustup and verified `cargo fmt --all --check`. Local Rust compilation is blocked by the machine's missing Visual C++ linker (`link.exe`). The GNU fallback also lacks a working external MinGW `dlltool` chain. Windows CI is the independent compile/test path; installing the multi-gigabyte Visual Studio C++ Build Tools was intentionally not performed implicitly.
- Added `開啟 Starroom.cmd` and `scripts/start-starroom.ps1` as the supported double-click Windows entry point. Direct `file://` access to the Vite source `index.html` now shows a launch explanation instead of a blank page; the launcher verifies/builds `dist`, starts the local preview service, waits for readiness and opens `http://127.0.0.1:4173`.

## 2026-08-09 functional browser editing slice

- Replaced the decorative library, filmstrip, histogram and adjustment controls with a working browser implementation. Imported JPEG, PNG, WebP and SVG files now become independent photo items with selection, rating, edit counts, history and library filters.
- Added a deterministic CPU canvas pipeline for exposure, contrast, highlights, shadows, whites, blacks, temperature, tint, vibrance and saturation. The histogram is calculated from the rendered preview rather than placeholder data.
- Added per-photo undo/redo, Before/original preview, reset, Fit/100% display and full-resolution JPEG export. Export creates a new download and never overwrites the source file.
- Browser previews are limited to a 1,800-pixel longest edge to keep interaction responsive; export re-renders from the decoded source dimensions.
- This slice is rendered-file editing only. RAW decoding, ICC-aware color management, GPU/wgpu rendering, crop/geometry, curves, masks, optics and detail tools remain future milestones. Unimplemented tool buttons are visibly disabled instead of simulating edits.

## 2026-08-09 complete interactive shell pass

- Made the Library, Edit and Compare header workspaces functional. Library presents a photo grid, Edit presents the single-photo editor, and Compare renders original and edited output side by side.
- Added safe removal from the Starroom workspace through the editor toolbar, filmstrip thumbnails, library cards and the Delete key. Removal revokes browser object URLs but never deletes the source file from disk; the last remaining photo is protected.
- Enabled every inspector tool with reversible CPU-rendered controls: three-region tone curve, sharpness/clarity/noise reduction, a feathered radial center mask, vignette/edge brightness, 90-degree rotation and horizontal/vertical flips.
- These are intentionally bounded first implementations, not claims of full production equivalents. Curve control is three-region rather than a freeform spline; Masks currently supplies one centered radial mask; Optics does not yet use camera/lens profiles; Geometry does not yet crop or correct perspective.
- Replaced the initial decorative curve graphic with a parameter-driven SVG. Its smooth line and Shadow/Midtone/Highlight control points now update from the same authoritative values used by the pixel pipeline and stay clipped within the graph viewport.

## 2026-08-09 direct-manipulation editing pass

- All range controls now pair with bounded numeric inputs. White balance uses a 2,000–12,000 Kelvin value with 6,500 K as neutral. The redundant Simple/Pro switch was removed so there is one complete inspector.
- Replaced the fixed three-region curve UI with a serializable point list used by both preview and export. Left-click adds points, dragging changes input/output, right-click removes non-endpoints, and the selected point also exposes numeric input/output fields.
- Detail uses a signed -100…100 contract for Sharpness, Clarity and Noise Reduction. Positive sharpness now applies a stronger unsharp term; negative sharpness blends toward the local blur. Pixel tests cover visible sharpening and Kelvin channel response.
- Radial mask geometry is normalized and serializable. The photo overlay supports click-to-place, interior drag, independent width/height handles and a rotation handle; the inspector exposes numeric center, size and angle values. The same rotated ellipse drives CPU preview and export.
- Geometry now accepts arbitrary -180…180 degree values, calculates a non-clipping output canvas, and retains 90-degree shortcut and flip controls.
- The editor viewport supports 25–600% wheel zoom, Fit reset and left-button pan while zoomed. Zoom/pan remain view state and do not alter exported pixels.
- Edit history snapshots now include adjustments, freeform curve points and mask geometry. Source files remain untouched.
- Playwright validated numeric input, 9,000 K, curve add/drag/right-click-delete, on-photo mask placement/resize/rotation, 33.5-degree rotation, 47%/212% wheel zoom and drag pan. The final browser console check reported zero errors and zero warnings.
- Remaining production gaps are unchanged: no RAW decoding, ICC display/output management, wgpu path, multi-mask stack/brush masks, lens-profile correction, crop or perspective correction.
# RC2 Field Validation (2026-09-06, in progress)

- Diagnosed missing Tauri asset-protocol permission for generated Library thumbnails. Grant
  the exact cache file only; do not expose original directories or broaden wildcard scope.
- Removed the frontend all-thumbnail display barrier. Thumbnail work is bounded/progressive
  and native raster generation runs outside the Library mutex. Concurrent cache writes use
  unique temporary paths and atomic replacement; a corrupt cache stays in place until replaced.
- Added catalog-only transactional bulk removal and retired-ID allocation protection so a newly
  imported photo cannot reuse a removed photo's history sidecar identity. Original images are untouched.
- Added stable-ID range selection, bulk removal UI, and clear-rating keyboard command with regressions.
- The user chose to retain BiSeNet local-only and not publicly bundle Face/Skin in rc.2.
  This exception is a release limitation, not a claim of clean-install offline readiness.
- Actual acceptance and remaining blockers are tracked in `docs/38_RC2_FIELD_VALIDATION.md`.
- Replaced synchronous preview IPC with background Native execution, a one-active/one-latest
  per-surface queue, request-scoped shared-graph checkpoints, a bounded decoded-source cache and
  reusable GPU device. Stale completion is rejected before it can update the WebView.
- Added a dedicated Desktop Native targeted CI job because the earlier `src-tauri` path classifier
  only ran Web tests and could let IPC compilation errors escape a targeted push.

# M27 Professional Export Completion (2026-08-25)

- Added an `f32` encoded-output surface to the existing Native shared graph. LittleCMS still owns
  the one working-to-output transform; RGB8/RGB16 quantization moved to the file encoder boundary.
- Added real 16-bit PNG and TIFF encoders, ICC/EXIF/XMP container metadata, decoder round-trip depth
  checks, 1,024-step precision regression and an explicit JPEG-16 typed rejection.
- Added Print Low/Standard/High sharpening whose Gaussian radius and gain depend on final dimensions
  and resize ratio. Screen and Print have distinct parameterization and finite regressions.
- Expanded export transport/UI to select PNG/TIFF 16-bit and Print sharpening. React transports
  intent only; it performs no color, precision, resize or sharpening math.
- GPS remains off by default and explicit preservation requires real transferable source EXIF.
  Rating/keywords/copyright/camera/capture fields use XMP plus safe EXIF according to policy.
- `crc32fast 1.5.0` is now a direct image-I/O dependency solely for standards-compliant PNG XMP
  chunk CRC generation; the resolved version was already in the codec dependency closure.
- Durable architecture and validation details are in `docs/32_M27_PROFESSIONAL_EXPORT_COMPLETION.md`.

# M24-M26 Library, history and professional export (2026-08-24)

- Added `starroom-library`: SQLite WAL/foreign-key/busy configuration, ordered migration V1, reference import, deterministic sampled asset fingerprint with full-hash escalation, duplicate/relink state, existing RAW/rendered metadata adapters, three-size native thumbnail cache, typed queries/workflow/keywords/collections/missing/project relationships and Tauri Library UI.
- Added `starroom-history`: typed command/state history with stable SHA-256 edit versions, slider/curve/brush transaction coalescing, deterministic undo/redo and redo truncation, periodic checkpoints, atomic versioned persistence, named snapshot lifecycle/undoable restore and cache identity.
- Added `starroom-export`: immutable full-source decode into the existing shared graph, LittleCMS sRGB/Display P3/Adobe RGB/Rec.2020 output, JPEG/PNG/TIFF 8-bit encode, Lanczos3 resize, separate Screen output sharpening, safe metadata intent, Windows-safe templates, collision policy, recipe identity, batch isolation/cancel and atomic writes.
- React remains UI/interaction only. Library queries, history replay and all image/color/export processing remain Rust Native. Browser-demo assets are original-only and cannot edit or export; native errors never activate a browser creative fallback.
- Current validated capability deliberately rejects 16-bit PNG/TIFF and Print sharpening with typed errors. No silent depth downgrade, ICC fallback, overwrite, duplicate, relink or source modification is permitted.
- See `docs/29_M24_LIBRARY_WORKFLOW.md`, `docs/30_M25_HISTORY_SNAPSHOTS.md` and `docs/31_M26_EXPORT_ENGINE.md` for the durable architecture and acceptance contract.

# RC3 field workflow completion (2026-09-12)

- Left and right workspace panels now expose pointer-driven accessible resize separators. Widths are bounded, persisted locally and honored by the glass theme, collapsed state and Library workspace.
- Retired the obsolete `lensBrightness` adjustment from the current editor contract. Lens correction remains the Lensfun-backed Native optics state; vignette remains the explicit finishing control.
- Removed Browser Canvas creative rendering and browser JPEG export from the application production bundle. Non-Tauri demo assets display their original pixels with an explicit Native-desktop requirement; all real edits and exports use the Rust shared graph.
- Added a local-only YuNet + BiSeNet setup flow. Users choose both ONNX files, Rust verifies the pinned SHA-256 identities before copying them into the per-user application-data model directory, and Face/Skin becomes available only after both installed copies verify. No model is downloaded, uploaded or committed to the public repository.

# Edit Workspace UI V2 (2026-09-19)

- Rebuilt the Edit workspace visual hierarchy around centralized `--sr-*` design tokens, bounded
  glass surfaces and module-level Bento grouping. Persistent blur remains limited to the three
  major workspace surfaces; sliders and individual controls do not create nested blur layers.
- Added semantic color tracks for Temperature, Tint, saturation and all eight target-aware OKLCh
  Hue/Chroma/Lightness controls. The controls transport existing edit state and do not perform
  image processing in React.
- Added a state-connected, keyboard-accessible OKLab grading wheel for Global/Shadows/Midtones/
  Highlights while retaining precise numeric controls and the Native grading graph.
- RAW decode now records correlated as-shot Kelvin derived from real camera-neutral/profile data.
  A read-only Tauri command exposes that estimate from a background blocking worker, rather than
  pausing the UI thread. A unit test caught and corrected the McCamy denominator sign for a D65
  neutral. If the matrix is absent or decoding fails, the UI explicitly reports that no reliable
  Kelvin estimate is available. Rendered RGB sources remain labeled relative, not fabricated K.
- Added actual current-photo camera, resolution, lens, shutter and ISO display when library
  metadata exists. Existing Histogram, Curve, Masks, History, AI, Filmstrip, Preview and Export
  state/actions remain connected to their prior implementations. The left Presets tab now exposes
  only real saved custom curves and portable `.srlook` load/save actions, with an explicit empty
  state rather than fabricated preset cards; the filmstrip shows filename and real rating.
- Browser layout audits at 1920x1080 and 2560x1440 found no page-level horizontal scroll or
  numeric editor overflow. The signed text input avoids browser number-field sanitization and
  native spinner overlap. These screenshots use the explicit low-resolution Browser demo asset,
  not a native RAW photo, so they establish UI layout rather than native image-quality acceptance.

## Field repair: preview reliability, edit history and glass inspector (2026-10-01)

- Native GPU preview now catches device/driver panics inside the cache boundary and clears an
  already-poisoned cache. A failed GPU request is visibly labelled Native CPU rather than leaving
  every subsequent photo in `PreviewGpuFailed: poisoned device cache` state. Wgpu validation scopes
  now cover both exposure and creative pipeline construction; oversize dispatches are rejected as
  typed GPU errors before submission. Export remains on the shared authoritative Rust graph.
- Native History commands are serialized. Commits use the last backend-acknowledged state, and
  editing is held while the selected asset's persisted history opens, preventing the observed
  stale-`before` `InvalidHistoryEntry` race on rapid slider changes/photo switches.
- The duplicate Presets/History strip under the photo was removed. The inspector now owns one
  scrollable tool region, a working disclosure button, RGB display histogram, single-column Color
  layout at normal widths, and legible glass/mask surfaces. RGB histogram code analyzes only
  already-rendered preview bytes and never participates in color science or export.
- The private per-user model installer verifies fixed SHA-256 values for YuNet, BiSeNet, BiRefNet,
  SegFormer and NAFNet before copying existing local files. It does not download or publish them;
  the public Windows installer still bundles only the redistribution-approved BiRefNet weight.
- A manually invoked local-only smoke test initializes all private ONNX Runtime providers and
  executes YuNet detection, BiSeNet parsing, SegFormer sky masking and a NAFNet denoise tile.
  The public release self-test separately executes BiRefNet Subject inference. The local-only
  smoke is intentionally ignored by public CI because its required weights cannot be checked in.
- Browser checks cover 1280 and 1920 widths, Color overflow, inspector disclosure and AI action
  routing. A browser demo cannot prove native RAW quality or GPU recovery; those require the
  rebuilt desktop executable and real-photo field validation.

## Field verification and catalog repair (2026-10-02)

- Presets/Looks and History now have exactly one production JSX owner, the left sidebar. The
  retired center strip and its CSS/grid rows are removed, not merely hidden. Structural guards
  ensure the central photo workspace cannot accidentally regain preset/history action paths.
- The mask workspace exposes selected-layer radial, linear, brush, luminance and color-range
  tools, visibility/opacity/order/duplicate/delete/invert, manual refinement and shared Native
  local tone. Canvas radial/linear edits retain feather and operate on the selected layer; the
  optional overlay is a Native preview-only intent, never exported into the image.
- Portrait/generated mask rasters are validated once at the shared-graph boundary instead of
  rescanning the full raster for every sample. Sampling remains bounds/shape/finite checked.
  A 512x512 regression took 0.016 s in debug; the old quadratic path exceeded 164 CPU seconds
  before completion. Unsampled NaN/Inf and malformed buffers are still rejected by both graphs.
- UI request epochs reject stale AI, reference, look, color-sample and optics completions after
  photo changes. Snapshot rename/delete flush and serialize history, and cannot replace another
  selected photo's history. Duplicate IDs are created before React schedules an updater. Brush
  and healing capacity errors preserve all previous edits rather than silently slicing them.
- Native header and Develop keyboard ratings persist to the catalog; Library rating commands
  still use the batch path. Library metadata/keyword controls have a real disclosure/300px track,
  and export settings are reachable from both workspaces and the shared command dispatcher.
  Supported compact widths retain workspace navigation. Browser originals are explicitly
  read-only: disabled/inert edit controls and a state-update guard prevent non-rendering edits.
- LibRaw active-area dimensions and orientation, not the 32-pixel metadata preview or doubled
  half-size estimate, now populate RAW Library metadata. `library_refresh_metadata` repairs only
  a selected legacy record on a blocking worker, outside the catalog mutex. Fingerprints and
  catalog source identity are checked before/after decode and again before metadata merge;
  relink/file-change races are typed errors. Ratings, keywords and history remain untouched.
  Real Nikon RAW regression repairs 32x21 to 2012x1324, checks reopen/source hash/history bytes,
  and verifies workflow updates interleaved with decode are not overwritten.
- Full local Windows MSVC validation: 299 Rust tests pass, four opt-in tests remain explicitly
  ignored in the ordinary suite; all doc-test runners, format and warning-denied Clippy pass.
  Frontend has 92 tests (77 behavior/unit plus 15 production TSX/CSS structural guards), lint,
  TypeScript and production build. Golden manifest is 11/11 with five immutable photographic
  sources and six public CC0 RAW fixtures. JSON/schema and license/release validators pass.
- The current GitHub stable compiler deprecated `AtomicU64::fetch_update`. Import batch tokens
  now use a standard compare-exchange loop compatible with both release Rust 1.97.1 and newer
  stable Rust, without suppressing warnings or changing the CI toolchain. The returned token is
  the successfully committed new value; a fixed-clock, backward-clock and 2,048-token concurrent
  regression also prevents the old-value reuse bug. Library tests and warning-denied Clippy pass.
- RGB histogram channels have shared base fill/stroke styles for gray and light themes as well
  as the dark glass overrides. A cross-theme structural guard prevents SVG default-black paths;
  the histogram remains display analysis and does not alter source or rendered image pixels.
- Rust 1.99 stable additionally linted the decoded-preview test's constant pixel chunk size.
  The regression now uses `as_chunks::<4>()` with the same pixel sequence and assertions; it
  passes on release Rust 1.97.1 without allowing or disabling the new lint. The GPU recovery
  pixel regression and full warning-denied Clippy pass locally.
- CI's npm audit surfaced development-only Vitest/mocker path traversal and brace-expansion
  denial-of-service advisories. Vitest is now exactly 4.1.11, brace-expansion resolves to 5.0.12,
  the license lock identity is regenerated, and npm audit reports zero known vulnerabilities.
  All 92 frontend tests still pass. Product/runtime dependencies and image processing are unchanged.
- The separately invoked private-model regression executes actual YuNet/BiSeNet inference,
  Skin shared-graph preview/export, SegFormer sky and tiled NAFNet residual application on the
  photographic NASA fixture. Debug CPU inclusive timings were 4.066/5.944/2.391/6.350 s;
  these are initialization/inference test timings, not slider latency or human image-quality
  acceptance. Private weights remain ignored and are never part of the public installer.
- All installer launch/self-test paths explicitly use the installed model root and working
  directory, preventing private AppData/cwd weights from contaminating a clean-install test.
  Public resource inventory requires exactly the pinned MIT BiRefNet model; Face/Skin, Sky and
  NAFNet must report typed-unavailable there. This machine's five verified local weights are
  automatically discovered without selecting them for each photo.
- The desktop test tool initially launched a same-name older executable, so that screen was
  rejected as evidence. The user subsequently authorized launch, and the uniquely named exact
  `991b048` executable was tested through the native UI. No shell/UI automation bypass was used.
  Actual interactions exposed additional release blockers; automated Native/installer tests
  are not substituted for this gate.

## Native desktop interaction defects and repair batch (2026-10-03)

- A real NASA portrait at Exposure -0.07 / Shadows +49 produced solarized tonal bands on both
  CPU and DX12 GPU (only 1 LSB between them). The shared additive shadow mapping reversed
  luminance ordering at its fade-out zone; GPU transport was not the cause. The fix adapts the
  already pinned/licensed darktable generalized-loglogistic response to a normalized shadow
  interval, preserves true black and 18% gray with a smooth endpoint slope, and keeps HDR
  values unbounded. Dense monotonic ramps, real photographic detail and shared Preview/Export
  plus CPU/GPU tests cover the defect. React still performs no creative image math.
- The M1C Browser migration fixture retains all frozen Browser RGB bytes and exact JS tests.
  Its affected tone case receives a separately identified approved Native darktable oracle:
  the old Browser additive shadow response is not a professional tone-quality target. The new
  Native bound is 1 LSB / 0.5 mean rather than weakening the old 12 LSB / 7 migration gap.
- Manual, AI and portrait mask layer controls used UI `exposureEv`, while the shared Rust
  `ToneParameters` wire schema requires `exposure_ev`. A dedicated validated bidirectional
  compact-state adapter and a JSON fixture consumed by both TypeScript and the actual Tauri
  request deserializer cover the boundary, including old camelCase History/project intent.
  The same real deserializer exposed snake_case fields inside Rust mask enum variants; the
  adapter must recursively map linear/portrait/generated mask fields and preserve their model,
  cache and operation identities. Local relative color, RGB curves, mixer and grading intent
  returned by Native projects must survive layer copy/edit/history paths even when the current
  UI does not expose each local-color control.
- The same UI pass found rotation remained at 90 degrees after Undo. History hydration must
  map every existing Native adjustment back to UI units, including geometry, grading, detail,
  Lensfun and hue-lock. The generated legacy radial layer must be restored exactly once, not
  retained as a layer and synthesized again. Full settings round trips are required.
- These are bug fixes under feature freeze, not new milestones. `991b048` remains a rejected
  interactive candidate despite its green CI; source and rebuilt desktop verification must
  complete before a new acceptance claim.
- Dense gray ramps additionally exposed negative Highlights/Whites folding and combined
  Shadows/Blacks folding. Negative Highlights and Whites now use the same pinned darktable
  paper shoulder with unit-slope anchors at 0.34 and 0.72 respectively. Each downstream zone
  is evaluated against its own stage input, not stale pre-shadow luminance. All 30 single-control
  neutral/extreme cases and 124 paired/all-six extreme combinations are finite and monotonic;
  combined cases cover separate 65,537-sample dark (0..0.3) and HDR (0..16) ramps. No tolerance
  was relaxed and no image-quality test was removed.
- The UI projection retains Native-only sharpen threshold, mixer band width, grain seed and
  AI provider through preview/export, History, Snapshot, Look and copy/paste paths. Local color,
  curves, mixer and grading survive layer duplication and local tone editing. Canonical Rust
  defaults are shared as a fixture for older legal History JSON; original acknowledged JSON
  and hash chains are never rewritten. The effective native sharpening/denoise values override
  obsolete convenience scalars when projecting into the existing UI.
- Pure RGB curves, WB mode/sample and non-default Native constants now count as edit intent,
  enabling Reset and edited-state detection. Generated legacy radial controls hydrate once;
  modified/reordered reserved layers remain intact and a new center control gets a distinct ID.
- Persisted grain retains its u64 schema. The JSON UI boundary explicitly rejects seeds above
  9,007,199,254,740,991 with `UnsafeRenderSeed` / `NativeRenderContractInvalid`, rather than
  silently rounding a native-authored seed and changing deterministic texture.
- The local full gate passes 306 Rust tests (four opt-in gates remain separate), all doc-test
  runners, format, warning-denied Clippy, 132 frontend tests in 20 files, TypeScript, lint,
  production build and JSON/packaging/Golden validation. Real photographic Golden passes all
  five immutable photos; sensor regression passes six CC0 RAW files. Private real-model AI
  inference/graph parity additionally passes once (YuNet/BiSeNet 4.076 s, Skin 6.083 s,
  SegFormer 1.828 s, NAFNet 5.288 s inclusive debug CPU measurements, not slider latency).
  Exact rebuilt UI and same-SHA release qualification remain required and are recorded in the
  dated deliverable report when complete, not inferred from source tests.

## Native numeric-edit history debounce repair (2026-10-03)

The rebuilt `8e48dc7` GUI proved local-mask IPC and natural Shadows +49 rendering, but a delayed
numeric edit exposed an additional History bug: focus/pointer-down created an unchanged-state
debounce, which consumed the gesture token before the user committed a value. The photograph
changed, yet Undo/Redo restored the preceding recorded state. Native persistence now schedules
every actual render-state change against the last acknowledged Native state, independent of
gesture lifetime. Focus-only state creates no command; restore/open do not create synthetic
commands; a paused slider continuation still persists. Three additional production-helper tests
cover delayed numeric commit/Undo/Redo, paused drag continuation, and open/restore suppression.
The acknowledged JSON/hash chain and serial command architecture are unchanged. This is an
interaction/persistence bug fix, not image math, a schema migration or a dependency change.
The final rebuilt GUI must repeat delayed numeric input, Undo/Redo and close/reopen before delivery.
The state comparison is canonical JSON (sorted object keys, preserved array order and omitted
undefined optionals), since Rust serde_json::Value does not preserve JavaScript insertion order.
Both debounce scheduling and queued commits use the same comparison; two further regressions
prevent focus-only phantom commands without altering acknowledged JSON or persistent hashes.

## Common-tool and thin-mask field qualification (2026-10-03)

- Production radial/linear canvas controls are extracted into `MaskOverlay.tsx`. Main outlines
  use 1.1 CSS px, feather guides 0.75 px, no filled wash, and 8 px visible pins with separate
  24 px transparent hit targets. Sizes remain CSS-pixel constant across zoom/aspect changes.
  Pointer capture and existing edit callbacks are preserved; focus states and keyboard fine/
  coarse movement, resize and rotation are tested. The local visual fixture imports those
  actual components and production CSS; it is explicitly **not** a Native render/runtime proof.
- Manual LensIdentity now has one camelCase wire format and legacy snake_case read aliases.
  A shared JSON fixture passes TypeScript serialization and the actual Tauri deserializer plus
  mature Lensfun profile resolution. Native f32 values round-trip as typed values, not a false
  bit-exact comparison with decimal JSON literals. Old History is projected for display and
  comparison only, never rewritten or rehashed; aliases cannot create focus-only edits.
- The existing white-balance clipboard now transports Temperature/Tint with mode and sampling
  intent, preserving unrelated adjustments and Undo/Redo. It never copies the source camera's
  neutral/profile into another camera. RAW-only and encoded-only modes are visibly disabled for
  the wrong source kind. AI skin protection is visibly unavailable until a real referenced Skin
  raster exists; model unavailability is not hidden behind an apparently working slider.
- Native color targeting and neutral sampling use the actual post-lens/post-geometry image.
  Immutable source-space semantic rasters are mapped through the existing inverse geometry and
  Lensfun green-channel sampling grid for local masks, Skin and AI preserve-skin. Exact cardinal
  rotations avoid negative trigonometric round-off black edge pixels. New deterministic tiny
  fixtures, real LibRaw sensor sampling and DX12/CPU plus Preview/Export tests cover the changes.
- NAFNet residual cache identity must describe the actual precreative input (source, WB, optics,
  geometry and source region), not only dimensions. Creative tone/curve/color controls remain
  excluded from inference identity. Advisor uses the same attached generated/portrait/denoise
  artifacts as Preview/Export instead of failing or dropping valid edits.
- Per-control release profiling exercises 27 control families on one 24 MP source: five
  **changed** settings per family at the 1024-edge interactive tier, median and nearest-rank
  p95 reported independently. Identical cached-frame reopening is measured separately, never
  presented as slider latency. Same-machine Lightroom A/B has not been performed; these
  timings are Starroom measurements, not a claim of identical proprietary behavior/speed.
- No dependency, model asset, mature foundation revision, license or source-photo bytes changes
  in this batch. Public clean-install AI remains limited to the distributable bundled BiRefNet;
  separately licensed/local-only weights retain their explicit boundary. The user requested
  final GUI inspection on their own; automatic Native/installed qualification remains required.
- Same-source AI History/Snapshot/session restoration now lazily regenerates a missing in-memory
  mask using the exact pinned local provider and validates original source/model/semantic/cache
  identity. Portrait/Skin intent carries optional versioned Native source-pixel crop metadata;
  older absent metadata remains absent in JSON and is recovered only when the original crop hash
  can be proved. Cross-source mask/face reuse is explicitly rejected with instructions to generate
  a mask or detect a face on the current photo; no arbitrary raster is silently substituted.
  No pixels, model weights or camera calibration are added to project/history JSON.
## 2026-10-10 DNG AnalogBalance and actual metadata promotion

The no-ForwardMatrix resolver previously inverted CC*CM while omitting DNG AnalogBalance.
The C ABI now extracts the original four-channel gains; RawMetadata serializes them with
identity default for older records. Resolver v5 appends inverse(AB) to inverse(CC*CM) before
the existing baked-WB undo/measured-white adaptation. It never multiplies decoded sensor
pixels again and does not alter LibRaw cam_xyz's already incorporated analog correction.
Invalid/non-positive/non-finite analog metadata takes explicit Generic Profile status.

The actual authored full/half sensor decode initially failed: public dng_color held CM and
illuminant values but parsed_fields=0, causing a Generic profile. Upstream identify.cpp copies
selected-IFD values without their presence bits. The narrow covered modification now promotes
each actual bit alongside that selected copy, preserving the pre-populated Leica ForwardMatrix
guard. No value-based presence inference, custom TIFF parser or demosaic was introduced.
The pinned upstream commit remains unchanged; modification/hash/CDDL obligations are recorded
in vendor STARROOM_MODIFICATIONS and provenance/NOTICE. No new external package/version is
added; serde_json is an existing workspace package newly used by imageio tests only.

Regressions include independent noncommuting AB*CC*CM order, no duplicate LibRaw correction,
invalid explicit status, strict original 24-patch chart and actual sensor ramp against fixed
double-precision Bradford/XYZ coefficients, immutable bytes and old/new metadata round-trip.
Full/half generator presence and AnalogBalance are checked, not merely mocked input structs.
The original headroom test/thresholds remain unchanged. Generator LF hash is updated in the
RAW manifest. Complete signature validation, Forward/mixed conversion, iterative dual-white,
physical editable RAW WB and every remaining production/release gate remain open.

RAW decode policy advances to `starroom-libraw-v3-dng-metadata-analog-balance`; existing
Native, thumbnail and Export identities bind that policy together with resolver v5. The
targeted RAW gate now includes actual imageio sensor-headroom/analog metadata regressions.

Local Full Rust passed format, warning-denied workspace Clippy, every ordinary unit/integration
test and doc-test runner (`test-timing-1791625273478.json`). Full Web passed 198 tests/29 files,
lint, TypeScript/build, all 11 manifest cases/5 immutable photos/6 CC0 RAWs and packaging config
(`test-timing-1791625066923.json`). The six real sensor formats passed (51.07 seconds in Full);
actual synthetic full/half sensor tests and strict four chart tests pass. License validator
passes 561 Rust packages / 6 npm production packages / 269 retained notices after lock identity
refresh. These are local component/integration gates, not installed/offline/100MP Final acceptance.
