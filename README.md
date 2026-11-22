# Advanced Sitemap

**One engine. Multiple languages. Every framework.**

Advanced Sitemap is a sitemap engine written in Rust. It validates URL data
with structured diagnostics, writes deterministic sitemap XML as a stream,
splits output exactly under the protocol limits, builds the sitemap index,
and publishes through a staging area. It does not depend on any web
framework. The same engine is meant to back the FomaDev integrations for
Next.js, Nuxt, Laravel, Django and others.

> Status: **0.1.0, pre-release.** Usable for real tests and supervised
> pilots, not yet for critical production: see the
> [production readiness audit](docs/production/readiness-matrix.md).
> Nothing is published to crates.io or npm. APIs and contracts may change
> before 1.0.

## What works today

- Validation of `loc`, `lastmod` (W3C Datetime), `changefreq`, `priority`,
  hreflang alternates, images, Google Video and Google News metadata, in
  `standard` and `strict` profiles. Every problem is reported with a stable code, a severity, a
  location (record, line, field) and a suggested fix.
- Streaming generation from JSON Lines with bounded memory. Without
  deduplication, memory stays constant: 5 MiB peak working set for 5 million
  URLs on the reference machine.
- Exact partitioning at 50,000 URLs and 50 MiB per file by default, with
  configurable lower limits. Three strategies: `sequential`; `stable-hash`,
  where a URL keeps its file across runs; and `consistent-hash`, which also
  keeps most URLs in place when the partition count grows (`explain` gives
  the exact count of URLs that would move).
- Delta mode: `apply` takes a feed of upserts and deletes and rewrites only
  the partitions it touches, with the same bytes as a full generation from
  the URLs sorted by `loc`.
- Automatic sitemap index, with each child's `lastmod` set to the latest
  `lastmod` of its entries.
- Staged publication (child sitemaps, then index, then manifest). Nothing is
  published when a run fails. Published files are never deleted; files that
  are no longer produced are reported. Each run takes an exclusive lock on
  its output (and state) directory, so concurrent runs cannot interleave.
- robots.txt generation (RFC 9309) listing the generated sitemap.
- Cross-entry hreflang checks (return links, conflicting language codes).
- Bounded, secure reading of existing sitemaps (`inspect`, plain or gzip),
  and comparison of two published sets (`diff`).
- URL-level snapshot (`--state-dir`) with a change report, and
  `--skip-unchanged`, which republishes only files whose bytes changed.
- Output that is identical byte for byte on every platform for the same input.
- A CLI with `generate`, `validate`, `explain`, `inspect`, `diff` and
  `apply`, human or JSON output, and stable exit codes.
- A versioned JSON protocol (`schemaVersion` 1.0 and 1.1) with JSON Schemas.

Not available yet: automatic choice between full, skip-unchanged and delta
runs, IndexNow, and the Node.js, Python and PHP bindings (separate work
after the Rust core is stable). Known limits are listed in
[docs/production/risk-register.md](docs/production/risk-register.md).

## Quick start (CLI)

```sh
cargo build --release -p sitemap-cli
# urls.jsonl: one record per line, e.g. {"loc":"https://example.com/","lastmod":"2026-11-21"}
./target/release/advanced-sitemap generate --input urls.jsonl --out dist \
    --site https://example.com --public-base-url https://example.com/
./target/release/advanced-sitemap explain --input urls.jsonl
```

## Quick start (Rust)

```rust
use advanced_sitemap::{Sitemap, SitemapUrl};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sitemap = Sitemap::builder()
        .url(SitemapUrl::new("https://example.com/")?)
        .build()?;
    println!("{}", sitemap.to_xml()?);
    Ok(())
}
```

For large or streamed inputs, use `Engine` (see
[docs/getting-started](docs/getting-started/README.md)).

## Documentation

| Topic | Location |
|---|---|
| Getting started | [docs/getting-started](docs/getting-started/README.md) |
| CLI and exit codes | [docs/cli](docs/cli/README.md) |
| JSON protocol | [docs/protocol](docs/protocol/README.md), [schemas/](schemas) |
| Diagnostic codes | [docs/concepts/diagnostics.md](docs/concepts/diagnostics.md) |
| Architecture and ADRs | [docs/architecture](docs/architecture/overview.md) |
| Performance | [docs/performance](docs/performance/README.md) |
| Security model | [docs/security](docs/security/README.md) |
| Production readiness | [docs/production](docs/production/audit.md) |
| Audit of the existing ecosystem | [docs/research](docs/research/existing-ecosystem.md) |

## Workspace

| Crate | Role |
|---|---|
| `sitemap-model` | Validated types, raw records, diagnostics, protocol limits |
| `sitemap-validation` | Raw record to typed entry, with diagnostics |
| `sitemap-xml` | Streaming XML writer |
| `sitemap-storage` | Staged artifact stores (file system, memory, null), directory lock |
| `sitemap-index` | Partitioning and index layout |
| `sitemap-protocol` | Versioned JSON documents and bounded readers |
| `sitemap-reader` | Bounded reader of existing sitemaps (XML or gzip), used by `inspect`, `diff` and `apply` |
| `sitemap-robots` | Validated, deterministic robots.txt (RFC 9309) |
| `sitemap-snapshot` | URL-level snapshot: BLAKE3 digests, sorted format, external sort, diff |
| `advanced-sitemap` | Public facade: engine, builder, planner |
| `sitemap-cli` | `advanced-sitemap` binary |

## License

Advanced Sitemap is **source-available** under the
[Fordima Public License (FPL) v1.0](LICENSE). It is not an OSI-approved open
source license.

In short: you may use it free of charge, including in commercial websites,
applications and CI pipelines, study the code and contribute. A commercial
license is required to build or sell a competing sitemap engine or a
"sitemap as a service" offering based on this code. The [LICENSE](LICENSE)
file is the only binding text. Proposed clarifications are under review in
[docs/legal/fpl-review.md](docs/legal/fpl-review.md) (French).
