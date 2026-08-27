# Development Acceleration and Acceptance

## Three-level ladder

1. **Level 1 / targeted:** `npm run test:<target>` runs the owning Rust crate/module, direct regressions, related Vitest files and validates the selected Golden fixture contract. Use `-- --rust-only` or `-- --web-only` for a single side.
2. **Level 2 / milestone:** `npm run test:milestone -- <target>` adds the Native shared graph, render invalidation, format, warning-denied Clippy, lint and production frontend build.
3. **Level 3 / full:** `npm run test:full` runs JSON/Golden validation, format, full workspace Clippy/tests, all Vitest, lint, production build and Tauri packaging-configuration/artifact validation. Use only for batch/release/final acceptance. It remains authoritative if a narrower gate differs.

Targets are `color`, `tone`, `curve`, `raw`, `detail`, `optics`, `geometry`, `gpu`, `tiles`, `masks`, `portrait`, `ai`, and `web`. Each invocation writes a machine-readable report under ignored `.starroom-reports/`.

## Golden selection

`npm run golden:select -- --tags=color,portrait,skin,neon,landscape` selects the union relevant to a future M7. `--all-tags` selects the intersection. With no tags it returns the full set. Selection never promotes a `planned` source to `active` and Full Acceptance validates the whole manifest plus all active fixture assets.

## CI and caching

The `changes` job classifies the Git diff with the dependency map. Leaf changes run only relevant checks; shared pipeline/render/project/workspace/CI changes broaden all targeted categories. A `[full-acceptance]` push, release tag, or manual Full input runs both Full Rust and Full Web gates. PR #2 can therefore retain an authoritative Full Check on its acceptance commit without running Level 3 for every development push.

Windows Cargo caches include registry, git checkout, compiled `target` output (including LibRaw and LittleCMS native build products) and reserved Lensfun/model cache roots. Keys include OS, architecture, complete `rustc -Vv` hash, `Cargo.lock` hash and target class. The npm cache is keyed by `package-lock.json` through `setup-node`; `npm ci` still verifies/reconstructs `node_modules`, so stale installed trees are not restored. Each job prints cache hit/miss and uploads timing JSON.

## Commit and waiting policy

Development commits contain code plus tests. TODO/notes/provenance/roadmap are synchronized at milestone acceptance or when dependency/license/architecture changes. CI execution is not an inner-loop stopping point: continue all local work, then wait only for the final required Full Check. Never weaken regression coverage, Native Preview/Export parity, RAW correctness, color accuracy, finite-value contracts or typed errors to gain speed.

## Timing report contract

Reports record command, exit code and wall duration for targeted Rust, RAW, Golden subset/full, Vitest and build stages. CI uploads them per job. Compare warm-cache runs with the same lockfiles/compiler/platform; cold and warm numbers must not be mixed. The acceptance baseline and remaining bottlenecks are recorded in `docs/IMPLEMENTATION_NOTES.md`.
