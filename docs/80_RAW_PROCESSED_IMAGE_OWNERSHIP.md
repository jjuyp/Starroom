# LibRaw processed-image ownership and measured peak memory

## Foundation choice

Integrate the pinned LibRaw0.22.2 allocation API; do not replace sensor decode,
demosaic, WB, camera profile, normalization or working-space math. Upstream
`src/postprocessing/mem_image.cpp` at existing commit
`b93f6e45c194f5df9b02a43b1af9a54b4f41f33f` allocates the processed image
independently of the processor and releases it via `LibRaw::dcraw_clear_mem`.
The CDDL-1.0 selection and existing notices remain unchanged.

## Production change

The bridge previously allocated and memcpy'd another full RGB16 frame, then
cleared the upstream image. It now transfers the original processed-image owner
and a read-only sample pointer to Rust RAII. Drop calls the official clear API
on the owner, never free on the interior pixel pointer. Compressed input bytes
are released after the bridge/processor returns, before the working f32 frame
is allocated. A pre-write ABI-size check and zero/count/alignment/slice-size
validation reject invalid buffers with typed errors. Error-path ownership stays
empty and existing decode failures keep their classifications.

No precision, orientation, WB, camera transform or image semantics change. The
Native decode API used by Preview/Before/Export is unchanged. No dependency,
lockfile, model, fixture, output format or capability claim was added.

## Measurement

Same Windows MSVC compiler rustc1.97.1, Release profile, six pinned CC0 photographic
RAW fixtures, full and LibRaw half-size, three fresh processes per case. Exact
SHA-256 of every output f32 bit and complete serialized metadata matches the
pre-change baseline for all36 decodes; source hashes are immutable. Probe sources,
binary and adapter hashes must remain unchanged during a run. This verifies
implementation equivalence, not an independent photographic color oracle.

Baseline: `.starroom-reports/raw-ownership-1794316984597.json`.
Qualified sequential post-change probe: `raw-ownership-1794317343944.json`.
An earlier diagnostic `1794317193342` overlapped compilation and source editing;
retain it but do not use it for performance/implementation qualification.

Peak is Windows' process PeakWorkingSet64 at completed decode, before output
fingerprinting/validation; it is not committed/private bytes, system/GPU memory,
whole editor/100MP peak or total probe-runtime peak. Values below are three-run
medians, MiB. OS working-set variation and short sampling remain limitations.

| Fixture | Full before | Full after | Half before | Half after |
| --- | ---: | ---: | ---: | ---: |
| Nikon D1 NEF |66.63|52.37|28.28|24.46|
| Sony A100 ARW |228.87|180.65|83.94|69.42|
| Canon S2 IS CR2 |120.64|94.04|47.29|39.98|
| Canon G5X II CR3 |451.39|357.36|158.75|129.53|
| Apple iPhone SE DNG |273.30|216.21|98.57|81.19|
| Fuji X-Pro1 RAF |374.52|286.43|141.35|118.02|

Decode medians also remain explicitly reported, not universally faster: CR3 full
2773.00→2540.84ms, DNG full1656.53→1663.65ms, DNG half346.47→362.16ms, RAF
full9329.94→9209.15ms. This is a buffer/peak-memory optimization; it does not
resolve first-RAW-1:1 latency or qualify the100ms whole-UI target.

## Validation

Two new direct FFI tests check ABI layout/empty ownership and corrupt input with
no published owner. Targeted warning-denied Clippy and rustfmt pass. Canonical
`test:raw` report1794317456590 passes62 tests: RAW35, ColorChecker5, photographic
sensor3, unbounded resize4, sensor headroom8, shared graph3 and forward graph4.
RAW/Golden manifests and source identities pass. Existing incremental-cache
hard-link warnings copy artifacts instead; there are no failed tests or code
Clippy diagnostics. CI, full final release, Windows UI/installer and large-image
gates remain required. Lockfiles and all image-math source files are unchanged.
Full local Rust report1794330135797 completes format, warning-denied workspace
Clippy and tests/doc-tests with exit0:504 registered,499 ordinary passed and five
existing opt-ins not executed. The earlier interrupted invocation had no completed
report and was not accepted. This is not the heavy/installed final Release gate.
