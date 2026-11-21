//! Writes a deterministic JSONL dataset for benchmarks.
//!
//! ```text
//! cargo run --release -p advanced-sitemap --example make_dataset -- 1000000 bench-output/1m.jsonl
//! ```
//!
//! Every record has a `lastmod`; one in ten has two hreflang alternates.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(count), Some(path)) = (args.get(1).and_then(|s| s.parse::<u64>().ok()), args.get(2))
    else {
        eprintln!("usage: make_dataset <count> <output.jsonl>");
        return ExitCode::from(2);
    };
    match write(count, path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("make_dataset: {error}");
            ExitCode::from(3)
        }
    }
}

fn write(count: u64, path: &str) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = BufWriter::with_capacity(1 << 20, File::create(path)?);
    for i in 0..count {
        let loc = format!(
            "https://shop.example.com/products/item-{i}?ref=catalog&page={}",
            i % 50
        );
        let lastmod = format!("2026-{:02}-{:02}T10:00:00Z", i % 12 + 1, i % 28 + 1);
        if i % 10 == 0 {
            writeln!(
                out,
                r#"{{"loc":"{loc}","lastmod":"{lastmod}","alternates":[{{"hreflang":"en","href":"{loc}"}},{{"hreflang":"fr","href":"https://shop.example.com/fr/produits/item-{i}"}}]}}"#
            )?;
        } else {
            writeln!(out, r#"{{"loc":"{loc}","lastmod":"{lastmod}"}}"#)?;
        }
    }
    out.flush()
}
