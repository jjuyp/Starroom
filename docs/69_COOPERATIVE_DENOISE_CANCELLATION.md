# Cooperative cancellation inside the real Native denoise loop

2026-10-18. Focus: stop superseded CPU work, not change the accepted image algorithm
or equate a kernel benchmark with whole-UI performance.

## Production wiring

Native Preview already enters the shared graph with a request-scoped cancellation
token. Its thread-local checkpoints did not propagate into Rayon pixel workers.
`apply_detail_stage` now explicitly clones that same request token and calls
`denoise_owned_cancellable`. Without a request token, Export continues through the
ordinary owned API. Both APIs execute the same NR implementation, neighbour order,
range exponentials, weights, f32 accumulation and immutable component reads.

Cancellation is checked before work, by component-conversion row, after conversion,
by filter row and each64-pixel block. Once ANY worker observes it, a request-local
atomic latch stays cancelled, even if a caller resets its input flag. Internal
partial buffers are dropped; the public cancellable API returns only
`Err(DetailCancelled)`, mapped to the existing `PipelineError::Cancelled`. No partial
frame is presented or inserted as a completed result. The ordinary public APIs
retain their signatures and use the same kernel with a compile-time false probe.

Shared detail also checks cancellation between NR/local detail/sharpen/finishing.
This batch does NOT interrupt Gaussian/local-detail/sharpen/finishing inner loops,
LibRaw decode, LCMS transform, GPU dispatch/readback, allocations or every AI stage.
Those remaining cancellation/latency requirements stay OPEN.

## Regression evidence

- Existing frozen serial detail comparisons remain unchanged. New252 comparisons
  cover1/2/4 threads,7 empty/border/normal shapes,3 radii and4 strengths, including
  negative/HDR/signed-zero pixels. Every output f32 bit, pointer and capacity stays
  exact with cancellation disabled.
- Deterministic controlled checkpoints exercise cancellation at entry, during
  component conversion and during the actual filter loop. Even a one-checkpoint
  request latches; no partially processed success can escape. The next independent
  request still matches the frozen kernel; source pixels remain unchanged.
- The real shared detail-stage test exercises the actual scoped token, typed cancel,
  unchanged uncancelled output and absence of a leaked request token afterward.
- Native performance requests now enter the same cancellation scope as the real
  IPC command, instead of timing the non-token branch. The performance runner also
  hashes `cancellation.rs`; existing real GPU flags,157 CPU/export comparisons,
  unchanged-private-original and same-source-code assertions are not relaxed.

## Real MSVC Release kernel measurement

`node scripts/detail-cancellation-performance.mjs` invokes the real owned NR APIs on
1024x683 deterministic unbounded float pixels, radius4, luminance0.7/chroma0.8. Five
alternating uncancelled samples and five actual AtomicBool-cancelled jobs are kept
with all source hashes/compiler/revision/dirty state and bit-exact assertions.
Input allocation is outside timing. Cancellation is requested2ms after a worker
signals entry; settle measures actual signal-to-join, not where a photographed UI
slider would have sent the signal. Controlled unit checkpoints separately prove
the inside-kernel cancellation cases.

Initial report `detail-cancellation-1792328547764.json`: uncancelled baseline median
342.140ms versus controlled357.453ms (about4.5% overhead); cancel settle median1.300ms,
max/p95 of five2.031ms. Retained as an unfavorable result, not discarded.

Refined implementation groups64 pixels structurally instead of testing a modulus
branch at every pixel. Report `detail-cancellation-1792328615934.json`: baseline
median342.485ms, controlled333.503ms, settle median1.195ms, max/p95 of five1.461ms.
Every uncancelled output remains bit-exact. This small run does not establish a
universal throughput gain or the product100ms target; its purpose is to measure
real cancellation/overhead and retain the first regression. Full Native measurements
and ordinary acceptance are recorded separately when terminal.

## Actual Native RAW run and remaining performance failures

Recovered terminal report `native-preview-rgba8-1792328897844.json` after the
observation handle expired: exit0,157 actual GPU frames versus CPU/export, max RGB8
difference1, original/source-code hashes unchanged during measurement. Same read-only
field RAW, MSVC1.97.1/RTX3050 DX12, interactive1024/Final JPEG96 and primed-cache
conditions as doc67; the request-scoped token is now explicitly active in the probe.

| Combination | Prior median/p95 ms | Current median/p95 ms |
| --- | ---: | ---: |
| ToneColor |58.761/60.373|60.144/71.489|
| DenoiseSharpen |107.894/110.939|111.039/112.683|
| LocalDetailFinishing |96.761/97.159|98.464/100.428|
| ToneDenoiseDetail |130.348/134.703|140.603/154.663|

First interactive51.146ms, separate preparation19.140ms, warm50.517ms,
Final273.955ms, first RAW1:1=3166.224ms/cached2:1=36.742ms. Exposure52.991/80.209ms
and ChromaNR96.350/111.393ms. Unfavorable timings are retained: no universal speedup,
100ms hard-target or first-zoom acceptance is claimed. This run measures uncancelled
actual Native frames, not concurrent supersession, IPC/browser/OS presentation or
the installer's behavior. It does not replace the kernel cancellation measurements.

## Frontend cancellation lifecycle

An executable RED regression showed1000 Native cancel callbacks for1000 pending
updates that NEVER invoked `run`. These create unnecessary IPC and orphan Native
early-cancel metadata rather than cancelling real work. `LatestPreviewQueue` now
calls the Native cancel callback only for its actual active request; discarded
pending/completed work is rejected locally without an IPC. The existing early-cancel
protection for a dispatched request remains unchanged. The new1000-update test covers
both ordinary and same-source interactive queues, one active cancellation, zero
pending dispatch/cancel callbacks, all superseded promises and successful reuse.
Existing continuous-drag publication, Before/After isolation, final/source switching,
latest pending and delayed-decode generation tests remain unchanged. Focused63 tests,
affected ESLint and TypeScript pass. This is not a whole-UI latency measurement or
proof that every Native cancellation registry is bounded for arbitrary IPC callers.

## Ordinary acceptance (2026-10-19)

Full Rust `test-timing-1792342269311.json` PASS: format, warning-denied workspace
Clippy, ordinary tests and doc-tests, including actual RAW/photographic/shared-graph
regressions. Inventory488 registered/484 ordinary/four unchanged opt-in exclusions.
Detail12 tests and real shared-stage test pass; no frozen regression is deleted.

Web attempt `test-timing-1792342056231.json` records JSON, Golden manifest, full
ESLint and224 Vitest tests/31 files PASS. Vite then exited with Windows fast-fail
code3221226505 after transformation, without a source diagnostic. The failure report
is retained. The identical canonical `npm run build` was rerun independently on
unchanged source: both TypeScript projects, Vite production build and package
validator PASS. This is a targeted build recovery, not a fabricated all-green
original report or a claim that the transient toolchain cause is identified.
Affected63 Vitest tests, ESLint/TypeScript and license561/6/269/infrastructure19 pass.
No new lockfile/dependency/license changes. Current-SHA remote acceptance remains
required after commit. Prior8a9ff84 Full39129301054/secondary39129305989 SUCCESS.

No new dependency/model, image-science replacement, quality reduction, source-file
write, public private-photo/model distribution, installer or Final acceptance.
