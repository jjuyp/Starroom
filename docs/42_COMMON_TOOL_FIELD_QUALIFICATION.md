# Existing-tool field qualification — 2026-10-04

This field-fix batch starts at `7d917d22504631cc8b92663bbd57ac9fb164c7de` on
`agent/starroom-v0.2-core-quality`. The historical `v1.0.0-rc.3` tag is unchanged.
Final evidence and the dated installer identify the new commit separately.

## Repairs and functional coverage

| Existing workflow | Qualification and repair |
| --- | --- |
| Exposure, contrast, highlights, shadows, whites, blacks | Real Native pixel-change tests; photographic shadow tone-order test; black/midtone/HDR and CPU/GPU/Preview/Export regressions. |
| WB, gray picker, temperature, tint | RAW camera-neutral and encoded-relative semantics; post-Lensfun/post-geometry sample coordinates; complete WB copy/paste; incompatible source modes disabled. |
| Master/R/G/B curves, mixer and grading | Native curve, selected-color and four-way grading regression; compact UI transport; current post-geometry targeted-color sampler. |
| Sharpen, classical denoise, texture, clarity, dehaze | Shared spatial graph regression and changed-setting interactive profiling; effective native parameters used for measurements. |
| Lens correction, crop, rotation, perspective, flip | Real Lensfun identity deserialization/resolution; historical aliases; geometry dimension/parity tests; exact cardinal-rotation edges. |
| Local radial, linear, brush, luminance and color masks | Native local-tone serialization and pixel-change tests; boolean/opacity composition; thin radial/linear controls with pointer capture and keyboard adjustment. |
| AI Subject/Background/Sky, portrait and Skin | Source/model identity verification, geometry-aware raster mapping, cold-cache same-source restoration and explicit cross-photo rejection; real local-model inference and export parity. |
| AI denoise and Advisor | Residual identity follows precreative source/WB/lens/geometry/region; creative edits reuse valid inference; Advisor attaches actual mask/Skin/denoise state. |
| Clone/Heal, Looks, reference, effects | Existing native spot/edge, Preview/Export, deterministic grain/vignette, weighted Look and reference-category regressions retained. |
| Library, History, Snapshot, session and export | Existing full suite covers persisted state, Undo/Redo, import/relink, restart, ICC/metadata, real 16-bit, atomic/cancel and batch paths. |

## Mask interaction design

Main outline 1.1 CSS px; feather guide 0.75 px; rotation guide 0.8 px. Visible pins are
8 px across with separate invisible 24 px hit targets. No filled color wash obscures the
photograph. Focus indication, arrow-key fine movement and Shift coarse movement are available.
Actual component screenshots and pointer/keyboard checks use the production component/CSS and
a licensed NASA fixture. They are design evidence, not a claim that a Browser fixture executes
the Native pipeline. The user will open the delivered desktop for final visual field inspection.

## Measurement method

Windows x86_64 MSVC 1.97.1; Intel Core i5-11400H; NVIDIA RTX 3050 Laptop / Intel UHD machine.
The release benchmark uses one 6000 × 4000 JPEG and the 1024-edge interactive tier. Each of 27
control families receives five different settings; median and nearest-rank p95 are recorded.
Each changed request must execute the final encoder, preventing repeated cached frames from
being mistaken for slider speed. Full-quality final rendering and true-resolution zoom are
measured separately. This is engine request latency, not end-to-end pointer-to-screen latency.

Local release results (ms; five changed samples per control):

| Control | Median | p95 |
| --- | ---: | ---: |
| Exposure | 102.662 | 108.609 |
| Contrast | 102.284 | 106.723 |
| Highlights | 96.605 | 99.750 |
| Shadows | 99.047 | 100.554 |
| Whites | 95.961 | 99.673 |
| Blacks | 99.824 | 102.701 |
| Temperature | 98.325 | 106.223 |
| Tint | 99.306 | 100.338 |
| Vibrance | 97.933 | 102.182 |
| Saturation | 103.932 | 110.705 |
| Master curve | 95.487 | 104.273 |
| Red curve | 99.062 | 100.570 |
| Green curve | 100.331 | 102.755 |
| Blue curve | 99.695 | 104.378 |
| Mixer hue | 98.652 | 108.062 |
| Mixer chroma | 99.152 | 103.309 |
| Mixer lightness | 101.336 | 105.506 |
| Grading global | 105.831 | 109.030 |
| Grading shadows | 109.957 | 115.814 |
| Grading midtones | 104.083 | 114.574 |
| Grading highlights | 104.039 | 112.642 |
| Sharpen | 238.392 | 248.765 |
| Luminance denoise | 481.602 | 487.789 |
| Chroma denoise | 481.633 | 483.268 |
| Texture | 264.834 | 267.760 |
| Clarity | 264.991 | 269.471 |
| Dehaze | 267.155 | 270.843 |

First 24 MP JPEG Fit 2871.740 ms; cached identical reopen 0.315 ms; final Exposure refine
405.121 ms; true-source 100% tile 234.469 ms; 200% tile 116.492 ms. The latter uses a smaller
source region, not an enlarged low-resolution thumbnail. Recorded process peak 458,047,488 bytes
through the final/zoom measurements; it is not a claim about 100 MP or full AI peak memory.
The small RAW fixture first Fit 135.089 ms / cached reopen 0.384 ms is fixture-specific and must
not be presented as the opening time of a full-resolution camera RAW.

## Distribution and remaining scope

- Core adjustments, manual masks and export are local/offline. No original photograph is modified.
- Only the reviewed MIT BiRefNet Subject/Background weights are in the public installer.
  Face/Skin, Sky and NAFNet can use the user's already-installed verified local weights.
  Those private weights are not placed in the public repository or installer.
- Cross-photo AI mask/face copying requires regeneration on the target photo. A saved raster
  from another photo is rejected with actionable guidance. Same-source restoration verifies
  source/model/crop identity before regenerating an absent cache.
- The existing reserved AI inpaint mode is explicitly unavailable; Clone/Heal remain available.
  This maintenance batch does not add unimplemented Lightroom capabilities or new model families.
- Same-photo/same-hardware Lightroom A/B and a second physical computer have not been tested.
  Passing these regressions does not establish identical proprietary rendering or performance,
  or guarantee that every real-world photo/hardware combination is defect-free.

## Local validation

- Complete workspace: 323 unique ordinary Rust tests passed; format and warning-denied Clippy
  passed; all doc-test runners passed (no executable doc examples). Four explicit opt-in gates
  are counted separately, not reported as ordinary workspace passes.
- Frontend: 153 tests in 23 files passed, including mask interactions, compact Native contracts,
  History, viewport presentation, WB clipboard and actionable AI restoration errors.
- TypeScript, ESLint, Unicode-safe production build, four JSON schemas and packaging checks passed.
- Golden manifest 11/11; five immutable photographic assets; six CC0 RAW sensor fixtures passed.
- Private real-model gate passed: YuNet/BiSeNet detection/parsing, Skin, SegFormer, NAFNet,
  Advisor/Preview/Export parity, cold-cache exact restoration, legacy/custom crop and foreign-source
  guards. Total debug test 61.72 s; release test 13.45 s (detection/parsing 1.851 s, Skin 0.689 s,
  Sky 0.602 s, NAFNet/parity 3.760 s, restart/identity guards 6.461 s). These include many full
  render/inference passes and are not slider
  latency. The initial pre-restart test fixture was corrected to include all ten production Skin
  protection semantics before requiring exact restoration equality.
- License validation: 561 Rust packages, six production npm packages and 269 unique notice texts.
  No dependency lockfile or model distribution policy changed.

Final CI URLs, commit, installer SHA-256 and installed-runtime results belong to the delivery
verification report generated for the immutable candidate. Earlier green runs are baseline only.
