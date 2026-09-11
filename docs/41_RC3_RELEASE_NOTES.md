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

## Local release qualification

- Warning-denied full workspace, 55 frontend tests, 11/11 Golden cases and all six public RAW
  fixtures pass on Windows MSVC.
- Release interaction: 1024-edge adjustment preview 94.516 ms; cached reopen 0.253-0.282 ms;
  final refinement 367.345 ms.
- The real-pixel 24/45/60/100 MP gate passes. At 100 MP the Fit preview is 4.352 s, a requested
  viewport tile is 392.62 ms and full export is 67.334 s; no proxy is presented as 1:1 detail.
- The NSIS clean-install gate passes launch, deterministic export/reopen self-test, bundled offline
  Subject/Background inference, legal-resource hashes and silent uninstall.
- Local candidate hashes: executable `4d1ef32bd8533860d7e89fdd845d4168d7517bbb9489b4b9a6b89fe0515b4775`;
  installer `f41f4e0a2ecbc8b4f0f40ea0f15197fc1eee6a395ed74936fde09c072fa869a0`.

Publication still requires the same committed SHA to pass GitHub Blueprint and Release Candidate
workflows; these local results are not substituted for CI.

## Unchanged limitations

- Face/Skin BiSeNet remains local-only and is not included in public installers.
- Subject/Background remains the bundled offline BiRefNet capability.
- Sky and AI Denoise remain explicit optional local capabilities.
- This is not Starroom v1.0 Final. PR #2 remains Open/Draft and `main` remains unmerged.
