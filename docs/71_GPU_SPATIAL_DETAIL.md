# Retained GPU local-detail and sharpening

2026-10-22. Local numeric/ordinary acceptance passes. It does not qualify the complete
Production goal, installed UI latency, GPU-resident ICC or direct presentation.

## Reference and order

This is a backend port of the existing GPL Starroom detail implementation at
`17c7fe2d83d4b3612dcf90e7620b4fd8e9cc13e4`, using the same pinned wgpu backend.
The CPU Gaussian coefficient generator is now shared, without changing its math.
Texture, Clarity, Dehaze and Sharpen retain the original luminance-domain formulas,
parameter clamps, edge masking, halo protection and HDR behavior. No third-party
version, model, image, transfer function or export precision changes.

The actual shared preview graph executes creative -> NR -> local detail -> sharpen
on retained GPU buffers, followed by ONE pixel readback, CPU finishing and LittleCMS.
This early spatial chain is allowed only when intervening Mask/Skin/Healing stages
are identity. Otherwise the original CPU spatial order remains authoritative.
Export remains the CPU oracle. The GPU path suppresses only its already-applied
stages; Sharp Off explicitly sets amount=0 (the detail struct default is not Off).
The existing stricter vignette fusion predicate is unchanged.

Gaussian uses the same f32 weights, horizontal/vertical loop order and clamped
border samples. The broad local mean uses a compensated separable f32 box sum with
the same valid rectangle as the CPU f64 summed-area mean. This is mathematically
the same filter, **not bit-exact floating-point accumulation**; strict GPU/CPU
float and photographic output regressions are therefore required.

An experimental cooperative 64-pixel shared-line/halo implementation passed numeric
tests but did not improve the measured heavy combination. It was removed, retaining
the simpler contiguous dispatch and unchanged coefficients/accumulation order.

## Cache and resources

Two lazily created pipelines handle filtering and pointwise merging. Local-detail
and Sharpen retain separate output/intermediate buffers. Each blur key contains
the actual upstream fingerprint, dimensions and radii. Strength-only changes reuse
blurred pixels; source/radius changes invalidate them. Complete output keys include
all effective merge parameters. Downstream Sharpen consumes the actual Local output
identity. Original source and creative buffers are never overwritten.

Each encoded pass gets a separate aligned uniform slot. Hardware timestamps use
the existing wgpu query mechanism and share the single final pixel mapping. Cached
passes report no invented executions/time. Live buffer-byte telemetry includes
both spatial families and NR, but is not physical peak-VRAM/RSS measurement.
Errors invalidate recorded output identities, including work that failed before
submission, so a later request cannot reuse an unexecuted result.

No f16/8-bit intermediate or weakened tolerance is introduced. The additional GPU
buffers are bounded by retained family capacity and adapter storage/dispatch limits;
this is not yet the 100MP memory acceptance or real visible-tile qualification.

## Executable evidence

- 75 real-device parameter/shape cases plus cached repeats: negative/HDR values,
  one-pixel edges, partial workgroups, NR/local/sharpen order and exact alpha.
  Existing float bound `2e-5 * (1 + abs(reference))` is retained.
- Hardware query checks: cold Local=7 spatial passes; strength-only=1; upstream
  source change recomputes; cold Sharpen=5, amount-only=1, changed radius=5.
  Whole cached outputs execute zero passes, with exactly one readback per call.
- Five actual public NASA portrait shared-graph cases include positive/negative
  local controls, wide/narrow sharpening, NR, tone/color/grain/vignette and an active
  local layer before CPU detail. Preview/CPU Export max RGB8 delta remains <=1.
- Targeted Clippy and the device/portrait tests pass. The first milestone run's
  test-initialization Clippy failure is retained in local report1792598598950;
  the initializer was corrected without changing tests or tolerances.
- Native timing requires 20 actual hardware spatial frames for Texture/Clarity/
  Dehaze/Sharpen, in addition to the existing ten NR frames and 157 CPU/export
  comparisons on the read-only original.

## First real RAW measurement

Local report `native-preview-rgba8-1792598926333.json`: Windows MSVC Release,
RTX3050 Laptop/DX12, private read-only Nikon D7506032x4032 original,1024 interactive
RGBA8 and unchanged Final JPEG96.157 CPU/export comparisons pass maxRGB8delta1;
20 spatial/10 NR hardware frames pass; source and code hashes unchanged during run.
No simultaneous local heavy test was run. CPU reference comparison is outside timing.

| Control | doc70 median/p95 ms | This run median/p95 ms |
|---|---:|---:|
| Texture |109.413 /135.977|71.235 /90.113|
| Clarity |87.528 /94.160|73.625 /76.064|
| Dehaze |88.040 /100.456|72.831 /73.928|
| Local detail + finishing |96.822 /98.801|80.449 /99.215|
| NR + Sharpen |77.399 /83.023|80.453 /85.017|
| Tone + NR + detail |101.027 /106.016|108.840 /111.774|

The slower combined cases are retained. First-use Sharpen p95182.407ms and Luma NR
p95120.783ms also FAIL the100ms target; samples include lazy pipeline compilation.
This is a ready-cache Native measurement, excluding UI/IPC/presentation, not a whole
editor responsiveness claim. Additional last-frame stage profiles are being captured
to distinguish real compute, readback wait and host/CMM costs before further changes.

The second pre-tiling report `native-preview-rgba8-1792599202889.json` also passes
157 comparisons and 20 hardware spatial frames. Its heavy combination's last frame
takes 105.6501ms: actual GPU work 43.953152ms across 13 passes, GPU host scope
64.728ms (including readback wait 59.1484ms), and output color management 23.2359ms.
These timings overlap; they must not be added together. The shared-line experiment
in report `native-preview-rgba8-1792631638051.json` passes all 157 comparisons but
measures heavy median/p95 108.469/114.065ms, GPU 44.618752ms on the last frame.
No speedup was established, so that experiment was removed. The report is retained.

## Remaining acceptance

Current retained source report `native-preview-rgba8-1792666481462.json` passes
157 CPU/export comparisons (max RGB8 delta1), ten NR/twenty spatial hardware frames,
and unchanged original/code hashes. Median/p95 ms: Exposure50.802/58.082,
Texture69.803/93.910, Clarity69.762/71.634, Dehaze69.389/70.112,
Sharpen63.188/186.343, ToneColor62.254/65.745, NR+Sharpen86.293/90.498,
LocalDetailFinishing81.165/101.329, heavy108.552/116.230. Cold Sharpen and heavy
combinations remain failures, not excluded samples. First interaction57.083ms requires
separate18.993ms preparation. These are Native-only timings, not installed UI latency.

Full run1792631914428 passed format/Clippy but compiler/linker parallelism exhausted
memory. Retry with `CARGO_BUILD_JOBS=1` retained every test; report1792632127107
then exposed sandbox `os error 5` on six History/export atomic replacement tests.
The identical full gate passed with approved normal filesystem permissions in
report1792632362628: format, warning-denied workspace Clippy, workspace tests and
doc-tests all exit0. Inventory is493 tests (489 ordinary, four existing opt-in gates
unchanged). No persistence implementation, assertion or tolerance was changed for
either environment failure. Script ESLint,19-target infrastructure and JSON validation
also pass. The frontend production source is unchanged from the preceding accepted
224-test/build baseline; this is explicit reuse, not a new installed-runtime claim.

Own-SHA remote CI and complete performance/installed qualification remain required.
No installer was created and no desktop app was launched. All unrelated
color, alpha/monitor, photographic, real UI input-to-present, visible-tile, AI,
24/45/60/100MP lifecycle and Windows installed-runtime requirements remain open.
