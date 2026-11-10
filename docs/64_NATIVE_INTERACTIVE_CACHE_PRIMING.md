# Bounded Native interactive cache priming

2026-10-18. Move existing input preparation off the first drag; do not change image math.

## Production integration

After a successful, uncancelled Final/Fit request, the real `native_preview` command
returns its already-rendered response without awaiting an optional blocking-worker
cache task. The task only starts when the Native preview worker pool is idle. A new
request epoch prevents obsolete work from starting/continuing into input preparation.
It never schedules/completes a render job, publishes a frame, touches the GPU or
performs file pixel decode. Before/After and foreground work retain the same two-slot
limit; optional work does not queue ahead of active work.

The task borrows a direct1024 tier or derives a bounded1024 proxy from a retained
original preview using doc61's policy. It calls the same `SourcePreparationCache`
input/WB/lens/geometry path used by foreground rendering. `prime` populates that real
immutable cache; creative/display/encode stages do not execute. Renderer lookup and
priming share one implementation, including exact upstream parameter identity, weak
source ownership, four-entry/128MiB bounds and oversized owned/bypass behavior.
No weaker color transform, new resampler, shader, profile, dependency or model is added.

Optional errors cannot replace a successful photograph with an error frame. They are
retained explicitly in the existing scheduler-status command's nested `priming`
diagnostics: running/completed/skipped/failed, elapsed nanoseconds and last error.
Existing scheduler fields remain flattened/unchanged. No private paths, filenames or
pixels are new status fields. Final/export still use authoritative source semantics;
their quality, current state and serialization are unchanged.

## Targeted regressions

- Pipeline priming uses actual LittleCMS/input/geometry preparation, performs no
  creative/display/encode work, reuses its result for exact CPU/export pixels, rebuilds
  on real geometry changes, respects tiny cache budgets and rejects invalid picker ROI.
- Native worker tests prove foreground/Before-After availability, no queued optional
  work while busy, epoch rejection, typed missing-source diagnostic retention and
  permit release. No GPU cache is acquired or poisoned by optional preparation.
- Actual PNG portrait/large photographic JPEG/Nikon RAW tests preserve original hashes
  and exact binary RGB/alpha versus the shared CPU/export graph. Priming leaves render
  scheduler status/generation/frame-cache contents unchanged. The first changed drag
  after preparation has no file decode, resize or input-transform execution.
- Existing source-preparation tests, new Native tests, format and warning-denied targeted
  Clippy pass. Full Rust1792296168945 passes format, warning-denied workspace Clippy,
  workspace tests and doc-tests (473 registered,469 ordinary, four opt-in excluded).
  Full Web1792295985908 passes219 tests/30files, lint, TypeScript/production build,
  JSON/Golden and packaging configuration. License561Rust/6npm/269notice texts and
  test infrastructure pass. Own remote CI remains pending at commit creation.

## Actual Native ready-cache measurements

The explicit `--prime` diagnostic waits for the same Native cache function to finish,
then measures first interaction. This isolates mechanism/cost; it does **not** measure
actual UI background completion, immediate-drag races, IPC or input-to-present.
Production itself does not wait for this task before returning the Final frame.

Same existing private D750 RAW/JPEG originals, MSVC1.97.1/RTX3050/DX12,1024 interactive,
unchanged Final JPEG96. Reports record source/key-code hashes and reject changes during
measurement; the runner now also hashes its own source and `source_preparation.rs`.

| Measurement | RAW ms | JPEG ms |
| --- | ---: | ---: |
| Previous unprimed first interaction (doc62) | 67.443 | 103.773 |
| Completed preparation cost, separately measured | 18.265 | 49.708 |
| First interaction with cache ready | 50.393 | 53.475 |
| Warm changed exposure request | 48.305 | 52.311 |
| Exposure median / p95 | 49.575 / 50.621 | 51.787 / 53.935 |
| LumaDenoise median / p95 | 107.830 / 108.026 | 107.615 / 109.621 |
| ChromaDenoise median / p95 | 107.377 / 109.043 | 110.007 / 110.536 |
| First1:1 viewport | 2929.686 | 252.700 |

RAW report`native-preview-rgba8-1792295804475.json`, JPEG report
`native-preview-rgba8-1792295704541.json`: both137 actual Native GPU-path frames agree
with CPU/export reference within one RGB8 level; originals/key sources unchanged.
All27 control results, including RAW grading-shadow p9581.183ms and earlier regressions,
remain retained. These are separate runs, not a same-session causal speedup guarantee.

Preparation cost is moved earlier, not eliminated: adding it back explains essentially
the previous first-request cost. Warm steady-state speed is not claimed improved.
Universal100ms still fails for denoise, zoom/refine and unprimed/late-preparation cases.
Whole UI latency, scheduling/race measurement, GPU direct presentation/residency,
photographic IQ across all scenes,100MP/lifetime memory, installation/offline and every
other original Production requirement remain open. No new installer/Final is implied.
