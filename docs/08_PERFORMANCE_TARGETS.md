# 08 — Performance Targets

Engineering targets: 60FPS UI; Fit-view slider visible response ideally <=50ms; stale jobs cancelled; histogram 10–15Hz during drag. RAW open may show embedded preview first, progressively replaced without UI blocking.

AI Mask uses analysis resolution appropriate to model, normally <=2048 long edge when valid. AI denoise previews visible/crop region first; full image is tiled with visible progress.

M17-M20 timing policy: record M17 skin-stage render, M18 patch render, M19 analysis/rule evaluation, and M20 first inference/cache-hit separately. The fixed M20 contracts currently use BiRefNet 1024x1024 and SegFormer 512x512 analysis tensors; model weights are local-only, so CI records compile/contract timings while real DirectML/CPU inference timing is a provisioned-machine benchmark. A cached mask must be reusable across global Exposure, WB, Tone, Curve, Mixer, Grading and Detail changes because none changes the source/model inference identity.

GPU working-cache tiers: 512MB/1GB/2GB. Benchmarks cover 24/45/60/100MP and Intel iGPU, Intel Arc, NVIDIA midrange, AMD midrange and CPU-only fallback. Record first preview, exposure drag, five-mask composite, AI mask, AI denoise, export, RAM and GPU allocation estimate.
