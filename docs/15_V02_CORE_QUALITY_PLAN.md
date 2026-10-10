# Starroom v0.2 — Core Quality Update

## Goal
Turn the v0.1 browser vertical slice into a trustworthy editing baseline before RAW/GPU/AI expansion. v0.2 prioritizes image-quality correctness, explainable state, reusable layer/mask architecture, and regression protection.

## Open-source strategy
Starroom v0.2 is currently a private/personal-use build.

- darktable (GPL-3.0): direct study, adaptation and porting of useful implementations is allowed for this private-use build. Every directly derived component must be clearly marked `GPL-derived / private-use` with source file, upstream revision and license note so a future distribution decision is explicit rather than accidental.
- Oklab/OKLCh: use published color-science equations and Starroom-native Rust/wgpu implementations for hue-locked selective editing.
- Lensfun: integrate the library/database as the optics profile provider instead of rebuilding the lens database.
- colour-science: use as a development/validation oracle where useful; production advisor logic remains native Rust and offline.
- MediaPipe: integrate behind `FaceLandmarkProvider`; the Starroom core stays provider-neutral so face runtime can be replaced later.

If Starroom is ever distributed outside private use, perform a complete GPL/LGPL/CC dependency and source audit before shipping.

## darktable integration map
| Starroom panel | Primary upstream reference / source |
| --- | --- |
| Basic / Light | `src/iop/exposure.c` plus relevant tone operators |
| Tone Curve | `src/iop/basecurve.c` and tone-curve spline implementation |
| Color Mixer | `src/iop/colorzones.c` selection/overlap ideas; Starroom output model uses OKLab/OKLCh |
| Color Grading | `src/iop/colorbalancergb.c` |
| Detail | `src/iop/sharpen.c` plus denoise modules |
| Lens Correction | Lensfun plus darktable lens integration patterns |
| Geometry | darktable `ashift` perspective module |
| Effects | `src/iop/vignette.c`, `src/iop/bloom.c` |
| Calibration | `channelmixerrgb` / color calibration module |

## v0.2 architecture
```text
React/TypeScript UI
        |
        v
Tauri IPC / commands
        |
        +-- starroom-core               source/state primitives
        +-- starroom-imageio            JPEG/PNG/TIFF decode + metadata, JPEG encode
        +-- starroom-color              tone + OKLab/OKLCh + Color Mixer + curves
        +-- starroom-color-management   ICC-provider contract + Bradford adaptation
        +-- starroom-grading            shadows/midtones/highlights/global grading
        +-- starroom-project            versioned layers + mask trees + sidecars
        +-- starroom-render             graph/cache invalidation + halo tile planner
        +-- starroom-detail             Gaussian/detail/NR CPU references
        +-- starroom-optics             Lensfun-facing correction contract
        +-- starroom-geometry           crop/rotate/keystone matrix core
        +-- starroom-advisor            deterministic local suggestions
        +-- starroom-portrait           face provider + 2D frequency separation
        +-- starroom-heal               deterministic healing reference
        +-- future starroom-gpu / raw
```

## P0 — Image quality correctness
- [x] Replace browser shadow/highlight/white/black RGB-to-white interpolation with luminance remapping and RGB scaling.
- [x] Preserve a black anchor when Shadows are raised.
- [x] Add Rust `starroom-color` CPU reference engine.
- [x] Add Rec.2020/D65 luminance basis.
- [x] Add Oklab/OKLCh conversion and hue rotation reference.
- [x] Add eight-band OKLCh Color Mixer core with smooth overlapping hue regions.
- [x] Add monotone cubic Hermite tone-curve reference.
- [x] Wire the browser master-curve preview/visualization to the same monotone semantics.
- [x] Add bounded-output OKLCh chroma gamut compression reference.
- [x] Add regression tests preventing the known Shadows white-veil failure.
- [x] Replace encoded-image Kelvin UI with relative Temperature/Tint controls; physical Kelvin is reserved for RAW.
- [x] Add ICC transform-provider boundary plus D50/D65 Bradford adaptation reference.
- [ ] Implement the production LittleCMS provider and display/output profile discovery.
- [ ] Add R/G/B curve channels in addition to the master monotone curve.
- [ ] Add Golden Image fixtures for portrait, dark portrait, HDR, neon, ColorChecker, fine texture and high ISO.

## P1 — Core architecture
- [x] Add native JPEG/PNG/TIFF decoder abstraction with embedded ICC/EXIF extraction and JPEG encoder boundary.
- [x] Add versioned `AdjustmentLayer` model with opacity, blend mode, order, mask and parameter map.
- [x] Keep old v0.1 projects readable when `layers` is absent.
- [x] Activate native color/advisor/portrait/detail/render/geometry/optics/grading/color-management/heal crates in the workspace.
- [x] Add render-graph stage dependencies, stable cache keys and downstream invalidation.
- [x] Add halo-aware tile planning for neighborhood filters and large full-resolution images.
- [x] Prevent slider dragging from creating one Undo snapshot for every intermediate value.
- [ ] Finish Begin/Preview/Commit/Cancel transactions for numeric entry, curves, masks, crop and healing.
- [ ] Remove the duplicate legacy frontend history reducer/state implementation.
- [ ] Split oversized `App.tsx` into workspace/library/tools/state/bridge modules.
- [ ] Retire creative math from `src/imagePipeline.ts` after native renderer parity is reached.
- [ ] Add preview pyramid/cache and execute actual tiled full-resolution rendering.

## P2 — Lightroom-style editing modules
- [x] Light CPU reference: exposure, highlights, shadows, whites, blacks and contrast share luminance-domain semantics.
- [x] Curve: master monotone curve engine and browser preview.
- [ ] Curve: independent R/G/B curve UI and native render wiring.
- [x] Color Mixer engine core: eight smooth overlapping OKLCh hue bands, each Hue/Chroma/Lightness.
- [ ] Color Mixer UI and per-band visualization.
- [x] Color Grading CPU core: shadows/midtones/highlights/global wheels with blending/balance/amount.
- [ ] Color Grading UI and render-graph wiring.
- [x] Detail CPU reference: separable Gaussian, thresholded sharpen, luminance/chroma classic NR.
- [ ] Detail production controls, masking and GPU kernels.
- [x] Optics correction contract: distortion, lateral CA, vignette correction and provider interface.
- [ ] Integrate the Lensfun database/provider and GPU sampling path.
- [x] Geometry matrix core: rotate/flip/scale/offset/crop/keystone and inverse transform tests.
- [ ] Geometry crop/straighten/perspective UI and resampling path.
- [ ] Effects: vignette, grain, bloom/glow production modules.
- [ ] Calibration: camera/input calibration separated from creative grading.

## Semantic Advisor V1
- [x] Native Rust deterministic rule engine.
- [x] Suggestions contain control, bounded value, confidence and explanation.
- [x] No API/network dependency.
- [x] Add native linear-RGB analysis for shadow/highlight concentration, clipping, median luminance and broad warmth bias.
- [x] Expose advisor through the Tauri native command boundary.
- [ ] Connect render-analysis samples to an asynchronous UI background job.
- [ ] Add UI enable/disable switch and Apply/Ignore actions.
- [ ] Validate numerical helpers against colour-science reference calculations where applicable.

## Layer / Mask Manager V1
- [x] Layer data model supports independent order, enable, opacity, blend mode, mask and adjustments.
- [x] Mask definition supports none/radial/linear/brush/provider forms.
- [x] Add composable Add/Subtract/Intersect mask tree nodes while preserving legacy leaf-mask JSON.
- [ ] Cache runtime raster masks independently from layer adjustments.
- [ ] Add Layer Manager UI, drag reorder, enable/opacity/blend controls and per-layer invalidation.

## Portrait / Skin Retouch V1
- [x] Introduce provider-neutral face-landmark interface.
- [x] Build normalized/expandable face ROI from landmarks.
- [x] Upgrade frequency-separation reference to 2D RGB images with mask-limited smoothing.
- [x] Healing V1 CPU reference: nearby texture source + low-frequency adaptation + feather blend.
- [ ] Integrate MediaPipe adapter after dependency/runtime review.
- [ ] Build semantic skin mask and exclude eyes/lips/brows/hair regions.
- [ ] Refine skin selection with perceptual color likelihood plus editable brush mask.
- [ ] GPU separable Gaussian/edge-aware low-frequency stage.
- [ ] UI controls: Skin Smooth, Texture Preserve, Tone Evenness, Face Exposure, Skin Hue, Skin Chroma.
- [ ] Healing brush UI, source-point drag and layer/history wiring.

## Tauri integration
- [x] Replace the old `M0_READY` stub with `V0_2_CORE_QUALITY` status.
- [x] Expose native engine-capability reporting.
- [x] Expose local advisor command.
- [ ] Expose native decode/render/export commands and migrate preview off browser pixel math.

## Validation gates
Every completed rendering stage must include:
1. neutral/identity test,
2. finite-value/NaN guard,
3. extreme-control test,
4. CPU reference test,
5. golden-image or perceptual regression when the stage becomes visual,
6. later GPU/CPU parity test before replacing CPU preview.

## v0.2 release boundary
v0.2 is the Core Quality baseline. It is considered releasable only after the current branch passes web build/tests plus workspace Rust fmt/clippy/tests. RAW and wgpu are explicitly subsequent engine milestones and must not be falsely reported as complete in v0.2.

## v0.2 distribution boundary
- Current target: private/personal use.
- GPL-derived darktable source/ports are permitted within this private-use target and must remain traceable.
- No copied Lightroom implementation, assets, profiles or presets.
- No cloud AI requirement.
- No AI inpainting in V1 healing.
- No claim of physical Kelvin for encoded JPEG/PNG/TIFF editing.
- Before any external distribution, complete a license audit and decide whether to comply with GPL distribution obligations or replace the GPL-derived components.
