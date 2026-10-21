# GPU denoise in the actual Native shared graph

2026-10-19 implementation,2026-10-21 local batch acceptance. Current-SHA remote gate
is still required. This is not whole GPU residency/direct presentation or Final acceptance.

## Mature reference and processing order

The accepted CPU edge-aware Y/Cb/Cr denoiser remains the oracle and Export stage.
The GPU backend consumes the same `starroom-detail::DenoiseParameters`, uses the
same f32 spatial weights, effective ISO/strength/radius/detail protection, clamped
neighbour addresses, dy/dx accumulation order, luminance/chroma reconstruction and
unbounded linear Rec.2020 D65 samples. No blur replacement, approximate CPU fast-exp,
8-bit intermediate, changed noise semantics or new model is introduced. The existing
wgpu30 backend and existing local detail crate are reused; provenance/lockfile license
report records the new dependency edge (no new external package/version).

The real path is input/source preparation -> existing fused creative GPU output ->
GPU NR -> ONE CPU readback -> canonical local-detail/sharpen/grain/vignette and
LittleCMS output transform. No CPU upload/readback is inserted between creative and
NR. Lazy NR pipeline/resources are retained, not created every frame. The original
creative API remains valid and does not allocate/compile NR until requested.

NR may move before the first readback ONLY when every intervening local-layer, Skin
and Healing operation is an identity. Active local processing keeps its original
CPU NR order; NR is never duplicated. Texture/Clarity/Sharpen/finishing remain after
NR. AI residual/source preparation remains before global creative. Vignette fusion
retains its existing stricter predicate, and cannot silently move ahead of active NR.
Explicit GPU errors still propagate to the existing labelled Native CPU fallback.

## Resources, identity and instrumentation

The retained creative output is not overwritten by NR. Separate NR output/staging,
uniforms and weights grow only when required under the existing adapter/storage/
dispatch bounds. NR identity contains actual creative fingerprint, dimensions and
all prepared parameters/weights, not a source path alone. Changing NR can reuse the
upstream creative result; changing source/creative invalidates the downstream result.
Off returns the exact ordinary creative result without a YCbCr round-trip.

Both compute timestamp pairs share the same final pixel mapping/submission, with no
extra map/poll/readback. Cached stages do not invent pass time. Actual NR hardware
time populates Detail/GPU-spatial fields only when available; total hardware count
includes each observed pass. The longstanding creative host-region elapsed metric
still encompasses the combined renderer call; it is NOT a pure hardware creative
duration. Hardware query totals/passes are kept distinct from that host elapsed time.
VRAM diagnostics count live buffer sizes, not physical allocator residency or RSS.

## Tests already executed

- Real wgpu NR test:60 parameter/shape cases, each with a second cached frame. HDR,
  negative values,1-pixel borders,65-wide partial workgroups, alpha, radius and separate
  luma/chroma/ISO extremes compare against the unchanged CPU oracle. f32 bound is
  `2e-5 * (1 + abs(reference))`; identity/alpha/cached frames are exact. One final
  readback per call; cache hits execute zero hardware passes; lazy pipeline count3.
- Actual photographic PNG shared-graph test: isolated NR and tone/WB/mixer/detail/
  sharpening/grain/vignette combinations match CPU Export within the unchanged
  max RGB8 delta1, including an active layer BEFORE CPU NR. Source bytes and settings
  remain unchanged. Actual hardware spatial measurement/cache is checked when supported.
- Eligibility tests keep active Skin/layers out of the early NR pass; the existing
  vignette, CPU reference and noncommuting-tail tests remain intact.
- Targeted Clippy/check pass. Actual Native performance probe additionally requires
  ten individual-control NR frames with observed hardware spatial timestamps and
  retains every original137/157 CPU/export comparison. Source-hash protection now
  includes the actual GPU implementation and WGSL, not just the Native adapter.

## Actual Native RAW performance (terminal)

Report `native-preview-rgba8-1792343560451.json`: MSVC1.97.1/RTX3050 DX12, same
read-only field RAW, interactive1024/Final JPEG96 and primed cache. Exit0, all157
actual CPU/export comparisons pass with maxRGB8 delta1, unchanged source/code hashes,
and10 individual NR frames have real hardware spatial timestamps. Previous doc69
report1792328897844 remains the before measurement; no heavy concurrent local job
ran during either timing. These are small-five-sample Native timings, not UI/IPC/OS.

| Operation | Before median/p95 ms | GPU NR median/p95 ms |
| --- | ---: | ---: |
| Chroma NR |96.350/111.393|63.153/67.302|
| NR + Sharpen |111.039/112.683|77.399/83.023|
| Tone + NR + Detail |140.603/154.663|101.027/106.016|
| ToneColor (no NR) |60.144/71.489|58.634/59.184|
| LocalDetailFinishing (no NR) |98.464/100.428|96.822/98.801|
| Exposure (no NR) |52.991/80.209|58.947/66.179|

Luma NR64.085/104.026ms includes first-use lazy pipeline cost in its samples.
Texture109.413/135.977ms, Clarity87.528/94.160ms and Dehaze88.040/100.456ms still
use the CPU detail engine. Retain the adverse/over100ms results; neither a universal
speedup nor the hard100ms target is achieved. No conclusion about the cause of every
unrelated-control timing difference is inferred from these independent runs.

First interactive50.763ms, separate prep19.001ms, warm47.984ms, Final258.410ms,
first RAW1:1=3107.553ms and cached2:1=40.475ms. Full sensor first zoom remains slow.
The original's first Fit label is historically `jpeg_first_fit_ms` even in explicit
RAW mode (1747.626ms in this run); do not mislabel that as a JPEG source measurement.

## Local batch acceptance

Full Rust `test-timing-1792597236422.json` PASS: workspace format, warning-denied
Clippy,491 registered/487 ordinary tests and doc-tests. The unchanged four opt-in
private-AI/performance/large-pixel/thumbnail cases are not claimed as ordinary gates.
The former28102 handle was missing after interruption and produced no new report;
that stopped attempt was not inferred successful or confused with the earlier SHA.
Current report includes actual photographic/RAW and all shared-graph regressions.

Focused53 frontend contract/ownership tests pass. Affected-script ESLint, JSON,
lockfile license561 Rust/6 npm/269 notices and infrastructure19/19 pass. Frontend
production source is unchanged from db082c7's224-test/full-lint/TypeScript/build
acceptance (including its recorded transient Vite failure and identical build retry);
that is explicit reuse, not a newly claimed Full Web report. Prior db082c7 Full
39147064134/secondary39147069313 SUCCESS. The new lockfile edge and license report
are synchronized, without model/private-photo redistribution.

Remaining: current-SHA remote acceptance, other GPU/CPU spatial work and cold-path
latency, full UI/IPC/presentation, memory/100MP/photographic and release qualification.
No global latency gain, universal<100ms, complete photographic quality,100MP/memory,
GPU-resident ICC/direct presentation, installer or Final claim is made here.
