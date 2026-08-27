# Complete Internal Build Plan

## M30 Starroom v1.0 Release Candidate (feature freeze, in progress 2026-08-26)

- [x] Move the engineering task to M30 feature freeze and pin official Tauri CLI 2.11.4.
- [x] Synchronize the immutable RC identity `1.0.0-rc.1` across Cargo workspace/lock, npm
  package/lock and Tauri bundle configuration, with a validator that rejects stale path-package
  versions.
- [x] Add release identity/model-exclusion/offline API validation and a Windows MSVC + NSIS
  build/install/launch/uninstall workflow with executable/installer hashes.
- [x] Add third-party notices and an honest Level-4 matrix that keeps photographic Golden,
  non-Exposure GPU stages, full 100 MP workflow, unbundled AI weights and physical display testing
  visible as release blockers/field-validation gates.
- [ ] Pass the first real Windows installer/runtime workflow and repair all packaging/runtime faults.
- [x] Add a production-API recovery matrix for corrupt Session/History, missing source/thumbnail,
  deterministic History persistence and incomplete export temporary files; retain clean-install and
  full Level-4 execution as acceptance gates.
- [x] Reject corrupt/future Library databases without reinitialization or downgrade, and rebuild a
  damaged thumbnail cache from the immutable source.
- [x] Distribute the complete GPLv3 text, third-party notices and model provenance in the Windows
  bundle and fail release validation when any required legal resource is absent.
- [x] Generate and lock-check the complete 556-crate Rust and six-package production npm license
  closure; fail RC packaging on a missing, unknown or stale dependency license record.
- [x] Bundle 268 content-hash-deduplicated upstream license/notice texts and verify their installed
  hashes during clean-install smoke testing.
- [x] Add a packaged-executable offline self-test for real Library/History/Session/shared Export
  APIs, deterministic second export, source immutability and explicit local-model discovery state.
- [x] Add a versioned Project sidecar load boundary: schema-1 compatibility, schema-2 current,
  typed future/corrupt rejection and fsync-backed atomic persistence.
- [ ] Verify the Geometry/Detail identity zero-copy paths against the former full path, including
  buffer reuse, profiler-stage retention and shared Preview/Export regressions.
- [x] Add five immutable, license-reviewed photographic Golden assets with author/source/license,
  camera/format/hash metadata and an executable shared Native Preview/Export determinism, finite,
  extreme-control and source-immutability regression. ColorChecker remains a numerical oracle.
- [x] Make the expensive RC workflow manual-only, pin Rust 1.97.1 and separate reusable MSVC test
  and release caches by lockfile, target and profile; ordinary pushes use classified Blueprint jobs.
- [ ] Pass the explicit real-pixel 24/45/60/100 MP Windows gate covering bounded preview plus
  full-resolution masked/healed Native export with source immutability and measured peak memory.
- [ ] Resolve every Level-4 release blocker, synchronize version `1.0.0-rc.1`, pass final gates and
  create the RC tag/artifacts. Do not declare Final, merge PR #2 or start M31.

## M29 Production UX / Reliability (accepted 2026-08-26)

- [x] Route the Command Palette and all required keyboard shortcuts through one real command catalog.
- [x] Add pixel-free atomic autosave, explicit Recover/Discard crash recovery and clean session restore.
- [x] Map Native typed failures into user-facing categories without discarding diagnostics.
- [x] Add Tauri Native file drop with explicit unsupported rejection and no silent Browser fallback.
- [x] Add DPI-invariant image-coordinate regressions, focus containment/restoration, ARIA/focus styles,
  reduced motion and responsive desktop behavior.
- [x] Exercise command search/tool invocation in a real browser, audit requests as local-only, and add
  deterministic Native workflow -> session reopen -> second Export integration coverage.
- [x] Record warning-denied Full Acceptance for production code in push run `32942892981`; the
  dedicated M29 acceptance commit below receives its own final run before M30 begins.
- [x] Keep Draft PR #2 open and unmerged; M30 installer/runtime qualification remains separate.

## M28 Performance / Scale / GPU Optimization (accepted 2026-08-26)

- [x] Add per-stage shared-graph timing, process working-set/peak telemetry and cache counters.
- [x] Add interactive preview priority, generation cancellation, exact cache identity, bounded LRU and
  tile/dirty-region planning without changing final render semantics.
- [x] Exercise deterministic 24/45/60/100 MP plans, 100,000 metadata assets, 10,000 history commands
  and 100/500-item bounded batch queues under the Full Acceptance performance gate.
- [x] Retain CPU as the complete reference; GPU Exposure is parity-tested and all unsupported GPU
  stages remain explicit rather than being reported as migrated.
- [x] Publish measured Windows runner numbers and honest workload boundaries in
  `docs/33_M28_PERFORMANCE_REPORT.md`, including a 64 -> 4 dirty-tile comparison.
- [x] Acceptance commit `94a9ccc`; Full Acceptance `32942892981` passed Web and Rust.

## M27 Professional Export Completion (accepted 2026-08-25)

- [x] Preserve the existing Native shared graph as float through the selected LittleCMS output
  transform and quantize once at the codec boundary; no 8-bit intermediate or second color graph.
- [x] Export real RGB16 PNG and TIFF at full resolution with ICC/EXIF/XMP, atomic write, batch,
  cancellation, naming/collision and explicit JPEG-16 rejection without silent downgrade.
- [x] Validate RGB16 decoder round trips, 1,024-step gradient precision, highlights/shadows finite
  range, selected-profile identity, no duplicate transform and Preview/Export graph compatibility.
- [x] Add distinct Screen and dimension/resize-aware Print Low/Standard/High output sharpening.
- [x] Carry copyright/rating/keywords/camera/capture metadata through safe EXIF and XMP/IPTC
  representation; GPS defaults off and explicit preserve requires real source EXIF.
- [x] Retain 24/45/60/100 MP conservative memory preflight and atomic cancellation cleanup.
- [x] Update provenance and durable implementation/architecture documentation. Keep Draft PR #2
  open and unmerged; M28 begins only after the M27 acceptance commit is green.

## M24-M26 local workflow / history / export (accepted 2026-08-24)

- [x] M24: local-first `rusqlite`/SQLite Library with versioned migration, WAL/foreign-key/busy policy, reference imports, fingerprint V1 duplicate/relink rules, existing RAW/image metadata providers and filesystem thumbnail cache.
- [x] M24: typed parameterized search/filter/sort/paging, workflow enums, normalized keywords, normal/smart collections, missing/relink/project relationship and native Library grid/multi-select/metadata UI.
- [x] M25: versioned command/state history, deterministic undo/redo, interaction and brush coalescing, redo-branch truncation, periodic checkpoints, crash-safe persistence and cache/state identity.
- [x] M25: named snapshot lifecycle and undoable restore, full non-destructive state preservation, comparison through the shared Native graph and 5,000-entry replay regression.
- [x] M26: immutable full-resolution shared Native export for JPEG/PNG/TIFF 8-bit, LittleCMS sRGB/P3/Adobe RGB/Rec.2020 profiles, Lanczos sizing, Screen output sharpen, metadata privacy intent, safe templates/collision and recipe identity.
- [x] M26: Library batch export, per-item isolation, cancellation, memory preflight, atomic temp/fsync/rename, versioned presets and typed capability/error surface. 16-bit PNG/TIFF and Print sharpening remain explicitly unsupported.
- [x] M24-M26 cross-integration and final Level 3 acceptance; keep Draft PR #2 unmerged and stop before M27.

## M21-M23 native intelligence / look batch (accepted 2026-08-20)

- [x] M21: pin local-only NAFNet-SIDD width-32, deterministic static 512/opset20 ONNX export and exact hashes; implement Linear Rec.2020 D65 model-domain V1, 512/64 raised-cosine tiling, visible-first priority, cancellation, typed errors and distinct inference/adjustment cache keys.
- [x] M21: place the cached residual stage after optics/geometry and before every tone/creative/detail operation; expose Amount, Detail, Color Noise and Preserve Skin through compact Tauri state; Preview and Export require the same Native residual graph with no silent substitute.
- [x] M21: enforce a 2 GiB conservative working-memory budget, cancel superseded preview inference, reject stale scheduler completion, and expose active DirectML/CPU plus the explicit fallback reason. CPU fallback is attempted only for classified DirectML runtime/inference failures; model/hash/output errors remain visible.
- [x] M22: implement deterministic Native luminance quantiles, OKLab mean/covariance, eight OKLCh hue bands and shadow/midtone/highlight statistics; fit bounded monotonic tone/curve, relative WB, mixer and grading intent with skin protection and confidence.
- [x] M22: add reference-photo selection/apply UI and compact analysis/recipe IPC. React transports parameters only and contains no reference-match color science.
- [x] M22: expose Select/Analyze/Preview/Apply/Reset/Save-as-Look plus independent Amount/Tone/Color/Grading/Protect Skin controls; enforce exact Amount-zero identity and shared semantic curve/hue interpolation.
- [x] M23: replace the placeholder with strict versioned `.srlook` v1 JSON; include only portable tone/curve/mixer/grading/detail/grain/vignette state and explicitly exclude geometry, masks, healing, faces and camera state.
- [x] M23: add parameter-aware Amount, A/B semantic blend primitives, circular hue and sampled curve interpolation, deterministic identity-seeded grain and HDR-safe vignette in the shared Native graph, plus save/load UI.
- [x] M23: complete strict metadata/schema-version validation, corrupted/unknown/future rejection, explicit A/B file selection and normalized weights, Grain Color, Vignette Highlight Protect, and Reference Match to Look round-trip.
- [x] Final Level 3 Full Acceptance on the final `[full-acceptance]` commit; keep Draft PR #2 unmerged and stop before M24.

## M17-M20 Local portrait/editing batch (accepted 2026-08-20)

- [x] M17: run frequency-aware Skin Smooth, Texture Preserve, Tone Evenness, Skin Hue/Chroma and Face Exposure in the Native shared graph, weighted by M16 soft semantics with protected-feature exclusion and finite/identity regressions.
- [x] M18: implement non-destructive Clone/Heal operations with auto/manual source, patch geometry, feather, opacity, rotation/scale, tone/texture adaptation and a shared image-space freehand brush input for masks and healing.
- [x] M19: implement Native image/portrait statistics and a deterministic, bounded, explainable advisor with Preview, Apply, Ignore, Dismiss and safe Apply All; no LLM, cloud or API dependency.
- [x] M20: integrate the fixed local BiRefNet Subject/Background and SegFormer-B0 ADE20K Sky models through the existing Rust `ort` runtime, plus M16 Person/Skin/Hair reuse, typed failure states, cancellation and native-only raster caching.
- [x] Persist compact M17-M20 intent and model/cache identity while keeping raster pixels out of project JSON and React; Preview, Before/After and Export resolve the same Native graph.
- [x] Keep local model weights Git-ignored and unbundled. Exact hashes, upstream revisions and non-commercial restrictions remain recorded in `MODEL_PROVENANCE.md`.
- [x] Final Level 3 Full Acceptance commit and both GitHub Actions check suites green; keep Draft PR #2 unmerged and stop before M21.

## M16 Portrait Detection / Semantic Mask Foundation (accepted 2026-08-14)

- [x] Pin local-only YuNet `face_detection_yunet_2026may.onnx` and BiSeNet **ResNet18** ONNX identities, SHA-256 verification and explicit source/license policy in `MODEL_PROVENANCE.md`.
- [x] Implement Rust `ort` ModelRegistry/ModelSession with CPU and DirectML-requested provider paths, typed unavailable/init/hash/output errors and no cloud/browser fallback.
- [x] Implement source-space YuNet multi-face decode, confidence/NMS, stable per-image geometry IDs, 1.4 square crop and eye-line inverse transform.
- [x] Implement BiSeNet 512 RGB/mean/std/CHW inference contract, soft-logit probabilities, semantic Face/Skin/Eyes/Brows/Lips/Mouth/Hair mapping and skin exclusions.
- [x] Extend M15 `MaskTree` with compact serialized `portraitSemantic` leaves and resolve cached source R16Float-compatible rasters only in the Native shared Preview/Export graph.
- [x] Add local Native portrait UI state, All Faces/individual face selection, detection status/error state and MaskTree leaf creation; do not add M17 retouch controls.
- [x] Complete mock/semantic/serialization/cache regressions, run the local frontend gates and pass focused GitHub Windows compile feedback (run `31724891922`).
- [x] M16 implementation acceptance: local frontend gates, focused Windows compile (`31724891922`) and Level 3 Full Acceptance (`31727256150`) are green. Do not start M17 in this milestone.

## Continuous M7-M11 acceptance (2026-08-12 specification)

- [x] M7: Native eight-band Red/Orange/Yellow/Green/Cyan/Blue/Purple/Magenta OKLCh Color Mixer; H/C/L controls, circular overlap, hue-lock state, Native targeted sample, undo/serialization, Preview/Export shared stage and finite/HDR regressions.
- [x] M8: Four-way Color Grading with Global/Shadows/Midtones/Highlights H/C/L, balance, blending and amount; shared Native graph, undo/serialization, finite/HDR and Preview/Export parity regressions.
- [x] M9: Native Detail Engine with edge-masked/halo-protected multi-scale sharpen, edge-aware luma/chroma/high-ISO denoise, distinct Texture/Clarity/Dehaze frequency bands, serialization/undo and Preview/Export parity regressions.
- [x] M10: pinned Lensfun v0.3.4 database/provider, LibRaw/EXIF metadata, auto/manual camera-mount-aware matching, explicit missing/unknown/mismatch/ambiguous states, distortion/TCA/vignette/auto-scale switches, persisted UI and shared Native Preview/Export optics stage.
- [x] M11: Native crop (free/original/common/custom ratio), rotate/fine-rotate/flip/scale/offset, horizontal/vertical keystone, draggable four-point correction, image-derived Upright modes, overlays, project/undo state and explicit source/post-lens/post-geometry/viewport coordinate contract.
- [x] Level 3 Full Acceptance: 123 Rust tests and 26 Vitest tests passed with warning-denied Clippy, rustfmt, JSON/Golden/RAW validation, lint and production build in push run `31613588937`; acceptance record remains on Draft PR #2 and `main` is unmerged.

## M12 GPU Render Engine acceptance (2026-08-12)

- [x] Integrated pinned wgpu 30.0.0 as a DX12-first, explicit Native Preview acceleration backend; CPU remains the reference oracle and Export remains the same CPU graph for deterministic output.
- [x] Added linear Rec.2020 D65 RGBA16Float / R16Float resource contracts, WGSL exposure compute dispatch, buffer lifecycle, shader validation, device/unsupported/OOM/loss typed fallback and UI-visible backend/fallback status.
- [x] Added strict CPU/GPU HDR/skin/neon/shadow parity corpus, shared Native Preview graph integration, GPU CI gate and relevant Golden selection. Push run `31620924649` passed all Web, GPU, RAW and dependent gates.

## M13 Tile / Preview Pyramid / Render Scheduler (2026-08-13 candidate)

- [x] Added a Native `starroom-render::scheduler` with fixed 512/1024/2048/4096 pyramid levels, deterministic 24/45/60/100 MP tile plans, graph-declared halos and visible/nearby/remaining viewport priority.
- [x] Added generation-based supersession, stale-output rejection, source/version + graph + level + region cache identities, and bounded RAM/VRAM LRU accounting. The Tauri preview request now selects its native pyramid level, schedules it and reuses only an exact cached derived frame; Export still reopens the full immutable source.
- [x] Added `test:tiles`, scheduler regressions and a plan-only benchmark report. No image/color foundation was replaced and GPU unavailability remains the existing explicit CPU-reference status.
- [x] Windows Level 3 Full Acceptance passed on GitHub Actions run `31707532900` after the M12 Clippy and desktop lockfile corrections; M14 may begin.

## M14 Non-destructive Adjustment Layers (2026-08-13 candidate)

- [x] Added typed Native layer intent with ordered Normal blend, enabled state, finite opacity validation and explicit rejection of malformed IDs before the shared graph runs.
- [x] Native Preview, Before/After and Export now receive the same compact layer stack; stack evaluation occurs in linear working RGB after global creative controls and before Detail/output.
- [x] Added UI add/delete/rename/duplicate/enable/reorder/opacity and per-layer Exposure operations with the existing reversible snapshot history; persisted project layers validate IDs, order, opacity and finite values before a sidecar write.
- [x] Added native contract, pipeline order/finite and project persistence regressions plus `test:layers`. Windows Level 3 Full Acceptance run `31710577180` passed Web and Rust; M15 may begin.

## M15 Native Mask Tree / Layer Compositing (2026-08-13 candidate)

- [x] Expanded persisted MaskTree leaves to Radial, Linear, Brush/Eraser, Luminance and Color Range, with Add/Subtract/Intersect/Invert composites and legacy JSON compatibility.
- [x] Added a native normalized-coordinate CPU reference evaluator and connected mask weight × layer opacity to M14 linear working-space compositing. Provider-only masks return a typed error; no Browser Canvas fallback is used.
- [x] Tauri Preview/Before-After/Export share the same compact mask-layer contract. The existing interactive radial overlay now becomes a Native M15 layer; layer UI can select all native mask families.
- [x] Added mask algebra, invalid/provider, persistence and compositing regressions, plus portrait/dark/backlight/HDR Golden mask selections. Windows Level 3 Full Acceptance run `31713812877` passed Web and Rust after the clippy-only correction; stop after M15.

## Product principle — Open-source Foundation First

Before expanding advanced Starroom-specific features, baseline image quality must stand on mature open-source foundations wherever a proven implementation already exists.

Read `docs/16_OPEN_SOURCE_FOUNDATION.md` before foundation work.

Rules:
- do not replace a mature RAW/tone/color/denoise/optics/perspective implementation with a weaker custom prototype;
- prefer adapters/providers so third-party APIs do not leak into React or unrelated crates;
- CPU reference implementations are validation tools unless they meet the production replacement gate;
- Starroom-native replacements must be equal/better on relevant regression fixtures or be justified by platform, architecture, performance, maintenance, or licensing constraints;
- directly derived darktable code in the current private-use build must record provenance and be marked `GPL-derived / private-use`;
- a trait/struct/UI/placeholder is not a completed production feature.

## F0 Foundation Quality Gate — highest priority

- [x] Document Open-source First strategy and Codex decision rules.
- [x] CI green on current v0.2 branch after all native crates are enabled.
- [x] Create third-party provenance inventory: project, source file, upstream revision, license, integration mode, Starroom files. See `docs/17_THIRD_PARTY_PROVENANCE.md`.
- [x] Establish the executable Golden Image/regression specification and required-case manifest: portrait, dark portrait, white/black clothing, HDR, backlight, night, neon, ColorChecker, fine texture, high ISO and mixed color temperature. See `docs/18_GOLDEN_IMAGE_SPEC.md` and `fixtures/golden/manifest.json`. (Acquiring license-cleared active fixture files remains a later quality-report task.)
- [ ] Define per-stage quality comparison reports against the selected mature foundation.
- [ ] Do not advance a custom foundation replacement unless the replacement gate passes.

## Development Acceleration Pass — accepted before M7

- [x] Add canonical `test:color/tone/curve/raw/detail/optics/geometry/masks/portrait/ai/web/full` commands and machine-readable duration reports.
- [x] Define Level 1 targeted, Level 2 milestone and Level 3 Full Acceptance without weakening the final gate.
- [x] Upgrade Golden manifest to validated multi-tag schema with deterministic tag subset selection.
- [x] Add dependency-aware path classification and targeted web/color/RAW/detail/optics/geometry/AI CI jobs.
- [x] Add compiler/lockfile/platform-keyed Cargo/native caches, npm download cache, hit/miss output and timing artifacts.
- [x] Add `codex/CURRENT_TASK.md`, module dependency/test-invalidation map and pinned M7–M12 implementation map.
- [x] Preserve Draft PR #2, do not merge `main`, and stop before M7.

## Browser vertical slice — completed 2026-08-09
- [x] Direct numeric entry for every slider control.
- [x] Initial encoded-image white-balance control.
- [x] Direct curve editor: add, drag, numeric edit and right-click delete points.
- [x] Detail controls and browser pixel regression tests.
- [x] On-image radial mask placement, move, width/height resize and rotation handles.
- [x] Arbitrary -180…180° rotation plus 90° shortcuts and flips.
- [x] Wheel zoom (25–600%), drag-to-pan and Fit reset.
- [x] Removed duplicate Simple/Pro switch; one complete inspector remains.
- [x] Playwright interaction pass and zero browser console errors/warnings at the original slice milestone.

> This is a rendered-file CPU browser slice. It is not the production image engine and must not be used as the quality reference for RAW/ICC/tone/detail/optics work.

## M0 Workspace / CI
- [x] Rust workspace + Tauri 2 + React/TS.
- [x] crate boundaries, lint/format/test, Windows CI scaffold.
- [x] fixture manifest and source immutability hash test.
- [x] Keep all newly activated native crates green under fmt/clippy/test.

## M1 Native Render Foundation

### M1A Rendered-file I/O
- [x] Production JPEG/PNG/TIFF decoder connected to native render path.
- [x] Preserve embedded ICC/EXIF metadata through decode boundary.
- [x] Production JPEG/PNG/TIFF export path does not use preview pixels as source.

### M1B Color management — mature foundation required
- [x] Integrate real LittleCMS provider (`lcms2 6.1.1` + statically bundled LittleCMS 2.19).
- [x] Explicit ICC input -> linear Rec.2020 D65 working -> display/output transforms in the native shared graph.
- [x] Validate D50/D65 adaptation and all four ICC rendering intents.
- [x] Missing input profile is explicitly reported as assumed sRGB; invalid embedded/display/output profiles are typed errors and never silently substituted.
- [ ] Multi-monitor/display-profile test plan.

### M1C Native rendered-file pipeline
- [x] Native vertical slice: file-path decode -> input transform -> linear Rec.2020 D65 -> relative WB -> exposure/tone -> curve -> color -> preview/export output in one Rust graph.
- [x] Native preview and export entry points use the same logical processing graph and differ only by requested display/output profile.
- [x] Tauri 2 request/result contract sends source path + edit state as small JSON and returns preview JPEG through versioned binary `ipc::Response`; full-resolution export writes directly to the selected path.
- [x] Real desktop photo preview, Before/After and JPEG export use the Native CPU graph; Browser Canvas remains an explicitly labelled browser/demo fallback and is never selected after a native error.
- [x] Browser reference vs Native CPU regression fixtures cover identity, Exposure, relative WB, Tone and Curve with explicit migration tolerances.
- [x] Move masks, optics and geometry into the native graph; Native M1C reports unsupported edits instead of silently ignoring or falling back.
- [ ] Replace the temporary JPEG preview payload with a measured cache/shared-buffer strategy only if profiling proves decode/encode overhead is material; do not introduce shared memory without a cross-platform lifecycle design.
- [ ] Remove temporary Browser Canvas creative math after all remaining rendered-file tools have native coverage and regression acceptance.

## Active sequential foundation milestones (PR #2)

These milestone numbers supersede the older capability-group numbering below for the current acceptance sequence. A later milestone stays blocked until the preceding milestone's local checks and GitHub Actions are green.

### M2 RAW original-file processing

- [x] Vendor and compile real LibRaw 0.22.2 through a narrow Rust provider; select and record the CDDL-1.0 license path.
- [x] Decode NEF, ARW, CR2, CR3, native DNG and RAF from sensor data; embedded JPEG remains Library-thumbnail-only and is never called by Develop.
- [x] Preserve typed metadata for RAW/active dimensions, margins, orientation, black/white levels, Bayer CFA or X-Trans layout, As-Shot multipliers and Camera Neutral.
- [x] Keep the authoritative decode output at 16-bit-to-f32 precision and enter the Native graph as linear Rec.2020/D65 without an 8-bit intermediate.
- [x] Use LibRaw AHD/X-Trans demosaic behind `starroom-raw`; no custom weak demosaic was introduced.
- [x] Route Native Preview, Before/After and Export through the same RAW-aware Rust shared render graph with no browser or embedded-thumbnail fallback.
- [x] Add typed RAW errors, six immutable CC0 camera fixtures, byte/hash/license validation, real decode regressions and separate decode/first-preview/slider-rerender measurements.
- [x] GitHub Actions green for M2 on both push run `31513488590` and Draft PR run `31513493542`; M3 may now begin.

### M3 Camera profiles / RAW color

- [x] Production Camera Profile Resolver with DNG ColorMatrix1/2, ForwardMatrix1/2, CameraCalibration, CalibrationIlluminant and Camera Neutral metadata.
- [x] Explicit camera RGB -> XYZ -> linear Rec.2020/D65 stage with DNG D50 -> working D65 Bradford adaptation in the shared RAW path.
- [x] Extensible Nikon/Canon/Sony/Fujifilm families and explicit Generic Profile state; profile ID/version/SHA-256 is serializable in project state and the active ID is visible in Native UI/export results.
- [x] Add a retained-license colour-science v0.4.7 ColorChecker oracle plus identity/dual-illuminant/finite/round-trip and real RAW profile regressions; the CC0 Apple DNG fixture has no valid embedded profile and is asserted as visibly `Generic`, never silently substituted.
- [x] GitHub Actions and M3 color regressions green: push run `31561022046` and Draft PR run `31561024876` both passed; M4 may now begin when explicitly scheduled.

### M4 Professional Tone / Light

- [x] Directly adapt the stable darktable `sigmoid.c` generalized-loglogistic scene-linear shoulder under GPL-3.0-or-later, with source/revision/NOTICE and external-distribution obligations recorded.
- [x] Native luminance-ratio Tone Engine covers Exposure, Contrast, Highlights, Shadows, Whites and Blacks while preserving a true-black anchor, midtone pivot and unbounded working range.
- [x] Add native numerical portrait-low-key/backlight/HDR/night-style regression vectors: identity, targeted shadow lift, highlight roll-off, narrow white/black zones, hue/chroma ratio preservation and finite extremes.
- [x] GitHub Actions green and M4 acceptance complete: push run `31589000442` and Draft PR run `31589004929` passed; M5 may now begin.

### M5 Professional white balance / calibration

- [x] Separate typed Camera/As-Shot RAW WB from rendered-file Relative Temperature/Tint in the Native shared graph; invalid source/mode combinations are typed errors, never silent fallbacks.
- [x] Add an active deterministic gray-world Auto WB provider plus normalized Native Neutral Picker sampling; both operate before the shared creative graph.
- [x] Persist WB mode, relative Temperature/Tint and optional picker ROI in project state with backward-compatible defaults; UI transport keeps the state small and pixel-free.
- [x] Add neutral-gray, auto/extreme and semantic-error numerical regressions. M5 CI acceptance passed: push run `31591507530` and Draft PR run `31591511674` are green; M6 may now begin.

### M6 Professional tone curves

- [x] Native Master and R/G/B monotone cubic-Hermite curves with endpoint/HDR-safe extrapolation and backward-compatible legacy Master curve migration.
- [x] Channel-aware UI editing (tabs, add/drag/delete, numeric endpoint editing) and undo snapshots now serialize and send the full curve set to Native Preview/Export.
- [x] Add named Identity/S-curve/Black-fade presets and live histogram curve background; preset application is undoable and uses the same Native curve contract.
- [x] Final M4-M6 acceptance includes provider-backed M5 skin/mixed-light coverage plus M6 identity, S-curve, HDR extreme, RGB channel, real RAW shared-graph and portrait/gradient Golden vectors. Coverage push run `31597151940` and Draft PR run `31597154881` are green.

## M2 Tone / Color Foundation — use mature open-source behavior

### M2A Exposure / Tone
Preferred foundation: darktable exposure/tone implementations and validated scene-referred math.

- [ ] Record selected upstream modules/revisions and integration approach.
- [ ] Exposure ±5 EV.
- [ ] Highlights / Shadows / Whites / Blacks share one coherent tone model.
- [ ] Preserve black anchor and highlight detail where mathematically possible.
- [ ] Contrast uses documented pivot/curve semantics.
- [ ] Compare against mature foundation on Golden Image fixtures.
- [ ] Remove temporary browser tone math after native acceptance.

### M2B White balance / calibration
Preferred foundation: mature darktable color-calibration/channel-mixer concepts plus standard color science.

- [ ] Encoded JPEG/PNG/TIFF uses relative Temperature/Tint semantics, not fake physical Kelvin.
- [ ] RAW uses camera/metadata/profile-aware WB path.
- [ ] Calibration remains separate from creative grading.

### M2C Curves
Preferred foundation: mature darktable curve behavior where useful plus Starroom monotone requirements.

- [ ] Master curve.
- [ ] R/G/B curves.
- [ ] Monotone interpolation / no unintended overshoot.
- [ ] UI curve matches actual render curve.

### M2D Selective color — Starroom differentiator
- [ ] Eight-band OKLCh Color Mixer UI: Red/Orange/Yellow/Green/Aqua/Blue/Purple/Magenta.
- [ ] Hue / Chroma / Lightness per band.
- [ ] Smooth circular overlap.
- [ ] Hue-lock behavior validated with gamut compression.

### M2E Color grading
Preferred foundation: darktable `colorbalancergb` as mature reference/port source.

- [ ] Shadows/Midtones/Highlights/Global grading.
- [ ] Balance/Blending.
- [ ] Record GPL-derived provenance if code is directly adapted.

## M3 RAW Foundation — mature decoder first
Preferred foundation: LibRaw or another proven RAW decoder abstraction.

- [ ] LibRaw bridge and RAW metadata model.
- [ ] CFA, active area, black/white levels, normalized mosaic.
- [ ] Bad-pixel stage.
- [ ] Bayer demosaic provider.
- [ ] Generic CFA/X-Trans interface.
- [ ] Camera-profile resolver, RAW WB, DNG matrices.
- [ ] NEF/RAF/CR3/ARW/DNG fixtures.
- [ ] Nikon/Fujifilm/Sony/Canon regression samples where legally usable.
- [ ] Do not claim RAW support until real files render through the shared graph.

## M4 GPU Graph
- [x] Render graph stage/dependency/cache-key/invalidation foundation exists.
- [x] Halo-aware tile-planning foundation exists.
- [ ] wgpu device/surface and Windows DX12 preferred path.
- [ ] RGBA16Float working preview and R16Float masks.
- [ ] Preview pyramid.
- [ ] Tiled full-resolution rendering/export.
- [ ] Device-loss recovery.
- [ ] CPU fallback.
- [ ] CPU/GPU parity tests for every migrated image stage.

## M5 Layers / Masks — Starroom-owned architecture
- [x] Versioned AdjustmentLayer data model: mask, adjustments, blend mode, enabled, opacity, order.
- [x] Mask tree data model with Add/Subtract/Intersect.
- [x] Brush/Eraser/Linear/Radial/Luminance/Color Range production masks in the persisted Native shared graph.
- [ ] Independent raster-mask cache.
- [ ] Layer drag reorder and per-layer invalidation.
- [ ] Local tone/color/detail semantics match global controls.
- [ ] Frozen AI-mask persistence.
- [ ] Full Layer Manager UI.

## M6 Detail Foundation — mature open-source first

### M6A Sharpen
Preferred foundation: darktable `sharpen.c` and other proven sharpening references as appropriate.

- [ ] Record upstream module/revision and integration approach.
- [ ] Amount / Radius / Detail / Masking production model.
- [ ] Avoid halos and color shifts on regression fixtures.

### M6B Denoise
Preferred foundation: mature darktable classic/profiled denoise implementations where suitable.

- [ ] Luminance/color noise controls.
- [ ] Preserve edges/fine texture.
- [ ] High ISO fixture suite.
- [ ] Do not use a generic blur as production denoise.
- [ ] Keep AI denoise as a later local provider, not a substitute for a good classic baseline.

### M6C Texture / Clarity / Dehaze
- [ ] Multi-scale local-contrast foundation.
- [ ] Verify no severe halos at edges/high-contrast boundaries.

## M7 Optics / Geometry Foundation

### M7A Lens correction
Preferred foundation: Lensfun.

- [ ] Integrate real Lensfun library/database provider.
- [ ] Lens identification.
- [ ] Distortion.
- [ ] Lateral chromatic aberration.
- [ ] Vignetting profile correction.
- [ ] Lensfun result feeds Starroom CPU/GPU renderer through adapter boundary.

### M7B Geometry / Perspective
Preferred foundation: darktable `ashift` and proven projective math.

- [ ] Crop.
- [ ] Rotate / straighten.
- [ ] Perspective / keystone.
- [ ] Architecture fixture regression set.
- [ ] Record GPL-derived provenance if directly adapted.

## M8 Effects / Calibration
- [ ] Vignette using mature reference where appropriate.
- [ ] Grain.
- [ ] Bloom/glow using mature reference where appropriate.
- [ ] Color calibration implementation separated from creative color grading.

## M9 Semantic Advisor — Starroom differentiator
- [x] Deterministic local rule-engine foundation.
- [x] Basic image statistics foundation.
- [ ] Run analysis as cancellable/background native job.
- [ ] UI switch.
- [ ] Explainable suggestions with Apply/Ignore.
- [ ] Validate numerical helpers against colour-science where useful.
- [ ] Advisor never silently mutates edits.

## M10 Portrait / Skin / Healing — Starroom workflow
- [x] Provider-neutral FaceLandmarkProvider contract.
- [x] Frequency-separation CPU reference foundation.
- [x] Healing CPU reference foundation.
- [ ] Integrate MediaPipe adapter after runtime/license/privacy review.
- [ ] Face ROI and facial-feature exclusions.
- [ ] Continuous skin likelihood/refinement without a single hard skin-color rule.
- [ ] Editable skin mask.
- [ ] Skin Smooth / Texture Preserve / Tone Evenness / Face Exposure / Skin Hue / Skin Chroma.
- [ ] Production healing brush UI and history transactions.
- [ ] AI inpainting remains post-V1.

## M11 AI Runtime
- [ ] Windows ML/ONNX bridge, model manifest, device enumeration.
- [ ] Offline CPU fallback, async/cancellable jobs, model cache/fingerprints.
- [ ] Local-first only for core AI.

## M12 AI Mask
- [ ] MaskProvider, interactive point/box provider.
- [ ] Subject / Background / Sky / Person.
- [ ] Face / Skin / Hair where supported.
- [ ] Editable manual refinement and frozen persistence.

## M13 AI Denoise
- [ ] DenoiseProvider, LinearRGB path, tiled overlap/blending.
- [ ] Strength/detail protection, benchmark suite.
- [ ] Optional RawMosaic hook, model-version persistence.

## M14 Look Engine
- [ ] `.srlook`, LookDescriptor, mood axes, basis mapping.
- [ ] Parameter-aware Amount and Style Mixer.
- [ ] Protected categories and regression gallery.

## M15 Reference Match
- [ ] Analyzer, exposure/WB estimation, quantile tone mapping.
- [ ] Monotonic curve, hue-band/grading estimates.
- [ ] Optional semantic matching, bounded refinement, confidence/explain report.

## M16 Workflow
- [ ] Strip, ratings/flags, copy/paste/sync, batch export.
- [ ] Unified Begin/Preview/Commit/Cancel transaction system.
- [ ] Remove duplicate frontend history logic.
- [ ] History/snapshot/compare/survey/metadata/presets.

## M17 UI Refactor / Design System
- [ ] Split oversized `App.tsx` into workspace/library/tools/state/bridge modules.
- [ ] Load `design/STARROOM_DESIGN_DNA.json`.
- [ ] Semantic theme tokens: Dark / Gray / Light.
- [ ] Brand gradient tokens and functional accent subset.
- [ ] Collapsible Library, Filmstrip, hybrid Inspector.
- [ ] Resizable left/right panels.
- [ ] Category icon rail + accordion inspector.
- [ ] Slider numeric bubble and direct numeric entry.
- [ ] Proof background switch independent from theme.
- [ ] Mask floating toolbar + right mask tree.
- [ ] UI quality modes Auto / High / Balanced / Performance.
- [ ] Reduced-motion support.
- [ ] Screenshot regression tests for all three themes.
- [ ] UI quality mode cannot change image/export output.

## M18 Release Candidate
- [ ] Camera/GPU/AI matrices.
- [ ] Multi-monitor color tests.
- [ ] Corrupt input/fuzz.
- [ ] Complete dependency/model/GPL/LGPL/CC provenance audit.
- [ ] 24/45/60/100MP performance and memory suite.
- [ ] Golden-image regressions.
- [ ] Installer/update/uninstall validation.

## Product acceptance rule
The foundation wins before novelty.

Do not call Starroom production-ready if its RAW, tone, color, denoise, sharpening, optics, or perspective baseline is materially worse than the mature open-source foundation selected for that stage, even if advanced Starroom features are already impressive.
