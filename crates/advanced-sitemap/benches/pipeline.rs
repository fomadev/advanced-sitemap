//! Criterion benchmarks of the engine pipeline.
//!
//! Run with `cargo bench -p advanced-sitemap`. Results depend on the machine;
//! see `docs/performance/README.md` for the measurement protocol.

#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

use std::hint::black_box;
use std::num::NonZeroU32;

use advanced_sitemap::{
    Engine, EngineConfig, NullStore, PartitionStrategy, PublicBaseUrl, RawAlternate, RawUrlRecord,
    RunMode, SitemapUrl, UrlEntry, W3cDateTime, records,
};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

/// Deterministic, realistic records: lastmod on every URL, alternates on 1 in 10.
fn dataset(n: usize) -> Vec<RawUrlRecord> {
    (0..n)
        .map(|i| {
            let loc = format!(
                "https://shop.example.com/products/item-{i}?ref=catalog&page={}",
                i % 50
            );
            let mut record = RawUrlRecord {
                lastmod: Some(format!(
                    "2026-{:02}-{:02}T10:00:00Z",
                    i % 12 + 1,
                    i % 28 + 1
                )),
                ..RawUrlRecord::with_loc(loc.clone())
            };
            if i % 10 == 0 {
                record.alternates = vec![
                    RawAlternate {
                        hreflang: Some("en".into()),
                        href: Some(loc.clone()),
                    },
                    RawAlternate {
                        hreflang: Some("fr".into()),
                        href: Some(format!("https://shop.example.com/fr/produits/item-{i}")),
                    },
                ];
            }
            record
        })
        .collect()
}

fn engine(strategy: PartitionStrategy) -> Engine {
    let mut config = EngineConfig {
        strategy,
        ..EngineConfig::default()
    };
    config.layout.public_base_url =
        Some(PublicBaseUrl::new(SitemapUrl::new("https://shop.example.com/").unwrap()).unwrap());
    Engine::new(config).unwrap()
}

fn full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline/validate+dedupe+partition+xml");
    group.sample_size(10);
    for n in [1_000usize, 10_000, 100_000] {
        let data = dataset(n);
        let engine = engine(PartitionStrategy::Sequential);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &data, |b, data| {
            b.iter(|| {
                let mut store = NullStore::new();
                let report = engine
                    .run(records(data.iter().cloned()), &mut store, RunMode::DryRun)
                    .unwrap();
                black_box(report.stats.urls_written)
            });
        });
    }
    group.finish();
}

fn strategies(c: &mut Criterion) {
    let n = 100_000;
    let data = dataset(n);
    let mut group = c.benchmark_group("pipeline/strategy-100k");
    group.sample_size(10);
    group.throughput(Throughput::Elements(n as u64));
    let cases = [
        ("sequential", PartitionStrategy::Sequential),
        (
            "stable-hash-8",
            PartitionStrategy::StableHash {
                partitions: NonZeroU32::new(8).unwrap(),
            },
        ),
    ];
    for (name, strategy) in cases {
        let engine = engine(strategy);
        group.bench_function(name, |b| {
            b.iter(|| {
                let report = engine
                    .run(
                        records(data.iter().cloned()),
                        &mut NullStore::new(),
                        RunMode::DryRun,
                    )
                    .unwrap();
                black_box(report.stats.urls_written)
            });
        });
    }
    group.finish();
}

fn xml_only(c: &mut Criterion) {
    let n = 100_000;
    let entries: Vec<UrlEntry> = (0..n)
        .map(|i| {
            UrlEntry::new(
                SitemapUrl::new(&format!("https://shop.example.com/products/item-{i}")).unwrap(),
            )
            .with_lastmod(W3cDateTime::parse("2026-11-21T10:00:00Z").unwrap())
        })
        .collect();
    let mut group = c.benchmark_group("xml/encode-100k");
    group.throughput(Throughput::Elements(n as u64));
    group.bench_function("encode_url", |b| {
        let mut buffer = Vec::with_capacity(64 * 1024 * 1024);
        b.iter(|| {
            buffer.clear();
            for entry in &entries {
                advanced_sitemap::xml::encode_url(entry, &mut buffer).unwrap();
            }
            black_box(buffer.len())
        });
    });
    group.finish();
}

fn validation_only(c: &mut Criterion) {
    let n = 100_000;
    let data = dataset(n);
    let validator = advanced_sitemap::Validator::new(Default::default());
    let mut group = c.benchmark_group("validation-100k");
    group.throughput(Throughput::Elements(n as u64));
    group.bench_function("validate", |b| {
        let mut out = Vec::new();
        b.iter(|| {
            let mut valid = 0;
            for (i, raw) in data.iter().enumerate() {
                out.clear();
                let location = advanced_sitemap::Location::record(i as u64 + 1, None);
                if validator.validate(raw, &location, &mut out).is_some() {
                    valid += 1;
                }
            }
            black_box(valid)
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    full_pipeline,
    strategies,
    xml_only,
    validation_only
);
criterion_main!(benches);
