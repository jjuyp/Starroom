# First-drag preview pyramid: avoid repeating full file decode

2026-10-18. Native interactive-only optimization; Final/export remain authoritative.

## Measured defect

Retained MSVC Release before report `native-preview-rgba8-1792237433220.json`,
925c721 plus benchmark instrumentation, actual generated24MP6000x4000 source:
first loaded Fit2904.560ms; first interactive request466.811ms, of which RawDecode
390.955ms (one real encoded-source decode); warm changed exposure54.266ms. Final
refine255.524ms;1:1/2:1 viewport191.111/104.532ms. These exclude IPC/browser/OS input.
The earlier benchmark discarded the first interactive warm-up; it is now explicitly
retained, profiled and required by the runner rather than hidden in warmed medians.
This is evidence for a cold-tier defect, not proof of every user-reported5-second cause.

## Production change

- Exact direct decoded-tier lookup remains first. On an interactive miss, use a
  retained original higher preview, preferring the nearest larger tier, bounded to
 4096 source edge and1024 destination edge. Clone Arc ownership, not source pixels.
  Pixel processing runs after releasing the cache mutex.
- RAW uses the unbounded mature Lanczos3 adapter from doc60. Opaque bounded encoded
  RGBA uses the same pinned upstream weights/pass/accumulation and final clamp policy;
  it retains ICC/EXIF/format metadata, not a second color engine or JS image math.
  Transparent/HDR-encoded images keep the existing file decode path; this is an
  explicit eligibility limitation, not a claim that their alpha/color issue is closed.
- Full-resolution sensor cache entries and already-derived children are never parents;
  there is no repeated proxy-to-proxy resampling chain. Small source frames reuse the
  existing Arc without pixel work. Missing eligible parents use normal original decode.
- Derived pixels live under a separate source/policy/parent-tier namespace. Encoded
  frame identity also includes this lineage, so direct/derived/parent differences
  cannot alias an already-rendered frame. Existing actual-pixel Native/GPU fingerprints
  and source preparation cache continue to own their normal identities.
- Final, high-resolution viewport and export never request derived entries. Final
  reacquires its own direct tier; export always decodes the original independent of
  preview caches. New integration verifies exact final JPEG96 bytes against a fresh
  authoritative Native render, not only dimensions or a mock result.
- Same-key concurrent Before/After inserts retain one decoded tier in the existing
  four-entry/512MiB conservative cache. The shared image remains immutable.

## Tests and current evidence

- Existing RAW sampler oracles still pass. New opaque RGBA tests compare every f32
  bit with actual image0.25.10, including sharp0/1 edges, overshoot, odd/single-axis
  downsampling and upsampling. No tolerance is relaxed.
- Public proxy tests preserve source/ICC/EXIF/format, reject invalid dimensions/
  shape/nonfinite values and explicitly decline alpha/HDR-encoded conversion.
- Actual controlled LibRaw DNG derived preview equals direct preview RGB bits,
  preserves metadata and retains >3 headroom, with original bytes unchanged.
- Cache test proves nearest-original selection, source/tier/Final namespace isolation,
  no full-sensor/derived-child parent, warm Arc reuse, duplicate suppression and bounds.
- Actual native first-drag test uses immutable512px PNG portrait,4496px photographic
  JPEG and Nikon sensor RAW. First drag has no file decode; the small PNG has no
  resize either. A second changed edit has neither decode nor resize. Final at the
  same lower edge performs authoritative decode and matches fresh Native JPEG96
  exactly, including source hash preservation. These are real Native operations.
- Fast imageio tests, both desktop targeted tests and warning-denied targeted Clippy
  pass. Actual post-change MSVC Release report1792292729541 passes27 changed controls,
  with compiler/key source hashes and changedDuringRun:false. Full/own remote still
  require actual terminal evidence before ordinary batch acceptance is recorded.

Full local acceptance: Rust1792293196410 passes format, warning-denied workspace
Clippy, all workspace tests/doc-tests (470 registered,466 ordinary/four opt-in excluded).
Web1792293034145 passes218 tests/30files, ESLint, TS/build, JSON/Golden and packaging
configuration. Infrastructure validator and561Rust/6npm/269notice license gate pass.
No thresholds, ignored gates or photographic hashes were relaxed. Own remote is pending.

Own ordinary remote acceptance verified2026-10-18:cd5d8d31e9903d67737929cc2f35cc650060c4af
Full39092968506 and secondary39092972326 SUCCESS; PR#2Open/Draft. The whole100ms gate
and every wider original requirement below remain unqualified.

## Actual before/after Native results

Same generated24MP source,1024 interactive size and unchanged Final JPEG96. Before:
`native-preview-rgba8-1792237433220.json`; after:
`native-preview-rgba8-1792292729541.json`. Different dates are retained, not represented
as same-session hardware conditions. No competing build/test ran during either probe.

| Measurement | Before ms | After ms |
| --- | ---: | ---: |
| First interactive cold tier | 466.811 | 112.460 |
| First-interactive file decode | 390.955 / one execution | 0 / no execution |
| First-interactive proxy resize | not separately executed | 32.231 |
| Warm changed exposure request | 54.266 | 58.700 |
| Cold first Fit | 2904.560 | 2911.110 |
| Final refine | 255.524 | 269.481 |
| 1:1 viewport | 191.111 | 201.262 |
| 2:1 viewport | 104.532 | 110.347 |

All27 control median/p95 observations remain in the report. Selected unchanged-size
controls: Exposure58.429/59.995ms, Sharpen75.805/80.701ms,
LumaDenoise104.318/106.070ms, ChromaDenoise104.094/104.684ms,
Texture88.105/107.820ms, Clarity87.810/88.946ms, Dehaze89.274/99.657ms.
**The universal100ms gate still fails**, including first interactive112.460ms and
several spatial p95 values. Warm/open/refine/viewport observations are slower than
the retained before run; no universal speedup is claimed. First interactive now has
32.231ms resize plus16.611ms input transform and29.562ms inclusive output CMM, rather
than another390.955ms file decode. Background bounded priming, output-transform work,
adaptive preview and full input-to-present validation remain required; this report
does not turn a4.15x scoped improvement into a complete responsiveness acceptance.

## Scope limits

Interactive source approximation is explicitly a different sample lineage, not a
claim of identical proxy/full pixel samples or universal<100ms. It does not reduce
Final/export quality. Cold first open, absent/evicted higher parents, transparency,
HDR-encoded formats, persistent pyramid/disk caching, prefetch, GPU direct presentation,
all-stage residency, actual pointer-to-present, photographic resampling IQ,100MP and
Windows installed/runtime acceptance remain separate original requirements.
