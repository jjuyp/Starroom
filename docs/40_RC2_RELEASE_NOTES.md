# Starroom v1.0.0-rc.2

Starroom rc.2 is a field-validation release focused on Library correctness, interactive Native
preview responsiveness, high-resolution zoom, explicit offline AI availability and desktop
release reliability. It remains a release candidate, not the final v1.0 release.

## Highlights

- Library thumbnails progressively appear from the Native cache and remain available after restart.
- Import registration no longer holds the catalog while expensive metadata is decoded.
- Multi-selection, range selection, filtered select-all, rating and Recent Imports use stable Native
  catalog identities.
- Asset removal is transactional, never deletes source photos and prevents retired IDs from
  inheriting old history.
- Native preview uses non-blocking latest-wins scheduling with cancellation and bounded decoded
  source reuse.
- Fit preview gives immediate Native thumbnail feedback; 100% and higher zoom request a real
  source-positioned high-resolution Native region.
- Public rc.2 installers bundle a pinned, hash-verified MIT BiRefNet model for offline
  Subject/Background masks.
- The installed executable runs deterministic core and real CPU ONNX inference self-tests.
- Windows release artifacts include a true MSVC executable, NSIS installer and SHA-256 manifest.

## AI packaging policy

Face/Skin remains an explicit local-only optional capability. BiSeNet is not included in the public
repository or installer, by user decision and the recorded redistribution policy. A clean install
shows `Model not installed`; Starroom never silently downloads a model or falls back to cloud/API.
Subject/Background is available offline from the verified bundled BiRefNet model.

## Known limitations

- High-resolution local-region rendering is used only for graph states proven tile-safe. Geometry,
  Lens, Masks, Healing, Skin, finishing effects, AI and image-statistical white balance use the
  explicitly reported full-frame compatibility path to preserve correctness.
- Face/Skin requires a separately installed, locally licensed BiSeNet model.
- Release-candidate field validation remains required before a final v1.0.0 decision.

PR #2 remains Open and Draft. It is not merged into `main` by this release.
