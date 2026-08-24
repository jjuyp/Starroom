# Current Task

## Current Batch

M24 → M25 → M26 — **Complete 2026-08-24; M27 Not Started**.

M1-M23 remain accepted. This batch adds a local-first SQLite Library, persistent command/checkpoint
history with named snapshots, and a full-resolution professional export system. Stop after final
M24-M26 acceptance; do not begin M27.

## Goal

Complete M24 asset/workflow management, M25 deterministic history/snapshots and M26 professional
export without resetting prior native pipelines. Preview, Before/After and Export retain one shared
Rust graph; React transports only paths, queries, serialized edit intent and interaction state.

## Relevant modules

- `crates/starroom-library` SQLite schema, migrations, identity, import/query/workflow/cache
- `crates/starroom-history` commands, checkpoints, persistence and named snapshots
- `crates/starroom-export` full-resolution output, sizing, naming, queue and atomic files
- `crates/starroom-pipeline`, `starroom-color-management`, `starroom-imageio` shared graph/output
- `src-tauri/src/lib.rs` bounded native runtimes and compact IPC
- `src/nativeRender.ts`, `src/App.tsx` interaction/state presentation only

## Required files

- `AGENTS.md`
- This file
- `docs/16_OPEN_SOURCE_FOUNDATION.md`
- `docs/19_MODULE_DEPENDENCY_MAP.md`
- `docs/20_OPEN_SOURCE_IMPLEMENTATION_MAP.md`
- `docs/21_DEVELOPMENT_ACCELERATION.md`
- `MODEL_PROVENANCE.md`

## Open-source decision

Use SQLite through pinned `rusqlite` with bundled SQLite, existing LibRaw/image metadata providers,
LittleCMS output profiles and `image` Lanczos/codecs. No second RAW/color pipeline, React image
math, cloud catalog, telemetry or database raster storage was added.

## Acceptance criteria

- M24–M26 production implementation, targeted regressions, cross-milestone scenarios and Level 3
  acceptance all pass on the final acceptance commit.
- Preview, Before/After and Export retain one Native shared graph; unavailable providers remain
  explicit typed states rather than transparent/silent fallbacks.

## Acceptance evidence

- M24 acceptance commit: `d867018`; dependency-corrected acceptance is included in `a6c82d2`.
- M25 acceptance commit: `a6c82d2`; push and PR workflow runs `32746036320` and `32746029652` passed.
- M26 acceptance commit: `38dd905`; push and PR workflow runs `32746978693` and `32746984258`
  passed. Final Level 3 evidence is the successful GitHub Actions `Blueprint Check` attached to the
  branch's final `[full-acceptance]` HEAD, so this document does not retain a stale pre-fix run ID.
- The final `[full-acceptance]` commit runs warning-denied workspace Clippy, rustfmt, every Rust
  test, all frontend tests/lint/build, JSON/schema, Golden/RAW manifests and packaging validation.
- Cross-milestone regression binds Library identity/project state to persistent history/snapshots and
  binds full-resolution export to the M1-M23 shared graph, including layers/masks/denoise/retouch/heal/looks.

## Targeted tests

- `npm.cmd run test:library`
- `npm.cmd run test:history`
- `npm.cmd run test:export`
- Native preview/export contract and shared graph Level 2, then Level 3 acceptance

## Stop conditions

Do not merge `main`, force-push, make PR #2 ready, or begin M27. M24 Complete. M25 Complete.
M26 Complete. M27 Not Started.
