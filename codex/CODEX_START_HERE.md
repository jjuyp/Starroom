# Codex Start Here

Read these files before coding, in this order:

1. `AGENTS.md`
2. `docs/16_OPEN_SOURCE_FOUNDATION.md`
3. `TODO.md`
4. the relevant blueprint/spec documents for the milestone
5. `docs/IMPLEMENTATION_NOTES.md`

Then implement one milestone at a time.

## Non-negotiable foundation rule
Starroom uses an **Open-source First** strategy for baseline image quality.

Before writing any RAW, tone, white-balance, color-management, sharpening, denoise, optics, perspective, grading, or calibration algorithm from scratch:

1. identify the mature open-source implementation already available;
2. check the current private-use license policy and provenance requirements;
3. prefer integration or porting through a Starroom adapter/provider boundary;
4. keep the Starroom UI and project model independent from dependency-specific APIs;
5. write reference/regression tests before changing behavior;
6. replace a mature implementation only when the Starroom version is measurably equal/better or is required for architecture, platform, performance, or licensing reasons.

A short custom implementation is not a success if it lowers baseline image quality.

CPU reference code is useful for validation and GPU parity. It must not be mistaken for the final production algorithm when a stronger mature implementation is planned.

## Preferred foundations

- RAW decode/metadata: LibRaw or another proven RAW decoder abstraction.
- ICC transforms: LittleCMS.
- Lens profiles/corrections: Lensfun.
- Exposure/tone/color calibration/color grading/detail/perspective: mature darktable implementations or validated underlying math where appropriate.
- Face landmarks: MediaPipe behind `FaceLandmarkProvider` after dependency/runtime review.
- Color-science validation: colour-science where useful as an oracle, while production runtime stays native/offline unless justified.

For direct darktable-derived code in the current private-use build, record upstream source file, revision and GPL provenance and mark `GPL-derived / private-use`.

## Starroom-owned differentiation
Spend original engineering effort primarily on:

- simple Lightroom-style UX over complex image science;
- OKLab/OKLCh selective color and hue-lock workflow;
- Adjustment Layer Manager;
- composable Mask Tree;
- transaction/history semantics;
- render graph, cache invalidation and Windows GPU scheduling;
- explainable local Semantic Advisor;
- local AI orchestration and editable AI artifacts;
- skin-retouch workflow;
- Reference Match, Look Engine and Style Mixer.

## Recommended layout

```text
app/                    React + TypeScript UI
src-tauri/              Tauri command boundary
crates/starroom-core/
crates/starroom-color/
crates/starroom-color-management/
crates/starroom-raw/
crates/starroom-render/
crates/starroom-gpu/
crates/starroom-detail/
crates/starroom-optics/
crates/starroom-geometry/
crates/starroom-grading/
crates/starroom-mask/
crates/starroom-project/
crates/starroom-export/
crates/starroom-advisor/
crates/starroom-portrait/
crates/starroom-heal/
crates/starroom-ai/
native/winml-bridge/
models/manifests/
shaders/
fixtures/
tests/
docs/
schemas/
```

## Immediate v0.2 priority
Do not expand novelty before the foundation pass is trustworthy.

Work in this order unless a blocker is documented:

1. make CI green;
2. complete the native rendered-file pipeline;
3. integrate the real ICC/LittleCMS provider;
4. replace temporary/basic tone behavior with the selected mature foundation and regression tests;
5. integrate mature sharpen/denoise foundations;
6. integrate Lensfun;
7. integrate perspective/keystone foundation;
8. complete RAW decoder/profile pipeline;
9. move validated stages to wgpu/DX12 with CPU/GPU parity;
10. then aggressively expand Starroom-specific workflow features.

Layer/Mask/Advisor/Portrait work may continue in parallel only when it does not delay foundation correctness.

## Definition of done
Do not mark a feature complete because a trait, struct, UI slider, adapter stub, or placeholder CPU function exists.

A foundation feature is complete only when the real implementation or validated production-quality alternative is connected to the shared preview/export graph, required state is serializable/reversible, regressions pass, provenance/license is recorded, and no silent fallback changes image semantics.

If external APIs have changed, use current official APIs while preserving architecture boundaries and document the change. If a dependency/model license is unclear, do not bundle it.
