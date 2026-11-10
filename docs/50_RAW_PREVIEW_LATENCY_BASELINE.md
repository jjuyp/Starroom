# Physical RAW preview latency — Native component baseline

Measured 2026-10-12. Kernel HEAD: `9c836957fe62bddaf8b988b52b360f5a270fbf5d`.
**Not pointer-to-present, installer acceptance or a complete 24/100MP performance gate.**

The existing Native latency probe now also supports the license-cleared Fujifilm X-Pro1
X-Trans sensor fixture, configurable preview edge, explicit physical Temperature/Delta-uv,
source-byte immutability, the unchanged CPU/GPU RGB8 tolerance and an exclusive JSON report.
No processing algorithm or preview resolution was changed by this instrumentation.

Command: MSVC release `color_latency_probe --prepared --raw --edge=1024`, with
`STARROOM_BENCHMARK_HEAD` set to the actual kernel SHA. Local report:
`.starroom-reports/color-latency-1791769362374-75416.json`.

Adapter: NVIDIA GeForce RTX 3050 Laptop GPU / DX12. Preview: 1024 x 680.
Six renders per control; the first is warm-up, five are measured. P95 is the maximum of
that small five-sample set, not a high-confidence population percentile. CPU reference render
and source-byte checks are outside the measured accelerated interval. Decode measured
263.2524 ms after the immutability read (OS file cache may be warm), not uncached disk startup.

| Control | Median ms | Five-sample P95 ms | Max CPU/GPU RGB8 delta |
|---|---:|---:|---:|
| Exposure | 57.5883 | 58.4726 | 1 |
| Legacy Relative Temperature | 56.8747 | 57.5393 | 1 |
| Legacy Relative Tint | 56.7237 | 57.5395 | 1 |
| Saturation | 58.1960 | 58.8584 | 1 |
| Vibrance | 57.6907 | 58.1431 | 1 |
| Auto WB (same parameters) | 56.6263 | 57.2105 | 0 |
| Moving Neutral Picker | 59.9386 | 60.3181 | 0 |
| Physical Temperature | 65.3407 | 67.1443 | 0 |
| Physical Delta-uv | 65.5568 | 66.5429 | 0 |

An earlier independent invocation measured physical Temperature/Tint medians
64.5451/64.2333 ms, demonstrating ordinary run variation rather than an optimization gain.

## Actual bottleneck evidence

The final physical frames spend 22.0640/22.0709 ms in the CPU LittleCMS output boundary;
their source WB/profile preparation spends 4.3867/4.0642 ms. Physical Tint's measured GPU
readback wall time is about 3.8 ms. These are recorded component/wall intervals, not fabricated
hardware GPU timestamps. Therefore optimizing only the camera matrix cannot account for the
whole 65 ms render, much less prove end-to-end desktop latency.

The 54 requests build source preparation 14 times and hit 40 times; four retained entries
hold 33,423,360 RGB-buffer capacity bytes with ten evictions. GPU reports 33,456,640 allocated
buffer bytes, 54 final readbacks and 20 source uploads. ICC cache builds once and hits 107
times. Buffer capacities are not physical VRAM or whole-process peak-memory acceptance.

Future comparisons must keep this legal source, edge, release profile, controls, five-sample
warm-up policy and original RGB8 tolerance. Any optimized result needs its own kernel SHA and
measured report. Full common-control input-to-present, actual 24/100MP workflows, timestamp
profiling/direct presentation and complete IQ/Windows gates remain required.
