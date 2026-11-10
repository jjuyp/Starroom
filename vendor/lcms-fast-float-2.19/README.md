# LittleCMS Fast Float source

Unmodified upstream subset from https://github.com/mm2/Little-CMS/tree/21c582a594fe5279f90c0b93437c398f93bf62b0/plugins/fast_float

Version: LittleCMS 2.19, commit `21c582a594fe5279f90c0b93437c398f93bf62b0`.
Copyright (c) 1998-2026 Marti Maria Saguer. GPL-3.0-or-later; see COPYING.GPL3 and individual source headers.

Compiled statically into Starroom. Starroom's adapter is separate in `crates/starroom-color-management/src/fast_output.rs`; upstream algorithms are not modified. Preserve these notices and corresponding source when distributing GPL binaries. This plugin is not covered by the core LittleCMS MIT license.

Only exact known sRGB output profiles can select this accelerator. Input transforms and other ICCs retain the generic engine. Negative/HDR/out-of-gamut and TRC-knee pixels retain the canonical evaluator; no clamping or profile substitution. Acceptance and release qualification remain pending.

## SHA-256 of retained upstream files

- `COPYING.GPL3`: `c53a65c2fd561c87eaabf1072ef5dcab8653042bc15308465f52413585eb6271`
- `include/lcms2_fast_float.h`: `dcc6b0c9c4c743d3c4de32fd0db06ec48e5680033eb7e4f8efa4d0e777467511`
- `src/fast_16_tethra.c`: `5f6dc361bc531599b060c3a6d7cd139c94a67b1e99c4975dbaf13474747650a5`
- `src/fast_8_curves.c`: `bceb4c3a1af44446d893da7b24876035215e77967fcdd5722d43b982613b8404`
- `src/fast_8_matsh_sse.c`: `b11b4fff3e847ef0b1f912fa99f6d52fe7471c1b8bcbadc56dcd795b22a1d543`
- `src/fast_8_matsh.c`: `cf9c9c1fcc42625da0b4cd984b7d14bfbfa56c8f2a753c68201225af1637de32`
- `src/fast_8_tethra.c`: `407896f6fc05f38861956aef1166e4a2b4c3c315bfd1c378884bf60729506a67`
- `src/fast_float_15bits.c`: `bb46b398bb1df265514964e001daa3a77c01dceb94d57384da7f3fb4fb5c1482`
- `src/fast_float_15mats.c`: `2631f395a01c1e797f50eaa8545135f548d3a6506ad44781439f482bdbf0657f`
- `src/fast_float_cmyk.c`: `cc922211fa672c25d5baebe8ee5b1caeda8202b57bb802587cba58aa0de8151e`
- `src/fast_float_curves.c`: `2df0490e6a8c89db798e4e5894a75c73e66480d59a81e9b52783e1ef434b5eec`
- `src/fast_float_internal.h`: `4e966eb582e4f14dc037d842927599a7cc497ca036a32f94b654ad6e3aac039d`
- `src/fast_float_lab.c`: `a9a8a55767b779e11d493ddd9780f57ab538dc8244c3f67e91d47e3b07813560`
- `src/fast_float_matsh.c`: `4948017e68228534e416f81a0737e1c6359b005d5f5f0a3c31dd1fc0f7cc84e9`
- `src/fast_float_separate.c`: `7a430b83291d8949bda64b9b82b31728ac7de2ebe3f5e79dca600d956894bc05`
- `src/fast_float_sup.c`: `27fce1d6f6c966b26657e12ec394d32dee893324b106ab69553691883063f25a`
- `src/fast_float_tethra.c`: `3b0d1277134c2647ccbe16ae678786ef5ed79ee1b25015269e667ae7cc4d0afd`

