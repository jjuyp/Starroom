# Exact detail scheduling and true Off finishing

Remote ordinary acceptance verified2026-10-18:ddbdef8211202ca934ca54c0046766495cf11a43
Full39028054270 and secondary39028057407 SUCCESS; PR#2Open/Draft. Original wider gates
below remain open; this is not Final release acceptance.

2026-10-17. Same Native Preview/Export algorithms; optimization of scheduling,
neighbor-address reuse, invariant weights and an actual no-op path. No reduced
resolution, removed stage, replacement blur, changed radius or new image engine.

## Production implementation

- Existing pinned Rayon1.12.0 executes independent Gaussian rows, sharpen/local-detail
  pixels, denoise component conversion/output, and broad-blur output rows. The global
  pool is reused; no per-frame pool or per-worker full-frame scratch is introduced.
- Gaussian RGB neighbors share one boundary/address computation, preserving every
  channel's original kernel order and f32 accumulation. Vertical/horizontal pass
  order and clamp-to-edge semantics are unchanged. Broad-blur f64 integral-prefix
  construction remains serial and unchanged; only independent output rows parallelize.
- Denoise precomputes the same f32 spatial weights once in the same dy/dx order;
  range weights, accepted neighbors, per-channel and weight-sum accumulation order
  remain unchanged. Neighbor RGB shares one address, not three clamp/sample calls.
- When both Grain and Vignette amounts are exactly zero, finishing no longer runs
  per-pixel radius/sqrt/noise hashes/grain calculations simply because another detail
  control is enabled. Shape and finite validation are retained. Real Off now preserves
  negative zero exactly rather than adding signed zero noise to each pixel. Active
  finishing (including tiny nonzero amounts) retains the complete original path.
- This is CPU parallelism, not GPU detail residency. Native GPU/CPU/shared-export
  semantics and all source/prepared/creative cache identities remain unchanged.

## Frozen reference and validation

Test-only `crates/starroom-detail/tests/oracles/serial_detail_v1.rs` retains the
pre-optimization arithmetic from source blob4a4f0472f3d2fd41112d1309395f5f628e341456
atd14c979. There is no runtime alternative engine or TS image math.

The new serial/parallel unit executes588 function comparisons: seven empty/single-
row/single-column/odd2D shapes, five Gaussian radii, three sharpen strengths,15
independent/extreme denoise parameter/radius combinations, five independent/combined
local controls, each in1/2/4-thread pools. It compares every f32 bit, dimensions and
source immutability, including negative/HDR values. Debug and MSVC Release pass.
A separate finishing test covers Off HDR/negative/signed-zero identity and rejection
of malformed/NaN images. All original tests remain, with no tolerance changes.

Final targeted detail milestone `.starroom-reports/test-timing-1792234814420.json`
passes detail/heal/portrait/AI-denoise/look tests, all75 shared pipeline unit cases,
actual photographic/RAW/WB/source-preparation/CPU-GPU regressions,39 render tests,
format and warning-denied workspace Clippy. An earlier scheduling-only milestone
1792234183982 is retained. Full Rust1792235622485 passes format, warning-denied
workspace Clippy and tests/doc-tests (462 registered/458 ordinary/four opt-in gates
excluded). Full Web1792235436963 passes216 tests/30files, lint/TS/build/JSON/Golden/
packaging configuration. License verification passes. Remote acceptance needs this
group's own commit/SHA. These are ordinary batch gates, not complete Final acceptance.

## Real Native request results

Same generated24MP source and 1024 interactive tier/unchanged Final JPEG96 as doc58.
Old RGBA baseline: `native-preview-rgba8-1792233954272.json`.
Final worktree: `native-preview-rgba8-1792235081478.json`, MSVC1.97.1/LLVM22.1.6,
revision5617731+dirty changes, key-source SHA256 recorded and unchanged during timing.
Five changed settings per27 controls; actual frames/Encode executions/source hash
verified, not key-only cache hits. Source and final quality are not downscaled further.

| Control | Before median / p95 ms | After median / p95 ms |
| --- | ---: | ---: |
| Exposure | 55.829 / 57.482 | 56.769 / 71.724 |
| Sharpen | 180.705 / 181.278 | 70.964 / 71.264 |
| LumaDenoise | 421.432 / 424.029 | 100.008 / 121.789 |
| ChromaDenoise | 428.896 / 430.125 | 100.717 / 102.704 |
| Texture | 215.803 / 217.311 | 81.785 / 108.059 |
| Clarity | 213.098 / 214.383 | 82.817 / 82.972 |
| Dehaze | 211.819 / 213.260 | 87.155 / 100.540 |

Intermediate parallel-only report1792234433114 (before shared neighbor reads/Off
repair) is retained: sharpen101.568/103.673ms, denoise119.920/122.072ms,
texture115.130/115.621ms. It is not substituted for the final result.

**The hard universal100ms gate still fails**: final LumaDenoise p95121.789ms,
ChromaDenoise102.704ms, Texture108.059ms and Dehaze100.540ms; exposure p95 worsened
from57.482 to71.724ms despite median56.769ms. These observations are retained, not
hidden by another favorable run. Cold generated24MP open2716.787ms, Final refine
258.692ms and1:1/2:1 viewport190.612/105.421ms remain separate optimization targets.
This excludes IPC/browser/input/OS presentation, high-radius/heavy-mask interactions,
100MP, private AI model inference, photographic detail-quality approval and installation.
It is not a whole-product realtime, Lightroom-equivalence or Final release claim.

## Dependency and memory scope

Cargo.lock adds only the existing Rayon workspace edge to starroom-detail, no new
package/version. Provenance and lockfile license report are updated;561Rust/6npm/
269notices pass. Output/intermediate full-frame buffer shapes are unchanged; no
100MP peak-memory/lifecycle gate is inferred. Further owned stage lifetimes and
preview-pyramid/cold-tier selection remain to be optimized separately.

