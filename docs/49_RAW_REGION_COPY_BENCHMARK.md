# RAW region-copy benchmark — component evidence

Measured 2026-10-11 with Windows MSVC Release `raw_crop_probe`, same executable/source,
eight alternating baseline/optimized crops. Scope: already decoded float RAW region copy,
**not complete slider/UI/100MP/installed acceptance**.

Source: immutable CC0 Fujifilm X-Pro1 RAF from the pinned RAW manifest, real LibRaw sensor
decode; output dimensions 4952x3288. Crop: centre 512x512. The baseline reproduces the previous
operation order: copy region, clone full DecodedRawImage, replace cloned RGB. The production
implementation now copies only the selected rows and clones metadata, never the full RGB Vec.
Samples, profile/metadata/timing/half-size flags are exactly equal; original file bytes unchanged.

| Measurement | Previous path | Production region-only path |
|---|---:|---:|
| Median crop time | 49.2084 ms | 2.5640 ms |
| p95 crop time | 57.6347 ms | 3.1723 ms |
| Unnecessary full-frame float clone | 195,386,112 bytes | 0 |
| Observed private process bytes at diagnostic point | 422,961,152 | 230,346,752 |

Private bytes are real Windows process observations, not VRAM or lifetime peak. The optimized
observation conservatively retains the baseline small crop as well as the new crop. PowerShell
memory sampling is outside the reported crop timing. Decode, GPU, ICC, encoding, UI publication,
whole application latency, other image dimensions and complete 100MP workflows are excluded.

Command: `cargo run --locked --release -p starroom-imageio --example raw_crop_probe`.
The example accepts a RAW path for further controlled measurements; it never writes that source.

The production crop boundary also validates source sample count with checked dimensions and
uses fallible reservation. Malformed buffers produce typed errors rather than slice panic.
Actual authored DNG crop regressions cover odd-origin rectangles, edges/full frame, immutable
source allocation, exact rows/profile/metadata and malformed sample count. The shared fixture
generator now uses existing tempfile 3.27.0 instead of timestamp-only filenames after four
parallel sensor tests reproduced a Windows AlreadyExists collision. TempPath owns cleanup.
No sensor math, profile transform, precision or downsampling changes; no new package/version.

All original color/alpha/gamut/monitor/WB, cache/tile/residency, 24/45/60/100MP memory and
end-to-end latency/Windows/Final requirements remain active.
