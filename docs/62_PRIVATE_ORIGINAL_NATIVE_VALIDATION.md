# Real field-original Native validation, without redistribution

2026-10-18. Supplemental local evidence; no private photograph/model is committed,
copied into fixtures, uploaded or made an official public CI input.

## Executed production path and safeguards

The existing MSVC Release27-control opt-in probe now accepts an explicit local
original plus authoritative dimensions. It checks actual Native frame dimensions,
requires lossless RGBA for pixel comparison, and reads/hash-verifies the source.
Synthetic fixture creation is in a separate branch: local mode never creates or
replaces the source. Reports omit private paths/filenames and remain Git-ignored.
The default generated24MP mode and public fixtures remain independently repeatable.
The same compiled binary's generated-mode replay1792294151200 passes all27 changed
controls and the new cold-tier field; first105.916ms is retained as another target
failure, not substituted for the earlier112.460ms observation.

Each local run compares137 real Native interactive frames (first/warm exposure plus
27 controls x five changed states) against the same source-proxy CPU/export graph.
CPU references are outside the measured Native interval. Actual binary RGB/alpha is
checked, not a mock renderer or only a shader compilation. Native GPU-path flags
must be present; fallback is not accepted as a GPU timing result. Original hashes
are checked by both the Rust probe and outer runner. No whole-app launch is needed.

Actual field-source paths were resolved via the existing Library in read-only mode;
they are not added to this report. Their hashes/dimensions/format are in local reports.
They remain private user inputs, with no assertion of public redistribution rights.
The six CC0 RAW and five public photographic fixtures are unchanged.

## Same compiled binary, sequential real RAW/JPEG results

Nikon D750 RAW6032x4032 and corresponding JPEG6016x4016 from the reported scene.
MSVC1.97.1, NVIDIA RTX3050 Laptop / DX12; currentcd5d8d3 plus diagnostic-only changes.
Reports: RAW`native-preview-rgba8-1792293970819.json`,
JPEG`native-preview-rgba8-1792294043135.json`. Both have changedDuringRun:false,
failure:null,137 compared frames, max RGB8 delta1 and immutable source hashes.

| Native request measurement | RAW ms | JPEG ms |
| --- | ---: | ---: |
| First interactive | 67.443 | 103.773 |
| First-interactive proxy resize | 14.299 | 29.944 |
| First-interactive input transform | already working RAW | 16.291 |
| Warm changed exposure request | 48.909 | 51.784 |
| Exposure median / p95 | 50.626 / 51.119 | 52.930 / 54.646 |
| Shadows median / p95 | 48.867 / 49.434 | 52.385 / 55.110 |
| Whites median / p95 | 48.799 / 48.913 | 51.161 / 51.935 |
| Blacks median / p95 | 48.870 / 49.536 | 50.647 / 51.364 |
| LumaDenoise median / p95 | 107.017 / 110.029 | 107.313 / 107.788 |
| ChromaDenoise median / p95 | 105.323 / 106.757 | 109.128 / 110.297 |
| First Native Fit | 2708.223 | 2716.246 |
| Final refine | 262.592 | 265.489 |
| First 1:1 viewport | 2981.589 | 254.517 |
| Subsequent 2:1 viewport | 32.860 | 162.503 |

The first attempted RAW run1792293708335 is retained as a failed diagnostic: the old
JPEG-only test expected three decode-cache entries; a RAW1:1 viewport correctly adds
a separate full sensor tier. The assertion now requires exactly four for RAW/three
for encoded inputs and independently verifies full-tier presence only for RAW.
The source hash remained unchanged. No pixel threshold or product test was removed.

## What this proves, and what remains

This proves the actual local originals execute through the new Native interactive
cache and preserve CPU/GPU/shared-export pixel agreement over these controls. It does
not prove absolute RAW rendering IQ against camera JPEG/Lightroom, physical WB across
all cameras, every local/AI tool, whole UI pointer-to-present,100MP, display/alpha
ownership or Windows installation/runtime acceptance. References use the exact same
interactive source, not a claim that proxies equal original-resolution pixels.

**Universal100ms still fails**: JPEG first interaction, both denoise cases, JPEG zoom,
Final refine and first RAW1:1 are explicit remaining costs. The first RAW1:1 incurs
full sensor decode; later views can reuse it. Do not describe these numbers as whole-
product realtime or hide the3-second viewport behind warm32.860ms. No new installer,
Final version/tag, public fixture license or PR merge is implied.
