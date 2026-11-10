# Preserve scene-linear RAW values during preview resampling

2026-10-17. Correctness repair before preview-pyramid/cold-tier optimization.

## Actual counterexample and foundation selection

The pinned image0.25.10 Float RGB Lanczos3 path bounds its final samples to0..1.
`cargo run -p starroom-imageio --example raw_resize_range_probe` retains the actual
upstream counterexample: constant8x8 RGB[-.25,1.5,4] becomes2x2 RGB[0,1,1]. Existing
`resize_raw_preview` used that path after sensor data had entered linear Rec.2020/D65.
This was intermediate destructive clipping, not an output-profile policy.

Integrate/adapt, not replace: `starroom-imageio/src/unbounded_resize.rs` adapts mature
image-rs sinc/Lanczos3 vertical-then-horizontal resampling at commit
76e57184f22772dad1138e96954e57945406b15e. Source file/version/hash/MIT choice and binary
redistribution are recorded in provenance/notices; full permission text is retained.
No new upstream package/version, model, library license or network runtime is added.
The existing pinned Rayon becomes an imageio dependency edge only.

## Production path

Real LibRaw sensor decode -> existing WB/camera transform -> unbounded working RGB
-> adapted Lanczos3 -> unchanged shared Native Preview/Before-After graph.
Full export still independently decodes original sensor pixels; it never uses a proxy.
LibRaw normalization, camera resolver/matrices, metadata, decode quality and working
space are untouched. No encoded JPEG/PNG/TIFF resampling or alpha policy is changed.

The adapter retains original filter weights, f32 accumulation and pass order. It
removes only the bounded RGB final clamp; shared precomputed axis weights/independent
row scheduling do not change each sample's arithmetic. Checked dimensions and fallible
buffers produce typed allocation/shape/finite errors; UI categorizes memory failure
before the enclosing RAW message without discarding diagnostic detail.

`RAW_DECODE_POLICY_VERSION` becomes `starroom-libraw-v8-unbounded-preview-resample`.
Existing Native cache, thumbnail and recipe identities invalidate old derived pixels
explicitly. No original photograph is written or stale cache silently reused.

## Evidence

- Three new units: upstream bit-exact bounded results over six odd/down/up/identity/
  single-axis shapes; negative/HDR/exposure scaling; shape/finite/signed-zero identity.
- Existing authored64x64 DNG, real LibRaw half decode then actual imageio16x16 preview:
  working blue remains>3, metadata/half-size state match, all finite, source unchanged.
  Manifest adds `unboundedPreviewResampling`; no new photographic coverage is claimed.
- RAW/shared/actual photographic/WB/GPU/render milestone1792236627764 passes, including
  all75 pipeline units,39 render units, format and warning-denied workspace Clippy.
  Separate sampler units and31 error-presentation tests pass; future RAW target includes
  sampler units explicitly. Existing Golden/RAW hashes and thresholds are unchanged.
- Actual MSVC Release RAW1024x680 on RTX3050/DX12, report
  `color-latency-1792236711783-13592.json`: source unchanged,54 CPU/GPU comparisons,
  maximum RGB8 delta<=1. Nine control medians45.839–53.152ms, largest p9558.990ms;
  decode220.912ms. These are Native engine measurements, not IPC/input-to-present.
  Exposure median46.717/p9549.567ms; physical temperature52.893/57.848ms,
  physical tint53.152/53.831ms. Earlier slower observations remain retained.
- Full Rust1792237107318 passes format, warning-denied workspace Clippy, tests/doc-tests:
  466 registered,462 ordinary executed,four explicitly opt-in excluded. Full Web
  1792236859080 passes218 tests/30files, lint, TS/production build, JSON/Golden and
  packaging configuration. License verification passes561Rust/6npm/269notice texts.

This repair deliberately changes previously clipped RAW preview pixels. It does not
claim old/new pixel identity, recovered detail from sensor saturation, Lightroom IQ
equivalence, complete photographic HDR/negative-gamut quality, alpha correctness,
monitor management, GPU direct presentation, first-drag cache performance,100MP memory,
installed Windows runtime or Final acceptance. Own remote acceptance remains pending
until this group's commit/SHA reaches terminal success.

Remote ordinary acceptance:925c72124ff01bd72581152b0a802ba410ade710 Full Check
https://github.com/jjuyp/Starroom/actions/runs/39030002986 and secondary39030007692
both SUCCESS, verified2026-10-18. PR#2 remainsOpen/Draft; no Final release is implied.
