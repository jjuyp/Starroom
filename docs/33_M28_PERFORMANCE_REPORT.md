# M28 Performance and Scale Report

## Measurement contract

This report records the Windows GitHub-hosted runner result for the exact acceptance commit. Wall
clock values are printed by the tests under `STARROOM_PERFORMANCE_GATE=true` with `--nocapture`;
the uploaded timing JSON records the enclosing commands. They are regression observations, not a
hardware promise. A future release-machine run must add CPU/GPU model, RAM, driver and warm/cold
cache metadata before product latency targets can be declared.

The measured corpus is deterministic and never weakens image quality. Final interactive preview
and Export use the full shared graph; interactive drag may request a 1024-edge pyramid but settling
requests the configured final level. Cache identity contains source, render, layer, mask, geometry
and color-transform state. Superseded generations are rejected rather than displayed.

## Acceptance observations

The acceptance corpus measures or validates:

- Native preview/export per-stage CPU time, current/peak process working set and cache counters;
- 24, 45, 60 and 100 MP tile-plan construction without allocating fake full-frame pixels;
- 100,000 metadata-only Library rows with indexed search, sort, paging, filters, collections and
  keywords;
- 10,000 History commands with checkpoints, restore, Snapshot, undo, redo and branch truncation;
- a 500-item bounded export queue with 499 successes, one isolated typed failure and 500 progress
  updates;
- production decode -> shared graph -> resize -> color transform -> encode profiling on a compact
  deterministic fixture; and
- the complete CPU graph against the wgpu Exposure hybrid path when an adapter is available.

## Windows runner measurements

Source run: [Blueprint Check 32940807628](https://github.com/jjuyp/Starroom/actions/runs/32940807628),
commit `0cae30b35ff36affa013818d4a5e866c8eedf991`, Windows x64, generated
2026-08-26T07:17:14Z. The complete Rust gate took 834.596 s: rustfmt 2.979 s,
warning-denied workspace/all-target Clippy 251.396 s and the full Rust workspace 403.409 s; the
remaining time is the separately repeated performance corpus.

| Observation | Result | Interpretation |
| --- | ---: | --- |
| 24 MP plan | 48 tiles; <1 ms timer resolution | planning only |
| 45 MP plan | 48 tiles; <1 ms timer resolution | planning only |
| 60 MP plan | 48 tiles; <1 ms timer resolution | planning only |
| 100 MP plan | 48 tiles; <1 ms timer resolution | planning only |
| 100,000 Library rows | 3,476.044 ms | transaction insert plus indexed query corpus |
| 10,000 History commands | 542.542 ms | commit/checkpoint/reload/snapshot/undo/redo/branch corpus |
| 500-item export queue | 7,363.446 ms | 499 completed, one isolated failure, 500 progress events; compact mock pixels |
| encoded production export profile | 12.7903 ms summed stage CPU | 4×3 decode/graph/resize/ICC/encode fixture |
| encoded preview profile | 3.4955 ms summed stage CPU | three-pixel instrumentation/parity fixture |
| RAW shared graph | decode 178.94 ms; first preview 3,497.42 ms; rerender 3,518.19 ms | licensed Nikon fixture on CI runner |
| export process working set | 11.616 MB before; 14.914 MB after; 15.254 MB lifetime peak | compact fixture; not a large-image peak |
| preview process working set | 11.981 MB before; 12.952 MB after/peak | compact fixture; not a large-image peak |

The CI stage map reported CPU durations for RAW/encoded decode, camera transform, WB, Tone, Curve,
Mixer, Grading, Detail, Geometry, Mask, Skin, Healing, Resize, output color transform and Encode.
GPU duration remained `null` on this unqualified runner and is not represented as zero.

## Architecture result

M28 removes avoidable interactive work through bounded preview pyramids, halo-aware tiles,
visible-first priority, dirty-region selection, exact cache keys and generation cancellation. The
same semantic graph remains authoritative for Preview and Export. Working buffers remain float and
no optimization inserts an intermediate 8-bit conversion or lowers final quality.

GPU status is intentionally narrow: scene-linear Exposure is the only migrated production node.
Tone, Curve, Color Mixer, Color Grading, Detail, Masks, Skin, Healing and Geometry still execute the
complete CPU reference. Unsupported GPU stages are not silently labelled accelerated and Export
does not depend on a GPU.

## Scope limits

The 24-100 MP scheduler observations are **plan timings**, not full-resolution pixel-render times.
The current shared renderer still needs a production end-to-end tiled execution path before a
100 MP open/mask/heal/export latency and true peak-memory claim can pass. The small profiling image
proves stage instrumentation and parity, not photographic workload throughput. Hardware GPU time is
not reported on a runner without a qualified adapter. These are release-visible M28 limitations,
not inferred successes.

M30 must re-run the final performance gate on the packaged Windows executable and treat any
regression as a release blocker.
