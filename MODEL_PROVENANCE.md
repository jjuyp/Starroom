# Local AI model provenance

This file is the M16 source-of-truth for model identity, privacy and redistribution review.
Starroom performs portrait inference locally through the Rust `ort` adapter; it makes no cloud
request, uploads no image/landmark/mask data, and requires no telemetry.

| Model | Purpose | Exact upstream pin and model URL | Local SHA-256 / size | License and usage decision | Repository / CI policy |
| --- | --- | --- | --- | --- | --- |
| OpenCV Zoo YuNet `face_detection_yunet_2026may.onnx` | Multi-face bounding boxes, confidence and five landmarks | [`opencv/opencv_zoo`](https://github.com/opencv/opencv_zoo) commit `47534e27c9851bb1128ccc0102f1145e27f23f98`; [fixed binary](https://media.githubusercontent.com/media/opencv/opencv_zoo/47534e27c9851bb1128ccc0102f1145e27f23f98/models/face_detection_yunet/face_detection_yunet_2026may.onnx) | `ebafce4e3c118d6554634be5c27ab333b4c047a9a8c3faf1d7cf93101c22f0f0`; 229,738 bytes | OpenCV Zoo model repository: MIT. **Approved** for this GPL/private-use project after the ordinary notice review. | Local `models/local/` only and Git-ignored. Never fetched at runtime; not sent to GitHub CI. A future public binary distribution must retain the model notice and recheck the exact model license. |
| yakhyo Face Parsing BiSeNet **ResNet18** `resnet18.onnx` | 19-class facial semantic logits for Face/Skin/Eyes/Brows/Lips/Mouth/Hair soft masks | [`yakhyo/face-parsing`](https://github.com/yakhyo/face-parsing) commit `8a4729d95118d0e97c44185f9bdef3d6bfeaaf99`; [release asset](https://github.com/yakhyo/face-parsing/releases/download/weights/resnet18.onnx) | `0d9bd318e46987c3bdbfacae9e2c0f461cae1c6ac6ea6d43bbe541a91727e33f`; 53,205,364 bytes | Upstream code is MIT, but pretrained-data/model provenance includes CelebAMask-HQ. **NON_COMMERCIAL_ONLY; REVIEW_REQUIRED_BEFORE_PUBLIC_RELEASE.** No ResNet34 model is used. | Local `models/local/` only and Git-ignored. Never fetched at runtime, committed, packaged, or uploaded to CI. It must be removed/replaced or separately cleared before any public release. |
| BiRefNet general Swin-Tiny `BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx` | M20 Subject soft mask; Background is its exact complement | [ZhengPeng7/BiRefNet official v1 release](https://github.com/ZhengPeng7/BiRefNet/releases/download/v1/BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx), release `v1`, asset created `2024-08-18` | `5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333`; 224,005,088 bytes | BiRefNet repository: MIT. **APPROVED** for offline local use; no model-specific commercial restriction asserted by that project license. | Local `models/local/` only, Git-ignored, not packaged or supplied to CI. Runtime has no network dependency or telemetry. |
| NVIDIA SegFormer-B0 ADE20K `segformer-b0-ade20k-489d5cd.onnx` | M20 semantic scene provider; currently exposes Sky only | Official [nvidia/segformer-b0-finetuned-ade-512-512](https://huggingface.co/nvidia/segformer-b0-finetuned-ade-512-512) revision `489d5cd81a0b59fab9b7ea758d3548ebe99677da`; official `model.safetensors`, deterministic local PyTorch 2.13.0 export, opset 17 | `56d255beface9e9f82ab68a1292b8b03881aa45161dffe914b7fb9657133dc58`; 512x512 input, 150x128x128 logits | NVIDIA non-commercial research/evaluation license. **NON_COMMERCIAL_ONLY; REVIEW_REQUIRED_BEFORE_PUBLIC_RELEASE; commercial use blocked.** Training set: ADE20K. | Local `models/local/` only, Git-ignored, not packaged or supplied to CI. Runtime has no network dependency or telemetry. |
| NAFNet SIDD width-32 `nafnet-sidd-width32-512-opset20.onnx` | M21 RGB photo denoise in the Linear Rec.2020 shared graph through a deterministic model-domain adapter | Official [`megvii-research/NAFNet`](https://github.com/megvii-research/NAFNet) commit `2b4af71ebe098a92a75910c233a3965a3e93ede4`; official `NAFNet-SIDD-width32.pth` Google Drive asset `1lsByk21Xw-6aW7epCwOQxvm6HYCQZPHZ`; deterministic Starroom export script uses PyTorch 2.13.0 CPU, FP32, static 1x3x512x512, opset 20 | Source checkpoint SHA-256 `89c70e808d1783b6c07911306e106aaf0d4f7f3da8c61078b99ff7f8929a26f4`; exported ONNX SHA-256 `0e522d6de607958c283c834e6459a37b2fccbf5c19223a289393b4745f0cb633`; 117,341,892 bytes | Upstream repository is MIT. The pretrained checkpoint was published by the upstream project and trained on SIDD; **LOCAL PRIVATE USE APPROVED, MODEL REDISTRIBUTION REVIEW REQUIRED BEFORE PUBLIC BINARY RELEASE.** | Checkpoint and ONNX remain under ignored `models/local/`, are never committed, packaged, auto-downloaded or uploaded to CI. CI exercises the exact domain/tiling/provider contract with deterministic inferencer fixtures. |

## Runtime pin

`ort = 2.0.0-rc.10` (MIT OR Apache-2.0) is the single local ONNX Runtime binding. The Rust
adapter verifies each model file SHA-256 before opening a session, attempts the explicitly
requested DirectML provider, and creates a documented CPU session when DirectML is unavailable.
Missing/invalid models, invalid inference output and initialization failures have typed errors;
there is no browser, cloud or placeholder-model fallback.

## Model installation contract

Place reviewed binaries in the ignored local directory (or set
`STARROOM_LOCAL_MODELS` to an alternate local-only directory):

```text
models/local/face_detection_yunet_2026may.onnx
models/local/bisenet_resnet18.onnx
models/local/nafnet-sidd-width32-512-opset20.onnx
```

The application verifies the hashes above. Any changed file reports `modelHashMismatch` rather
than silently accepting a different model. A no-model workstation reports an explicit unavailable
state and continues safely without portrait detection.
