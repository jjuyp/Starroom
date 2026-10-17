# M26 Professional Export Engine

Status: production implementation on the v0.2 core-quality branch.

## Shared full-resolution path

`starroom-export` reopens the immutable source with `decode_source`, resolves the current edit state and executes the same Rust Native graph used by Preview and Before/After. It never upscales a preview or transports full pixels through JSON. RAW enters through LibRaw and the camera-profile stage; rendered files enter through the ICC input stage. The graph remains scene-linear Rec.2020 D65 until the intended LittleCMS output transform.

## Output contract

- JPEG, PNG and TIFF encode 8-bit RGB. A 16-bit request returns `UnsupportedBitDepth`; there is no silent downgrade.
- sRGB, Display P3, Adobe RGB and Rec.2020 use generated standard ICC profiles through LittleCMS and may embed the exact profile.
- Original, width, height, long/short edge, percentage and fit-within sizing preserve aspect ratio and use `image` Lanczos3.
- Output sharpening is separate from M9 Detail. M26 exposes Off and validated Screen Low/Standard/High only.
- Filename tokens include original name, date, sequence, rating, camera and look; Windows-invalid characters and reserved names are sanitized.
- Collision defaults to auto-rename. Fail and explicit overwrite are separate policies; overwrite is never silent.

Metadata policy is explicit: all safe metadata, camera metadata, copyright-only or none. GPS defaults off and is only eligible when `include_location` is explicitly enabled; source metadata is never mutated. Presets are versioned portable settings and omit the destination. Recipe identity binds source fingerprint, edit-state identity, export settings and engine version.

## Queue, cancellation and files

The Tauri command prepares compact per-asset edit intent then performs full-resolution work on a blocking worker, keeping the UI responsive. Items are isolated into completed/failed/cancelled/skipped result lists. A cancellation flag is checked before expensive stages and before finalization. Encoding writes a process-scoped temporary sibling, flushes/syncs, then atomically renames it; failure/cancellation removes the temporary and never leaves a corrupt final file. A conservative 32-bytes-per-pixel preflight rejects pathological dimensions before allocation.

## Acceptance and limitations

Regression coverage includes three codecs, quality bounds, four ICC outputs/profile embedding, every resize mode, naming/collision, metadata policy intent, presets, recipe identity, mixed batch isolation, cancellation/temp cleanup and 24/45/60/100 MP memory scheduling checks. Preview/export stage semantics are shared even though interactive preview intentionally decodes a bounded source.

Known explicit limitation: 16-bit PNG/TIFF is not enabled by the current validated encoder path. Print output sharpening is also intentionally unavailable. Both are typed capabilities rather than silent substitutes.
