# Retire the unused direct JPEG export endpoint

2026-10-18. One release-critical legacy-path cleanup, not a new export engine.

## Audit and production removal

Actual UI calls `exportNativeLibraryBatch` or `exportNativeBatch`; both invoke the
existing professional `native_export_batch`. Repository callers were checked before
removal. The old `native_export_jpeg`, its request/result, frontend wrapper/result
and unused save-path picker had no UI caller. It synchronously decoded/rendered and
used direct `std::fs::write`, JPEG-only quality clamping and no embedded ICC, rather
than the professional atomic/collision/metadata/profile/batch/cancellation policy.
Keeping it exposed through Tauri left an avoidable weaker route despite correct UI.

Remove that command from registration and delete only those dead helpers/types.
Preserve the professional commands, shared Native graph, all supported format/depth/
profile controls and the `same_file` helper also used by Reference Match. The existing
Style save dialog still uses its normal save import. No source photograph, project,
edit state, fixture or regression is deleted. Code is recoverable from Git history.

This intentionally retires an unused pre-release IPC endpoint; it does not silently
redirect its old API to a different format or downgrade a requested output. UI has
no alternate action path to migrate, and project/history schemas are unchanged.

## Evidence

- New ownership regression is red before removal, green afterwards. It forbids the
  old writer/bridge/picker and independently requires professional Native registration,
  destination guard and both actual UI export callers. This is an architecture guard,
  not a replacement for encoder/write tests.
- Targeted Vitest51 tests pass (ownership and Native contracts), targeted strict
  Clippy and format pass. Existing exporter27 executed tests plus four Native export
  integration cases pass. Coverage includes real JPEG/PNG/TIFF, true16-bit round-trip,
  ICC embedding/profile identity, original protection, atomic/collision/cancellation,
  private metadata policy, batch500/failure isolation, saved edits and restart workflow.
- The opt-in high-memory100MP gate is explicitly not run by those ordinary tests;
  its requirement and every existing regression remain. Full local/own remote must
  reach terminal results before ordinary batch acceptance is recorded.

Full local acceptance: Rust1792294738034 passes format, warning-denied workspace
Clippy and tests/doc-tests (470 registered/466 ordinary/four opt-in excluded).
Web1792294565947 passes219 tests/30files, ESLint, TS/production build, JSON/Golden
and packaging configuration. License561Rust/6npm/269notices and infrastructure
validators pass. Own remote acceptance remains pending until this commit succeeds.

No algorithm, color transform, image precision, dependency/model version or license
changes. No speedup, complete Phase21/23/24/30, installed/runtime or Final claim follows
from deleting a dormant path. Current UI/packaging and all original color/performance/
memory/release requirements still need their own scope-matched acceptance.
