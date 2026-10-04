# M1–M30 current-product audit

Scope: the user's original sequential M1–M30 roadmap. Historical milestone acceptance does not
establish that every UI action currently works. This document does not narrow the requested scope
to the last installer, a mock, or the subset that passes today's tests. No M31 is authorized.

Baseline: `aa5ed018ae1321bb8832169fcded37c7f67eeadc`. Its installer and full/release CI are
recorded in the dated delivery report under `output/`; they do not qualify subsequent source edits.

## Requirements to production ownership

| Milestone | Production ownership / verification focus | Current audit disposition |
| --- | --- | --- |
| M1 Native foundation | imageio, color-management, pipeline, nativeRender transport | Baseline shared graph/ICC tests; retain exact preview/export boundary. |
| M2 RAW | raw/LibRaw bridge; RAW fixture manifest | Baseline real sensor decode; no embedded-JPEG Develop substitution. |
| M3 Camera profiles | raw/profile, LittleCMS working transform | Baseline matrices, Camera Neutral, D50/D65, explicit generic profile. |
| M4 Light | color tone engine + pipeline/GPU | October tone-order, shadow and combined-control regressions retained. |
| M5 WB | color-management, pipeline sampler, WB clipboard | Baseline RAW/encoded separation, sample coordinates, undo/copy/paste. |
| M6 Curves | color spline + four-channel UI | Baseline channel state, numerical editing, presets and native parity. |
| M7 Mixer | color + target sampler | Current repair: nearest circular sampled band; inert hue-lock UI removed. |
| M8 Grading | grading + ColorWheel UI | Current repair: signed vector, neutral and captured drag/keyboard geometry. |
| M9 Detail | detail + shared spatial pipeline | Baseline identity/finite/edge regressions; latency varies by spatial stage. |
| M10 Optics | optics/Lensfun | Baseline manual identity IPC, explicit missing profile and shared resampling. |
| M11 Geometry | geometry + native sampling/display | Baseline cardinal rotation, dimensions, source-space sampling and crop overlays. |
| M12 GPU | render/gpu + hybrid pipeline | Baseline device-cache recovery and pixel parity; not universal hardware certification. |
| M13 Tiles | render scheduler, source-region decode, preview queue | Baseline actual 1:1 tiles, latest-wins and identity/cache tests. |
| M14 Layers | pipeline/project + layer UI | Baseline editable stack, local-tone contract, enable/order/opacity. |
| M15 Masks | pipeline MaskTree + mask UI/overlay | Baseline boolean algebra; thin handles and geometry mapping. |
| M16 Portrait | portrait YuNet/BiSeNet | Actual installed local weights tested; private-only licensing unchanged. |
| M17 Skin | portrait + pipeline skin stage | Actual skin masks/protected features and cold restore tested. |
| M18 Healing | heal + source-space brush | Clone/Heal baseline; reserved generative inpaint is not represented as implemented. |
| M19 Advisor | advisor + native active-state attachment | Actual current masks/Skin/AI residuals included; repaired repeat-preview/apply accumulation. |
| M20 AI Masks | portrait scene/foreground + generated-mask resolver | Current repair: real whole-person class 12, not face-only selection. |
| M21 AI denoise | ai-denoise + residual graph | Actual local NAFNet tested; model is not in public installer. |
| M22 Reference | reference + recipe transport | Baseline weighted category controls, native recipe and Look round-trip. |
| M23 Look | look + portable schema | Baseline save/load/A–B blend/effects; no camera- or source-specific mask baking. |
| M24 Library | library SQLite + query/UI | Current repair: collection scope, smart-rule IPC and persisted Edited-album filtering. |
| M25 History | history + serialized command queue | Baseline restored/canonical state and numerical debounce; audit catalog edited-state integration. |
| M26 Export | export + native batch queue | Baseline full-source atomic/cancel/metadata/ICC pipeline. |
| M27 Professional export | export high-precision codecs | Baseline real 16-bit, profile/sample round-trip and print policy. |
| M28 Performance | profiling/cache/scale tests | Measured baseline in doc 42; not proof of Lightroom performance parity. |
| M29 Desktop UX | commands/session/error UI | Baseline recovery/keyboard/offline; collection and edited-album fixes above also apply. |
| M30 Release | immutable SHA full/release + installer self-test | New batch not release-qualified until all same-SHA gates finish. |

## Open field defects / qualification gaps

- **Repaired product defect:** native Edited album now validates durable history state in a
  background query worker and applies matching IDs in SQL before pagination/Select All. The
  regression checks restart, Undo/Redo, neutral endpoint equivalence, corrupt state and empty
  subsets. No source pixels are loaded and no second edit-state database can become stale.
- **Repaired product defect:** collection/search/page/filter and off-page editor selection now
  restore after Library initialization, before autosave. Old version-1 sessions remain readable;
  corrupt/missing-collection sessions stay intact until explicit discard. Verified by seven UI
  intent tests, Native session/workflow tests and actual App browser IPC-ordering audit.
- **Repaired, release qualification pending:** sidebar album totals now use Native SQL COUNT
  over the catalog, not the visible page or loaded editor records. Edited totals use persisted
  History identities intersected with existing catalog rows. The 100k metadata regression and
  production-App synthetic IPC audit verify that a one-row filtered page still shows catalog
  totals. This is not a new installer qualification or a claim that all M1-M30 gaps are closed.
- **Product audit:** finish original requirement-to-click-path checks beyond the baseline test
  inventory, including remaining multi-asset workflow actions.
- **Repaired, release qualification pending:** cross-page export previously discarded selected
  IDs outside the loaded UI page and used neutral settings for unvisited assets. The real command
  now resolves compact Library IDs and durable History one item at a time, with an explicit active
  workspace override, per-item missing/corrupt failures and bounded shared-graph export. Tests
  cover 500-ID transport and real sensor edited/deterministic output; installed self-test uses
  the same persisted-state adapter. Remaining multi-asset copy/paste/sync paths still need audit.
- **Performance:** the expanded private AI test passed in 82.12 seconds including first cold
  scene-session startup. Profile cold vs warm inference separately; do not advertise this as
  instant AI or compare that whole-test duration with slider latency.
- **External field validation:** same-photo/same-hardware Lightroom comparison and physical
  second-machine/mixed-DPI monitor validation are not established by CI or mathematical tests.
- **Distribution:** only BiRefNet is redistributable in the current installer. This user's verified
  local YuNet/BiSeNet/SegFormer/NAFNet files are discovered automatically but not copied into a
  public "complete AI" installer. No unauthorized model replacement or runtime download.

All remaining original requirements stay in scope. This is an audit ledger, not a declaration of
perfect completeness or an excuse to stop at the first independently repairable defect.
