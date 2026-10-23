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
| Invisible thumbnails | VERIFIED | Exact-file Tauri scope, restart reuse and corrupt-cache regeneration passed Blueprint and release self-test coverage. |
| Library registration/import latency | VERIFIED | The release workload registered 200 assets in 65.952 ms; all first thumbnails completed in 487.647 ms and cached restart in 57.610 ms. |
| Progressive thumbnail display | VERIFIED | Bounded three-worker progressive delivery and failure isolation passed frontend and Native regression. |
| Removed assets resurrect | VERIFIED | Transactional removal, cascade cleanup, source immutability, retired-ID non-reuse and reopen persistence passed. |
| Selection / rating / recent imports | VERIFIED | Stable range/additive/full-query selection, 0-5 rating and schema-v2 latest-batch behavior passed the final Blueprint. |
| Preview latency / latest-wins | VERIFIED | Release results: cached reopen 0.355 ms, interactive Exposure 93.010 ms, final refine 1460.835 ms; stale/cancelled results cannot publish. |
| True 1:1 viewport tiles | VERIFIED | First/second 1:1 tile were 620.706/232.651 ms. The 24/45/60/100 MP real-pixel gate passed with bounded 4,718,592-byte viewport transport and explicit full-frame compatibility. |
| AI availability | VERIFIED with accepted Face/Skin limitation | Installed BiRefNet CPU inference produced a finite 1024x1024 Subject mask. Face/Skin, Sky and AI Denoise returned explicit typed-unavailable; no network fallback exists. |
| Final workspace / snapshots / control layout | VERIFIED in automated desktop gates | Snapshot/session/recovery and command workflows passed. Physical mixed-DPI and human multi-monitor evaluation remains post-RC field validation. |
| Export performance | VERIFIED | The real 24 MP Native workflow exported in 17.447 s. Compared with the rc.1 field observation of at least two minutes, this is at least 85.5% faster. |
| RC2 release | READY FOR SAME-SHA FINAL GATE | Product blockers are verified. Tagging remains conditional on the final documentation SHA passing Blueprint and Release Candidate Gate. |

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
- The public installer does not contain BiSeNet. It does contain only the approved BiRefNet v1
  Subject/Background model, obtained by the release runner from asset id `186739942` and rejected
  unless its 224,005,088-byte size and pinned SHA-256 both match. Runtime first resolves bundled
  `models/local`, with an explicit environment override reserved for reviewed local model packs.
  The installed executable must also initialize the CPU ONNX provider and return a finite real
  Subject mask from a deterministic offline fixture; hash-only discovery is not sufficient.
- Fit continues to request a bounded Native preview. 1:1 and zoom above Fit carry an explicit
  `highResolution` contract; interactive drag stays on the low-cost tier and final refinement uses
  an SRP3 source-positioned viewport payload instead of enlarging the Fit frame. The Native path
  preserves f32/RAW metadata, expands tile-safe processing by the graph halo, crops only the halo
  interior, and caches the encoded viewport under source/edit/region identity. Geometry, Lens,
  Masks, Healing, Skin, finishing effects, AI and image-statistical WB are deliberately classified
  as `full-frame compatibility`; correctness is preserved and the UI exposes that state instead of
  silently claiming the local optimization. The final 24/45/60/100 MP real-pixel gate passed.
- Professional batch previously decoded every source during preparation even when AI Denoise was
  disabled and then decoded it again in `FullResolutionRenderer`. The unused first decode is gone;
  shared graph, color precision, output bytes, atomic write and AI-enabled behavior are unchanged.
- The center preview immediately publishes the current asset's Native-generated Library thumbnail
  while the first Native Fit render is pending. This supplies usable asset-switch feedback without
  blocking input; the state is explicitly labelled refining and cannot satisfy or replace 1:1.
- The manual Release Gate now emits `RC2_LIBRARY_PERF` for 200 registrations, first thumbnails and
  cached restart, and `RC2_PREVIEW_PERF` for a real 24 MP cold Fit, cached reopen, interactive
  Exposure, final refine and 100%/200% source-positioned tiles. Final measured numbers belong in
  the performance report, linked to release run `34305888090` for implementation SHA `6e3e44d`.

## Release evidence at implementation freeze

- Blueprint push run `34305108067` and Draft PR run `34305110825` passed on `6e3e44d`.
- Release Candidate Gate `34305888090` passed all three jobs: Library/Preview performance,
  real 24-100 MP Native workflow, and Windows MSVC installer/runtime.
- The installed executable completed deterministic Library -> History -> Session -> Native Export
  restore/export parity, kept the source immutable, and performed real offline BiRefNet CPU inference.
- Portable executable SHA-256: `4f0c94fc0f527728c501c5fe2d26acc270ad0c246068d7434a5ef5efa6a5963a`.
- NSIS installer SHA-256: `98c3e180f422d9a10fb4b1453ac0df873a37f154c1b3f40979ef5bfb396e7520`.
- The final documentation commit must rerun the same gates. Only that later SHA may be tagged.

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
