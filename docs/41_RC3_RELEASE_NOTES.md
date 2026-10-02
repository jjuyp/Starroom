# Starroom v1.0.0-rc.3

Starroom rc.3 is a field-validation hotfix for the Windows close-request, Library/RAW import,
Nikon color, preview responsiveness and desktop workspace issues found in rc.2. It remains a
prerelease; Preview and Export still share the same Native graph.

## Fixed

- Closing the main window now completes after the clean Session state is persisted.
- Tauri grants the narrow `core:window:allow-destroy` permission only to the `main` window.
- If Session persistence genuinely fails, Starroom still stays open and preserves recovery state.
- Release validation prevents this permission from being accidentally omitted again.
- Exact-file/drop imports persist to the Library and receive Native thumbnails.
- LibRaw camera-matrix direction is corrected; Nikon RAW no longer receives the reported red/green
  cast, and existing DNG/RAW profile regressions remain required.
- Prepared curve coefficients, display-demand preview tiers and bounded RAW decode reuse reduce
  slider latency while retaining final-quality and 1:1 source detail.
- The preview no longer changes layout size as interactive/final rasters arrive.
- The dark workspace now uses the requested blue frosted-glass hierarchy and visual Color controls.
- Native preview now reuses persistent GPU resources and executes WB/Tone/four curves/Color Mixer/
  Grading/Vignette as one production compute pass. High-resolution local masks and explicit-source
  manual Heal render only the required source region plus conservative halo; correctness-sensitive
  auto-source Heal remains an explicit full-frame path.

## Historical local qualification (prior field-fix snapshot)

The counts, timings and binary hashes below describe the earlier snapshot, not the October
field-repair source or newly rebuilt installer. They are retained as baseline evidence only.

- Warning-denied full workspace, 59 frontend tests, 11/11 Golden cases and all six public RAW
  fixtures pass on Windows MSVC.
- GPU-resident release interaction: 1024-edge adjustment preview 106.170 ms; cached reopen 0.249
  ms; final refinement 416.602 ms. This machine does not meet the aspirational 50/200 ms targets;
  the stage report identifies CameraTransform, LittleCMS output and JPEG encode as remaining CPU
  costs, and no resolution/ICC/creative stage was skipped to manufacture a passing number.
- The real-pixel 24/45/60/100 MP gate passes. At 100 MP the Fit preview is 2.731 s, a requested
  viewport tile is 346.77 ms and full export is 60.964 s; no proxy is presented as 1:1 detail.
- The NSIS clean-install gate passes launch, deterministic export/reopen self-test, bundled offline
  Subject/Background inference, legal-resource hashes and silent uninstall.
- Local candidate hashes: executable `b78c53d1571945ed3b381ea537a9981a9dc8746813dc57318889266b5de78dcb`;
  installer `f6366f055ac6b09224aad122d8559525230f2aa175685cae01e77870964409ef`.

Publication still requires the same committed SHA to pass GitHub Blueprint and Release Candidate
workflows; these local results are not substituted for CI.

## Unchanged limitations

- Face/Skin BiSeNet remains local-only and is not included in public installers.
- Subject/Background remains the bundled offline BiRefNet capability.
- Sky and AI Denoise remain explicit optional local capabilities.
- This is not Starroom v1.0 Final. PR #2 remains Open/Draft and `main` remains unmerged.

## October field-repair verification

- GPU poison recovery, queued History, RGB histogram, responsive glass/Color inspector,
  selected-layer mask controls and sidebar-only Presets/History are implemented with regressions.
- RAW catalog dimensions come from LibRaw active area/orientation, with identity-checked
  selected-record repair for older tiny metadata. No source photo or history sidecar is changed.
- Local source gate: 298 Rust tests, 91 frontend tests, format, warning-denied Clippy, doc-test
  runners, lint, TypeScript/build, JSON/schema, 11/11 Golden manifest and six CC0 RAW fixtures.
  Private real-model AI inference/preview-export parity is an additional manually run gate, not
  a public-CI test. Current license inventory is 561 Rust/6 production npm/269 notice texts.
- Final installer hashes and same-SHA CI links are recorded with the dated deliverable report,
  not substituted with the historical hashes above. No new release tag is authorized by local
  source tests alone.
- Newly built desktop GUI interaction/screenshot acceptance remains unverified: the test-tool
  launch approval expired. Older installed-app screenshots were rejected. This must be resumed
  with app-launch authorization before claiming all nine field requests have full visual and
  interactive acceptance. A second computer still requires independent field validation.
