# RAW working-buffer ownership repair

Status: local targeted/milestone/Full evidence passed; remote pending. Not a complete 100MP gate.

Full Rust report 1791771667236 and Full Web report 1791771448027 pass (209 frontend tests).

The old source-preparation path allocated `Vec<[f32;3]>`, then flattened it into another
full-frame `Vec<f32>`. The MSVC release `buffer_layout_probe` on Rust1.97.1 confirms Rust did **not** reuse
the first allocation: 2,088,960 samples / 8,355,840 bytes in each buffer at 1024 x 680.
This is measured allocation ownership, not an assumed compiler optimization or physical-RAM peak.

Production now reserves one flat working Vec fallibly, copies the immutable decoded samples
once and borrows its RGB triples in place with `as_chunks_mut`. Exact existing camera/WB
operations and ordering remain unchanged; no new approximation, precision drop, clamp,
decode/JPEG substitution, cache policy or source mutation is introduced. Overflow/empty/
mis-sized dimensions fail before allocation. Reservation failure is typed and maps to Memory UX.

The actual Nikon sensor regression compares old/new float bits exactly for SourceDefault,
AsShot, Camera, Auto and deferred NeutralPicker, each with/without physical WB. It also proves
source data unchanged and rejects overflowing/empty dimensions. Actual DNG shared graph and
the unchanged photographic/GPU/strict chart gates pass RAW milestone 1791771181496. Memory
message test passes. All original higher-level memory/release requirements remain required.

## Observed Native timing, not a universal speed claim

Same licensed X-Pro1 sensor fixture, 1024 x 680, RTX3050 DX12, MSVC release, cache and five
measured samples after one warm-up; original CPU/GPU RGB8 tolerance remains <=1.
Baseline kernel is `9c83695`; after report is explicitly labelled uncommitted raw-flat-buffer,
not misrepresented as final acceptance at a frozen SHA. Reports:

- `.starroom-reports/color-latency-1791769362374-75416.json`
- `.starroom-reports/color-latency-1791770784010-89792.json`

| Control | Before median ms | After median ms | Before five-sample P95 | After five-sample P95 |
|---|---:|---:|---:|---:|
| Exposure | 57.5883 | 59.0817 | 58.4726 | 60.7951 |
| Neutral Picker | 59.9386 | 60.3778 | 60.3181 | 68.1905 |
| Physical Temperature | 65.3407 | 63.1510 | 67.1443 | 64.2286 |
| Physical Tint | 65.5568 | 62.5470 | 66.5429 | 62.6172 |

The physical controls improve in this run, but unchanged/cache-hit controls show variation
and regressions; do not claim universal latency improvement or attribute all differences to
the allocation change. Physical frames retain delta0; all controls remain within original delta1.
ICC/output transform remains about 21–22ms, a larger remaining bottleneck. Whole-process peak
is dominated by decode/driver history, so this run does not prove a corresponding physical-RAM
peak reduction. At 100MP, one omitted RGB float frame would be 1.2GB arithmetically; that is a
buffer-size model, **not an executed 100MP stress/memory acceptance**. Installed input-to-present,
all controls, large-image lifecycles and exact-SHA release gates remain open.
