# M20 — Local AI Masks

## Fixed providers

M20 reuses the M16 Rust `ort` runtime. `PortraitProvider` supplies Person/Skin/Hair from the fixed YuNet/BiSeNet chain. `ForegroundProvider` runs the fixed BiRefNet v1 Swin-Tiny ONNX model for Subject and defines Background as its exact probability complement. `SemanticSceneProvider` runs the fixed NVIDIA SegFormer-B0 ADE20K export and extracts config-verified Sky class ID 2.

Weights are local-only, SHA-256 verified, Git-ignored and not packaged. The exact upstream identities and licenses are in `MODEL_PROVENANCE.md`. SegFormer is non-commercial research/evaluation only; commercial or externally distributed builds require a replacement or a new license review.

## Runtime and cache

Both M20 sessions use one `AiMaskOnnxProvider`. DirectML is requested coherently; initialization failure deliberately recreates the provider on CPU and reports the actual provider. Missing/hash/runtime/init/DirectML/inference/tensor/output/OOM/cancelled failures are typed. There is no cloud or Browser fallback.

BiRefNet preprocesses a 1024x1024 ImageNet-normalized CHW tensor. SegFormer uses 512x512 and validates 150 output classes before softmax Sky extraction. Soft probabilities remain Native R16Float-compatible rasters.

Cache identity includes immutable source hash, semantic/provider contract and model hash. Exposure, WB, Tone, Curve, Mixer, Grading and Detail therefore do not invalidate inference. Cancellation is checked around inference; pixels never enter project JSON or large JSON IPC payloads.

## Generated mask integration

`GeneratedMaskNode` serializes provider/model/version/hash, semantic class, threshold, feather, invert, cache identity and metadata. The M15 evaluator resolves the Native raster and supports Add, Subtract, Intersect and Invert, including generated-mask plus Brush or Luminance combinations. Preview, Before/After and Export use the same resolver and layer compositor.

The UI exposes Subject, Background, Person, Sky, Skin and Hair, actual DirectML/CPU status, generation/cancellation, unavailable errors and threshold/feather/invert refinement. Licensed real-model visual corpora are local provisioning; CI validates provider contracts, model identity, typed absence, soft-mask algebra, serialization and shared-graph behavior without distributing restricted weights.
