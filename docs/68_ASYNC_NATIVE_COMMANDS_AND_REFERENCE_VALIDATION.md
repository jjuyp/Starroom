# Native command scheduling and real Reference Match regressions

2026-10-18. This is a focused production repair, not whole-UI latency or Final
Acceptance. The installed executable has not been launched or replaced.

## Actual asynchronous boundary

The installed Tauri2.6.3 macro source defaults ordinary commands to Blocking and
directly invokes their body (`tauri-macros/src/command/wrapper.rs`). The real
`ai_availability_status`, `native_optics_status`, `native_sample_color` and
`native_reference_match` commands previously did model verification/file decoding/
native graph work in those synchronous bodies. These same registered command names
now enter an async handler and execute the original core through Tauri's existing
`spawn_blocking`, not an async executor thread. No new runtime/dependency is added.

Successful wire results and all original core errors remain unchanged. A worker
join failure is explicit (`<Operation>WorkerFailed`), not a silent fallback. Direct
non-IPC callers of availability keep the same synchronous core. Availability is a
startup/status operation, NOT verified as a per-slider cause. Moving it does not
make the image graph itself faster or establish input-to-present performance.

Two worker tests hold an actual blocking job, poll the async boundary, prove a
different worker thread and progress of an unrelated async task, then release the
job. They also preserve an original error, contain a worker panic and run another
job afterward. Real command tests compare model status, actual photographic PNG
lens resolution, post-geometry color sampling and a two-image Reference Match
against their cores. File/decode and invalid-control errors stay explicit; source
bytes are unchanged. Source-ownership tests guard actual command registration and
async worker wiring in addition to these executable tests.

## Two failures exposed by the real Reference Match command

The initial test failed, rather than being changed to accept an error:

1. Existing native Look blending samples32 intervals including BOTH endpoints,
   therefore33 points. The old Native legacy-curve limit was32. The existing
   `.srlook` schema and Look validator already accept256 points/channel. The Native
   boundary now shares that existing256 limit via `starroom_look::MAX_CURVE_POINTS`;
   all four channels plus the legacy master are bounded, finite and within0..1.
   No sampling, spline, point or precision is removed. Tests keep32/33/256 input
   points exactly, reject257 and reject non-finite/out-of-range channel values.
2. The next run failed `native color mixer settings are outside supported ranges`.
   `blend` incorrectly treated the mixer's signed -30..30 degree adjustment as an
   absolute wheel angle; negative offsets wrapped near360. Mixer offsets now blend
   as signed offsets, within their convex range. Absolute grading-wheel hues STILL
   use the existing shortest circular interpolation. A125-combination test covers
   negative/positive endpoints, zero and intermediate amounts across all8 bands,
   validates both inputs/results, and checks the exact expected offsets. The prior
   circular wheel regression remains intact.

The unchanged real Reference Match test now succeeds with33 points and renders its
actual result through the CPU Native Preview AND Export graphs with exact equality.
Same-source rejection, NaN/Inf/out-of-range amounts and original source bytes are
also checked. This validates the command and common Native graph, not screenshot
IQ for every reference or GPU/full-resolution parity for every mixed Look.

## Local acceptance evidence

- Full Rust `test-timing-1792327964107.json`: PASS; format, warning-denied workspace
  Clippy,485 registered/481 ordinary tests and doc-tests. Four unchanged private-AI/
  Native performance/large-pixel/thumbnail opt-in cases are not ordinary acceptance.
- Full Web `test-timing-1792327796192.json`: PASS223 tests/31 files, ESLint,
  TypeScript, production build, JSON,11/11 Golden manifest cases,6 CC0 RAW fixtures
  and authored sensor fixture/package configuration. Actual photographic/RAW graph
  tests also ran in Full Rust; manifest validation alone is not pixel acceptance.
- Focused command tests5/5 plus curve boundary1/1 and Look11/11 pass, including
  both retained real-command failures above after repair; frontend wiring53/53.
- License validation561 Rust/6 npm production packages/269 notices and infrastructure
  19 targets/19 tags pass. Lockfiles/provenance dependencies are unchanged.
- No end-to-end timing or synchronous-versus-async image speedup is inferred from
  scheduler tests. Prior157-frame Native control timings in doc67 remain the actual
  performance evidence, with the combined>100ms failures still OPEN.

## Remaining gates

No RAW/LittleCMS/tone/spline algorithm, library dependency, model, license or schema
is replaced; no private asset is redistributed. Existing processing order remains.
The earlier owned-detail/grain batch65ad6aa passed its exact-SHA Full Check
39127505993 and secondary39127509627. Current batch local/remote gates are recorded
below when terminal; prior-SHA CI is not current acceptance.

This does not close synchronous AI inference/model install/Advisor commands, their
cancellation races, overall scheduling concurrency, GPU residency/direct display,
combined detail>100ms, first RAW1:1 decode,100MP memory/workflows, monitor/alpha,
whole photographic quality, installer/offline runtime or original Phases0–30.
No final/installer/Lightroom-equivalence claim is made.
