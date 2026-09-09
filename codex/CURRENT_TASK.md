# Current Task

## Current Milestone

M30 — **Starroom v1.0.0-rc.3 field-validation hotfix (feature freeze)**.

M27 acceptance is `225a7ae`; M28 acceptance is `94a9ccc` with push CI `32942892981`; M29
acceptance is `d83fd9f` with push CI `32943720530`. M1-M29 are immutable quality baselines.

## Goal

Produce a verifiable Windows `v1.0.0-rc.3`, not Final, from the published rc.2 baseline. Repair the
Tauri close-request ACL regression reported during field validation without changing imaging
semantics, then repeat installer/runtime qualification on one immutable candidate SHA.

## Validation order

Use Fast -> Targeted -> Full -> Release. Batch related fixes, run only affected local/Blueprint
targets first, and keep `.github/workflows/release-candidate.yml` manual-only until one immutable
candidate HEAD is ready. Heavy 100 MP and installer jobs run together only at the final Release gate.
The active field hotfix is recorded in `docs/41_RC3_RELEASE_NOTES.md`.

## Relevant modules

- `.github/workflows`, `scripts/test-target.mjs`, packaging/release validation
- `src-tauri`, Tauri configuration and Windows installer/runtime smoke
- `crates/starroom-library`, `starroom-history`, `starroom-session`, `starroom-export`
- RAW/Golden manifests and full shared-graph integration tests
- `NOTICE.md`, `MODEL_PROVENANCE.md`, third-party provenance/notices
- `TODO.md`, `docs/IMPLEMENTATION_NOTES.md`, Level-4 release acceptance report

## Constraints

- Feature freeze: bug, regression, performance, compatibility, packaging, migration, security and
  documentation fixes only.
- No source overwrite, cloud dependency, telemetry, hidden upload, silent downgrade/fallback or
  quality reduction.
- BiSeNet remains local-only by explicit user decision. The exact MIT BiRefNet Subject/Background
  asset may be acquired at release-build time, hash-verified and bundled with its license; no model
  may be downloaded at runtime or claimed available without installed inference validation.
- Do not tag or publish `v1.0.0-rc.3` until every same-SHA release gate is green. Do not declare Final, merge
  `main`, close Draft PR #2 or begin M31.

## Acceptance

Warning-denied full CI plus real Windows release build, installer install/launch/uninstall, clean
state, migration/corruption/recovery, offline/privacy/network scan, notices, RAW/Golden, parity,
100k/100MP plan and M28 performance regression gates, plus installed close/runtime validation.
Record executable/installer/model hashes and CI URLs. Only then create `v1.0.0-rc.3` and stop for
field validation.
