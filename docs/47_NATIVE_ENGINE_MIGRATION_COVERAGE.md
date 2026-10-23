# Retire Browser creative engine without losing regression intent

The Native shared graph is authoritative. `src/imagePipeline.ts` is removed, not moved to a
hidden compatibility folder. It was already unused by the production bundle, so this cleanup
does not claim a new slider-speed improvement. The read-only demo and display-only histogram
remain; neither performs creative photo editing. Native preview/export processing is unchanged.

Eight former Browser pixel assertions now execute against production Rust preview AND export
in `crates/starroom-pipeline/tests/legacy_browser_intents.rs`. They passed before removing the
old engine, with original fixtures/assertions retained rather than tolerances widened:

| Original regression intent | Native coverage |
| --- | --- |
| Neutral [40,80,120] is exact | `legacy_neutral_adjustments_remain_pixel_exact_in_native_graph` |
| Exposure +1 raises every channel | `legacy_exposure_changes_all_rendered_channels_in_native_graph` |
| Curve raises 35-gray and lowers 220-gray | `legacy_curve_controls_change_actual_native_shadow_and_highlight_pixels` |
| Monotone control points remain bounded/monotone | `legacy_monotone_curve_intent_is_preserved_by_shared_native_spline` |
| Shadows +50: positive dark gain > twice midtone gain | `legacy_shadow_lift_targets_dark_pixels_more_than_twice_midtone_gain` |
| Shadows +100 preserves black | `legacy_black_anchor_survives_native_shadows_at_maximum` |
| Sharpness +100 increases the same 3x3 edge center | `legacy_sharpness_produces_a_visible_native_edge_change` |
| Encoded Relative Temperature +70 warms neutral gray | `legacy_relative_temperature_warms_encoded_native_gray` |

The two UI intents (neutral edit-state detection and normalized display histogram) remain
frontend tests in `src/nativePipelineOwnership.test.ts`. Recursive source ownership checks
and corpus hashing live in Node-only test tooling, not production React. Relevant color,
tone, curve and detail gates run the migrated Native test file explicitly; full workspace
tests also discover it normally.

Frozen M1C reference observations and all Native migration tolerances are UNCHANGED:
`tests/fixtures/m1c/browser-native-reference.json`, canonical LF SHA-256
`f618d1e873d2433ae39f05297ebd425e347cdb3eab3c8d14ed1707bbb70fdb47`.
The frontend verifies immutable data integrity rather than rerunning retired Browser math.
The existing `browser_native_reference.rs` still compares actual Native output with that corpus
using its existing documented tolerances. It is migration evidence, not the formal tone IQ oracle.

Counts are intentionally reported across both languages: eight image tests moved to Rust;
no pixel-behavior assertion was deleted to make CI green. Current full frontend is 198/198
(29 files), with additional source/data integrity checks; Native adds eight production tests.
Build/lint/types and infrastructure validation pass. No image algorithm, parameter semantics,
profile, precision, dependency or source-photo files were changed. Removed source/tests remain
recoverable in Git history. Remaining full production phases and Final Acceptance stay open.
