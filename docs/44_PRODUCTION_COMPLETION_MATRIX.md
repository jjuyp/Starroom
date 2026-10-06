# v1.0 Production Completion Matrix

Status: **Phase 0 audit in progress; Final Acceptance is NOT achieved.**

Scope: the user's `FINAL COMPLETION & COLOR ENGINE PERFECTION PASS`, Phases 0–30.
Baseline source is `b36692f55a50a64106815b55083582bbb13d523d`. Historical M1–M30
checkmarks and green RC workflows are implementation evidence, not proof of these stronger
requirements. Do not shrink this matrix to existing tests, tag Final, merge main or publish
private model weights. All original requirements remain active.

## Evidence rules

GPU allocation repair (2026-10-06): unused working/mask buffer reservations removed without
pixel/shader changes. Real 512px/DX12 buffer sizes 12,616,192 bytes versus prior declarations
17,859,072 bytes; ownership/reuse/resize/profile tests pass. Native-only timing and actual
buffer bytes do not verify end-to-end latency, physical VRAM, 100MP or GPU direct presentation.

- `IMPLEMENTED / PARTIAL` means inspected production code exists; it does not mean accepted.
- `OPEN` means a demonstrated defect or missing production requirement.
- `REVIEW` means evidence is incomplete, including real UI, photographic or hardware verification.
- A phase can become `VERIFIED` only with real source -> Native -> UI -> preview -> export,
  targeted/Golden/performance evidence appropriate to its scope. No phase is VERIFIED yet.
- The source review is ongoing. Function references below identify inspected paths, not a claim
  that every line of every module has already been reviewed.

## Phase requirements to actual production ownership

| Phase / feature | Implementation | Native / GPU / CPU | Preview | Export | Consistency evidence | Existing tests | Performance evidence | Known issue / status | Required action |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 Repository audit | AGENTS, current task, TODO, architecture and owned source | All ownership boundaries | Inspect actual call path | Inspect actual call path | Older ledger is incomplete | Workspace inventory | Existing reports only | REVIEW | Complete per-module read/sweep and requirement trace; keep every phase in scope. |
| 1 Color science | `starroom-color-management`; pipeline source/working/output boundaries | LittleCMS CPU; RAW matrix CPU | Default sRGB display bytes | Four generated ICC profiles | Shared graph; supplied display profile unit test | ICC/adaptation/finite tests | Per-frame source/output transformations | OPEN | Wire monitor/profile ownership; audit alpha, RAW headroom, named transfer curves, negative RGB, SDR/HDR and destination gamut. |
| 2 Tone / curve | `starroom-color::apply_tone`, `PreparedCurve`; GPU Native-generated tone LUT | CPU reference / fused GPU | Real Native | Real CPU shared graph | Existing tone/curve GPU tests | Black/midtone/HDR/monotonic/identity/extreme | Basic local samples ~96–110ms, not full UI latency | REVIEW | Mature-reference/image-quality metrics and end-to-end latency, spline nonuniform spacing/endpoint stress. |
| 3 WB / saturation / vibrance / mixer / grading | LittleCMS measured/relative CAT, protected chroma, Native mixer/grading | CPU reference / fused GPU globals / prepared local CAT | Connected | Connected | Matrix + real portrait/RAW CPU/GPU/shared/local tests | Exact neutral/black/axes/picker/finite/circular bands/true grayscale/protected vibrance | 512px Native controls ~30–32ms median; not full UI latency | OPEN | Relative and sampled WB now use CAT; physical editable RAW camera gains, robust sampling and full perceptual/end-to-end acceptance remain open. |
| 4 GPU residency / direct presentation | `GpuRenderer::apply_creative`, Native binary JPEG frame, canvas presentation | GPU compute -> CPU float readback -> ICC/JPEG -> WebView | Hybrid, not direct surface | CPU reference | GPU basic parity only | Resource reuse/readback counters | One readback + CPU conversions per frame | OPEN | Audit practical Native presentation, remove avoidable transfers/allocations/polls; retain correctness and typed fallback. |
| 5 Shader fusion | Tone/curves/mixer/grading/relative color/vignette fused | GPU | Globals fused; local/spatial CPU | CPU same operators | Fusion tests cover selected combinations | Shader/parity tests | Fused wall-time measurement | OPEN | Vignette currently precedes downstream local/detail work on GPU but follows it on CPU; verify cross-combinations and fix order. Remove unused old WGSL tone routine. |
| 6 Interactive proxy / HQ refine | Native request phase, bounded 1024 edge, Fit/high-resolution contracts | Native CPU/GPU | Real proxy + final | Full source | Final graph common | Interactive edge/final tests | Engine samples, not pointer-to-paint | REVIEW | Adaptive capability/viewport tiers, no accidental full source interaction and real UI response metrics. |
| 7 Latest-wins | `latestPreviewQueue`, Native cancellation, paint-generation checks | JS interaction / Native work | Intermediate same-source frames; latest final | Not a distinct export engine | New continuous-input/source/final/JPEG tests | 184 Vitest total at baseline | Synthetic actual-App drag published frames; not GPU speed | REVIEW | Real-photo installed drag under new latency target; ensure no old frame overwrites an already newer/final frame. |
| 8 Tiles / priority / prefetch / LRU | scheduler primitives; Tauri viewport rendering; imageio region decoding | CPU planning / hybrid render | Eligible viewport processing | Full-frame renderer | Existing ROI/full-reference tests | Halo/source coordinates/seams/scale plans | 100% and 200% samples | PARTIAL | Validate actual prefetch/dirty execution, not only plans; eliminate repeated full decode and full-frame export memory. |
| 9 Dependency cache | `StageStateIdentity`, decoded tiers, GPU fingerprints and frame cache | Native CPU/GPU | Partial stage reuse | Fresh full graph | Pixel fingerprints prevent some stale reuse | Cache-key/invalidation tests | Source upload/working hit counters | OPEN | Default graph order/capability metadata differs from actual execution; repeated source transform/full-pixel hashing/copies remain. Establish true stage reuse and revision identity. |
| 10 RAW quality | LibRaw 0.22.2 bridge, resolver, sensor-white/unclip normalization, RAW/imageio graph | Mature decode/demosaic CPU -> f32 restored headroom | Sensor-derived | Sensor-derived | Real RAW shared-graph + controlled sensor normalization | Six legal sensor fixtures, authored full/half DNG, numerical chart | Decoder/RAW timings; full new quality corpus absent | OPEN | WB headroom and declared-white defects repaired; acquire all requested scene-quality coverage and validate effective precision / highlight recovery, not relabel decoder fixtures as photography. |
| 11 Detail / AI denoise | detail crate, portrait, NAFNet/ORT DirectML | Spatial CPU; optional model CPU/DirectML | Real shared stages | Real shared stages | Existing numerical/model regressions | Halo/edge/finite/residual tests | Sharpen ~238ms; NR ~482ms; local detail ~265ms in prior local report | OPEN | Production GPU/tile spatial execution meeting target without weak blur; photograph quality/texture/ringing metrics and actual fallback/offline validation. |
| 12 Masks / AI masks | MaskTree, Native prepared layers, source-semantic sampling | Manual CPU; model CPU/DirectML; not complete GPU mask pipeline | Real | Real | Geometry-aware shared tests | Boolean/feather/source/crop/model guards | Limited per-case timings | PARTIAL | Independent mask raster/tile caches, local-stage breadth, frozen lossless persistence and model-removal restoration; full real UI all mask families. |
| 13 Geometry / Lens | native geometry + Lensfun | CPU | Connected | Connected | Existing cardinal/crop/lens parity | Real Lensfun/coordinate tests | ROI compatibility limits | REVIEW | Full crop/perspective/lens + mask/skin/heal combinations and tile halos; retain mature providers. |
| 14 Preview/export determinism | Shared Native pipeline / export adapter | Hybrid preview / CPU export | JPEG presentation | Integer 8/16-bit codecs | Pre-codec equality covered | Shared raw/photo/cross-crate tests | No full production-codec perceptual report | OPEN | Test actual presented preview vs decoded export in common color space, including vignette combinations, high chroma/skin/dark/highlight/gradient. |
| 15 Golden suite / metrics | Manifest and photographic/ColorChecker tests | Native | Active photos | Active photos | Repeatability/parity; not complete accepted appearance baseline | 11 cases / five photos / six RAW decoder files | Timings recorded | OPEN | Executable 18-category RMSE/DeltaE/luminance/clipping/finite/deterministic gates and reviewed, immutable expected baselines. |
| 16 Responsiveness / UI thread | Native blocking worker, queue/phase, count invalidation ledger | UI interaction; Native work | Connected | Background batch | Unit + synthetic real-App timing/order | Sustained-input regression | Many real control samples exceed 100ms; no end-to-end 100ms acceptance | OPEN | Real input-to-present p50/p95/p99 for every named control; full UI thread, queue, histogram and inference pressure. |
| 17 Library | SQLite, native query/import/thumbnail/remove/selection/collections/counts | Native metadata / CPU thumbnail | Real grid/filmstrip | Full selected ID batch | Restart/scoped/cross-page tests | 100k metadata; 200 thumbnail gate | Actual cache/import timings | PARTIAL | Complete bulk copy/paste/sync and real multi-page UI/delete/restart traces; no ghost items. |
| 18 History / gestures | Native History + React gesture/debounce snapshots | Native persistence / UI intent | Undo/redo restores graph | Durable state export | State/checkpoint/snapshot tests | Scale/retention/corruption | 220ms debounce can commit while a held gesture pauses | OPEN | One semantic command per pointer gesture with loss-free numeric/focus/brush/AI transactions; retain user-approved bounded history semantics. |
| 19 Memory / orphan work | Bounded decoded/frame caches; OS telemetry; surface cancellation | CPU/GPU/ORT | Partial bounded paths | Full-frame float buffers | Identity zero-copy tests | Allocation/scale/cancellation | 100MP process peak about 6.45GB on prior RC runner | OPEN | Correct before-allocation budget, reduce full copies, GPU unused storage/orphans/session churn, real hardware stress. |
| 20 100MP stress | Release real-pixel workflow | Native | Proxy/eligible ROI | Full masked/healed export | Actual dimensions/source immutable | Opt-in 24/45/60/100MP | Passed RC runner, not memory-efficient universal readiness | OPEN | Streaming/tiled output and measured peak limits; heavy state/AI/zoom/cancel/low-memory tests, not only simple workload. |
| 21 Export | native float render, LittleCMS, RGB8/RGB16 codecs, atomic naming/metadata/batch | CPU shared graph | Corresponding view | Real five format/depth combinations | Existing precision/profile/recipe tests | Atomic/cancel/metadata/queue/round-trip | Full allocation before output resize | PARTIAL | Preflight before render, responsive in-render cancellation, source integrity, profile/domain provenance, streaming and full color/alpha policy. |
| 22 UI polish | Bento/glass tokens, three themes, panels/filmstrip, actual controls | React presentation | Real inspector/canvas | Real export panel | Visual tests not color acceptance | Mask/thin outline/layout/focus tests | No full paint/GPU overhead report | REVIEW | Preserve design; all themes/HiDPI/accessible actions, no inert controls or expensive decorative hot-path work. |
| 23 Legacy removal | Native desktop path; `src/imagePipeline.ts` remains | Deprecated JS creative engine still in source | Read-only Browser demo | Native only | Historical migration reference | Browser math tests remain | May tree-shake, but duplicate source still exists | OPEN | Remove production JS image/color math; preserve historical fixtures and regression intent without a second image engine. |
| 24 Typed errors | Native enums/adapters, real GPU failure callbacks, bounded owned readback, explicit model/profile/runtime errors | All | Critical failed GPU retires to labeled CPU reference | Mostly explicit | Recovery/corruption + GPU retirement tests | Actual isolated device destroy, deadline/disconnect/OOM/classification | GPU wait bounded for faults; early allocation guard incomplete | REVIEW | Complete remaining release-path unwrap/expect/panic/fallback and real hang/OOM/disk-full/unsupported/corrupt stress; bounded error wait is not normal latency acceptance. |
| 25 Performance matrix | Profiling / M28/RC measurements | CPU timings + GPU host wall times | Engine timing | Export timing | No complete same-hardware before/after new-target matrix | Scale/per-control gates | GPU "time" includes host work, not timestamp queries | OPEN | Accurate CPU/GPU/queue/readback/presentation timings and all required 24/100MP scenarios, before/after each optimization. |
| 26 Color quality / precision | f32 source/reference; fused GPU f32 storage buffers | Float32 compute; RGBA16Float/R16Float contract helpers | Encoded JPEG final | RGB16 correct at codec boundary | Several numerical GPU tolerances | Gradient/finite/parity tests | Precision/transfer tradeoffs unqualified as full matrix | REVIEW | Real production formats, perceptual metrics, destination gamut, HDR/negative policy, no unnecessary quantization. |
| 27 Automated acceptance | Fast/Targeted/Full/Release scripts | Owned paths | Selected paths | Selected paths | Baseline full green | Unit/native/Vitest/Golden/RAW/parity/scale | Existing targets weaker than new specification | OPEN | Add missing quality/interaction gates and run every required final gate at one immutable HEAD, no deleted tests/tolerance weakening. |
| 28 Windows production | Tauri/NSIS and installed executable self-test | MSVC/Windows/ORT | Real installed launch | Real installed self-test | Clean-directory workflow passed | Notices/model/launch/uninstall/core self-test | Hosted Windows already has WebView runtime | OPEN | Truly offline pristine runtime dependency/upgrade/install matrix; monitor/DPI/hardware fallback and all high-use real UI workflows. |
| 29 Blocker sweep | Regex + manual ownership audit | Production vs tests vs mature vendor | All handlers | All handlers | No automated "no keywords == ready" shortcut | Needs per-finding disposition | Pending complete sweep | REVIEW | Classify all requested markers, unreachable/placeholder code and unsafe waits; legitimate typed fallback is not a stub. |
| 30 Final acceptance / v1.0 | Not started | Same Native graph remains mandatory | Unproven for new targets | Unproven for new targets | RC success is baseline only | No complete new-phase acceptance | No full final performance/quality evidence | OPEN | All above gates, reports, one Final candidate, real installed workflow and correct version/installer evidence before Production release. |

## Confirmed high-priority source findings (not hypothetical causes)

**Additional DNG color boundary:** a real Native resolver on authored D65 ColorMatrix/neutral
returned working [.915141,1.012765,1.330013], because inverse ColorMatrix was treated like an
already-WB-balanced D50 ForwardMatrix. No-ForwardMatrix baked-WB undo/measured-white adaptation
now maps to D65; independent strict chart and real RAW/shared tests pass, with version identities
updated. Complete calibration/AnalogBalance/mixed-profile/dual-white/physical-WB coverage remains
open. Do not infer general camera IQ or whole Phase 1/3/10 acceptance from this targeted repair.

0. **Original-file / replacement safety:** professional export resolves `Overwrite` to an existing
   path without rejecting the current source or other registered Library sources. `atomic_write`
   removes the old destination before rename, so replacement is not atomic/failure-safe, and its
   predictable PID temp is opened with truncating `File::create`. The older single-JPEG command
   has a current-source guard, but the actual professional batch path does not inherit it.
   Reproduce only on an owned temporary image, then close this data-loss gate before other repairs.
   **Targeted repair evidence:** the real probe changed source bytes on the baseline and now
   rejects with SourceOverwriteForbidden, keeping them identical. Core/desktop/Library tests also
   cover all supported formats, another original, lost paths, aliases, cancellation, concurrent
   no-clobber and Windows locked-destination failure. Mature tempfile replaces delete-first/PID
   truncation. This closes the targeted defect, not the remaining whole-phase release gates.

1. **RAW headroom:** bridge enables camera WB then asks LibRaw for RGB16, leaving default
   highlight handling. Upstream `scale_colors` chooses the minimum multiplier at highlight=0,
   and `scale_colors_loop` calls `CLIP(val)`. Rust receives values divided by 65535 afterwards.
   Add a controlled sensor fixture/probe and preserve valid WB highlight headroom through the
   mature provider; do not replace demosaic with a custom weak algorithm.
   **Targeted repair evidence:** a real LibRaw decode of authored 14-bit Bayer DNG confirmed
   blue working output plateau near 1.0 and content-dependent observed-white normalization.
   Mature `highlight=1` plus disabled `adjust_maximum` and f32 scale restoration now retain the
   declared sensor white and a monotonic ramp to 3.23969. Full/half-size normalization oracle,
   six unchanged licensed RAW inputs and native shared graph pass. Scene-specific photographic
   highlight recovery, all precision stress and editable RAW WB remain open.
2. **WB/color control semantics:** `apply_relative_color` adds temperature/tint to OKLab b/a;
   the same family feeds RAW and encoded editing. Its chroma scale leaves 15% at saturation=-1
   and has no skin likelihood term. Numerical CAT helpers existing elsewhere are not evidence
   that this path actually uses CAT or real editable camera WB.
   **Targeted chroma repair:** saturation=-1 now gives zero chroma even with positive Vibrance;
   the CPU/fused GPU operator protects low/moderate-chroma skin-like colors continuously, favors
   low chroma and preserves perceptual hue/lightness before output conversion. Real portrait and
   numerical/extreme/parity tests retain the one-code bound. This is not full WB/gamut acceptance.
   **Measured-neutral repair:** Auto WB and visible Gray Picker now use the prepared mature
   LittleCMS CAT, not a working-RGB diagonal. CMM/geometry/RAW/photographic CPU-GPU tests pass,
   preserve neutral luminance/HDR and reject invalid samples. Temp/Tint offsets, editable RAW
   WB, robust/outlier sampling and full quality/latency gates remain open.
   **Relative-control repair:** Temperature/Tint now use the same prepared LittleCMS CAT
   globally, locally and in the fused GPU, with daylight-relative/perpendicular u/v control
   semantics. Axis/black/identity/HDR/continuity/real-image tests pass. Exact local/global testing
   also repaired neutral Mixer/Tone drift. Physical RAW camera-gain/profile resolution remains
   open; relative working correction is not substituted as proof of that capability.
3. **ICC transfer identity:** `builtin_output_profile_bytes` uses gamma 2.2 for Display P3 and
   Rec.2020. Review exact named TRCs against authoritative definitions and bind output recipe
   versions/hashes to any correction; self round-trip of the same generated profile is insufficient.
   **Targeted repair evidence:** Display P3 now uses mature LCMS analytic sRGB TRCs; stored-curve
   and transform tests against independent formulas pass. The .02 gray probe changes from
   ~.00018293 to ~.00154786 (standard ~.00154799). Output recipes bind actual ICC bytes and engine
   version. Rec.2020 gamma-2.2 photographic SDR is explicitly labeled, not asserted as PQ/HLG or
   a different scene/display video transfer. Monitor ownership and full perceptual gates remain open.
4. **Output gamut/alpha:** compression bounds Rec.2020 channels before destination conversion,
   not the selected destination gamut. `to_working_image` drops decoded alpha and exported buffers
   are RGB-only. Declare/implement correct transparency and SDR/HDR behavior instead of silently
   flattening hidden RGB; measure hue/luminance/clipping in each supported output profile.
5. **GPU order:** fused vignette is applied before CPU layers/Skin/Healing/detail. CPU export
   applies finishing effects after those stages. Existing isolated parity does not prove the
   non-commuting combinations. Fix correctness before optimizing the final stage.
   The Native RTX 3050 / DX12 probe confirms a maximum 21-code RGB8 difference with a local
   contrast/shadow layer, not merely a theoretical noncommutativity claim.
   **Targeted repair evidence:** identity-tail-only fusion now keeps finishing vignette after
   non-commuting work. The identical probe measures zero; six coupled cases meet the unchanged
   one-code bound and the complete pipeline/photographic/RAW suite passes. This is not completion
   of GPU spatial migration, direct presentation or the final performance requirement.
6. **Graph metadata:** default DAG lists optics/geometry after detail and labels nearly all stages
   GPU-capable; actual execution prepares optics/geometry before AI/creative and keeps spatial/local
   stages on CPU. Cache/planning declarations must reflect the real production graph.
   **Targeted repair:** canonical declarations now match actual preparation/creative/finishing
   ownership, source and visible WB have distinct keys, and only real wgpu global kernels are
   GPU-supported. Actual Native settings tests verify exposure/geometry/Auto/Picker/relative/
   finishing invalidation; no processing/math is reordered. Stage-result reuse and tile execution
   remain separate uncompleted work, not inferred from a valid DAG.
7. **Presentation/profiling:** storage-buffer source is rehashed and converted/copied per frame;
   final readback blocks with `wait_indefinitely`, then CPU ICC/JPEG and browser image decode run.
   Recorded GPU duration is host elapsed time. Actual hardware adapter identity and pointer-to-paint
   timings are required; a "Native GPU" label alone does not identify the adapter type.
   **Liveness repair:** both real readback paths now use an exact-submission bounded wgpu poll
   and callback deadline with RAII mapping cleanup. Real device loss/uncaptured OOM/validation
   latches failure/diagnostics and Native preview retires critical failed devices to labeled CPU.
   Actual isolated device destroy and production deadline/error/classification tests pass.
   Readback removal, direct presentation, GPU timestamp profiling and final hardware stress remain open.
8. **Memory/export:** preflight is performed after `renderer.render`; output dimensions drive the
   estimate even if the full source was already allocated. Export cancellation is checked around
   expensive work but needs full in-render/inference validation. Full-frame RGB buffers/encoding
   still dominate the 100MP path; bounded queue does not mean bounded per-item memory.
9. **Durable identity:** camera profile is produced/reported and the Project schema has fields,
   but the inspected desktop path does not demonstrate durable profile/version/hash binding to
   each History/project. AI restoration currently can regenerate from pinned local models; this
   is not lossless frozen-mask persistence independent of model availability.
10. **Old engine/gesture:** deprecated browser tone/WB/detail processing remains in `src`.
    Native interaction helper exists, but the desktop saves on a 220ms debounce during a paused
    pointer gesture. Fix complete transaction semantics without losing numeric or interrupted edits.

## Dependency-first execution order

1. Finish Phase 0 source/read/sweep inventory and reproducible counterexamples; no blind rewrite.
   Close the demonstrated original-file/atomic replacement safety gate first with real source-file,
   catalog-source, alias, collision, cancellation and failure-preservation tests.
2. Color correctness group: RAW headroom, WB/CAT/skin/desaturation, ICC TRCs, output/alpha policy,
   actual graph order and camera/profile/domain provenance. Capture new, reviewed quality oracles.
3. Preserve semantics while completing prepared-stage cache, accurate profiling and a practical
   GPU-resident display/transfer path; only fuse proven commuting stages. Then spatial/local kernels.
4. Full tiles/cache/streaming/memory and cooperative cancellation; real 24/45/60/100MP measurement.
5. Gesture/history, frozen AI artifacts and remaining bulk workflow/UI/legacy/error gaps.
6. Expand Golden/DeltaE/production-codec/interaction/hardware/Windows-offline gates; validate final
   same-SHA candidate and publish v1.0 only when every new requirement is actually proven.

Private model redistribution remains prohibited by the user's earlier explicit decision. Real
local-model acceptance and public bundled-capability acceptance remain separate. Physical monitor,
second-machine and competitor comparisons must be reported honestly, never inferred from CI.
