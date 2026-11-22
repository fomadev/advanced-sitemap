//! Counts heap allocations per URL at each stage of the pipeline.
//!
//! ```text
//! cargo run --release --manifest-path tools/alloc-profile/Cargo.toml -- 200000
//! ```
//!
//! A measurement tool, not part of the engine: it wraps the system allocator
//! in a counter, which needs `unsafe`: this crate is outside the workspace,
//! whose crates forbid it.

#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use advanced_sitemap::protocol::JsonlReader;
use advanced_sitemap::{
    Engine, EngineConfig, Location, MemoryStore, NullStore, PublicBaseUrl, RunMode, SitemapUrl,
    SnapshotSettings, ValidationOptions, Validator, xml,
};

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);

fn grow(by: u64) {
    let live = LIVE.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: forwards every call to the system allocator unchanged and only
// updates two atomic counters.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        grow(layout.size() as u64);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        grow(new_size as u64);
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn dataset(count: u64) -> String {
    let mut out = String::with_capacity(count as usize * 130);
    for i in 0..count {
        let loc = format!("https://shop.example.com/products/item-{i}?ref=catalog&page={}", i % 50);
        let lastmod = format!("2026-{:02}-{:02}T10:00:00Z", i % 12 + 1, i % 28 + 1);
        if i % 10 == 0 {
            out.push_str(&format!(
                r#"{{"loc":"{loc}","lastmod":"{lastmod}","alternates":[{{"hreflang":"en","href":"{loc}"}},{{"hreflang":"fr","href":"https://shop.example.com/fr/produits/item-{i}"}}]}}"#
            ));
        } else {
            out.push_str(&format!(r#"{{"loc":"{loc}","lastmod":"{lastmod}"}}"#));
        }
        out.push('\n');
    }
    out
}

fn measure(label: &str, urls: u64, f: impl FnOnce()) {
    let (a0, b0) = (ALLOCATIONS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
    let start = Instant::now();
    f();
    let elapsed = start.elapsed().as_secs_f64();
    let a = ALLOCATIONS.load(Ordering::Relaxed) - a0;
    let b = BYTES.load(Ordering::Relaxed) - b0;
    println!(
        "{label:<44} {:>7.2} allocations/URL {:>7.0} bytes/URL {:>6.2} s",
        a as f64 / urls as f64,
        b as f64 / urls as f64,
        elapsed
    );
}

fn main() {
    let count: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(200_000);
    let text = dataset(count);
    let mut config = EngineConfig::default();
    config.layout.public_base_url =
        Some(PublicBaseUrl::new(SitemapUrl::new("https://shop.example.com/").unwrap()).unwrap());

    measure("decode JSONL", count, || {
        let n = JsonlReader::new(Cursor::new(text.as_bytes())).filter(|r| r.as_ref().unwrap().result.is_ok()).count();
        assert_eq!(n as u64, count);
    });
    let validator = Validator::new(ValidationOptions::default());
    let raws: Vec<_> = JsonlReader::new(Cursor::new(text.as_bytes()))
        .map(|r| r.unwrap().result.unwrap())
        .collect();
    let mut scratch = Vec::new();
    let mut entries = Vec::with_capacity(raws.len());
    measure("validate (raw record -> entry)", count, || {
        for (i, raw) in raws.iter().enumerate() {
            scratch.clear();
            entries.push(validator.validate(raw, &Location::record(i as u64, None), &mut scratch).unwrap());
        }
    });
    measure("  of which: URL parsing (loc only)", count, || {
        for raw in &raws {
            std::hint::black_box(SitemapUrl::new(raw.loc.as_deref().unwrap()).unwrap());
        }
    });
    measure("  of which: lastmod parsing", count, || {
        for raw in &raws {
            std::hint::black_box(advanced_sitemap::W3cDateTime::parse(raw.lastmod.as_deref().unwrap()).unwrap());
        }
    });
    let mut block = Vec::with_capacity(1024);
    measure("encode <url>", count, || {
        for entry in &entries {
            block.clear();
            xml::encode_url(entry, &mut block).unwrap();
        }
    });
    drop((raws, entries));

    let engine = Engine::new(config.clone()).unwrap();
    measure("engine validate (whole pipeline, null store)", count, || {
        let source = JsonlReader::new(Cursor::new(text.as_bytes()));
        engine.run(source, &mut NullStore::new(), RunMode::DryRun).unwrap();
    });
    let mut no_dedupe = config.clone();
    no_dedupe.dedupe = false;
    let engine = Engine::new(no_dedupe).unwrap();
    measure("engine validate, no dedupe", count, || {
        let source = JsonlReader::new(Cursor::new(text.as_bytes()));
        engine.run(source, &mut NullStore::new(), RunMode::DryRun).unwrap();
    });
    let engine = Engine::new(config.clone()).unwrap();
    measure("engine generate (memory store)", count, || {
        let source = JsonlReader::new(Cursor::new(text.as_bytes()));
        engine.run(source, &mut MemoryStore::new(), RunMode::Publish).unwrap();
    });
    let state = std::env::temp_dir().join(format!("alloc-profile-{}", std::process::id()));
    let mut with_snapshot = config;
    with_snapshot.snapshot = Some(SnapshotSettings::new(&state));
    let engine = Engine::new(with_snapshot).unwrap();
    measure("engine generate + snapshot (memory store)", count, || {
        let source = JsonlReader::new(Cursor::new(text.as_bytes()));
        engine.run(source, &mut MemoryStore::new(), RunMode::Publish).unwrap();
    });
    let _ = std::fs::remove_dir_all(state);

    // Memory held by a change feed: the operations are kept until the run
    // ends (one entry per changed URL).
    let ops = count as usize;
    let mut feed = String::with_capacity(ops * 120);
    for line in text.lines().take(ops) {
        feed.push_str(r#"{"op":"upsert","url":"#);
        feed.push_str(line);
        feed.push_str("}
");
    }
    let resolved = advanced_sitemap::ResolvedConfig::new(
        {
            let mut c = EngineConfig::default();
            c.strategy = advanced_sitemap::PartitionStrategy::ConsistentHash {
                partitions: std::num::NonZeroU32::new(8).unwrap(),
            };
            c.snapshot = Some(SnapshotSettings::new(std::env::temp_dir().join("alloc-profile-none")));
            c
        },
        None,
        std::env::temp_dir().join("alloc-profile-none-out"),
    );
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let feed_reader = advanced_sitemap::protocol::JsonlReader::<_, advanced_sitemap::protocol::ChangeRecord>::typed(
        Cursor::new(feed.as_bytes()),
        1 << 20,
    );
    let plan = advanced_sitemap::plan::explain_apply(
        &resolved,
        feed_reader,
        &advanced_sitemap::plan::CostModel::REFERENCE,
    )
    .unwrap();
    assert_eq!(plan.chosen, "none");
    let peak = PEAK.load(Ordering::Relaxed) - base;
    println!(
        "{:<44} {:>7.0} bytes/operation at peak ({} operations)",
        "change feed held in memory",
        peak as f64 / ops as f64,
        ops
    );
}
