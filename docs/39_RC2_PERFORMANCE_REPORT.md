# v1.0.0-rc.2 performance acceptance

This report defines the immutable-candidate performance evidence for rc.2. Exact measurements are
emitted by the manual `Release Candidate Gate` from optimized Rust release binaries on the
candidate SHA and retained in its job logs. They are not estimated from debug builds or
substituted with development-profile unit-test timings.

## Gates and machine-readable evidence

| Area | Production workload | Evidence record |
| --- | --- | --- |
| Library | Register 200 assets, generate first thumbnails, reopen cached catalog | `RC2_LIBRARY_PERF` |
| Preview | 24 MP cold Fit, cached reopen, interactive Exposure, final refine | `RC2_PREVIEW_PERF` |
| High resolution | 24/45/60/100 MP preview, viewport tile and full export | `M30_LARGE_IMAGE` |
| Installer runtime | Clean install, launch, deterministic self-test, offline AI, uninstall | `M30_INSTALLER_RUNTIME` |

The workflow fails on functional, memory-safety, deterministic-output, installed-runtime or model
availability regressions. Timings remain observational across shared GitHub-hosted runners; a timing
change alone is not converted into an unsupported percentage claim.

## rc.2 performance architecture

- Library import registers inexpensive identity/header metadata in one bounded transaction and
  performs full metadata enrichment outside the SQLite lock.
- Thumbnail generation is bounded and progressive; cached restart reuses persisted thumbnails.
- Preview rendering runs on blocking native workers with one-active/one-latest scheduling and
  request-scoped cancellation.
- Decoded preview sources and source-positioned high-resolution viewport results use bounded,
  identity-complete caches.
- Fit publishes a native Library thumbnail immediately while the full native Fit render refines.
- Professional batch export no longer performs an unused duplicate full-source decode.
- The 100 MP gate uses real full-pixel buffers and validates bounded viewport transport rather than
  moving the complete high-resolution frame through JSON or the WebView.

## Interpretation

The authoritative values for a published rc.2 are the records from the same commit named by the
`v1.0.0-rc.2` tag. Results from another SHA, a cancelled run, or a prior release are not acceptance
evidence. The release report must link that exact workflow run.

## Measured implementation-freeze results

Implementation SHA `6e3e44d98c7e695e0d497c43ba00508320a8916f`, Release Candidate Gate
[`34305888090`](https://github.com/jjuyp/Starroom/actions/runs/34305888090). Windows Server 2025,
MSVC release profile, GitHub-hosted four-logical-CPU runner with 16 GiB physical memory. Shared
host variance applies; all functional gates passed.

| Workload | Result |
| --- | ---: |
| Register 200 assets | 65.952 ms |
| Produce first thumbnails for 200 assets | 487.647 ms |
| Cached Library restart for 200 assets | 57.610 ms |
| JPEG 24 MP Native Fit | 2151.072 ms |
| JPEG cached reopen | 0.355 ms |
| Legal Nikon D1 NEF Native Fit | 463.501 ms |
| RAW cached reopen | 0.271 ms |
| Interactive Exposure response | 93.010 ms |
| Settled full-quality refine | 1460.835 ms |
| First / cached 1:1 viewport tile | 620.706 / 232.651 ms |

The complete JPEG Fit is above the aspirational one-second target. The product does not hide this:
it first publishes the already generated Native Library thumbnail as usable feedback, labels it
`refining`, and replaces it with the shared-graph Fit. Interactive response is below 100 ms. The
1.46 s final refine misses the absolute one-second target but is at least 85.4% faster than the
rc.1 field report's conservative ten-second lower bound, exceeding the specified 80% fallback.

| Real full-pixel workload | Preview | 1:1 tile | Export | Peak process memory |
| --- | ---: | ---: | ---: | ---: |
| 24 MP | 852.48 ms | 319.37 ms | 17.447 s | 1,586,749,440 bytes |
| 45 MP | 1397.27 ms | 368.74 ms | 35.902 s | 2,930,696,192 bytes |
| 60 MP | 1282.03 ms | 384.62 ms | 42.461 s | 3,888,836,608 bytes |
| 100 MP | 2081.94 ms | 555.15 ms | 72.123 s | 6,449,516,544 bytes |

Every viewport transported exactly 4,718,592 bytes rather than the full source frame. The 24 MP
export is at least 85.5% faster than the rc.1 field observation of two minutes or more, exceeding
the required 50% improvement. Because rc.1 observations were human-reported lower bounds rather
than instrumentation captured on this runner, these percentages are conservative and are not
presented as same-machine microbenchmarks.

## Optimization evidence

- Neutral Geometry, Denoise, Texture, Sharpen, Grain and Vignette reuse their working buffer while
  retaining stage/cache identity; deterministic parity tests protect the former output.
- Independent per-pixel WB preparation, Tone, Curve, Mixer, Grading and output conversion use the
  pinned Rayon 1.12.0 implementation without changing graph order.
- LittleCMS transforms use a documented no-shared-cache context per parallel chunk. A 32,777-pixel
  HDR regression matches the single-call transform within `1e-6` and preserves ICC semantics.
- Professional Export performs one required full source decode when AI Denoise is disabled.
