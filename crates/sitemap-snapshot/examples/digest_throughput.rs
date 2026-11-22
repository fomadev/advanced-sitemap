//! Measures BLAKE3 throughput on this machine (ADR-0011).
//!
//! ```text
//! cargo run --release -p sitemap-snapshot --example digest_throughput
//! ```

use std::time::Instant;

fn main() {
    // Large buffer: whole-file digests (partitions).
    let data = vec![0x5au8; 256 << 20];
    let start = Instant::now();
    let hash = blake3::hash(&data);
    let secs = start.elapsed().as_secs_f64();
    println!(
        "256 MiB in one call: {:.0} MiB/s ({})",
        256.0 / secs,
        &hash.to_hex()[..8]
    );

    // Small inputs: one <url> block of about 170 bytes, as for entry digests.
    let block = [0x5au8; 170];
    let n = 2_000_000u32;
    let start = Instant::now();
    let mut acc = 0u8;
    for i in 0..n {
        let mut b = block;
        b[0] = i as u8;
        acc ^= blake3::hash(&b).as_bytes()[0];
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
        "{n} blocks of 170 bytes: {:.2} M blocks/s, {:.0} MiB/s (checksum {acc})",
        f64::from(n) / secs / 1e6,
        f64::from(n) * 170.0 / secs / f64::from(1 << 20)
    );
}
