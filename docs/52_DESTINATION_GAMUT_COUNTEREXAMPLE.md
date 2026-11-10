# Destination-gamut counterexample — unresolved production gate

Measured 2026-10-12 with the existing pinned LittleCMS provider. This is executable evidence,
not a production gamut mapper or photographic/Final IQ acceptance.

The shared graph currently compresses against the **working Rec.2020 unit cube**, then applies
the chosen output ICC. A working-space in-gamut value is not necessarily destination in-gamut.
`destination_gamut_probe` invokes actual Native RGB float transforms and independent LCMS
proofing contexts (RelativeColorimetric, BPC, NO_CACHE, SOFT_PROOFING and GAMUT_CHECK).

Examples of actual encoded output before final quantization:

| Working linear Rec.2020 | Destination | RGB float output |
|---|---|---|
| [1,0,0] | sRGB | [1.2482357,-1.6096301,-0.23438673] |
| [0.8,0.1,0.2] | sRGB | [1.1047555,0.111865796,0.48359767] |
| [0.8,0.1,0.2] | Display P3 | [1.0149578,0.25579715,0.48861238] |
| [0.8,0.1,0.2] | Adobe RGB | [0.95367295,0.13373968,0.471579] |

The current working-gamut mapper leaves these inputs unchanged. Quantization clips the sRGB/
P3 over-range values, so the current working-only mapper does not prove destination hue/chroma
preservation. Adobe's negative-channel TRC may already produce zero, so checking only final
float channel ranges is not a general gamut oracle.

The existing mature proofing provider can detect this distinction: for red/green/blue/cyan/
the mixed vector/white, dual-alarm flags are sRGB and P3 [true,true,true,true,true,false], Adobe
RGB [true,true,true,true,false,false], Rec.2020 all false. Two isolated contexts with different
alarm codes avoid mistaking genuine white/black for an alarm; no process-global alarm state is
modified. Rec.2020 data for proofing is encoded through LCMS itself, not custom gamma math.
The release probe and warning-denied example Clippy pass. No library/profile policy is changed.

Next required work: actual destination-aware bounded-output mapping in the shared graph,
validated with in-gamut identity, neutral/black/white, saturated/mixed vectors, gradients,
perceptual metrics, named and supplied RGB ICCs, finite/invalid profiles, exact cache identities,
CPU/GPU parity and measured performance. The probe alone satisfies none of those production
requirements; do not simply clamp, silently replace a supplied profile or weaken existing gates.
