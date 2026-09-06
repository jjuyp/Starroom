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
| Preview latency / latest-wins | OPEN | Native command is synchronous; source decoding precedes cache lookup. Need cooperative cancellation, upstream reuse, nonblocking delivery and measured response. |
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
