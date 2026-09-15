# Starroom Complete Blueprint

Starroom is a Windows-first, local-first, non-destructive photo editor. This repository is the engineering blueprint Codex should implement before any public release.

Third-party distribution notices are in [NOTICE.md](NOTICE.md); the complete
source/version/license inventory is in `docs/17_THIRD_PARTY_PROVENANCE.md`.
The repository is GPL-3.0-or-later from M4 onward because it contains a
documented GPL-derived darktable sigmoid adaptation; corresponding source is
kept in this repository.

## Public 1.0 target
RAW and rendered-image editing, ICC-aware color management, GPU rendering, full manual/AI masks, AI denoise, optics/geometry/detail tools, presets/batch/history/compare/export, Look Engine, Style Mixer, Reference Match, and Beginner/Pro UX over one shared parameter model.

## Stack
- Tauri 2 + React + TypeScript
- Rust core
- wgpu photo renderer, DX12 preferred on Windows
- LibRaw decoder/metadata abstraction
- LittleCMS ICC abstraction
- ONNX models through a Windows ML / ONNX native bridge
- local CPU/GPU/NPU AI inference

## Codex read order
1. `AGENTS.md`
2. `codex/CODEX_START_HERE.md`
3. `docs/00_MASTER_PRODUCT_SPEC.md`
4. `docs/01_RAW_CAMERA_COLOR.md`
5. `docs/02_GPU_RENDER_ENGINE.md`
6. `docs/03_AI_LOCAL_ENGINE.md`
7. `docs/04_LOOK_REFERENCE_ENGINE.md`
8. `docs/05_MASK_ENGINE.md`
9. `docs/06_DATA_MODEL.md`
10. `docs/07_TEST_VALIDATION.md`
11. `docs/08_PERFORMANCE_TARGETS.md`
12. `docs/09_DEPENDENCY_LICENSE_POLICY.md`
13. `docs/12_TONE_COLOR_ENGINE.md`
14. `docs/13_UI_UX_SPEC.md`
15. `TODO.md`

## UI design source of truth
Before implementing the application shell or editor controls, Codex must also read:
- `design/STARROOM_DESIGN_DNA.json`
- `docs/13_UI_UX_SPEC.md`
- `docs/14_UI_MOTION_PERFORMANCE.md`
- `codex/UI_IMPLEMENTATION.md`
