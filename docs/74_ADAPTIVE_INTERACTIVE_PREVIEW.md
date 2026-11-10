# Adaptive interactive preview qualification

Status: implementation under qualification, not an installed latency or release gate.

## Scope and invariant

Presentation scheduling chooses 512/1024/2048 while dragging. Native continues to
own decoded proxy derivation, image processing, color management and output bytes.
No new image algorithm, dependency or model is introduced. Final/HQ and Export
do not use this controller. Existing Native original/derived cache lineage keeps
final output separate from interactive proxy pixels.

Each PreviewCanvas has its own controller, starting at1024. Requested edge is
bounded by actual display-pixel demand (including zoom/DPI), source dimensions and
the measured tier. Native and binary parser independently enforce2048 maximum.
CPU fallback is explicitly labelled and capped at1024; GPU permits2048 only after
sustained measured headroom. No unmeasured hardware model whitelist is used.

Two consecutive dispatch-to-draw samples at or above50ms lower one tier. Eight samples
below12ms raise one tier: four-times pixel work should still fit about48ms before
queue/scheduling overhead, but this is a policy heuristic, not a guarantee.
Neutral-band samples reset the corresponding streak. Old larger-tier, invalid and
smaller-viewport/source samples cannot train a more expensive resolution. Sustained
misses on a constrained source (for example 700px) can still downshift to 512;
misses at different edges do not accumulate toward a single downgrade. Real
status text reports received frame dimensions rather than hardcoded1024.

Existing request queue publication rules remain unchanged. The following final
render still uses authoritative requested quality. This is not permission to
reduce export resolution or claim512px has full-resolution inspection quality.

## Validation boundary

Frontend tests cover bounded startup, sustained downgrade/upgrade, CPU ceiling,
invalid samples, stale-tier isolation and anti-oscillation. Native transport and
TypeScript parser cover all three tiers with identical bytes and bounded frames.
Existing real PNG/JPEG/RAW first-drag regression passes: no repeat decode/resize,
exact authoritative Final JPEG and immutable source files.

The Native benchmark accepts explicit --edge=512/1024/2048 and records that edge;
CPU/export comparison uses the actual selected decoded tier. Existing --prime
remains explicitly1024-only so other tiers cannot falsely claim completed priming.
Native performance excludes IPC/browser/OS and must not be called full-UI latency.

Open: controlled real-photo tier timings, cross-tier perceived detail, live gesture
latency, first upgrade decode cost, larger sources/100MP, installed qualification.
No whole original phase or Final Acceptance is marked complete by this change.

## Real-source MSVC Release measurements, November 6

Separate runs on the same read-only Nikon D750 NEF (6032×4032), same 27 individual
controls and four combinations, five changed samples each. No explicit idle prime.
Frame headers are checked for size, GPU execution is required, and the same-tier
CPU/export oracle is evaluated outside the timed interval. These are native request
times, not pointer-to-present, and comparisons across edges are not equal-resolution
image quality comparisons. The photo is neither stored in Git nor redistributed.

| Native request (median / p95, ms) | 512 | 1024 | 2048 |
| --- | ---: | ---: | ---: |
| Exposure | 12.90 / 14.55 | 62.79 / 67.01 | 174.24 / 199.41 |
| Shadows | 12.12 / 12.36 | 52.27 / 61.03 | 160.81 / 161.79 |
| Sharpen | 13.34 / 79.67 | 55.56 / 142.13 | 129.97 / 324.61 |
| Luma denoise | 14.17 / 63.37 | 58.81 / 118.42 | 154.94 / 171.79 |
| Tone + denoise + detail | 24.69 / 26.80 | 94.89 / 100.46 | 220.84 / 354.31 |

Reports 1793966120772 (512) and 1793966199146 (1024) passed, each 157 comparisons,
max RGB8 channel error 1. At 512 first interactive frame was 25.495ms, but final
refine was 239.940ms and first 1:1 tile was 3170.782ms. These remain distinct costs.
This evidence motivated lowering the downshift threshold from 80 to 50ms: otherwise
common repeated exposure misses between 50 and 80ms would never select the faster tier.
Frontend full report1793966360774: 235 tests, lint/TypeScript/build, Golden and RAW
manifest validation and packaging configuration passed; not an installer runtime gate.

Initial 2048 report1793966206525 stopped at its exact cache-size assertion (expected
4 entries, observed 3). The final fit request decodes the same 2048 level; the probe
now requires the original entry to exist, forbids a duplicate derived entry at that
edge, and checks the exact number of distinct tiers. Pixel tolerances unchanged.
Corrected 2048 report1793966689003 passed157 comparisons (max RGB8 delta1), cold
interactive133.456ms. This is explicitly NOT fast enough at2048 on this device.
Across the three independent runs, 471 GPU frames matched their same-tier CPU/export
oracle. Native production code was unchanged by the last benchmark-only assertion fix.

Local Rust qualification report1793968079528 completed successfully: workspace
format, warning-denied Clippy and full workspace tests/doc tests, all exit0
(346.05s). The initial sandbox run1793967443556 could not perform atomic rename;
normal-permission run1793967554499 exposed an outdated priming test request:
it requested1800 while comparing the explicitly primed1024 source. The test
now requests1024 for the interactive phase, retains1800 for final, checks equal
pixel counts before comparing every RGBA pixel, and keeps source immutability.
This is local qualification, not installed application or final release acceptance.
