# Getting started

## Build

Requires Rust (the toolchain is pinned by `rust-toolchain.toml`).

```sh
cargo build --release -p sitemap-cli
cargo test --workspace
```

The binary is `target/release/advanced-sitemap` (`.exe` on Windows).

## From the command line

`urls.jsonl`:

```json
{"loc":"https://example.com/","lastmod":"2026-11-21","priority":1.0}
{"loc":"https://example.com/about","lastmod":"2026-10-01T08:00:00+02:00"}
```

```sh
advanced-sitemap validate --input urls.jsonl --site https://example.com
advanced-sitemap generate --input urls.jsonl --out dist \
    --site https://example.com --public-base-url https://example.com/
```

`dist/` then contains `sitemap.xml` and `sitemap.manifest.json`. Beyond
50,000 URLs or 50 MiB, it contains `sitemap-1.xml`, `sitemap-2.xml` and so
on, plus the index `sitemap.xml`.

Any language can drive the engine this way: write JSONL, run the binary,
and read the JSON report with `--format json`.

## From Rust: one document in memory

```rust
use advanced_sitemap::{ChangeFreq, LanguageTag, Sitemap, SitemapUrl, UrlEntry, W3cDateTime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = UrlEntry::new(SitemapUrl::new("https://example.com/")?)
        .with_lastmod(W3cDateTime::parse("2026-11-21")?)
        .with_changefreq(ChangeFreq::Daily)
        .with_alternate(LanguageTag::parse("x-default")?, SitemapUrl::new("https://example.com/")?);
    let xml = Sitemap::builder().url(home).build()?.to_xml()?;
    println!("{xml}");
    Ok(())
}
```

## From Rust: any size, streamed

```rust
use advanced_sitemap::{
    records, Engine, EngineConfig, FsStore, PublicBaseUrl, RawUrlRecord, RunMode, SitemapUrl,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = EngineConfig::default();
    config.layout.public_base_url = Some(
        PublicBaseUrl::new(SitemapUrl::new("https://example.com/")?).map_err(std::io::Error::other)?,
    );
    let engine = Engine::new(config)?;

    // Any iterator works: a database cursor, a file reader, a generator...
    let input = (0..200_000).map(|i| RawUrlRecord::with_loc(format!("https://example.com/p/{i}")));

    let mut store = FsStore::new("dist");
    let report = engine.run(records(input), &mut store, RunMode::Publish)?;
    for diagnostic in &report.diagnostics {
        eprintln!("{diagnostic}");
    }
    println!("{:?}: {} URLs in {} files", report.status, report.stats.urls_written, report.artifacts.len());
    Ok(())
}
```

`Engine::run_entries` accepts already-typed `UrlEntry` values.
`MemoryStore` keeps the output in memory, and `NullStore` keeps nothing
(useful for dry runs).
