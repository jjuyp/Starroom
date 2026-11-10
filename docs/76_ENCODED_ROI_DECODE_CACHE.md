# Encoded 1:1 source-region reuse

2026-11-09. Phases8/9/16/25, under qualification, not Final acceptance.

## Root cause and implementation

Eligible high-resolution JPEG/PNG/TIFF previews used decode_source_region before
the rendered-frame cache lookup. Even an unchanged viewport needed another codec
decode when the user changed exposure/tone/color parameters. JPEG region decoding
can require full compressed-image decompression; small output does not imply
cheap source access.

NativePreviewScheduler now retains exact immutable decoded regions in a separate
LRU, at most4 entries and32MiB of estimated float pixel storage. Entries above
the pixel budget are returned to the caller but not retained. Metadata/context
and codec temporary memory are not included in that pixel-retention bound.
Keys include authoritative source path/length/mtime identity, original source
dimensions and expanded x/y/width/height. The expanded region includes the existing
graph halo/healing bounds. Dimension mismatch returns PreviewViewportInvalid.
The decode happens outside the mutex; concurrent completion replaces duplicate
entries. Arc ownership avoids another pixel copy on a cache hit.

Only immutable encoded input is cached; no rendered settings, GPU outputs or
preview-resolution substitutions. SourcePreparationCache can reuse the same
source Arc with its existing independent WB/lens/geometry/region dependency key.
RAW full-sensor handling, geometry eligibility, shared graph and export sources
are unchanged. Existing mature image codecs remain the source decoder; no new
image algorithm, dependency or license change.

## Targeted evidence

- Public photographic PNG and JPEG: exact region Arc reuse, no RawDecode stage
  on hit, exact CPU output against fresh decode, separate source revisions and
  coordinates, invalid-dimension error, bounded4-entry LRU/eviction and immutable
  source hash. Large JPEG region above32MiB is decoded but not retained.
- Actual native_preview_inner calls: two changed exposure settings on a fixed
  high-resolution region; the second has no decode stage and its entire binary
  JPEG preview frame is byte-exact to an independently fresh Native shared graph.
- Full Rust report1794228710901: workspace format, warning-denied Clippy,
  all ordinary workspace tests/doc tests exit0; desktop55 passed/3 deliberate
  ignored opt-ins (including this separately executed benchmark).

## Timing gate

Run alone: node scripts/roi-preview-performance.mjs. Requires Windows MSVC Release.
Uses the public Nikon D7100 JPEG and22 changed-exposure CPU shared-graph comparisons.
Two warmups excluded;20 observations compare uncached decoder calls versus exact
region cache hits. The cold label means decoder-cache miss, NOT cold OS disk cache.
Measurements exclude subsequent processing, UI/IPC, GPU and physical display.
The script records HEAD/dirty/compiler/fixture/source hashes and rejects changes
during the run, missing comparisons, command failure or no median improvement.
The ignored timing test is a deliberate opt-in benchmark, not deleted coverage:
ordinary source/cache and real-Native parity tests remain non-ignored.

Release report1794228340005 passed on Windows MSVC: source4496x3000,
ROI512x512,22 CPU/shared-graph exposure comparisons, immutable source hashes.
Uncached decoder median159.8815ms/p95178.6223ms; cached retrieval
median0.0090ms/p950.0362ms. Timing excludes the CPU shared-graph comparison
which runs after the timed retrievals. No concurrent benchmark was running.
This does not fix the first full-resolution RAW decode or establish a whole-UI
latency threshold. Own GitHub qualification remains pending.
