# M30 Release Blockers

Status vocabulary is intentionally closed: `OPEN`, `FIXED`, `VERIFIED`, `EXTERNAL`.

- `OPEN`: product or release work remains.
- `FIXED`: production implementation and local fast validation exist, but the RC-candidate HEAD has
  not yet passed its authoritative gate.
- `VERIFIED`: immutable acceptance evidence exists for the exact recorded HEAD.
- `EXTERNAL`: the remaining condition is outside the repository and cannot be repaired by changing
  Starroom. External status never converts a product failure into success.

| ID | Status | Blocker | Required evidence |
| --- | --- | --- | --- |
| M30-R01 | FIXED | Identity Geometry and Detail stages previously copied/resampled neutral full frames. | Small old-path parity, exact buffer reuse, profiler-stage retention and targeted render tests are implemented; candidate CI verification remains required. |
| M30-R02 | OPEN | Real 24/45/60/100 MP open, bounded preview, Mask, Healing and full-resolution Export. | RC workflow log with dimensions, immutable-source assertion, elapsed time and process peak memory. |
| M30-R03 | FIXED | Installed production workflow self-test. | Exact candidate installer must run Library -> History/Snapshot -> Session -> two byte-identical Native exports and uninstall. |
| M30-R04 | FIXED | Offline AI availability contract. | Installed self-test must report bundled models available or unbundled models `typed-unavailable`; no download/cloud/API fallback. |
| M30-R05 | FIXED | Session, History, Project, Library and Export migration/corruption/recovery matrix. | Locked workspace tests and installed recovery path pass on the candidate HEAD. |
| M30-R06 | FIXED | Dependency redistribution and installed legal resources. | Cargo/package lock identities, reviewed closure, 268 deduplicated upstream texts and installed SHA-256 checks pass. |
| M30-R07 | FIXED | Privacy/offline production-source policy. | Release validator and installed self-test prove no telemetry, hidden upload, cloud or API-key dependency. |
| M30-R08 | FIXED | Photographic Golden corpus remains manifest-only for several required scenes. | Five immutable public-domain/CC assets now cover all photographic cases, provenance/hash validation is mandatory and a shared Native full-pipeline regression proves identity Preview/Export parity, finite output, deterministic edits and source immutability. Candidate CI remains required. |
| M30-R09 | VERIFIED | M28 accepted scale/cache/history/library/batch and current GPU-capable Exposure parity baseline. | M28 acceptance `94a9ccca81bf874fbe8013e694f1e8f0e691fda9`, run `32942892981`; CPU-only stages remain explicit, not fake GPU paths. |
| M30-R10 | OPEN | Exact-candidate Windows MSVC executable, NSIS clean install/runtime/self-test/uninstall and artifact hashes. | One successful Release Candidate Gate for the final candidate SHA. |
| M30-R11 | EXTERNAL | GitHub-hosted Windows jobs are currently returning zero-job `startup_failure` or remaining queued. | Retry after hosted runner recovery. Examples: `32985982566`, `32985843647`, `32985840314`. |
| M30-R12 | EXTERNAL | Physical 100/125/150/200% HiDPI and mixed-DPI monitor movement requires qualified display hardware and human observation. | Signed field-validation record; automated coordinate/DPI invariants remain mandatory in Level-4. |
| M30-R13 | OPEN | RC version synchronization, unique candidate HEAD, final Level-4 run, tag and artifacts. | Every product blocker `VERIFIED`, one shared HEAD for every Tier-4 result, then `v1.0.0-rc.1`. |

## Gate discipline

No row may be deleted to make acceptance green. `FIXED` becomes `VERIFIED` only from evidence for
the same immutable RC-candidate HEAD. A GitHub infrastructure incident is recorded as `EXTERNAL`
and retried later; it is not a Starroom regression and does not authorize tagging.
