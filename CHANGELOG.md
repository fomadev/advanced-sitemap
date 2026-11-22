# Changelog

All notable changes are documented here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0, minor versions may
contain breaking changes.

## [Unreleased]

### Added

- Lot D (security): reproducible fuzzing on stable Rust (`tests/fuzz.rs`,
  snapshot decoder) run with the tests and longer in CI, coverage-guided
  `cargo fuzz` targets with seed corpora (`fuzz/`, weekly workflow),
  GitHub Actions pinned by commit hash, Dependabot. Security report in
  `docs/production/security-report.md`.

- Lot C (performance): `state.maxFeedOperations` / `apply --max-operations`
  bound the change feed held in memory (default 500,000 changed URLs,
  about 400 MB); a larger feed is refused before writing. Resource limits
  are listed in `docs/concepts/limits.md`. `tools/alloc-profile` counts
  allocations per pipeline stage. Measurements at 10 M URLs.
- The planner's cost model is recalibrated on one consistent series of
  runs (full generations were overestimated up to twice).

- Lot B (planner): execution modes `full`, `skip-unchanged` and `auto`
  (`output.publish = "auto"`, `--execution-mode`); the report's `execution`
  records the mode and why. `explain` gives an execution plan (candidate
  modes, preconditions, estimated time, bytes, memory, partitions) with a
  state token; `--expect-state` refuses a run planned on another state
  (`EngineError::StateChanged`, exit code 2). `explain --changes` plans an
  `apply` without writing. Cost model `plan::CostModel`.
- Manifest entries and artifact reports carry the BLAKE3-128 digest of
  each file.

### Fixed

- A control character forbidden by XML in a news or video text passed
  validation, then stopped the run with an internal error in the XML
  writer. It is now a diagnostic (`NEWS_TEXT_INVALID`,
  `VIDEO_TEXT_INVALID`, `VIDEO_UPLOADER_INVALID`). Found by fuzzing.
- A run interrupted after publishing, before the snapshot was replaced,
  is detected: snapshot format v2 records the manifest digest of the
  publication it describes; `SNAPSHOT_STALE` warns and `apply` refuses.
- `explain` takes `--out`: it used to plan for another directory than
  `generate` and `apply` without a configuration file.

- Concurrent runs on the same output or state directory are refused
  (`EngineError::Busy`, CLI exit code 3, kind `busy`): they could
  interleave publications, and two `apply` runs could lose one feed's
  changes. The lock is an operating system lock, released on exit.
- Staging directories left by a killed run are removed by the next run.
- On Unix, the output and state directories are synced after renames.
- The declared minimum Rust version (1.85) was wrong: it is now 1.89 and
  checked in CI.

### Changed

- Every crate forbids unsafe code. Production readiness audit in
  `docs/production/`.
- Snapshot format v2 (v1 still read). `PublishPolicy` and
  `PublishSetting` gain `Auto`; `EngineConfig` gains `expected_state`;
  `EngineError` gains `StateChanged`; `GenerationReport` gains `execution`;
  `ArtifactReport` and `ManifestArtifact` gain `digest`; `PartitionSummary`
  gains `digest`; `Plan` gains `execution`.
- `ArtifactStore` gains `lock()` (default: no-op); `sitemap-storage` gains
  `DirectoryLock`, `LOCK_FILE` and `sync_dir`; `EngineError` gains `Busy`.

- Snapshot I/O is hashed and copied in 64 KiB blocks (same format): the
  `--state-dir` overhead at 1 M URLs fell from 4 to 5.5 s to 3.0 to 3.4 s,
  and `apply` with one operation from about 5 s to 3.3 s. `apply` merges
  the unchanged records of the previous snapshot instead of sorting them
  again (`SnapshotBuilder::finish_merged`).

### Added

- Phase D of the Lifecycle Engine, the delta mode: `ChangeRecord` feed
  format (`upsert`, `delete`; `schemas/change-record.v1.schema.json`),
  `Engine::apply_changes` and the CLI `apply` command. Only the touched
  partitions are read back, checked against the snapshot and rewritten;
  the result equals a full generation from the URLs sorted by `loc`.
  Refusals are `EngineError::DeltaRefused` (exit code 2).
- Round-trip property test on 3,000 random entries.

- Phase C of the Lifecycle Engine: `consistent-hash` partitioning strategy
  (jump consistent hash over FNV-1a 64, frozen by test vectors). Growing
  from N to N+1 partitions moves about 1/(N+1) of the URLs.
- `explain` gains a `migration` section when a previous snapshot exists:
  exact count of URLs changing file for hash strategies, notes
  `PARTITION_MIGRATION_COST` and `PARTITION_HEADROOM_LOW`.

- CLI `diff OLD NEW`: compares two sets of published sitemaps (files or
  directories) after canonical re-encoding, with bounded memory, exact
  counts, URL lists and `--fail-on-change`. Facade module
  `advanced_sitemap::diff`.

- Phase B of the Lifecycle Engine: `sitemap-snapshot` crate (BLAKE3-128
  digests, sorted prefix-compressed binary snapshot with an integrity
  trailer, external sort with a memory budget, streaming diff) and
  `state.directory` / `--state-dir`, which add a `changes` section to the
  report (added, removed, modified, moved, unchanged, partitions touched).
  New diagnostic `SNAPSHOT_INVALID`.

- Phase A of the Lifecycle Engine: `output.publish = "changed"` and
  `generate --skip-unchanged` republish only files whose bytes change, with
  the same published result. `FsStore` compares each new artifact with the
  published file while writing it and writes nothing while they match.
  Reports mark such artifacts `"unchanged": true`. Measured: an unchanged
  5 M URL rerun takes 22.6 s instead of 44 s or more.
- `ArtifactStore::set_compare_with_published` and
  `ArtifactStore::staged_matches_published`, with safe default
  implementations.

- Design of the Lifecycle Engine (proposed, not implemented):
  `docs/architecture/lifecycle/` (invariants I1 to I10, phases A to D,
  snapshot and diff, incremental generation, stable partitioning, planner
  and explain) and ADR-0010 to ADR-0013.

- Optional hreflang reciprocity check (`validation.hreflangReciprocity`,
  `--hreflang-reciprocity`): missing return links, conflicting language
  codes and links to URLs outside the run, as warnings and info. Measured
  cost about 110 bytes per alternate link.

- `sitemap-robots` crate and the `robots` configuration section: validated,
  deterministic robots.txt (RFC 9309) published with the sitemaps, listing
  the generated sitemap or index, with 7 new diagnostic codes
  (`ROBOTS_*`) and a `robots` conformance fixture.

- Google News extension: `news` in URL records (`output.extensions:
  ["news"]`), 5 new diagnostic codes (`NEWS_*`), XML in the order of
  Google's `sitemap-news/0.9` XSD, reading by `inspect`, and the 1,000
  news-per-file limit in partitioning. `news` conformance fixture validated
  against Google's XSD in CI.

- Google video extension: `videos` in URL records, validation with 15 new
  diagnostic codes (`EXTENSION_NOT_DECLARED`, `VIDEO_*`), XML output in the
  order of Google's `sitemap-video/1.1` XSD, and reading by `inspect`.
- Protocol 1.1: `output.extensions` declares extra namespaces (`video`).
  1.0 documents are still accepted and produce the same bytes.
- `video` and `video-undeclared` conformance fixtures; CI validates the
  video fixture against Google's XSD.

- `sitemap-reader` crate and CLI `inspect` command: bounded streaming reader
  for existing sitemaps and sitemap indexes, plain or gzip, that refuses
  DTDs, caps the uncompressed size, depth and text length, checks the XSD
  element order and validates every entry with the generator rules.
  12 new diagnostic codes (`XML_*`, `SITEMAP_TOO_LARGE`,
  `SITEMAP_URL_LIMIT_EXCEEDED`, `GZIP_INVALID`, `PRIORITY_INVALID`).
- `schemas/inspect-report.v1.schema.json`.

### Changed

- `FsStore` now calls `fsync` on each file right before renaming it into
  place, instead of at the end of its staging write: same durability, no
  cost for staged files that are never published.
- `ArtifactReport` gains `unchanged`; `EngineConfig` gains `publish`;
  `OutputSection` gains `publish`.
- `EngineError` gains `DeltaRefused` (exhaustive matches must add it).
  `JsonlReader` and `SourceRecord` gain a type parameter defaulting to
  `RawUrlRecord`. `Partitioner` gains `finish_partial`, `close_partition`
  and `store`; `sitemap-snapshot` gains `SequenceDigest`.
- `PartitionStrategy` gains `ConsistentHash` (exhaustive matches must add
  it), `StrategySetting` gains `ConsistentHash`, `Plan` gains `migration`.
  `PushError` messages say "hash partition" instead of "stable-hash
  partition".
- `EngineConfig` gains `snapshot`, `ConfigDocument` gains `state`,
  `GenerationReport` gains `changes`, `Partitioner` gains `last_block`.
- `UrlEntry` has a new `videos` field and `Namespaces` a new `video` field:
  code that builds them with struct literals must add the field.
  `Namespaces::DEFAULT` (`xhtml` and `image`) is what the engine declares by
  default; `Namespaces::ALL` now includes `video`.
- `UrlEntry` gains `news`, `Namespaces` gains `news`, `PushError` gains
  `NewsLimit`, and `InspectReport` gains `with_videos`, `with_news` and
  `news_elements`.
- `ArtifactKind` gains `Robots`; `EngineConfig` and `ConfigDocument` gain
  `robots`. `EngineConfig` and `ValidationSection` gain
  `hreflang_reciprocity`.
- `quick-xml` is now a runtime dependency (of `sitemap-reader`); `flate2`
  (pure Rust backend) is added. See ADR-0009.

- Cargo workspace with eight crates (model, validation, xml, storage, index,
  protocol, facade, CLI).
- Validation of URLs, W3C Datetime `lastmod`, `changefreq`, `priority`,
  hreflang alternates and images, with 29 stable diagnostic codes and
  `standard`/`strict` profiles.
- Deterministic streaming XML writer that rejects characters forbidden by
  XML 1.0.
- Exact `sequential` partitioning and `stable-hash` partitioning (FNV-1a 64),
  automatic sitemap index, manifest, and `STALE_ARTIFACT` reporting.
- Staged publication for the local file system, plus memory and null stores.
- JSON protocol v1.0 (configuration, URL record, report) with JSON Schemas.
- CLI `advanced-sitemap` with `generate`, `validate` and `explain`.
- Conformance fixtures, unit, integration, property-style and CLI tests,
  Criterion benchmarks and a dataset generator.
