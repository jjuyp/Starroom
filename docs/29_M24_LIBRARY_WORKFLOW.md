# M24 Library / Workflow Architecture

Status: production implementation accepted on the v0.2 core-quality branch.

## Ownership and storage

`starroom-library` owns a local SQLite catalog. It opens with foreign keys, WAL and a bounded busy timeout and applies ordered `PRAGMA user_version` migrations. The database stores asset identity, paths, extracted metadata, workflow fields, keyword/collection relationships, project references and cache identities. Original image bytes and all raster/model caches remain outside SQLite.

Imports are reference imports: Starroom never moves or rewrites the source. A recursive native scan filters RAW/JPEG/PNG/TIFF extensions, computes `StarroomAssetFingerprintV1`, extracts metadata through the existing image/RAW providers, commits bounded batches and generates native, color-managed thumbnail files at 256/512/1024 pixels. The Tauri contract transports paths and typed records, never raster JSON.

## Identity and duplicates

Fingerprint V1 hashes the byte length plus deterministic beginning/middle/end sample ranges. Same path is `already_present`; a matching fingerprint whose original is online is a duplicate; a matching fingerprint whose original is missing is a relink candidate. Relink recomputes identity and returns `RelinkMismatch` when bytes differ. A full SHA-256 helper is reserved for candidate escalation and audit.

## Schema

- `assets`: source identity/path, fingerprint version/digest, filesystem facts, image/camera/lens/exposure metadata, rating, flag, color label, missing status, project and thumbnail identities.
- `keywords` and `asset_keywords`: trimmed, case-insensitive unique keywords and many-to-many assignment.
- `collections` and `collection_assets`: normal ordered membership or smart collection identity.
- `smart_collection_rules`: versioned typed AND predicates; the representation leaves room for grouped boolean rules.
- `library_settings`: schema/library metadata.

Queries are native, parameterized and stably ordered by asset id after capture/import/name/rating. Pagination prevents unbounded DOM construction. Normal collections reference assets; smart collections evaluate current metadata dynamically.

## Failures and recovery

Database/migration/busy/import/fingerprint/metadata/thumbnail/duplicate/missing/relink/query/collection/corruption failures are typed. Per-file failures are isolated. Cancellation stops between bounded work units. Missing files retain all catalog and project relationships.

## Acceptance

Tests cover create/reopen/migration, recursive mixed imports, duplicates and relink, workflow enums, keyword normalization/removal, normal/smart collections, combined queries, stable paging, cancellation/rollback, cache identity and project relationships. Synthetic metadata cases exercise 1,000 and 10,000-row query/paging behavior without fabricating large RAW files.
