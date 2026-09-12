# M21 Native AI Denoise

## Production path

The fixed local model is NAFNet-SIDD width-32 at the revision and hashes recorded in
`MODEL_PROVENANCE.md`. The checkpoint and deterministic static 512x512 FP32 opset-20 ONNX stay
under ignored `models/local/`; they are not downloaded, committed, packaged or copied into CI.

The shared Native order is:

```text
decode / RAW develop -> camera color -> Linear Rec.2020 D65 -> optics -> geometry
-> AIDenoiseModelDomainV1 -> NAFNet residual -> tone / creative color -> detail / sharpen
-> grain / vignette -> output color transform
```

Preview and Export call the same precreative preparation and residual adjustment stage. An enabled
edit without the exact residual is a typed failure; classic denoise and Browser Canvas are never
used as substitutes.

## Domain, tiling and cache

- Finite-sanitize and robust p01/p99 exposure normalization precede the versioned linear-sRGB,
  signed range-compression and sRGB-transfer model adapter.
- Tiles are 512 pixels with 64-pixel overlap, visible-first scheduling and raised-cosine blending.
- The inference key contains source identity, model version/hash and domain version. Amount,
  Detail, Color Noise and Preserve Skin form a separate adjustment key, so slider changes reuse
  ONNX output.
- Full-frame working allocation has a conservative 2 GiB preflight. Superseded previews set a
  cancellation token and M13 rejects stale completion.

## Provider and failure policy

DirectML is preferred. Only classified provider/runtime/inference failures trigger an explicit CPU
retry. The model-status command reports the active provider and fallback reason. Missing model,
hash mismatch, invalid deterministic export, malformed tensor/output and cancellation remain
visible typed failures.

## Acceptance scope and timing

Deterministic CI inferencers cover domain round-trip, HDR/finite behavior, portrait/landscape/small
and large tiling, seam blending, visible priority, cancellation, memory budget, cache separation,
Amount zero, skin protection and synthetic high-ISO/skin/hair/fabric/foliage/night/neon vectors.
The 117 MB private local model is intentionally absent from CI, so real DirectML/CPU wall-clock
timing is hardware qualification data and is not replaced with an invented fixture timing.
