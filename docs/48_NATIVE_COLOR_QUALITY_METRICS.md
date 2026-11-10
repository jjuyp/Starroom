# Native color-quality metrics — component evidence

Status: measured 2026-10-11; **not complete photographic IQ or Final Acceptance**.

`starroom-color-management::quality::compare_xyz_d65` validates finite inputs, delegates
D65->D50 adaptation/Lab/CIEDE2000 to the pinned LittleCMS provider and accumulates XYZ RMSE
and absolute Y error in f64. It does not alter image pixels or replace the render graph.
XYZ is reference-white-relative; these values are not absolute HDR appearance measurements.

The five existing camera-profile tests execute actual Native resolver outputs against the
unchanged BSD-licensed BabelColor average 24-patch numerical fixture. Original per-component
1e-5/2e-5 assertions remain. Additional gates: CIEDE2000 < 0.01, XYZ RMSE < 2e-5, Y error < 2e-5.
Three metric boundary tests prove exact identity/repeatability, an independently expanded
achromatic lightness case/symmetry, and typed non-finite rejection.

Historical v7 maximum per 24-patch case (single-threaded actual test command):

The dual midpoint row predates v8 CameraNeutral-driven inverse-white solving. It is retained
as historical evidence, not a current v8 result. The v8 chart uses physically consistent
source-white/camera-neutral coordinates and unchanged strict thresholds.

Current v8 replay on 2026-10-11: all five tests/120 comparisons pass. The four non-dual
rows reproduce the values below; the dual 2000K white/CameraNeutral case measures maximum
CIEDE2000 0.0000478541744, XYZ RMSE 0.000000217645390 and Y error 0.000000119209290.
This is a different physically consistent authored domain, not a photographic quality
improvement inferred by comparing the old and new chart numbers.

| Authored profile/domain oracle | CIEDE2000 | XYZ RMSE | Absolute Y error |
|---|---:|---:|---:|
| D65 ColorMatrix | 0.0000151412 | 0.0000000344128 | 0.0000000596046 |
| AnalogBalance camera coordinates | 0.0000151412 | 0.0000000344128 | 0.0000000596046 |
| Dual CC/CM midpoint | 0.0000534510 | 0.000000233399 | 0.000000178814 |
| ForwardMatrix calibrated baked-WB | 0.000112630 | 0.000000243335 | 0.0000000596046 |
| D50 ForwardMatrix reference | 0 | 0 | 0 |

Command: `cargo test --locked -p starroom-raw --test colorchecker_profile -- --nocapture --test-threads=1`.
All five cases/120 patch comparisons passed. Zero means equality to that authored mathematical
reference, not perfect real-camera color reproduction. The existing actual full/half DNG sensor
and shared graph tests remain separate checks; none of these rows is a photographed camera chart.

The original metric-only batch changed no dependency or processing stage. The v8 replay follows
the separately recorded camera-domain repair and test-only lock identity update; output
quantization and BabelColor fixture bytes are unchanged. Source is recorded in provenance.
Complete 18-category photographic
metrics, clipping/continuity/precision, real RAW lighting/skin/ISO IQ, displayed-preview parity,
monitor/output/gamut/alpha, physical WB, latency and same-SHA Windows release gates remain open.
