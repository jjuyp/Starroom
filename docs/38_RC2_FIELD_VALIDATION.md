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
| Library registration/import latency | FIXED, verification pending | Phase A registers from file identity/header metadata in one bounded transaction; Phase B enriches full metadata on a background worker without holding SQLite during decode. Deterministic 200-asset release timing and installed progressive-use validation remain required. |
| Progressive thumbnail display | FIXED, verification pending | Remove Promise.all display barrier; bounded 3-worker queue publishes each result; cache raster work no longer holds SQLite lock. |
| Removed assets resurrect | FIXED, verification pending | One Native transaction deletes catalog membership, cascades keyword/collection membership, never source files. Retired-ID high-water prevents new imports from inheriting old history sidecars. |
| Selection / rating / recent imports | FIXED, verification pending | Stable-ID Shift/Ctrl/Meta/Ctrl-Shift selection, full native filtered Ctrl-A, one-transaction bulk removal, schema-v2 deterministic latest import batches, shortcuts 0-5 and thumbnail 0-5 rating controls are implemented. Installed workflow validation remains required. |
| Preview latency / latest-wins | FIXED, verification pending | Heavy work now runs on a blocking worker, per-surface latest-wins keeps one pending state, Native cancellation checkpoints reject stale publish, decoded source tiers and the GPU device are reused. Release-mode latency evidence remains required. |
| True 1:1 viewport tiles | IN PROGRESS | 1:1/high zoom now requests a separately identified full-source Native render and Fit remains bounded; final viewport tile transport/cache and installed latency acceptance remain open. |
| AI availability | FIXED, verification pending with Face/Skin exception | Native startup verifies model presence/hash before use and UI shows Ready / Model not installed / Invalid / Error. User-approved local-only BiSeNet policy remains. Installed packaging/offline validation pending. |
| Final workspace / snapshots / control layout | FIXED, verification pending | Develop now owns Navigator plus Presets/Layers/History; full Snapshot create/rename/delete/restore/compare is exposed there. Export is a floating context panel and Portrait/Layers/Looks use progressive disclosure. Real installed 1280/1920/2560 layout acceptance remains required. |
| Export performance | FIXED, measurement pending | Normal Professional batch no longer performs an unused full source decode before the FullResolutionRenderer decodes the same asset. AI Denoise retains its required residual prepass. Release-mode 24 MP comparison remains required. |
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

## Staged Library/query batch

- Catalog schema v2 adds a monotonic import-batch identity. Recent Imports selects exactly the
  newest batch; it does not infer a session from a timestamp window.
- Phase A performs registration and inexpensive header dimensions only. Phase B reads full EXIF/RAW
  metadata on a blocking worker without holding the catalog mutex, then commits short per-asset
  updates. Thumbnail workers remain separately bounded and progressively publish results.
- Ctrl/Cmd+A requests the complete active native query identity set rather than the current 200-row
  page. Five Stars uses a native minimum-rating query; thumbnail stars apply 0-5 immediately and
  persist through the same bulk workflow command.
- The left Develop sidebar no longer duplicates the Library tree. Snapshot lifecycle actions are
  visible under History; optional heavy panels only appear for their selected tool.

## AI availability / high-resolution / Export batch

- One Native availability command verifies the pinned portrait, Subject/Background, Sky and AI
  Denoise model files and hashes before a tool is clicked. The UI uses four non-ambiguous states:
  Ready, Model not installed, Invalid and Error. It never downloads a model or selects cloud/API.
- Fit continues to request a bounded Native preview. 1:1 and zoom above Fit carry an explicit
  `highResolution` contract; interactive drag stays on the low-cost tier and final refine reopens
  full source resolution rather than enlarging the 1800/4096 preview. Viewport-only tile transport
  remains a release blocker until its production cache/latency gate passes.
- Professional batch previously decoded every source during preparation even when AI Denoise was
  disabled and then decoded it again in `FullResolutionRenderer`. The unused first decode is gone;
  shared graph, color precision, output bytes, atomic write and AI-enabled behavior are unchanged.

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
