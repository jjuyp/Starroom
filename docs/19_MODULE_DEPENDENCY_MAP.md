# Module Dependency and Test Invalidation Map

This map is the fast routing layer for Codex and path-aware CI. Cargo manifests remain the executable dependency truth. A shared-stage change invalidates its downstream consumers even when their source files did not change.

| Capability | Owner / public boundary | Direct dependencies | Invalidates targeted gates |
|---|---|---|---|
| RAW | `starroom-raw` decoder/profile APIs | color management, vendored LibRaw | `raw`, `color`; shared graph when decoded output contract changes |
| Camera Color | `starroom-raw::profile` | LibRaw metadata, color management | `raw`, `color` |
| Color Management | `starroom-color-management` | `starroom-color`, LittleCMS | `color`, `raw`; shared graph |
| Tone / WB / Curve | `starroom-color` typed math, `starroom-pipeline` ordering | color management | `color`; use `tone`/`curve` locally, shared graph at Level 2 |
| Color Mixer | `starroom-color`, adapter consumed by pipeline | OKLab/OKLCh color primitives | `color` |
| Color Grading | `starroom-grading` | `starroom-color` | `color` |
| Detail | `starroom-detail` | none | `detail`; pipeline only when stage contract/order changes |
| Optics | `starroom-optics` provider boundary | none today; Lensfun planned | `optics` |
| Geometry | `starroom-geometry` | none | `geometry` |
| Masks | `starroom-project` persisted tree, `starroom-pipeline` native evaluator, `starroom-render` R16Float resource contract | core/project/pipeline state | `masks`; native layer/shared-graph and relevant image gates when compositing semantics change |
| Portrait | `starroom-portrait` local ONNX ModelRegistry/provider, `starroom-project` PortraitSemantic MaskTree leaf, `starroom-pipeline` native raster resolver | `ort`, M15 mask tree, M11 source coordinates, detail | `portrait`, `masks`, `geometry`; shared graph and web contract when the Tauri cache bridge changes |
| Healing | `starroom-heal` | detail | `detail` |
| Advisor | `starroom-advisor` | serialized suggestions only | `ai` |
| AI runtime | future typed runtime/provider crate | model manifest, local runtime | `ai`; relevant feature gate for generated editable artifact |
| AI Denoise | `starroom-ai-denoise` NAFNet provider/domain/tiler/residual contract | `ort`, detail image buffer, M13 priority conventions, M16 portrait skin raster | `ai`, `detail`, `tiles`; shared graph and web contract when settings/cache binding changes |
| Reference Match | `starroom-reference` analysis/recipe API | color OKLab/OKLCh, grading, detail image buffer | `color`, `tone`, `curve`, `ai`; web contract when workflow IPC changes |
| Portable Looks | `starroom-look` schema/blending/effects API | color curves/mixer, grading, detail | `color`, `curve`, `detail`; shared graph and web contract |
| Image I/O | `starroom-imageio` | RAW | `raw`, `web`; shared graph when decoded-source contract changes |
| Shared graph | `starroom-pipeline` | color, CM, grading, detail, image I/O, RAW | **all targeted gates** |
| Render/cache graph | `starroom-render` | stage dependency declarations | **all targeted gates** |
| Preview pyramid / tiles | `starroom-render::scheduler`, Tauri preview request boundary | render graph halo/dependency contract, wgpu resource budget | `tiles`, `gpu`; shared graph at Level 2 |
| Adjustment layers | `starroom-project` document model, `starroom-pipeline` native evaluator | typed tone/color/curve adapters | `layers`, shared graph; all image gates when blend/order changes |
| Project state | `starroom-project` | core | **all targeted gates** when serialization/common adjustment schema changes |

## CI path policy

- Leaf crate paths select the narrow gate shown above.
- `Cargo.toml`, `Cargo.lock`, common core, project state, pipeline, render graph, test-routing scripts and workflow changes broaden to all targeted Rust categories.
- Frontend, Tauri bridge, package/TypeScript/Vite/ESLint files select `web-check`.
- A push commit containing `[full-acceptance]`, an explicit workflow-dispatch Full input, or a release tag selects Level 3 Full Check.
- Documentation-only changes remain covered by path classification and repository review without consuming unrelated native runners.

This separation is a routing optimization, not permission to bypass a milestone's declared dependencies. If a public contract crosses a boundary, update this map and broaden the gate in the same acceptance change.
