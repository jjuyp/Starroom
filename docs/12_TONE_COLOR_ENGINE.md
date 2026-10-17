# Starroom Image Pipeline Specification V0.1

## Core principle
Do not perform creative color operations directly in display-encoded sRGB when linear-light or perceptual processing is more appropriate.

## Pipeline
```text
Decode JPEG/PNG/TIFF -> input profile -> working RGB -> linear RGB
-> White Balance -> Exposure -> Highlights -> Shadows -> Whites -> Blacks
-> Contrast -> Master Tone Curve -> OKLab/OKLCH -> 8-band Color Mixer
-> Skin Tone -> Vibrance with skin protection -> Saturation
-> RGB -> Gamut Compression -> Output/Display Transform -> Preview/Export
```

## Exposure
Engine range: -5.0 EV to +5.0 EV. In linear RGB: `RGB_out = RGB_in * 2^EV`.

## Tone region weighting
Highlights, Shadows, Whites and Blacks use smooth continuous weights, not binary cutoffs or per-channel thresholds. Whites/Blacks are narrower endpoint bands than Highlights/Shadows.

Negative Highlights performs highlight compression rather than a uniform negative brightness offset. Positive Highlights uses bounded expansion. Positive Shadows lifts lower luminance while retaining controllable black anchor; negative Shadows smoothly deepens it.

## Contrast
Pivot-based around a defined middle-gray reference, initially 0.18 linear. Production mapping is tuned with fixtures.

## Tone Curve
Master and public-1.0 RGB curves use monotonic interpolation by default, editable points/endpoints, fade-black and S-curve support, and no spline overshoot. Prefer monotone cubic interpolation rather than unconstrained cubic splines.

## Perceptual color
Use OKLab/OKLCH for selective hue/chroma/lightness work and skin-likelihood heuristics. UI may still call the feature HSL/Color Mixer.

## Saturation
Global chroma adjustment. At -100, output becomes perceptually neutral grayscale while preserving lightness as closely as practical.

## Vibrance
Low-chroma colors receive more effect, already-saturated colors receive less, and skin-like colors are protected. Conceptual weight: `low_chroma_weight * (1 - skin_protection * skin_weight)`.

## Skin likelihood
Continuous heuristic 0..1 using hue proximity + chroma + lightness, never hue alone. Future AI face/skin masks may multiply the heuristic.

## Color Mixer
Overlapping circular smooth hue bands: Red, Orange, Yellow, Green, Aqua, Blue, Purple, Magenta. Each exposes Hue, Chroma/Saturation and Lightness/Luminance without hard boundaries.

## Gamut
Creative intermediates are not hard-clamped to 0..1. Use dedicated smooth gamut compression near output, preserving hue as much as practical and preventing NaN/Inf.

## Encoded-image WB
JPEG/PNG/TIFF controls are relative, not physical Kelvin: Temperature -100..100, Tint -100..100. Do not label them Kelvin.

Production relative controls use a prepared LittleCMS chromatic-adaptation matrix in linear
working RGB, not per-pixel OKLab offsets. Temperature follows relative reciprocal-temperature
displacement on the mature daylight locus; Tint follows its perpendicular CIE 1960 u/v direction.
The D65 anchor is internal control mapping, not source Kelvin. Neutral and black are exact identities;
global/local/GPU use the same Native matrix. Physical RAW camera WB remains an earlier camera/profile
stage and must not be declared equivalent to this encoded/working-space relative correction.

## WB eyedropper
Sample a small area, initially 9x9, using robust statistics. User selects a neutral gray/white region.

## Numeric safety
Every nonlinear operator handles zero, negative intermediates where applicable, divide-by-zero, invalid pow/log domains, NaN and infinity. Operators document expected input/output domains.
