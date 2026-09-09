# Starroom v1.0.0-rc.3

Starroom rc.3 is a focused field-validation hotfix for the Windows close-request failure found in
rc.2. It remains a prerelease and does not change the Native imaging pipeline.

## Fixed

- Closing the main window now completes after the clean Session state is persisted.
- Tauri grants the narrow `core:window:allow-destroy` permission only to the `main` window.
- If Session persistence genuinely fails, Starroom still stays open and preserves recovery state.
- Release validation prevents this permission from being accidentally omitted again.

## Unchanged limitations

- Face/Skin BiSeNet remains local-only and is not included in public installers.
- Subject/Background remains the bundled offline BiRefNet capability.
- Sky and AI Denoise remain explicit optional local capabilities.
- This is not Starroom v1.0 Final. PR #2 remains Open/Draft and `main` remains unmerged.
