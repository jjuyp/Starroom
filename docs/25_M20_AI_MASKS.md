# M20 — Local AI Masks

## Fixed providers

M20 reuses the M16 Rust `ort` runtime. `PortraitProvider` supplies Face/Skin/Hair from the fixed YuNet/BiSeNet chain. `ForegroundProvider` runs the fixed BiRefNet v1 Swin-Tiny ONNX model for Subject and defines Background as its exact probability complement. `SemanticSceneProvider` runs the fixed NVIDIA SegFormer-B0 ADE20K export and extracts config-verified Sky class ID 2 and whole Person class ID 12. The October field repair corrects the old Person button, which selected facial regions rather than the body.

Weights are SHA-256 verified and Git-ignored. Only the reviewed MIT BiRefNet asset is publicly packaged; YuNet/BiSeNet, SegFormer and NAFNet use separately provisioned local files. Exact upstream identities and licenses are in `MODEL_PROVENANCE.md`. SegFormer remains non-commercial research/evaluation only and must not enter the public installer.

## Runtime and cache

Both M20 sessions use one `AiMaskOnnxProvider`. DirectML is requested coherently; initialization failure deliberately recreates the provider on CPU and reports the actual provider. Missing/hash/runtime/init/DirectML/inference/tensor/output/OOM/cancelled failures are typed. There is no cloud or Browser fallback.

BiRefNet preprocesses a 1024x1024 ImageNet-normalized CHW tensor. SegFormer uses 512x512 and validates 150 output classes before softmax Sky/Person extraction. Both semantics reuse one loaded scene session, but keep distinct cache identities. Soft probabilities remain Native R16Float-compatible rasters.

Cache identity includes immutable source hash, semantic/provider contract and model hash. Exposure, WB, Tone, Curve, Mixer, Grading and Detail therefore do not invalidate inference. Cancellation is checked around inference; pixels never enter project JSON or large JSON IPC payloads.

## Generated mask integration

`GeneratedMaskNode` serializes provider/model/version/hash, semantic class, threshold, feather, invert, cache identity and metadata. The M15 evaluator resolves the Native raster and supports Add, Subtract, Intersect and Invert, including generated-mask plus Brush or Luminance combinations. Preview, Before/After and Export use the same resolver and layer compositor.

The UI exposes Subject, Background, Person, Sky, Skin and Hair, actual DirectML/CPU status, generation/cancellation, unavailable errors and threshold/feather/invert refinement. Licensed real-model visual corpora are local provisioning; CI validates provider contracts, model identity, typed absence, soft-mask algebra, serialization and shared-graph behavior without distributing restricted weights.
