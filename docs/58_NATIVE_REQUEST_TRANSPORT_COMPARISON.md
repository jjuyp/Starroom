# Actual 24MP Native request transport comparison

2026-10-17. These are real Native shared-graph requests through decode/preparation,
GPU/CPU stages and binary framing, **not Tauri IPC, browser presentation or an
installed input-to-present acceptance**. No Final/100MP/whole-product claim follows.

## Reproducible runner

`node scripts/native-preview-performance.mjs --format=jpeg` and then
`node scripts/native-preview-performance.mjs --format=rgba8` invoke the same
MSVC Release opt-in Native performance test. The default release performance test
now selects the production UI's RGBA8 interactive transport; explicit JPEG exists
only as a diagnostic baseline. Final/HQ stays JPEG96. It verifies actual frame
magic, changed-control Encode executions (not identical cached frames), source
immutability and existing viewport/cache assertions. The 24MP source is generated
in an exclusively owned tempfile directory; the old PID-based pre-delete is removed.
No original image is written or deleted.

The runner enforces Windows MSVC and27 distinct controls, saves exit/failure/full
output, compiler/revision/dirty state and source hashes, and rejects measurements
if key sources change during the run. Reports use exclusive creation. Current
reports below predate added compiler/hash fields; they accurately retain dirty=true
and are not passed off as final clean-SHA acceptance. Test labels now say Native
request, not the misleading historical GPU_RESIDENT label: readback still exists.

## Sequential, no concurrent builds

Both runs use the same compiled binary and source worktree at1b40115 plus only the
performance-test/runner changes, before detail scheduling optimization. Source is
a generated6000x4000 JPEG gradient; actual interactive processing tier1024, five
changed states per control. This is not photographic or RAW-slider coverage.

- JPEG: `.starroom-reports/native-preview-jpeg-1792233936000.json`, test19.55s.
- RGBA: `.starroom-reports/native-preview-rgba8-1792233954272.json`.
- Earlier reports1792233648343(JPEG)/1792233855491(RGBA) remain retained. The first
  overlapped other compilation; its higher times are not used as a causal speedup.

| Control | JPEG median / p95 ms | RGBA median / p95 ms |
| --- | ---: | ---: |
| Exposure | 64.679 / 65.771 | 55.829 / 57.482 |
| Contrast | 62.297 / 62.394 | 53.069 / 53.564 |
| Highlights | 60.490 / 60.639 | 51.433 / 51.741 |
| Shadows | 59.064 / 60.132 | 51.523 / 51.849 |
| Whites | 59.489 / 59.886 | 51.837 / 52.070 |
| Blacks | 59.564 / 60.377 | 50.546 / 51.232 |
| Temperature | 59.155 / 59.741 | 51.051 / 51.402 |
| Tint | 59.049 / 60.900 | 50.582 / 51.099 |
| Vibrance | 63.330 / 64.930 | 54.960 / 55.308 |
| Saturation | 66.118 / 67.236 | 57.839 / 59.052 |
| MasterCurve | 58.931 / 59.575 | 50.628 / 50.913 |
| RedCurve | 59.558 / 60.014 | 51.099 / 51.759 |
| GreenCurve | 59.375 / 60.469 | 51.267 / 51.523 |
| BlueCurve | 60.198 / 60.388 | 50.290 / 51.102 |
| MixerHue | 60.356 / 61.183 | 50.966 / 51.334 |
| MixerChroma | 65.678 / 67.244 | 52.012 / 52.488 |
| MixerLightness | 60.495 / 62.478 | 50.458 / 51.843 |
| GradingGlobal | 63.432 / 68.017 | 53.351 / 53.841 |
| GradingShadows | 59.931 / 60.567 | 50.452 / 51.542 |
| GradingMidtones | 60.686 / 60.858 | 52.228 / 54.046 |
| GradingHighlights | 60.230 / 60.299 | 52.066 / 52.375 |
| Sharpen | 191.578 / 192.585 | 180.705 / 181.278 |
| LumaDenoise | 450.707 / 460.843 | 421.432 / 424.029 |
| ChromaDenoise | 461.859 / 473.455 | 428.896 / 430.125 |
| Texture | 220.450 / 222.554 | 215.803 / 217.311 |
| Clarity | 221.970 / 231.819 | 213.098 / 214.383 |
| Dehaze | 223.651 / 243.762 | 211.819 / 213.260 |

All27 operations execute actual changed renders. Transport removes about8-14ms
for many non-spatial controls in this controlled sequence. It does not solve the
remaining180-430ms spatial/detail cost or cold-open/refine/zoom delays, and does
not prove the <100ms requirement for every common edit. Previous adverse/outlier
reports remain valid. Browser/raw bytes and actual PNG export evidence are in doc56.

## Next measured bottleneck

The existing detail algorithms have serial row/pixel loops. Their scheduling is
being changed to the existing pinned Rayon while preserving exact per-pixel math,
neighbor order and f32/f64 accumulation. A frozen pre-change serial oracle records
source blob4a4f0472f3d2fd41112d1309395f5f628e341456 atd14c979. Do not declare this
optimization accepted without exact comparisons and a new actual request report.

