# M30 Level-4 Release Acceptance

Status: feature freeze; release-candidate qualification in progress. No `v1.0.0-rc.1` tag or Final
claim exists until every release blocker below is closed with an immutable CI URL and artifact hash.

## Automated gates

- [ ] warning-denied rustfmt/Clippy, Rust workspace and doc tests
- [ ] Vitest, ESLint, TypeScript, production frontend build and JSON/schema validation
- [ ] Golden 11/11 manifest and six immutable CC0 RAW fixtures
- [ ] shared Preview/Export, CPU/GPU Exposure parity, Library/History/export scale and M28 regression
- [ ] Windows MSVC release executable and NSIS installer build
- [ ] clean-directory silent install, executable launch, second installed launch and silent uninstall
- [ ] release identity, tracked-model exclusion and production network API scan
- [ ] GPL license, third-party notices and model provenance included in the Windows bundle
- [ ] installer/executable SHA-256 artifact report

## Recovery and migration matrix

The automated release suite must exercise production persistence APIs rather than test-only
substitutes:

- Session: clean close, crash-recovery marker, corrupt JSON and unsupported/invalid state.
- History: persist/reload determinism, corrupt JSON, corrupt checkpoint, 10,000-command restore,
  undo/redo and branch truncation.
- Library: empty-database initialization, schema reopen, corrupt/future database rejection, missing
  source, relink, missing thumbnail, damaged-cache regeneration, stable pagination and 100,000
  metadata-only assets.
- Export: atomic success, cancellation cleanup, isolated batch failure and an incomplete temporary
  file left by another process that cannot replace or block a valid final output.

The cross-crate release cases live in `crates/starroom-export/tests/m30_release_recovery.rs`; the
remaining cases are colocated with the owning production crate and run under the locked workspace
test gate.

## Release blockers not represented by a green build alone

- The photographic Golden cases are still planned; the active suite is numerical/synthetic plus
  RAW decode fixtures. A green manifest is not a human-reviewed portrait/night/full-pipeline image
  comparison.
- wgpu production migration covers Exposure only. The other requested CPU/GPU parity stages remain
  explicit CPU reference stages.
- 100 MP coverage is tile-plan and conservative memory-preflight validation, not a measured full
  100 MP open/mask/heal/export workflow.
- Local-only AI weights are not distributable in the installer. Clean install can verify discovery
  and typed unavailable state but cannot claim offline AI Mask/Skin/Denoise inference without
  legally packageable weights.
- Physical 100/125/150/200% HiDPI multi-monitor movement and real photographic field/competitor A/B
  require a qualified Windows machine and human image review.

These limitations must either be resolved or explicitly approved as RC field-validation scope;
they cannot be silently relabelled as Level-4 successes.
