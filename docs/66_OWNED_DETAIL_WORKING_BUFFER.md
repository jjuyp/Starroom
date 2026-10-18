# One owned working buffer through Native detail stages

2026-10-18. Continue doc65 without new image math, dependencies, profile or quality policy.

## Implementation

`apply_detail_stage` now consumes its working image through actual `denoise_owned`,
`local_detail_owned`, `sharpen_owned` and `apply_finishing_effects_owned` implementations.
Preview and Export call the same function. Individual inactive stages return the same
allocation; active stages also write each final independent pixel into that allocation.
Neighbour inputs remain immutable and separate: denoise's Y/Cb/Cr buffer and the existing
Gaussian/broad-filter intermediates are not overwritten during sampling. NR addressing,
all weights, f32 operation order, interpolation and finishing noise seed remain unchanged.

Existing borrowed public APIs remain available for callers that must retain their input;
they clone once and enter the same production kernel, not an alternate image engine.
The pipeline no longer retains successive whole-frame input/output bindings through the
detail chain. Grain/Vignette Off still validates finite values; checked layout products
now reject overflow as `LookError::InvalidImage` rather than panic.

For an all-Off chain entered because another tool/stage is active, four full RGB clones
are no longer required. Four RGB-f32 allocations are cumulatively1.152GB at24MP,2.16GB
at45MP,2.88GB at60MP and4.80GB at100MP (decimal units). **This is allocation arithmetic,
not measured application peak memory.** Necessary spatial intermediates, decode/cache,
creative processing and color transform still consume memory;100MP is not accepted here.

## Validation

- Existing frozen detail serial oracle continues to compare every f32 bit for588 cases
  in1/2/4-thread pools, plus doc65's63 border/signed-zero/HDR cases. No tolerance changes.
- New owned-detail test compares denoise/local/sharpen against that independent serial
  oracle across Off/medium/extreme combined chains and checks pointer/capacity retention.
- A test-only frozen finishing kernel from commit37387e3 preserves the pre-ownership
  loop.36 shape/grain/vignette combinations compare every f32 bit and pointer/capacity,
  including negative/HDR/signed zero. Borrowed and owned APIs reject malformed shape,
  overflow and NaN/Inf explicitly for both active and Off settings.
- Shared pipeline test exercises all32 combinations of denoise/local/sharpen/grain/
  vignette over three shapes (96 cases), checks exact borrowed-chain output and the
  same allocation throughout; source data remains unchanged.
- MSVC Release detail/look tests, targeted pipeline test, format and warning-denied
  targeted Clippy pass. Detail/shared/photographic/RAW/GPU milestone1792298548619
  passes all77 pipeline units,39 render tests and strict workspace Clippy. Broader
  Full Rust1792326254186 passes format, warning-denied workspace Clippy, workspace
  tests/doc-tests (478registered/474ordinary/four opt-in excluded). Export target
  1792325961859 passes27 ordinary cases; heavy100MP remains excluded. Frontend sources
  and locks are unchanged from37387e3's222-test/lint/TS/build Full Web evidence;
  selected Native ownership/contract52 tests and JSON pass again. License561/6/269
  and infrastructure pass. Own remote pending at commit creation. Doc67 records a separately discovered
  real grain source-binding mismatch and the necessary production/input repair.

## Actual private RAW Native measurement

Same doc65 read-only field original, ready-cache diagnostic, MSVC1.97.1/RTX3050/DX12,
1024 interactive, unchanged FinalJPEG96. Report`native-preview-rgba8-1792298387106.json`
retains all27 controls,137 actual CPU/export comparisons(maxRGB8delta1), immutable
original/key-source hashes and real stage/resource data. No other benchmark/test was
running during the measured interval. Comparing separate sequential runs:

| Native interval | Before median / p95 ms(doc65) | Owned median / p95 ms |
| --- | ---: | ---: |
| Exposure |49.234 /59.706|50.043 /51.071|
| Sharpen |77.066 /90.709|69.898 /72.355|
| LumaDenoise |97.721 /111.158|92.376 /92.488|
| ChromaDenoise |102.597 /107.013|93.078 /94.652|
| Texture |85.971 /87.330|80.614 /96.987|
| Clarity |86.802 /98.449|86.990 /87.585|
| Dehaze |93.126 /111.240|86.618 /86.859|

First ready-cache interaction50.095ms, prep18.056ms separately, warm49.174ms,
Final260.408ms, first RAW1:1=3004.889ms, cached2:1=32.964ms. All27 individual controls'
five-sample p95 values are below100ms **in this run only**, not a universal latency
gate. Exposure/Clarity median and Texture p95 regressions remain visible. Heavy radius,
simultaneous controls, masks/healing, full UI/IPC/OS presentation, unprimed/immediate-
drag races and broader corpus are not qualified. The first RAW zoom still takes3s.
Existing memory counters include the unchanged open/zoom baseline, not an isolated
detail allocator peak comparison; no memory-usage reduction percentage is claimed.

No whole-product latency/photographic/100MP/memory-lifetime/installed/Final claim.
Every original requirement remains active in doc44. Prior doc65's local ordinary Full
Rust474registered/470ordinary and Web222 pass; own remote39097422697 and secondary
39097426197 have reached SUCCESS, tracked separately and never substituted for these
current changes.
