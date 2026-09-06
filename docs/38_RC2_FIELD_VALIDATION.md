# v1.0.0-rc.2 field-validation fixes

Baseline: `f4d958c70b42340d1b11ed77e13068298e36b7ca` (`v1.0.0-rc.1`).
Branch: `agent/starroom-v0.2-core-quality`. PR #2 stays Open/Draft. No M31 or Final release.

## Acceptance policy

Fast -> Targeted -> Full -> Release. A fix is not VERIFIED until its real production path
has passed the required regression and field check. Unit/UI transport tests alone do not prove
installed WebView rendering, native latency, or installer model availability. One immutable
candidate SHA must pass all final release gates before an rc.2 tag is created.

## Explicit user model decision (2026-09-06)

Keep BiSeNet local-only. Do not redistribute or bundle Face/Skin for rc.2. The user explicitly
selected this option after the redistribution conflict was raised. This is an accepted release
limitation, not an offline-ready claim. Clean installs must say Model not installed; there must
be no download/cloud/API fallback. Preserve the existing model license/hash verification.
The remaining core AI and optional-pack requirements still need production validation.

## Product blockers

| Area | State | Evidence / remaining acceptance |
| --- | --- | --- |
| Invisible thumbnails | FIXED, verification pending | Generated file URL had no Tauri asset-scope grant. Grant only the exact returned cache file. Validate installed JPEG/RAW WebView display and restart/corruption handling. |
| Library registration/import latency | OPEN | Import metadata currently decodes pixels while holding the catalog mutex. Staged registration and measured 200-asset timings remain required. |
| Progressive thumbnail display | FIXED, verification pending | Remove Promise.all display barrier; bounded 3-worker queue publishes each result; cache raster work no longer holds SQLite lock. |
| Removed assets resurrect | FIXED, verification pending | One Native transaction deletes catalog membership, cascades keyword/collection membership, never source files. Retired-ID high-water prevents new imports from inheriting old history sidecars. |
| Selection / rating / recent imports | OPEN | Stable-ID Shift/Ctrl/Meta/Ctrl-Shift range helper and bulk UI added; filtered Ctrl-A and deterministic import batches still required. Rating 0 command added. |
| Preview latency / latest-wins | FIXED, verification pending | Heavy work now runs on a blocking worker, per-surface latest-wins keeps one pending state, Native cancellation checkpoints reject stale publish, decoded source tiers and the GPU device are reused. Release-mode latency evidence remains required. |
| True 1:1 viewport tiles | OPEN | Validate production tile transport and source-resolution coordinates, not scaled preview. |
| AI availability | OPEN with Face/Skin exception | User-approved local-only BiSeNet policy above; remaining availability UI/runtime/packaging gates pending. |
| Final workspace / snapshots / control layout | OPEN | Real installed workflow and 1280/1920/2560 layout acceptance required. |
| Export performance | OPEN | Preserve precision/color/source invariants; require measured release-mode baseline comparison. |
| RC2 release | OPEN | No acceptance tag/artifact claim until same-SHA final gates pass. |

## First Library batch regression

- Rust: 50-source bulk delete across 20/30-item transactions, idempotence, reopen persistence,
  immutable source bytes, collection cascades, retired ID non-reuse, forced transaction rollback.
- Frontend: query-order selection/toggling/ranges, additive range, changed-filter anchor;
  progressive 100-result loading with bounded concurrency and failure isolation; stale-query suppression;
  rating shortcuts 0 through 5.
- Native thumbnail command grants asset-protocol access only after resolving/generating the cache file.
  Neither source-directory nor unrestricted asset-scope access is added.
- Local targeted Library web tests: 20 passed. Full frontend build and lint are run before push.
- Local MSVC tests attempted: compiler stopped because `link.exe` is unavailable. This is not a
  passing native result; Windows validation is required. No test/coverage requirement is waived.

No performance improvement percentage has been claimed from these unit tests.

## Preview scheduling batch

- The Tauri command is asynchronous and dispatches CPU/GPU work through `spawn_blocking`; the
  WebView/event loop is no longer occupied by synchronous Native rendering.
- Each visible PreviewCanvas owns a latest-wins queue. One request may execute and only the newest
  pending state is retained. Superseded native work receives a request-scoped cancellation token,
  and stale output is rejected on both sides of IPC.
- The shared Rust graph checks cancellation before/among camera/WB, Tone, Curve, Mixer, Grading,
  Mask blocks, Skin, Healing, Detail, Geometry and output transform. Export executes the identical
  graph with no cancellation token and retains deterministic parity.
- A bounded 128 MiB decoded-source cache is keyed by immutable source identity plus preview level;
  it reuses RAW decode/camera input for slider changes and A -> B -> A without caching creative output.
  A process-wide GPU renderer is initialized once and reused.
- Frontend regression proves intermediate state suppression even when native cancellation arrives
  late, one-active/one-latest behavior, separate Before/After surfaces and recovery after failure.
  Rust regression proves a cancelled preview cannot leak cancellation into later Preview/Export.
