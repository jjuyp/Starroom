# Golden Image regression specification

Status: M30 executable photographic corpus. The fixture manifest is `fixtures/golden/manifest.json`; CI validates its required cases and contracts with `scripts/validate-golden-manifest.mjs`. Schema v3 retains canonical multi-value tags and adds immutable assets with source page/file, author, upstream revision, license, camera, format, dimensions, bit depth, ICC/EXIF state, byte length and SHA-256.

The supported tag registry is `raw`, `camera-color`, `tone`, `wb`, `curve`, `color`, `grading`, `detail`, `optics`, `geometry`, `mask`, `portrait`, `skin`, `ai`, `night`, `high-iso`, `neon`, `landscape`, and `hdr`. Select a union with `npm run golden:select -- --tags=color,portrait,skin`; add `--all-tags` to require the intersection. Omitting `--tags` selects the complete manifest. A selected `planned` case remains visible rather than being silently treated as an active photographic regression.

The license-cleared RAW decoder matrix is active in `fixtures/raw/manifest.json`. It is deliberately separate from the scene-quality Golden set: the six CC0 files prove real sensor decode/metadata/CFA/WB/demosaic coverage, but they are not relabeled as portrait, HDR, night or ColorChecker scenes without matching visual evidence. M30 separately activates five photographic assets under public-domain, CC0-1.0 or CC-BY-SA-3.0 terms and maps each required photographic case only to a visually applicable source.

The ColorChecker case has an active numerical oracle at `fixtures/colorchecker/babelcolor-average-v0.4.7.json`: 24 BabelColor Average xyY patches under ICC D50 from the pinned BSD-3-Clause `colour-science/colour` v0.4.7 dataset. CI retains and validates the upstream license and M3 tests exercise D50 -> D65 adaptation and finite profile transforms for every patch. It remains explicitly `fixtureKind: numerical`; Starroom does not pretend this is a photographic chart capture.

M5 adds executable Native white-balance vectors while photographic Golden assets remain planned: neutral-gray picker must equalize its sampled cast, the active gray-world provider must remain finite with mixed/HDR samples, invalid picker ROI must fail explicitly, and encoded Camera/As-Shot requests must fail rather than substitute a Relative correction. The retained RAW fixture matrix continues to validate LibRaw Camera/As-Shot metadata and the shared RAW graph.

## Common capture and storage contract

- Sources are immutable, redistributable files with creator/license/provenance, SHA-256, dimensions, bit depth, format, embedded ICC status, EXIF status and reference ROIs recorded in the manifest.
- Prefer a RAW plus a rendered reference when legally available. Rendered JPEG/PNG/TIFF fixtures must include both embedded-profile and missing-profile coverage across the set.
- Accepted baselines store stage fingerprints and metrics, not only a final screenshot: decode/input transform, linear Rec.2020 D65 working image, tone/color result, detail result and encoded output.
- Preview and export invoke the same logical graph. Output scaling/encoding may differ only where explicitly recorded.
- Baseline replacement requires a reviewed reason, before/after metrics and visual approval; CI must never auto-accept a new baseline.

## Assertions required for every image

1. **Identity:** neutral settings preserve the intended input appearance. For rendered sRGB, output error is at most 1 code value per 8-bit channel after one input/working/output pass; profiled images compare in CIE Lab after the declared display/output transform.
2. **Extreme control:** every control named by the case is tested at its documented minimum and maximum. Output must remain bounded, deterministic and visually directional; a nonzero expected ROI must change by at least the case threshold.
3. **NaN/Inf:** every floating stage has zero non-finite samples. Invalid source/profile data returns a typed error rather than sanitized pixels.
4. **Tone regression:** against the accepted CPU baseline, linear-light per-channel RMSE <= 0.002 and maximum absolute error <= 0.01 outside deliberately clipped output pixels.
5. **Color regression:** ColorChecker/declared color ROIs use CIEDE2000; default tolerance is median ΔE00 <= 0.25, P95 <= 0.75 and max <= 2.0. Neutral ROIs additionally require |Δa*| and |Δb*| <= 0.5.
6. **Determinism:** same build/input/settings produces identical RGB8 output bytes and stage-report schema on repeated CPU runs.
7. **Future CPU/GPU parity:** manifest reserves `cpuGpuParity`. When a stage migrates to GPU, it becomes mandatory with linear RMSE <= 0.002, max absolute error <= 0.01, P95 ΔE00 <= 1.0 and no additional clipped pixels.

## Required image cases

| ID | Scene / required evidence | Identity focus | Extreme controls and directional expectation | Tone/color regression ROIs |
|---|---|---|---|---|
| `portrait-daylight` | General portrait, daylight skin, neutral background | Skin hue and neutral background remain stable | exposure ±5 EV, temperature/tint bounds, saturation ±100; skin ROI moves in the requested direction without hue discontinuity | cheek/forehead skin, sclera, neutral background |
| `portrait-low-key` | Dark portrait with retained shadow facial detail | Black anchor and low-light skin hue | shadows ±100, blacks ±100, denoise/sharpen bounds; positive shadows reveals face more than background black | face shadow, hair, black background |
| `backlit-portrait` | Strong backlight and face in shade | Highlight hue and face neutrality | highlights -100 must recover bright ROI; shadows +100 must lift face more than sky; extreme combination remains finite | face, sky, rim light |
| `high-dynamic-range` | Bright speculars plus deep textured shadows | No unrequested global clipping | exposure, whites, blacks, highlights and shadows at both bounds; monotonic luminance ordering is preserved | specular, midtone gray, deep texture |
| `night-city` | Low-light city scene with point lights | Near-black neutrality and point-light color | exposure/shadows/black/denoise extremes; point lights do not produce NaN rings or hue inversion | sky/black, street neutral, lamps |
| `neon` | Saturated cyan/magenta/red signage | Hue continuity near gamut boundary | saturation/vibrance/color-band extremes; gamut compression remains finite and continuous | cyan, magenta, red, neutral sign border |
| `high-iso` | Visible luminance and chroma noise with a sharp edge | Neutral noise has no chroma bias | denoise and sharpen -100/100; denoise lowers flat-field variance while retaining declared edge contrast | flat patch, edge MTF ROI, chroma noise patch |
| `white-black-clothing` | White and black fabric in the same frame | Fabric neutrals and texture survive | whites/blacks/highlights/shadows extremes; white texture and black texture respond independently | white weave, black weave, skin/neutral reference |
| `colorchecker` | ColorChecker under controlled D50 or D65 illumination | ICC/adaptation correctness and patch neutrality | temperature/tint/saturation extremes plus all rendering intents; invalid/missing/embedded profile variants | all chart patches; neutrals receive stricter limits |
| `fine-texture` | Hair, fabric, foliage or printed microtexture | No default halo or blur | clarity/sharpen/denoise extremes; positive detail increases edge energy within halo limit | flat field, fine texture, high-contrast edge |
| `mixed-temperature` | Warm practical light plus cool daylight | Local hue relationships and neutral transition | temperature/tint extremes; no abrupt hue seam or channel NaN at the mixed boundary | warm neutral, cool neutral, skin/object reference |

## ICC matrix within the set

- Embedded sRGB: must be honored and reported as `embeddedIcc`.
- Missing profile: deterministic fallback is assumed sRGB and must be reported as `assumedSrgb`.
- Valid non-sRGB RGB profile: transformed through ICC PCS into linear Rec.2020 D65 and then to the requested display/output profile.
- Invalid embedded or output profile: typed error; never fall back silently.
- D50/D65: Bradford reference math round-trips and LittleCMS profile transforms are covered independently.

## Acceptance workflow

Every M30 case is active. Photographic entries require a reviewed asset record and pass byte/hash validation plus the production `m30_photographic_golden` test. That test proves identity Native Preview/Export byte parity, finite high-precision output, deterministic multi-stage edited output, material control response and immutable source bytes. The ColorChecker entry is the separately validated numerical oracle. Human visual and competitor A/B review remains RC field validation and is not inferred from automated hashes.

For M2 RAW, `scripts/validate-golden-manifest.mjs` additionally requires all six public RAW formats, checks each source byte length and SHA-256, and rejects any fixture whose per-file license is not CC0-1.0. Rust regression tests then require sensor—not thumbnail—decode, active-area metadata, black/white levels, CFA/X-Trans layout, As-Shot WB/Camera Neutral, finite linear Rec.2020/D65 output, full demosaic, source immutability and decode/preview/rerender timings.

## M21-M23 executable extensions

- M21 selects `ai,detail,high-iso,portrait,skin,night,hdr`. Active Rust fixtures additionally cover flat luminance/chroma noise, colored texture, portrait-like skin protection, scene-linear values above 1.0, portrait/landscape/small/large tile plans, overlap seams, cancellation and missing/hash/runtime/tensor/OOM typed failures. The private NAFNet file is never required by CI; a deterministic inferencer double validates domain/tiling/residual semantics, while an explicitly provisioned local model may run the same provider contract.
- M22 selects `color,tone,curve,portrait,skin,night,hdr`. Every analysis asserts deterministic quantiles, finite OKLab covariance/eight-band statistics, monotonic fitted curve, bounded existing adjustment parameters, identity neutrality, underexposure direction, mixed-color direction and reduced skin impact when protection is enabled.
- M23 selects `color,curve,detail,portrait,night,hdr`. Schema round-trip rejects unknown versions and forbidden/non-portable state by construction. Amount endpoints, A/B blend, circular hue, sampled curves, deterministic grain and vignette HDR finiteness are numerical regressions. The pipeline test requires identical Native Preview and Export bytes for the same source identity/settings, including normalized A70/B30 Look output composed through an adjustment Layer and radial Mask.
- Cross-milestone acceptance additionally composes the M21 residual with M9 Detail and M16/M17 portrait semantic retouch, while Reference Match save/reload proves that the applied recipe remains portable rather than becoming baked reference pixels.
