# M25 History / Snapshot Architecture

Status: production implementation accepted on the v0.2 core-quality branch.

## Model

`starroom-history` records versioned non-destructive edit commands, not pixels. Every accepted command has a sequence, parent/state version, timestamp, description, affected graph stage and typed before/after payload. State versions are stable SHA-256 identities and therefore participate directly in render/cache invalidation.

Interactive slider, curve and transform changes use begin/preview/commit coalescing. A pointer-down through pointer-up brush stroke is one command. Undo and redo apply inverse/forward payloads deterministically; accepting a new edit after undo truncates the active redo branch.

## Persistence and checkpoints

The versioned history document is atomically replaced on disk. Periodic full non-destructive state checkpoints bound replay cost; neither history nor checkpoints contain source images, preview pixels, mask rasters or model weights. Opening validates schema, state finiteness, sequence/parent versions and checkpoint replay. Corrupt data returns a typed error.

## Named snapshots

Snapshots hold a named, versioned full edit state including tone, WB, curves, grading/detail, layers, MaskTree intent, portrait retouch, healing and look state. Create/rename/delete are independent from history. Restoring a snapshot adds a normal, undoable `Restore Snapshot` command and leaves the snapshot intact. Current/snapshot and snapshot/snapshot comparison resolve each state through the existing shared Native renderer.

## Acceptance

Tests cover simple and 100-step undo/redo, slider/curve/stroke coalescing, structured layer/mask/healing/AI/reference/look commands, redo truncation, checkpoint creation, 5,000-entry bounded reload/replay, persistence, snapshot lifecycle/restore undo, cache identity and typed corruption/version failures. No raster is serialized.
