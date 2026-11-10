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

2026-10-11, Starroom project: existing TIFF/DNG metadata parser now retains exact
CameraCalibrationSignature/ProfileCalibrationSignature bytes with its existing tracked
memory manager. ASCII/BYTE UTF-8 identifiers compare byte-for-byte, without normalization
or truncation. Selected IFD metadata is promoted before camera coefficient construction;
both single-IFD and selected dual paths omit CC on mismatch. The narrow adapter transmits
the match result, not potentially large identifier strings or borrowed pointers.

Malformed type/termination/embedded-NUL/short-read metadata is rejected; an explicit 4MiB
per-signature resource limit rejects excessive data instead of silently equating prefixes.
No alternate TIFF reader, demosaic or camera-matrix algorithm is introduced. Coverage is
the currently selected primary profile; complete ExtraCameraProfiles/iterative-white/n>3
and photographic quality qualification remain separate requirements.

Current LF-normalized UTF-8 covered-source SHA-256 values:

| Covered source | SHA-256 |
|---|---|
| src/metadata/identify.cpp | 5242c2faf1b2f0d4ae379268c7860def9cb0934ec46dd14124771bbc558c3bf0 |
| src/metadata/tiff.cpp | b314fedad34262167b2978cde6221fe1d5075ce427d9c5a584bdb89fac269c09 |
| libraw/libraw_const.h | 4124a54e66e96e70324922713987ec16c413cf349f64d9b19f980df2f9e58189 |
| libraw/libraw_types.h | d7b8c4dfdcfb97fa29bc9d45de0a0a4c3a7291c0beed6515c1d282823e7bf44e |

This file and the modified covered source must accompany corresponding-source
distribution. The upstream version string remains 0.22.2-Release; the Starroom
resolver/cache identity separately records `starroom-camera-profile-v7-calibration-signatures`
and `starroom-libraw-v4-dng-calibration-signatures`.
