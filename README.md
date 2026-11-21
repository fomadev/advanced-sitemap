# Advanced Sitemap

**One engine. Multiple languages. Every framework.**

Advanced Sitemap is a sitemap engine written in Rust. It validates URL data
with structured diagnostics, writes deterministic sitemap XML as a stream,
splits output exactly under the protocol limits, builds the sitemap index,
and publishes through a staging area. It does not depend on any web
framework. The same engine is meant to back the FomaDev integrations for
Next.js, Nuxt, Laravel, Django and others.

> Status: **0.1.0, pre-release MVP.** Nothing is published to crates.io or
> npm yet. APIs and contracts may change before 1.0.

## What works today

- Validation of `loc`, `lastmod` (W3C Datetime), `changefreq`, `priority`,
  hreflang alternates and image locations, in `standard` and `strict`
  profiles. Every problem is reported with a stable code, a severity, a
  location (record, line, field) and a suggested fix.
- Streaming generation from JSON Lines with bounded memory. Without
  deduplication, memory stays constant: 5 MiB peak working set for 5 million
  URLs on the reference machine.
- Exact partitioning at 50,000 URLs and 50 MiB per file by default, with
  configurable lower limits. Two strategies: `sequential`, and `stable-hash`,
  where a URL keeps its file across runs.
- Automatic sitemap index, with each child's `lastmod` set to the latest
  `lastmod` of its entries.
- Staged publication (child sitemaps, then index, then manifest). Nothing is
  published when a run fails. Published files are never deleted; files that
  are no longer produced are reported.
- Output that is identical byte for byte on every platform for the same input.
- A CLI with `generate`, `validate` and `explain`, human or JSON output, and
  stable exit codes.
- A versioned JSON protocol (`schemaVersion: "1.0"`) with JSON Schemas.

Planned: video and news extensions, robots.txt, IndexNow, `inspect`/`diff`
of existing sitemaps, incremental generation, Node.js and Python bindings.
See [docs/architecture/mvp-scope.md](docs/architecture/mvp-scope.md).

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
| Audit of the existing ecosystem | [docs/research](docs/research/existing-ecosystem.md) |

## Workspace

| Crate | Role |
|---|---|
| `sitemap-model` | Validated types, raw records, diagnostics, protocol limits |
| `sitemap-validation` | Raw record to typed entry, with diagnostics |
| `sitemap-xml` | Streaming XML writer |
| `sitemap-storage` | Staged artifact stores (file system, memory, null) |
| `sitemap-index` | Partitioning and index layout |
| `sitemap-protocol` | Versioned JSON documents and bounded readers |
| `sitemap-reader` | Bounded reader of existing sitemaps (XML or gzip), used by `inspect` |
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
