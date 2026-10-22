# Bounded LittleCMS output acceleration

2026-10-23, implementation under acceptance; not an installed release or complete
Production qualification. Baseline c09cf3f8326fe7edbd1df2f6c2b4ae2530ef0b61.

## Mature engine integration and precision

Integrate the unmodified official LittleCMS 2.19 Fast Float plugin, pinned with the
same core source revision. Private thread context, NO_CACHE, identical rendering
intent/BPC, existing shared Preview/Export provider and bounded transform cache.
It is never registered globally. Only exact built-in sRGB bytes or the existing
missing-output sRGB policy qualify; other ICCs and all input transforms remain
on the canonical engine. The existing LCMS-derived affine validates the bounded
domain. Negative/HDR/out-of-gamut samples and the sRGB TRC knee retain canonical
evaluation, with no intermediate clamp or profile substitution.

Independent upstream probe rejected unrestricted use: Adobe RGB dark ramps differed
by up to 164 16-bit codes; unbounded sRGB float behavior also differs. These are not
accepted as approximate replacements. Two implementation failures were retained:
serializing working primaries introduced matrix quantization (fixed by sharing
in-memory parameters); serialized-sRGB TRC knee interpolation exceeded 2e-6 (fixed
by canonical evaluation within two upstream LUT cells of the knee). Tolerances
were not relaxed. Dense cube/knee, default/explicit ICC, four intents, both BPC
settings, unbounded bits, 16-bit quantization and parallel/isolation tests now pass.
43 color-management tests and targeted warning-denied Clippy pass. Wider gates pending.

## Actual Native measurement

Local report `native-preview-rgba8-1792689231463.json`: Windows MSVC Release,
private read-only Nikon D750 6032x4032 original, 1024 interactive RGBA8 / Final JPEG96.
27 individual controls and four combined cases; 157 CPU/export comparisons,
maximum RGB8 delta 1, source/code unchanged during measurement, exit 0.
The later added dense unit test does not alter production code; final same-HEAD
qualification still required. Baseline report `native-preview-rgba8-1792666481462.json`.

| Native request | Baseline median/p95 ms | Current median/p95 ms |
| --- | ---: | ---: |
| Exposure | 50.802 / 58.082 | 40.880 / 42.175 |
| Highlights | 50.750 / 51.410 | 35.193 / 37.088 |
| Shadows | 50.090 / 52.250 | 34.248 / 42.374 |
| Texture | 69.803 / 93.910 | 52.887 / 75.528 |
| Denoise + Sharpen | 86.293 / 90.498 | 69.329 / 87.869 |
| Local detail + finishing | 81.165 / 101.329 | 65.427 / 86.900 |
| Tone + denoise + detail | 108.552 / 116.230 | 93.473 / 115.385 |
| Sharpen | 63.188 / 186.343 | 69.967 / 174.520 |

Unfavorable results remain visible: first interaction 57.083 -> 62.517 ms,
separate priming 18.993 -> 26.933 ms, MixerHue 57.55 -> 60.710 ms median,
Sharpen median worsened and cold Sharpen/NR plus heavy-tail p95 still exceed 100 ms.
Heavy output color transform was about 23 ms and is 6.644 ms in this run.
These are sequential runs, not controlled whole-UI A/B measurements. They exclude
Tauri IPC/browser/OS presentation. Do not claim universal sub-100 ms, all-profile
acceleration, no readback, 100MP qualification, or that the user's installer changed.

## Redistribution and remaining acceptance

GPL-3.0-or-later plugin, not MIT core license. Source hashes/full license retained
in vendor/lcms-fast-float-2.19; NOTICE/provenance and generated installer-bundled
license texts updated. License validator passes 561 Rust / 6 npm / 270 unique texts.
Full source/build/package/installer and complete original phase matrix remain open.

Targeted integration evidence: export milestone report1792689669595 passes all
records (export, shared pipeline, render, Golden manifest11/11, workspace Clippy,
lint and production build). Color report1792689669282 passes all except its build:
two simultaneously invoked milestone bundles contended for dist (EBUSY). Both
terminal, standalone identical `npm run build` then passes TS/Vite. Original
sandbox Vitest EPERM report1792689359564 is retained; approved normal-permission
retry passes70 focused tests. Pipeline80 units plus photographic/RAW integration,
render42 tests, script lint/JSON/infrastructure19 pass. No product/test change was
made for these environment/orchestration failures. Future milestone bundles must
run serially because their common build output is shared. Full workspace tests,
same-HEAD remote CI and installer qualification remain pending for this group.

Local full acceptance completed: Rust report1792690429095 passes format,
warning-denied workspace Clippy, workspace tests and doc-tests. Inventory496,
492 ordinary tests pass; four existing environment-specific opt-ins remain excluded.
Web report1792689864727 passes225 tests/31files, lint, TypeScript, Vite, JSON,
Golden/RAW manifests and packaging configuration. The source-provenance regression
checks all17 retained upstream hashes and installer-bundled full GPL text.
Vendor-only changes now trigger the color CI target; infrastructure/script lint pass.
Remote same-commit CI remains pending at commit creation; this is not Level-4 release.
