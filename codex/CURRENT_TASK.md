# Current Task

## Current Milestone

M28 — **Performance / Scale / GPU Optimization (in progress)**.

M27 is accepted at `225a7ae0f8b7b92364fc953ddbf09874c05c78e8`; push CI
`32857958550` passed. M1-M27 remain immutable acceptance baselines.

## Goal

Measure and optimize the existing Native shared graph without changing output quality. Add real
stage/cache/memory telemetry, interaction-priority rendering with deterministic final parity,
complete cache identity and dirty-region scheduling. Validate CPU/GPU tolerances, 24/45/60/100 MP,
100,000 metadata assets, 10,000 history commands and 100/500 item export queues with bounded
resources. Publish actual baseline and optimized measurements.

## Relevant modules

- `crates/starroom-render` graph profiling, cache identity, tiles, dirty regions and scheduler
- `crates/starroom-pipeline` measured production stages and CPU/GPU final parity
- `crates/starroom-library` 100k metadata-only catalog scale
- `crates/starroom-history` 10k replay/checkpoint/snapshot scale
- `crates/starroom-export` bounded batch queue and 24-100 MP memory accounting
- `src-tauri/src/lib.rs` interactive/final preview priority and compact diagnostics
- `docs/33_M28_PERFORMANCE_REPORT.md`, `TODO.md`, `docs/IMPLEMENTATION_NOTES.md`

## Constraints

- Final preview/export output remains the accepted deterministic graph; interactive drag may request
  a smaller pyramid level but must visibly identify the mode and converge to the exact final render.
- Cache keys include source, full render/layer/mask state, geometry and output color transform.
- No stale-cache reuse, silent GPU fallback, unbounded queue, raster storage in the Library or
  quality reduction to meet memory limits.
- Performance claims require recorded measurements, not qualitative statements.

## Acceptance

Targeted performance/scale/parity tests, milestone Clippy/rustfmt/frontend gates and final GitHub
Actions must pass before M29 begins. Keep PR #2 Draft and do not merge `main`.
