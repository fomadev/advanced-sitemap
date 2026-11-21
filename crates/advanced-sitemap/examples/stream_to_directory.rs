//! Streams 200,000 generated URLs into a directory (the getting-started example).
//!
//! ```text
//! cargo run --release -p advanced-sitemap --example stream_to_directory -- dist
//! ```

use advanced_sitemap::{
    Engine, EngineConfig, FsStore, PublicBaseUrl, RawUrlRecord, RunMode, SitemapUrl, records,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "dist".to_owned());

    let mut config = EngineConfig::default();
    config.layout.public_base_url = Some(
        PublicBaseUrl::new(SitemapUrl::new("https://example.com/")?)
            .map_err(std::io::Error::other)?,
    );
    let engine = Engine::new(config)?;

    // Any iterator works: a database cursor, a file reader, a generator...
    let input = (0..200_000).map(|i| RawUrlRecord::with_loc(format!("https://example.com/p/{i}")));

    let mut store = FsStore::new(&out);
    let report = engine.run(records(input), &mut store, RunMode::Publish)?;
    for diagnostic in &report.diagnostics {
        eprintln!("{diagnostic}");
    }
    println!(
        "{:?}: {} URLs in {} files under {out}",
        report.status,
        report.stats.urls_written,
        report.artifacts.len()
    );
    Ok(())
}
