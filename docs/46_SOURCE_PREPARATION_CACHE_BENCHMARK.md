# Real source-preparation pixel reuse — 2026-10-10

Scope: existing licensed NASA portrait at 512px, release Native shared graph on RTX 3050 Laptop
/ DX12. Both modes use the SAME executable: without flags = original uncached graph;
`--prepared` = actual prepared pixel-stage cache. CPU export reference runs outside the timed
GPU interval. Five samples after warm-up per control; nearest-rank p95 is the maximum of five.
Auto WB repeats one unchanged intent; all other controls change parameters. Not UI/IPC/JPEG
presentation timing or 24/100MP acceptance. Hardware load and clock variability remain.

| Control | Original median ms | Cached median ms | Original p95 ms | Cached p95 ms | Max CPU/GPU RGB8 delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| Exposure | 34.4108 | 32.2243 | 36.6494 | 40.8821 | 1 |
| Temperature | 32.5997 | 27.4679 | 34.1048 | 29.1149 | 1 |
| Tint | 32.8340 | 24.8647 | 33.8793 | 26.6007 | 1 |
| Saturation | 33.9896 | 25.4363 | 35.8638 | 25.9286 | 1 |
| Vibrance | 32.6680 | 24.2689 | 35.1638 | 25.3011 | 1 |
| Auto WB | 33.0462 | 23.7194 | 35.3289 | 24.0208 | 0 |
| Neutral Picker | 38.9724 | 30.7749 | 43.8502 | 31.1976 | 0 |

Exposure p95 regressed in this small batch; do not claim every tail improved. Actual workload
reuse is proved independently by counters: 2 builds, 40 hits, 2 misses, 2 entries, no evictions
and 6,291,456 retained RGB-buffer bytes. Before splitting visible picker from source/geometry,
the same 42-request probe built 8 preparations and retained 12,582,912 bytes. Final picker
recomputes its measured CAT on a mutable working copy without recreating camera/source WB,
lens or geometry results. Caching never reuses a corrected picker result for another rectangle.

Implementation: `SourcePreparationCache` compares immutable decoded Arc allocation identity
(including all pixel/ICC/camera metadata) plus exact actual preparation settings: input intent/
BPC, source WB mode, active optics, geometry and source-region coordinates. Weak source refs
cannot pin decoded pixels or match a recycled allocation. Disabled optics/unused sample fields
do not create false dependency changes. Preparation is performed outside the mutex. The cache
retains actual f32 Linear Rec.2020 images and semantic coordinate maps, not only keys or traits.
Native CPU/GPU preview BOTH call it; uncached export invokes the same preparation and downstream
shared graph on full-resolution source, never preview buffers.

Limits: four entries / 128 MiB actual retained RGB Vec capacities. Metadata/caller working
copies/decoded cache/GPU storage are not included in that byte metric. Source estimates over
budget bypass retention through the unchanged owned graph without adding a cached full-frame
copy; actual oversized prepared allocations are consumed directly as well. This policy alone
does not prove physical 100MP peak-memory or stress acceptance.

Profiler stage hits are explicit, with zero fabricated executions/time. RAW source is already
working RGB from the decoder; its warm preparation does not invent an ICC/camera transform
execution. Source-WB reuse and per-render visible picker execution remain distinguishable in
the existing WB aggregate. Real portrait/RAW/rotation/Lensfun tests retain exact CPU/export
equality and <=1 GPU RGB8-code tolerance. Actual Native binary-preview requests prove one
preparation build across three different Exposure states; originals are unchanged.

Remaining Phase 9 work: finer downstream tone/color/local result caches, efficient ROI decoded
source identity, dirty/prefetch tile execution and GPU presentation. RAW WB/alpha/gamut/monitor,
perceptual scene coverage, end-to-end slider latency and same-SHA Windows release gates remain
open. No image algorithm, order, precision, dependency, fixture or third-party license changes.

Reproduce: `cargo run --locked --release -p starroom-pipeline --example color_latency_probe`,
then the same command with `-- --prepared`. Do not run timings concurrently with tests/builds.
