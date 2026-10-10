# M23 Portable Look Engine

## `.srlook` v1

`schemas/look.schema.json` is the strict public contract. A v1 file contains `schema`,
`schemaVersion`, `id`, `name`, portable metadata, Tone, relative encoded-image color intent,
Master/R/G/B curves, Color Mixer, Color Grading, classic denoise, local detail, sharpen, Grain and
Vignette. Unknown root fields, corrupted JSON, invalid/future schema versions, non-finite/out-of-
range controls and non-monotonic curves are rejected.

The schema deliberately has no Crop, geometry, healing operation, face ID, mask, source path,
camera profile or absolute camera-specific state. A Look is an adjustment recipe, never baked
pixels.

## Semantic interpolation and Style Mixer

- Look Amount interpolates from the current state to the Look. Amount zero and one preserve exact
  endpoint state.
- Style Mixer normalizes explicit A/B weights; for example A=70 and B=30 evaluates at B fraction
  0.30 before the global Look Amount is applied.
- EV/scalars use bounded linear interpolation. Hue follows the shortest circular arc. Curve
  channels are sampled into a fixed representation and blended under monotonic/endpoint policy;
  control-point indexes are not paired.
- Booleans/enums use deterministic nearest-endpoint selection.

Every result is reapplied through the existing Native parameter graph. There is no Look-only image
renderer and no creative math in React.

## Finishing stages

Deterministic procedural Grain uses Amount, Size, Roughness, Color and a Look seed combined with
the immutable image identity. Repainting therefore keeps the same grain and Export is repeatable.
Vignette supports signed Amount, Midpoint, Roundness, Feather and Highlight Protect. Both stages
run after detail/sharpen and before output color conversion, preserve scene-linear HDR values and
reject non-finite state.

## Acceptance

Regression covers save/load, schema v1, corrupted/unknown/future rejection, exact Amount endpoints,
359/1 degree hue interpolation, sampled monotonic curve blend, A/B 70/30, deterministic grain,
signed vignette extremes, highlight protection, HDR finiteness, Reference Match save/reload and
Native Preview/Export parity.

Cross-milestone graph coverage also exercises M21 AI Denoise with M9 Detail and M16/M17 portrait
mask retouch, plus a normalized A70/B30 Look flowing through an M14 adjustment layer, M15 radial
mask, Grain/Vignette and identical Native Preview/Export output. DirectML failure classification and
explicit CPU fallback policy are covered independently so invalid model/hash/output failures cannot
masquerade as device fallback.
