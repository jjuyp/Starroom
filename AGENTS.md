# AGENTS.md — Starroom Engineering Contract

## Mission
Build a professional-quality, non-destructive desktop photo editor. Optimize for correctness, image quality, responsiveness, saved-edit portability, privacy, and understandable UX.

## Invariants
1. Never overwrite source images.
2. Every adjustment is serializable and reversible.
3. Preview and export share one logical graph.
4. Full-resolution export never uses preview-resolution pixels as source.
5. Global and local edits share adjustment semantics.
6. UI code never implements color-science or image math.
7. AI outputs editable artifacts, never hidden destructive mutations.
8. Projects record schema, camera-profile and AI-model versions/hashes.
9. Avoid hard RGB clipping until explicitly bounded final output.
10. No NaN/Inf crosses a stage boundary.
11. Never silently substitute an incorrect camera/display profile or AI model.
12. Processing-order changes require regression tests.
13. Every dependency/model needs a recorded license review.
14. Do not copy proprietary Lightroom algorithms, assets, profiles, presets, or private behavior.
15. For solved foundation problems, prefer a mature open-source implementation over a weaker Starroom reimplementation.
16. A provider interface, placeholder CPU function, or UI control does not count as a completed production feature unless the real implementation is integrated and tested.
17. A Starroom-native replacement for a mature foundation must be measurably equal or better on the relevant regression set, or be required by platform, architecture, licensing, or performance constraints.

## Open-source foundation policy
Read `docs/16_OPEN_SOURCE_FOUNDATION.md` before implementing image-processing foundations.

Default policy:

- RAW decode/metadata: prefer LibRaw or another proven decoder abstraction.
- color management: LittleCMS at file/display/output boundaries.
- exposure/tone/color calibration/grading/detail/perspective: use mature darktable implementations or their validated math where appropriate for the current private-use target.
- lens correction: Lensfun.
- face landmarks: MediaPipe behind `FaceLandmarkProvider` after runtime/license review.
- Starroom-original engineering effort should focus on UX, OKLab/OKLCh workflow, Layer/Mask architecture, render/cache scheduling, local Advisor/AI workflow, Reference Match, Look/Style systems, and Windows responsiveness.

Do not write a short custom algorithm merely because it is easy when a mature implementation already solves the problem better. CPU references remain valuable for validation/GPU parity, but are not automatically the production implementation.

For directly derived GPL code in the current private-use target, record upstream project/file/revision/license and mark the component `GPL-derived / private-use`. Revisit all GPL/LGPL/CC/model obligations before any external distribution.

## Boundaries
React/TypeScript owns UI and interaction. Rust owns project state, image graph, mask graph, color, RAW, GPU scheduling, caching and export. A narrow native Windows AI bridge may wrap Windows ML/ONNX APIs behind a stable C ABI.

Third-party engines must normally enter through typed Starroom adapter/provider boundaries. Do not scatter dependency-specific code through React or unrelated crates.

## Precision
- RAW and CPU reference: f32
- GPU RGB preview: RGBA16Float by default
- GPU mask: R16Float by default
- Perceptual color: OKLab/OKLCH
- authoritative edit state is never 8-bit

## Color
Internal baseline is unbounded linear wide-gamut RGB using Rec.2020 primaries/D65. ICC/LittleCMS is used at file/display/output boundaries. D50/D65 adaptation must be validated.

## RAW
LibRaw is a decoder and metadata source, not Starroom's final rendering engine. Starroom owns normalization, demosaic-provider abstraction, camera profile, working-space conversion and creative processing.

## GPU
Photo rendering uses wgpu. Windows prefers DX12. Critical stages have CPU reference/fallback paths. GPU rewrites must preserve validated reference semantics and gain CPU/GPU parity tests before replacing the reference path.

## AI
Core AI is local. Runtime priority is hardware-optimized Windows ML EP when validated, then compatible GPU path, then CPU fallback. Model ID/version/SHA/license/input/output/precision/benchmark are mandatory metadata.

## Render invalidation
Each stage declares dependencies, parameter hash, halo, cache key, CPU/GPU availability. Invalidate only changed and downstream stages.

## Definition of done for foundation features
A foundation feature is done only when the real implementation or a validated production-quality alternative is integrated into the shared preview/export graph, state is reversible/serializable where applicable, regressions pass, provenance/license is recorded, and no silent fallback changes image semantics.

## Codex
Implement one milestone at a time. Run tests/lints after each milestone and record deviations in `docs/IMPLEMENTATION_NOTES.md`.

Start a milestone by reading this file and `codex/CURRENT_TASK.md`, then load only the listed modules and the referenced implementation-map section. Escalate tests progressively: targeted Level 1 during development, relevant Golden subset plus Level 2 at milestone acceptance, and Level 3 Full Acceptance only for batch/release/final acceptance. `npm run test:<target>` is the canonical local entry point; a commit containing `[full-acceptance]` opts a push into the authoritative Full Check.

Keep development commits focused on code plus tests. Synchronize TODO, implementation notes, provenance and roadmap in the milestone acceptance commit, or earlier only when a dependency/license or architecture decision actually changes.

Before coding a foundation feature, check `docs/16_OPEN_SOURCE_FOUNDATION.md` and explicitly decide whether to integrate, adapt/port, or replace the mature upstream solution. Do not invent a new baseline algorithm without documenting why.
