# Separate gamut search from ICC transform cost

2026-10-17. Instrumentation only; no pixel algorithm, dependency or cache identity
change. The previous inclusive `ColorTransform` stage remains unchanged. Its new
`OutputGamut` child measures just the existing destination proof/search call. Do
not sum the child into its inclusive parent; `totalCpuNanoseconds` remains actual
captured wall time. Old reports without this map entry remain valid legacy data.

The actual shared-graph profile regression retains its pixel-exact equality check,
requires both parent/child stages, exactly one execution each, and child duration
not exceeding parent duration. Targeted test and warning-denied Clippy pass.

## Actual measurement, not a speedup claim

MSVC Release command:
`cargo run --locked --release -p starroom-pipeline --example color_latency_probe -- --raw --prepared --edge=1024`.
Report `.starroom-reports/color-latency-1792232896259-28552.json` uses the existing
immutable Fujifilm X-Pro1 X-Trans fixture, actual RTX3050 Laptop/DX12,1024x680,
five measured changed states per control after warmup. GPU source upload/readback
and hardware timestamps are real; CPU reference is outside each timed interval.
The report was emitted from `2180079` plus only these two instrumentation-file
changes (`kernelHead:null`), not from a claimed final accepted SHA.

| Control | Engine median/p95 ms | Last inclusive CMM ms | Last gamut child ms | Last residual CMM ms |
| --- | ---: | ---: | ---: | ---: |
| Exposure | 47.8415/50.4890 | 24.9256 | 4.8941 | 20.0315 |
| Relative Temperature | 46.4421/46.9731 | 23.0913 | 1.9215 | 21.1698 |
| Relative Tint | 46.2425/47.8868 | 22.4741 | 2.3007 | 20.1734 |
| Saturation | 46.8414/50.3335 | 23.4290 | 2.5887 | 20.8403 |
| Vibrance | 46.5209/46.9021 | 22.6390 | 2.4517 | 20.1873 |
| Auto WB | 46.2541/46.7130 | 23.0565 | 2.1478 | 20.9087 |
| Neutral Picker | 49.6543/51.7215 | 22.9469 | 2.1289 | 20.8180 |
| Physical Temperature | 52.9242/54.0229 | 22.7875 | 2.0680 | 20.7195 |
| Physical Tint | 53.2749/54.3383 | 23.3176 | 2.3455 | 20.9721 |

Residual is parent minus child, not another independently captured interval; it
includes the real LCMS transform and finite checks. Last exposure hardware pass
0.129024ms versus host creative8.1842ms/readback2.9952ms. This demonstrates why
GPU-active status alone does not establish full real-time behavior, and why gamut
search should not be blamed for all24.9ms of the inclusive output stage.

All54 requests preserve CPU/GPU RGB8 delta<=1 (physical WB delta0); original source
bytes unchanged. Source preparation40hits/14builds, source uploads20/readbacks54.
These numbers exclude Native frame encoding, IPC, browser and OS presentation.
The older unfavorable >100ms outliers in doc54 remain valid evidence; this scoped
run does not erase them or pass the universal installed latency target.

## Mature upstream next-step review

The existing exact LittleCMS2.19 source pin includes an optional official GPL3
Fast Float plugin. The upstream author documents per-context installation and
specialized floating kernels: https://littlecms.com/plugin/ . It is not currently
compiled/enabled by the existing `lcms2-sys4.0.7` build. Inspection at existing pin
`21c582a594fe5279f90c0b93437c398f93bf62b0` found matrix-shaper sampled curves and
`flerp` returning its input outside the sampled interval; that is not automatically
the same as the generic engine's unbounded nonlinear TRC behavior. Therefore no
global registration, profile substitution, reduced HDR domain or precision change
was made to obtain faster numbers. Any integration must demonstrate real output
precision/HDR/ICC/Preview-Export acceptance, exact source/version and license closure.
This review is not a completed provider integration or an external blocker.
