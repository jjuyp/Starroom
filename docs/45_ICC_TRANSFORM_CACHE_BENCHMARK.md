# Bounded LittleCMS transform reuse — 2026-10-06

Scope: existing licensed NASA portrait, decoded at 512px (262144 pixels), release Native GPU
shared graph on NVIDIA RTX 3050 Laptop / DX12. Not desktop input-to-present, 24MP or 100MP.
Baseline source `9e23d85`, with identical profiler instrumentation added before measurement.
CPU export reference executes outside the measured GPU interval. Each control omits warm-up
and records five samples; p95 is the maximum of those five samples, not a large-corpus tail.
Auto WB repeats unchanged intent; other controls change their parameters.

| Control | Before median ms | Cached median ms | Before p95 ms | Cached p95 ms | Maximum CPU/GPU RGB8 delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| Exposure | 37.6833 | 34.9977 | 42.2545 | 37.0446 | 1 |
| Temperature | 36.9963 | 28.7923 | 38.2972 | 29.8003 | 1 |
| Tint | 40.8675 | 34.0602 | 43.7726 | 35.4991 | 1 |
| Saturation | 41.0279 | 29.6084 | 43.5297 | 30.3732 | 1 |
| Vibrance | 42.3961 | 35.0137 | 50.1843 | 45.0554 | 1 |
| Auto WB | 42.3361 | 34.3552 | 49.0226 | 45.0840 | 0 |
| Neutral Picker | 37.9644 | 31.1395 | 41.6435 | 31.6067 | 0 |

The final Exposure sample's profiled input/output stages are 5.7145/11.6258 ms before and
5.8885/10.6227 ms after. These are single stage samples, not stage medians. The input sample
did not improve; object reuse does not remove per-pixel conversion work. Timings include
ordinary process-memory observation; hardware load/clock variability remains. The table is an
observed small same-machine batch, not proof of universal acceleration or Lightroom parity.

Current dedicated-process counters: 2 builds, 166 hits, 2 misses, 2 retained transforms, zero
evictions and 3144 retained profile-key bytes. Unlike a parameter-key-only stub, real LCMS
transform objects are reused by both `input_to_working` and `working_to_output`. Pixel execution
continues on the existing mature NO_CACHE RGB_FLT transform and Rayon chunks. NO_CACHE disables
LittleCMS's mutable one-pixel cache, not Starroom's lifetime reuse of the immutable transform.

Retention limits: eight transform objects and 2 MiB of exact ICC key bytes. Native transform/CLUT
storage and active caller references are not included in that byte counter. Oversized valid ICCs
execute normally without retention; invalid ICCs remain typed errors, never substitutes. Cache
identity includes direction, exact optional ICC bytes, rendering intent and black-point
compensation. This fixed working-space/format provider has no runtime plugin/adaptation mutation.
Profile parsing/building and pixel conversion run outside the cache lock through the
pinned safe wrapper's documented Send/Sync support for DisallowCache transforms. No new unsafe
implementation, image algorithm, output clamp, precision reduction or dependency is introduced.

Four new unit regressions prove exact key separation/reuse, LRU/count/key-byte bounds,
invalid/oversized behavior, cold-build race deduplication and concurrent equality with the
uncached reference. Real photographic
Golden, RAW/shared graph and ICC tests retain their existing tolerances. Full final production
acceptance remains open: prepared pixel-stage reuse, RAW WB/alpha/gamut/monitor, GPU presentation,
full UI latency, 100MP and same-SHA installed/offline Windows gates are not closed by this repair.

Reproduce with `cargo run --locked --release -p starroom-pipeline --example color_latency_probe`;
the JSON reports actual profile stages, GPU resources and ICC object-cache counters. Do not run
the timing comparison concurrently with builds or regression workloads.
