# M17 — Native Skin Retouch

## Contract

M17 is a non-destructive Native stage. It receives linear Rec.2020 D65 working pixels plus the M16 soft Skin raster and protected Eyes/Brows/Lips/Mouth/Hair raster. React serializes bounded controls only; it never performs retouch or color math.

The stage order is frequency separation, mask/edge-protected smoothing, texture-preserving recombination, tone evenness, then skin-local hue/chroma and face exposure. Neutral controls are exact identity. Scene-linear values are not clamped to display range inside the stage, and non-finite parameters or incompatible rasters return typed errors.

## Shared graph and state

`SkinRetouchParameters` is part of the same edit settings used by Native Preview, Before/After and Export. Existing snapshot history supplies undo/redo and project serialization. M16 model/cache identity stays independent of M17 control changes.

## Acceptance

Synthetic regressions cover recombination, high-frequency reduction, spatial mask isolation, protected features, neutral identity, extreme finite output and invalid rasters. Golden tags select portrait, dark portrait, backlight, mixed-temperature and white/black clothing cases; real pores, beard, glasses and makeup remain visual review categories when licensed fixtures are provisioned.
