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
