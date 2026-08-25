# Current Task

## Current Milestone

M27 — **Professional Export Completion (in progress)**.

M1-M26 remain accepted at baseline `9516729cc524058228142cddc65515132695994d`. M27 closes
the production export precision, metadata, print-sharpening and scale-validation gaps without
changing the accepted RAW, colour, creative or non-destructive graph semantics.

## Goal

Ship real 16-bit/channel PNG and TIFF from the existing float Native shared render graph, preserve
the selected LittleCMS output transform and embedded profile, apply explicit metadata/location
policy, and provide distinct screen/print output sharpening. Prove codec sample depth, precision,
atomic/cancellation behaviour, colour-profile identity and memory-safe large-image preflight.

## Relevant modules

- `crates/starroom-pipeline` high-precision output surface over the existing shared graph
- `crates/starroom-imageio` real RGB16 PNG/TIFF encoders and metadata containers
- `crates/starroom-export` output precision, sharpening, metadata policy and regressions
- `src-tauri/src/lib.rs`, `src/nativeRender.ts`, `src/App.tsx` typed transport and UI selection only
- `docs/17_THIRD_PARTY_PROVENANCE.md`, `docs/IMPLEMENTATION_NOTES.md`, `TODO.md`

## Open-source decision

Continue using the pinned `image` PNG codec, pinned `tiff` encoder and LittleCMS provider. The
shared graph remains float/linear Rec.2020 D65 through colour output; quantization occurs exactly
once at the selected encoder boundary. Output sharpening uses a deterministic Gaussian unsharp
mask with separate screen and print parameterization derived from output dimensions and resize
ratio; it does not introduce browser image math or a second render graph.

## Acceptance criteria

- JPEG RGB8, PNG RGB8/RGB16 and TIFF RGB8/RGB16 are explicitly validated; JPEG 16-bit is a typed
  unsupported-depth error and no format silently downgrades.
- Decoder round trips prove 16-bit samples and more than 256 unique gradient levels.
- sRGB, Display P3, Adobe RGB and Rec.2020 use one LittleCMS output stage with the matching ICC
  embedded when requested and no duplicate transform.
- GPS remains excluded by default; explicit preserve requires real source GPS.
- Print sharpening is materially distinct from Screen and depends on output/resize geometry.
- Atomic/cancel, queue isolation, naming/collision, full-resolution and 24/45/60/100 MP memory
  preflight regressions pass; Preview/Export shared graph coverage does not regress.

## Stop conditions

Do not begin M28 before M27 acceptance is committed, pushed and green. Do not merge `main`,
force-push, close PR #2 or mark it ready.
