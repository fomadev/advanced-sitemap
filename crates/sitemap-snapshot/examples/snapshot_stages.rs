//! Times each stage of a snapshot on N records: building (sort and spill),
//! writing, reading back and diffing two snapshots.
//!
//! cargo run --release -p sitemap-snapshot --example snapshot_stages -- 1000000 DIR [sorted]

#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

use sitemap_snapshot::{Record, SnapshotBuilder, SnapshotReader, diff, digest128};

fn records(n: u64, sorted: bool) -> Vec<Record> {
    let mut out: Vec<Record> = (0..n)
        .map(|i| {
            // Scattered order unless sorted, like a real source.
            let id = if sorted { i } else { (i * 7_919) % n };
            Record::new(
                format!(
                    "https://shop.example.com/catalog/category-{}/product-{id}.html",
                    id % 97
                ),
                digest128(&id.to_le_bytes()),
                (id % 24) as u32 + 1,
            )
        })
        .collect();
    if sorted {
        out.sort();
    }
    out
}

fn build(dir: &PathBuf, records: Vec<Record>, out: &PathBuf) -> (f64, f64, usize) {
    let start = Instant::now();
    let mut builder = SnapshotBuilder::new(dir, 64 << 20);
    for record in records {
        builder.push(record).unwrap();
    }
    let pushed = start.elapsed().as_secs_f64();
    let runs = builder.spilled_runs();
    let mut file = BufWriter::new(File::create(out).unwrap());
    builder.finish(&[0; 16], &mut file).unwrap();
    file.flush().unwrap();
    let written = start.elapsed().as_secs_f64();
    file.into_inner().unwrap().sync_all().unwrap();
    println!("  sync: {:.2} s", start.elapsed().as_secs_f64() - written);
    (pushed, written - pushed, runs)
}

fn open(path: &PathBuf) -> SnapshotReader<BufReader<File>> {
    SnapshotReader::new(BufReader::with_capacity(
        256 * 1024,
        File::open(path).unwrap(),
    ))
    .unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: u64 = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);
    let dir = PathBuf::from(
        args.get(2)
            .cloned()
            .unwrap_or_else(|| "bench-output/snap".into()),
    );
    let sorted = args.get(3).is_some_and(|s| s == "sorted");
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.snapshot"), dir.join("b.snapshot"));

    let input = records(n, sorted);
    let (push, finish, runs) = build(&dir, input, &a);
    println!("build: push {push:.2} s, finish (merge + write) {finish:.2} s, {runs} runs spilled");
    let mut other = records(n, sorted);
    other[n as usize / 2].content = [1; 16];
    build(&dir, other, &b);
    println!("size: {} bytes", std::fs::metadata(&a).unwrap().len());

    let start = Instant::now();
    let count = open(&a).map(Result::unwrap).count();
    println!(
        "read: {:.2} s ({count} records)",
        start.elapsed().as_secs_f64()
    );

    let start = Instant::now();
    let set = diff(open(&a), open(&b), 10).unwrap();
    println!(
        "diff: {:.2} s ({} modified)",
        start.elapsed().as_secs_f64(),
        set.modified
    );
    let _ = std::fs::remove_dir_all(&dir);
}
