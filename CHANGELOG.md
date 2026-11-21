# Changelog

All notable changes are documented here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0, minor versions may
contain breaking changes.

## [Unreleased]

### Added

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
