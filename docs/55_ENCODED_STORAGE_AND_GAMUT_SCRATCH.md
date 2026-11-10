# Encoded working storage and bounded gamut-search scratch

Implemented/tested2026-10-15; interrupted handoff verified2026-10-17. This group changes
ownership/allocation, not LittleCMS/WB math, ICC selection, resolution, precision, search
iterations, tolerances or authoritative edit state. No dependency/lockfile change.

## Production implementation

`to_working_image` now fallibly reserves one flat float RGB buffer, borrows its triples for
the same LCMS/WB adapters and transfers it directly to LinearImage. The previous triple Vec
plus full flatten allocation is removed. Dimensions/lengths use checked arithmetic, including
empty/overflow rejection before allocation. EncodedWorkingAllocation has typed Memory UX.
At1024x680, the removed extra RGB buffer is8,355,840 bytes. At100MP,1.2GB is arithmetic only,
**not measured peak-memory acceptance**. Decoded RGBA and other graph buffers still exist.
This is not alpha preservation: that original Production requirement remains open.

The destination mapper retains16 exact search iterations but reuses each2048-pixel chunk's
trial/membership scratch. Candidate formula, order and membership thresholds are unchanged.
Named matrix profiles avoid repeated trial/membership Vec allocation. Other ICCs keep real
LittleCMS proofing and its existing temporary buffers, not an approximate/substituted profile.

## Evidence

- Real16-bit PNG round trips for sRGB/P3/AdobeRGB/Rec2020:16 valid WB/profile combinations
  match every previous f32 bit; eight encoded Camera/AsShot combinations retain typed semantic
  errors. Original decoded/source bytes are unchanged. Zero/overflow dimensions fail safely.
- Frozen previous allocating search vs production: all f32 bits match for4x4133 saturated
  vectors across chunk boundaries. Scratch lengths0/13/4133 retain the reserved allocation.
- Full Rust report1792078304648 passes format, warning-denied workspace Clippy, workspace
  tests and doc-tests:457 registered/453 ordinary passed/four opt-in excluded. Full Web1792077863686 passes213 tests/30 files, lint, TypeScript,
  production build, JSON/Golden manifests and packaging configuration. Four opt-in gates are
  excluded from ordinary Full; this is not Level-4/installed/100MP/Final acceptance.

## Performance: preserve unfavorable observations too

The existing `buffer_layout_probe` rotates method order for eight rounds (first omitted),
checks exact bits and prints seven-sample medians. MSVC release1024x680 RGBA packing:
previous triple/flatten8.0205ms; single flat extend3.9508ms; zero/assign4.8160ms. Production
uses single-extend. Prior flatten does not reuse allocation. These are packing measurements,
not ICC, full graph, process memory or photographic100MP results.

Uncached complete Native NASA portrait512x512, RTX3050/DX12, five measured requests after
warm-up, CPU reference outside the timed GPU interval:

| Control | Previous median/p95 ms | New median/p95 ms |
|---|---:|---:|
| Exposure |39.6747 /43.6367 |42.1172 /43.8603 |
| Relative Temperature |34.9368 /37.7301 |40.3237 /41.6741 |
| Relative Tint |34.5546 /36.3306 |41.0362 /43.8087 |
| Saturation |38.6058 /39.7384 |43.3785 /47.8288 |
| Vibrance |38.0558 /40.6449 |43.3454 /46.4893 |
| Auto WB |40.2715 /43.1988 |40.7742 /44.2264 |
| Neutral Picker |36.2487 /37.4534 |44.0832 /46.3178 |

Raw reports retained under `.starroom-reports`:
`color-latency-1792077265177-24084.json` (64621bf baseline) and
`color-latency-1792077572531-27556.json` (uncommitted encoded-flat/gamut-scratch label).
Whole-path observations are slower despite packing improvement. **No overall speedup is
claimed.** Cross-process host/driver/thermal activity was not controlled. CPU/GPU RGB8
deltas remain within the unchanged one-code bound (Auto/Picker0); source bytes are unchanged.
Doc54's >100ms outliers remain failures of a universal latency claim.

All original alpha/monitor/photographic/resident-GPU/tiles/100MP/Windows/Final requirements
remain active. Do not replace end-to-end acceptance with a favorable microbenchmark.
