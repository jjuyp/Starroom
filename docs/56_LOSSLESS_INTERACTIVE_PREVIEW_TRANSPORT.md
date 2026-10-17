# Bounded lossless interactive preview transport

2026-10-17. This is a production transport/presentation improvement, not completed
GPU residency, installed input-to-present acceptance or v1.0 Final qualification.

## Implementation and precision

The real React PhotoCanvas now explicitly requests `payloadFormat: rgba8` while
dragging. Native validates that RGBA8 is interactive only; its existing interactive
decode tier is bounded to a 1024-pixel maximum edge. `rgba_preview_frame` checks
dimensions/sample length, fallibly reserves the final binary frame once, and packs
the shared graph's already transformed sRGB RGB8 with opaque alpha. It performs no
new color transform and never quantizes authoritative working pixels. It cannot
transport an accidental full-resolution 100MP RGBA frame or silently downgrade.

SRP4 retains the 40-byte envelope and profile identifier, uses magic/version
SRP4/4 and encoding word 1 at offset34, and carries exactly width*height*4 bytes.
Legacy SRP3/3, reserved word0 and JPEG remain supported. Final/HQ and viewport
tiles retain JPEG96; explicit legacy interactive callers retain JPEG88. Source,
prepared and creative GPU cache identities are unchanged. Encoded frame cache
identity now includes encoding and JPEG quality, preventing both RGBA/JPEG mixing
and the previously possible JPEG88 cache hit for same-tier Final JPEG96.

The frontend parser checks magic/version/format, dimensions, source/tile bounds,
payload length and RGBA's 1024 limit, then borrows a byte view rather than copying
the payload. `decodeNativePreviewPayload` uses native ImageData/ImageBitmap APIs,
explicit sRGB and no additional color conversion. PhotoCanvas retains all existing
active-photo, Before/After, generation, final-quality and tile-placement guards.
A `finally` closes the bitmap or revokes the JPEG URL on success, stale rejection
and failed painting. JPEG decode failure also revokes its URL. No TS creative math,
new image engine, provider, dependency, lockfile or original-photo write is added.

This does **not** fix transparency: the existing graph is RGB-only; opaque alpha
represents that existing display output, not recovered source alpha.

## Numerical and actual file evidence

- Native unit: every 0..255 RGB byte is retained, alpha255, version/format/length
  correct; malformed/zero/oversized buffers fail explicitly. Interactive/final
  transport identities are distinct and noninteractive RGBA is rejected.
- Actual NASA portrait, three exposure settings with shadows/highlights/saturation:
  real Native JPEG88 -> RGBA -> Final JPEG96 -> cached RGBA, same source/tier/graph.
  Returned magic/cache results are distinct; both JPEG qualities differ. Real
  `export_one` writes an ICC-embedded PNG8; its actual decoded RGBA equals the binary
  preview byte-for-byte. Original source hash is unchanged. This is same-resolution
  sRGB file/transport evidence, not all output ICCs, all scenes or OS display parity.
- Frontend: legacy JPEG contract assertion is retained; new tests cover zero-copy
  offset/UTF-8 RGBA parsing, malformed versions/formats/lengths/dimensions/source
  bounds, oversized/truncated responses, and Memory UX for typed allocation failure.
- Headless Chromium155 uses the real production parser and presentation helper on
  Native-generated 1024x680 Fujifilm RAW frames. All22 RGBA draw/readback comparisons
  have **zero differing bytes**; all22 bitmaps are closed. Two real rendered images
  were visually inspected in the saved visible-viewport screenshot. This test does
  not emulate the desktop IPC, claim Nikon photographic approval or launch the app.

Reproducible browser helper: `scripts/fixtures/preview-transport-browser.mjs`,
`verifyNativePreviewTransport(prefix)`, served through Vite. It needs real owned
frames emitted by the example below; no mock photo transport substitutes for Native
rendering. The browser helper is diagnostics, not an automatically passed release gate.

## Measured transport costs

`cargo run --locked --release -p starroom-desktop --example preview_transport_probe -- --raw`
uses the existing licensed Fujifilm X-Pro1 RAF, shared Native GPU graph, actual
RTX3050 Laptop/DX12, exposure+0.35, 1024x680. Alternating method order, two warmups,
20 measured rounds. Original bytes unchanged. MSVC Release report:
`.starroom-reports/preview-transport-1792232141662.json`.

| Isolated Native encoding/packing | JPEG88 | RGBA8 |
| --- | ---: | ---: |
| Median ms | 15.5737 | 1.1822 |
| p95 ms (nearest rank19/20) | 16.3085 | 1.2652 |
| Binary frame bytes | 197454 | 2785356 |

Actual Chromium155 production parser/helper decode + draw + getImageData, same
frames, alternating order/two warmups/20 measured rounds:

| Browser operation | JPEG88 | RGBA8 |
| --- | ---: | ---: |
| Median ms | 7.35 | 2.8 |
| p95 ms | 10.0 | 3.5 |

Browser screenshot: `output/playwright/preview-transport-1792232141662-browser-visible.png`.
Initial reports selected sorted index10 as an upper median; the table uses the
arithmetic mean of central indices9/10 of the retained20 samples, and both probes
now implement that definition. Existing reports are retained, not overwritten.
The generated frames/report/screenshot remain local ignored evidence, not source assets.
The RGBA frame is about14.1x larger. **Neither measurement includes Tauri IPC, input
dispatch, queueing, graph execution, histogram scheduling, OS compositor or monitor**;
do not add independent medians into a fake end-to-end measurement. Larger IPC cost
must still be measured in the installed product. Full GPU readback and CPU LittleCMS
remain; this is not direct GPU presentation and does not close the <100ms gate.

## Acceptance scope

Targeted Native transport and actual decoded-export integration tests passed.
Targeted warning-denied Clippy, TypeScript, ESLint and86 frontend/queue/error tests
passed. Full Web1792232203917 passes216 tests/30files, lint/TS/production build,
JSON/Golden manifest and packaging configuration. Full Rust1792232440212 passes
format, warning-denied workspace Clippy, tests and doc-tests. This commit's remote
CI must reach terminal success before remote batch acceptance is recorded. Later
diagnostics/statistics/config/docs changes require their own targeted checks.
Final installed/100MP/offline/monitor/alpha/photographic/full latency gates remain OPEN.
