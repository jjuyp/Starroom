# Starroom Open-source Foundation Strategy

## Purpose
Starroom must not sacrifice baseline image quality in order to move faster on distinctive features. Mature open-source implementations are the default foundation for solved problems in RAW, color management, tone, denoise, sharpening, lens correction, perspective, and related image science.

The product rule is:

> Stand on mature open-source foundations first. Innovate only where Starroom has a clear product advantage.

A clever UI or AI feature does not compensate for weak exposure, tone, RAW color, denoise, sharpening, or lens correction.

## Codex decision rule
Before implementing any image-processing feature, Codex must answer these questions in order:

1. Does a mature open-source project already solve this problem well?
2. Is its license acceptable for Starroom's current private-use target?
3. Can it be integrated through an adapter/provider boundary without contaminating unrelated modules?
4. Does Starroom need to modify the behavior for UX, performance, OKLCh, local AI, or workflow reasons?
5. If Starroom wants to replace the mature implementation, is the replacement measurably equal or better on the required regression set?

If the answer to question 1 is yes, do not build a weaker prototype from scratch just because it is easier to code.

## Foundation map

| Capability | Preferred mature foundation | Starroom responsibility |
| --- | --- | --- |
| RAW decode / metadata | LibRaw and/or another proven RAW decoder abstraction | normalization, camera profile resolution, render graph integration, UX |
| Exposure / basic tone | darktable exposure/tone implementations where appropriate | adapter, simplified controls, parameter semantics, regression tests |
| Highlights / Shadows / Whites / Blacks | mature darktable tone operators and published scene-referred math | Lightroom-style controls, coherent parameter mapping, Starroom-specific UX |
| Tone curve | mature spline/curve behavior from darktable plus monotone interpolation requirements | master/R/G/B UI, state, GPU parity |
| Color calibration / WB | darktable color-calibration/channel-mixer concepts plus standard color science | simplified controls, camera/profile integration |
| Color management | LittleCMS | explicit input/working/display/output boundaries, profile policy, cache |
| Color grading | darktable colorbalancergb as mature reference/port source | simpler grading UI, OKLab/OKLCh integration where beneficial |
| Selective color | darktable colorzones selection concepts | OKLCh hue/chroma/lightness model and Starroom UI |
| Sharpening | darktable sharpen and other mature open algorithms | common parameter model, GPU implementation, UI |
| Denoise | darktable profiled/classic denoise implementations where suitable | local-first runtime, simplified controls, later AI denoise integration |
| Lens correction | Lensfun | provider adapter, GPU application, cache, UX |
| Perspective / keystone | darktable ashift and proven projective math | crop/straighten/perspective UX and render-graph integration |
| Effects | darktable vignette/bloom where useful | Starroom parameterization and presets |
| Face landmarks | MediaPipe behind FaceLandmarkProvider | skin workflow, editable masks, privacy-first runtime |
| ICC validation / color-science oracle | colour-science where useful during development | production runtime remains native and offline unless explicitly justified |

## What Starroom should own
Starroom should concentrate original engineering effort on areas where product differentiation matters:

- Lightroom-style understandable UX over complex image science.
- OKLab/OKLCh selective color and hue-lock behavior.
- Adjustment Layer Manager.
- composable Mask Tree with Add/Subtract/Intersect.
- render graph, cache policy, transaction/history model, and GPU scheduling.
- explainable local Semantic Advisor.
- local AI orchestration, editable AI masks, and reproducibility metadata.
- skin-retouch workflow and provider-neutral face/skin architecture.
- Reference Match, Look Engine, Style Mixer, and protected-category logic.
- Windows-first responsiveness and local-first privacy.

## What Starroom should not do by default
Do not create a new basic algorithm merely because a short implementation is possible.

Examples of rejected patterns:

- replacing a mature denoise implementation with a generic blur;
- replacing lens profiles with a hand-tuned vignette slider;
- inventing RAW white-balance behavior without validated camera/profile math;
- implementing Shadows by adding white to RGB channels;
- creating a new perspective solver before evaluating mature ashift/projective implementations;
- labeling encoded JPEG temperature as physical Kelvin;
- declaring a provider interface complete when the real dependency/runtime is not integrated.

CPU reference implementations are allowed when they are required for validation or GPU parity, but they are not automatically the final production algorithm.

## Private-use GPL policy
The current Starroom target is private/personal use. GPL-derived darktable code may be studied, adapted, or ported for this target.

Every directly derived component must record:

- upstream project and source file;
- upstream commit/revision;
- original license;
- whether code was copied, adapted, or used only as a behavioral/reference source;
- Starroom files containing the derived implementation.

Use a comment or adjacent documentation marker:

`GPL-derived / private-use`

If Starroom is ever distributed outside private use, perform a complete GPL/LGPL/CC/model-license review before release.

## Adapter architecture
Do not embed third-party behavior directly into React UI or scatter dependency-specific calls across the application.

Preferred structure:

```text
Third-party mature engine / algorithm
        |
        v
Starroom adapter / provider
        |
        v
Typed Starroom parameters
        |
        v
Render Graph
        |
        +--> CPU reference/fallback
        +--> GPU backend
        |
        v
Starroom UI / Layer / Mask / History
```

Dependency-specific details must terminate at the adapter boundary whenever practical.

## Replacement gate
A Starroom-native replacement for a mature open-source foundation is allowed only when at least one of the following is true:

1. the mature implementation cannot satisfy the architecture or platform constraints;
2. licensing makes the integration unacceptable for the intended distribution model;
3. Starroom has a clearly better algorithm validated by tests;
4. the mature implementation is obsolete, unsupported, or incompatible with the required working space;
5. performance requires a GPU-native rewrite while preserving the validated reference behavior.

For image-quality replacements, "works" is not sufficient.

The replacement must pass a comparison suite including, where relevant:

- portrait and dark portrait;
- black clothing / deep shadows;
- white clothing / highlight detail;
- high dynamic range / backlight;
- night scenes;
- neon / saturated gamut stress;
- ColorChecker or equivalent color reference;
- fine texture;
- high ISO noise;
- lens distortion / CA fixtures;
- perspective architecture fixtures.

It must also pass identity, finite-value, extreme-control, monotonicity, clipping, and CPU/GPU parity tests as applicable.

## Development order for v0.2+
Codex should prioritize foundation quality before adding more visible novelty.

1. Make current CI green.
2. Complete native rendered-file pipeline and remove fake/placeholder semantics.
3. Integrate real color-management provider.
4. Integrate mature tone/exposure foundation and validate against regression fixtures.
5. Integrate mature sharpening/denoise foundation.
6. Integrate Lensfun.
7. Integrate perspective foundation.
8. Complete RAW decoder/profile pipeline.
9. Move validated stages to wgpu with CPU/GPU parity.
10. Only then expand advanced Starroom-specific features aggressively.

Layer/Mask/Advisor/Portrait architecture may continue in parallel only when it does not delay foundation correctness.

## Definition of done
A foundation feature is not done because a struct, trait, UI slider, or placeholder CPU function exists.

It is done when:

- the real implementation/dependency is integrated or a validated production-quality alternative is in place;
- it is connected to the same preview/export render graph;
- required project state is serializable and reversible;
- identity and regression tests pass;
- no silent fallback changes image semantics;
- performance is acceptable or a documented GPU milestone exists;
- license/source provenance is recorded.
