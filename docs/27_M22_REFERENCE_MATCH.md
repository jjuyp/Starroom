# M22 Native Reference Match

## Analysis

Current and reference images are decoded locally and prepared through the Native precreative
working graph. `starroom-reference` computes p01/p05/p25/p50/p75/p95/p99 luminance, OKLab mean and
covariance, eight OKLCh hue-band statistics, and shadow/midtone/highlight lightness/chroma/hue
means. Source and reference fingerprints identify the analysis inputs; pixels never cross JSON.

## Recipe

Quantile pairs produce a bounded monotonic tone mapping fitted to existing Exposure, Contrast,
Highlights, Shadows, Whites, Blacks and Master Curve state. OKLab global differences and OKLCh
regional constraints produce existing relative WB, Color Mixer and Color Grading parameters.
Protect Skin attenuates hue/chroma transfer. The result stores confidence and both fingerprints.

Reference Match adds no renderer. Amount, Tone, Color and Grading use the portable semantic Look
interpolator to combine the current Native state with the fitted target; circular hue and sampled
monotonic curves therefore use the same policy as M23. Amount zero is exact state identity.

## Interaction and acceptance

The editor provides Select Reference, Analyze, Preview, Apply, Reset and Save Match as Look, plus
Amount/Tone/Color/Grading/Protect Skin controls. React carries only paths, scalar controls and the
compact recipe. Tests cover identical, warm/cool, low/high contrast, high/low key, portrait,
landscape, skin protection, neon, monochrome, extreme, deterministic/finite/serialized results,
category isolation and Reference Match to `.srlook` reload parity.
