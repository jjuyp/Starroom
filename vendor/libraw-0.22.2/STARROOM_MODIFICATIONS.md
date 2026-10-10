# Starroom modifications to LibRaw 0.22.2

Upstream source commit: `b93f6e45c194f5df9b02a43b1af9a54b4f41f33f`.
Selected covered-source license: CDDL-1.0; retain LICENSE.CDDL and existing copyrights.

2026-10-10, Starroom project: `src/metadata/identify.cpp` now promotes each
ForwardMatrix, ColorMatrix, CameraCalibration and CalibrationIlluminant parsed-field
bit when its value is copied from the IFD already chosen by upstream IFDCOLORINDEX.
Previously the copied matrix could have zero public presence flags; Starroom's real
synthetic sensor decode reproduced that failure. No coefficient inference, custom
TIFF parser, sensor scaling or demosaic change is added. Existing Leica pre-populated
ForwardMatrix values retain their upstream guard.

LF-normalized UTF-8 modified identify.cpp SHA-256:
`a7b9c912cab86d631f43b3125941b7e2714f0f0de5bee150bff8c55f2a239175`.

This file and the modified covered source must accompany corresponding-source
distribution. The upstream version string remains 0.22.2-Release; the Starroom
resolver/cache identity separately records `starroom-camera-profile-v5-analog-balance`.
