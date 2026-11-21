# Changelog

All notable changes are documented here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0, minor versions may
contain breaking changes.

## [Unreleased]

### Added

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
