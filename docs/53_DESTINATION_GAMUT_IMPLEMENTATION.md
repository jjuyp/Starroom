# Destination-aware Native output mapping

Implemented 2026-10-15 against the counterexample in doc52. This closes the working-cube-only
implementation defect, **not** all photographic IQ, monitor, latency or Production gates.

## Foundation and placement

The pinned LittleCMS 2.19 / lcms2 6.1.1 remains authoritative for ICC parsing, conversion,
proofing, rendering intent and BPC. `starroom-color-management::map_output_gamut` runs once
at the shared final bounded SDR output boundary, before the actual LCMS output transform.
CPU export and GPU-accelerated preview reach the same boundary. Creative/RAW buffers remain
unbounded; a supplied ICC is never silently replaced. Color-policy identity is now
`starroom-color-v6-destination-gamut`, invalidating older rendered-state identities.

In-destination-gamut pixels remain bit-exact before ICC conversion. Out-of-gamut pixels use
the existing Native OKLab coordinates: fixed lightness/hue, bounded 16-step chroma reduction.
The reference is [W3C CSS Color 4, 2026-10-09 draft, section 14.1.3](https://www.w3.org/TR/2026/CRD-css-color-4-20261009/#chroma-reduction).
This is **not** CSS local-MINDE conformance, not a claim of optimal photographic perceptual
mapping, and not an implementation of a printer's perceptual rendering table. Strong yellow/
cyan desaturation and photographic appearance require further independent IQ qualification.
Lightness beyond the SDR endpoints is explicitly mapped to white/black only at this boundary.

## Actual profile membership, safety and ownership

- Isolated LittleCMS proofing contexts with different alarm values distinguish genuine white/
  black from gamut alarms. No global alarm codes are changed. Context owners outlive their
  transforms; NO_CACHE immutable transforms can safely be used by parallel workers.
- ICC output plus inverse reproduction resolves coarse proof-LUT near-black false positives
  and valid wider profiles with negative or above-one working coordinates. No working-unit
  clamp is used as a substitute for the destination.
- For monotonic endpoint-normalized RGB matrix-shaper profiles without LUT overrides, LCMS
  constructs a private linearized profile and evaluates its exact affine coefficients.
  Independent mixed, negative and HDR values validate this extraction against LCMS before use.
  Boundary pixels still use full ICC membership; chroma-search candidates use a small inward
  numerical guard to avoid transfer-function-amplified rounding. The original ICC is unchanged
  for the final transform and embedding. Noncanonical profiles use the general CMM path.
- Proof cache: exact profile bytes + rendering intent + BPC, four-entry LRU and 2MiB aggregate
  profile-byte limit. Duplicate cold builds converge on one retained object. Oversized valid
  profiles still execute but are not retained. Invalid ICCs and unreproducible neutral anchors
  produce typed errors. Nonfinite input is rejected before processing.
- The final owned float detail buffer is borrowed as RGB triples and returned directly after
  transform. No extra full-frame triples/flatten allocation is needed. Temporary search/proof
  storage is bounded to parallel 2048-pixel chunks, not a second complete image.

## D65 defect discovered by the new gate

The old XYZ/LMS matrices mapped OKLab neutral L=0.999999 to working RGB approximately
`[1.0002134, .9999756, .99928325]`. This made near-white neutral anchors fail real ICC gamut
membership. Native CPU and WGSL now share the published white-consistent XYZ/LMS coefficients
from the same pinned [W3C draft, section 19](https://www.w3.org/TR/2026/CRD-css-color-4-20261009/#color-conversion-code).
No frontend color math or new dependency was introduced. Dense near-white tests expose the
original defect without changing tolerances; negative/HDR round trips remain finite.

## Regression evidence

Ten new gamut tests pass: all four named output profiles, exact neutral/valid-wide identity,
constant L/h chroma path, invalid/nonfinite/HDR boundaries, all intents/BPC states, dense
neutral anchors, affine/general membership agreement on 4x1000 independent colors, cache
identity/LRU/aggregate bytes/duplicate builds, concurrent proof use, and a deliberately
restricted-range supplied TRC using the actual general provider rather than sRGB substitution.

Four-profile 8193-step neutral ramps retain exact working values, monotonic encoding and
more than 8000 distinct adjacent float samples (no hidden 8-bit intermediate). Six 2049-step
hue/chroma rays per profile have adjacent LittleCMS CIEDE2000 <0.15 and bounded encoded RGB.
This numerical suite is not a photograph/printer-LUT/18-category Golden acceptance.
Full local Rust report1792075544926 passes format, warning-denied Clippy and workspace tests/
doc-tests: 451 registered, 447 passed, four explicitly opt-in gates not executed. Existing
actual-photo, RAW/shared, CPU/GPU, 16-bit, export/recovery gates pass unchanged. Full Web
report1792075555690 passes 211 tests/30 files, lint, TypeScript, production build, JSON/Golden
manifest and packaging configuration. License/infrastructure validators pass. Remote CI and
all stronger Final gates remain separate requirements; a green ordinary Full excludes the
four opt-in gates and is not Level-4/installed/100MP evidence.

## Measured Native engine latency — not pointer-to-present

Actual MSVC release, NVIDIA RTX3050 Laptop/DX12, legal immutable X-Trans RAW, 1024x680,
five measured requests per control after warm-up, CPU reference outside timed interval.
Reports retain uncommitted kernel labels; they are not same-SHA Final evidence.

| Control | Earlier v5 median/p95 ms | v6 owned-output median/p95 ms | Max CPU/GPU RGB8 delta |
|---|---:|---:|---:|
| Exposure | 59.0817 / 60.7951 | 74.9053 / 81.4680 | 1 |
| Neutral Picker | 60.3778 / 68.1905 | 66.6708 / 69.8085 | 0 |
| Physical Temperature | 63.1510 / 64.2286 | 70.7223 / 71.8661 | 0 |
| Physical Tint | 62.5470 / 62.6172 | 68.9005 / 69.0354 | 0 |

Authoritative raw reports: `color-latency-1791770784010-89792.json` (v5) and
`color-latency-1792074870247-21272.json` (v6 owned-output), under `.starroom-reports`.
The correctness addition is slower than v5 on this run; **no universal speedup is claimed**.
The initial general-only correct mapper measured physical Tint 200.5847/213.2764ms; the
validated affine/search optimization reduced that to 68.9005/69.0354ms with identical delta0.
Earlier inward-search-only run measured 64.9163/66.5311ms; buffer reuse reduces proven
allocations but did not improve elapsed time on this later run. System/build variance and
different dates prevent claiming a precisely isolated timing improvement from buffer reuse.

Still OPEN: actual input-to-present <100ms across common tools, GPU residency/direct display,
hardware timestamps, arbitrary LUT RGB/printer fixture qualification, photographic gamut IQ,
alpha, OS monitor ICC, complete 100MP lifecycle, installer/offline and same-SHA Final gates.
