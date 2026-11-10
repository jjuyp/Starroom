# Native Preview / Export deterministic grain binding

2026-10-18. Production counterexample found by actual combined-control validation,
not a request to relax tolerances or replace image science.

## Failure and cause

Explicit `--combined` extends the existing real-image Native probe with four groups,
five changed settings each: ToneColor, DenoiseSharpen, LocalDetailFinishing and
ToneDenoiseDetail. The original27 controls remain. Every local interactive frame is
compared against the actual CPU/export graph outside the measured interval; expected
comparison count is157, otherwise137 without this opt-in flag. Source/key-code hashes,
actual GPU flags, binary frame format and immutable-original assertions stay enforced.

Failed report`native-preview-rgba8-1792298907070.json` is retained: LocalDetailFinishing
first frame differed by8 RGB8 levels. The reference omitted runtime source identity.
Inspection also found a real production defect: Preview's `attach_ai_denoise` always
bound `settings.image_identity`, even with AI disabled, but the professional Export
adapter only called it with AI enabled. Ordinary AI-disabled Export therefore used
the portable default empty identity for deterministic grain. Shared math alone did
not guarantee the same inputs to the finishing stage.

## Production repair

One `source_bound_render_settings` validates portable edit intent and binds the
existing Native source identity. Both actual `native_preview_inner` and the actual
professional batch Export settings adapter call it, independent of AI enablement.
The benchmark reference now uses this same required runtime boundary instead of
unbound portable settings. No threshold is relaxed; all pixel comparisons still
require max RGB8 difference<=1, and actual CPU PNG comparison remains exact.

Preview grain appearance is preserved. Export now receives the matching seed input.
The existing path/size/mtime identity policy is deliberately not redesigned here;
content-stable grain across relocation is a separate limitation. Runtime local paths
are not serialized into portable edits/presets. No random seed, weaker grain kernel,
dependency/model or source-photo mutation is introduced.

`EXPORT_ENGINE_VERSION` becomes`starroom-export-v3-source-bound-grain`. Existing
recipe hashing includes this version, so corrected output cannot reuse a v2 recipe
identity. Container/profile/metadata/atomic/cancellation policy is unchanged.

## Regression evidence

- New runtime-binding test verifies deterministic binding, unchanged grain parameters,
  AI-disabled behavior, invalid-state rejection and absence of runtime source identity
  in portable serialized edits.
- Original three actual portrait binary/decoded-ICC-PNG comparisons remain; a fourth
  adds Grain + Texture + Vignette. It verifies exact Native display/PNG bytes, repeated
  frame identity, separate interactive/final transport and immutable source hash.
- The fourth case also renders the old unbound settings and explicitly requires a
  difference>1 from the corrected output. This retains a real pixel counterexample;
  it does not merely compare two wrappers that share the same mistake.
- Those actual Native tests, MSVCRelease frozen finishing oracle, targeted strict
  Clippy and Node syntax validation pass. Corrected actual RAW report
  `native-preview-rgba8-1792325829982.json` passes157 frames, maxRGB8delta1, unchanged
  original/key-source hashes and actual DX12 flags. No tolerance changed. All27
  individual controls and four combinations retain their measured data. Broader
  Full Rust1792326254186 passes478registered/474ordinary/four opt-in excluded,
  workspace Clippy/format/tests/doc-tests. Export1792325961859 passes27 ordinary
  cases;52 selected frontend Native contract/ownership tests, JSON, license561/6/269
  and infrastructure pass. Frontend source/locks unchanged from37387e3's Full Web222
  baseline; own same-SHA remote Full Web/Rust must still succeed.

| Corrected actual Native combination | Median ms | p95 ms |
| --- | ---: | ---: |
| ToneColor |58.761|60.373|
| DenoiseSharpen |107.894|110.939|
| LocalDetailFinishing |96.761|97.159|
| ToneDenoiseDetail |130.348|134.703|

Exposure50.542/50.812ms, LumaDenoise93.459/96.022ms; ready-cache first53.367ms,
separate prep19.029ms, warm56.348ms, Final295.658ms, firstRAW1:1=3069.470ms and
cached2:1=33.440ms. Compared with doc66's prior independent run, warm/refine and
some control intervals regress. No universal speedup or causal effect from the
grain binding is claimed; the binding repairs correctness, not filter throughput.

The failure run also measured DenoiseSharpen105.622/107.377ms (median/p95), while
ToneColor58.119/59.796ms. Keep this evidence: single-control<100ms does not prove
combined-tool, whole UI/IPC/OS, first RAW zoom or broader performance acceptance.
No Final/installer, full photographic quality,100MP or complete phase is claimed.
