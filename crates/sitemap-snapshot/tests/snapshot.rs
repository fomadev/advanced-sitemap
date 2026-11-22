#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::fs;
use std::io;
use std::path::PathBuf;

use sitemap_snapshot::{
    ChangeSet, Record, SnapshotBuilder, SnapshotReader, diff, digest128, to_hex,
};

fn temp(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("snapshot-test-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn record(loc: &str, version: u8, partition: u32) -> Record {
    Record {
        loc: loc.to_owned(),
        content: digest128(&[version]),
        partition,
    }
}

fn build(records: &[Record], budget: usize, dir: &PathBuf) -> (Vec<u8>, usize) {
    let mut builder = SnapshotBuilder::new(dir, budget);
    for r in records {
        builder.push(r.clone()).unwrap();
    }
    let runs = builder.spilled_runs();
    let mut out = Vec::new();
    builder.finish(&digest128(b"config"), &mut out).unwrap();
    (out, runs)
}

fn read(bytes: &[u8]) -> io::Result<Vec<Record>> {
    SnapshotReader::new(bytes)?.collect()
}

#[test]
fn digest_vectors_are_frozen() {
    // First 16 bytes of the official BLAKE3 test vector for empty input.
    assert_eq!(to_hex(&digest128(b"")), "af1349b9f5f9a1a6a0404dea36dcc949");
    // Frozen value for a URL: changing it would invalidate every snapshot.
    // Computed once with this implementation, which matches the official
    // vector above; it must never change.
    assert_eq!(
        to_hex(&digest128(b"https://example.com/")),
        "19f876713c227a568d939946496a0d30"
    );
}

#[test]
fn spilling_gives_the_same_bytes_and_sorted_records() {
    let dir = temp("spill");
    // Pseudo-random order, with a duplicate loc (deduplication off).
    let mut records: Vec<Record> = (0..5_000u32)
        .map(|i| {
            let k = (i * 7_919) % 5_000;
            record(
                &format!("https://example.com/p/{k}"),
                (k % 3) as u8,
                k % 9 + 1,
            )
        })
        .collect();
    records.push(record("https://example.com/p/42", 9, 1));

    let (in_memory, runs0) = build(&records, usize::MAX, &dir);
    let (spilled, runs) = build(&records, 4_096, &dir);
    assert_eq!(runs0, 0);
    assert!(runs > 10, "expected many runs, got {runs}");
    assert_eq!(
        in_memory, spilled,
        "external sort must not change the output"
    );

    let back = read(&spilled).unwrap();
    assert_eq!(back.len(), records.len());
    let mut sorted = records.clone();
    sorted.sort();
    assert_eq!(back, sorted);
    // Temporary run files are removed.
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn damaged_snapshots_are_rejected() {
    let dir = temp("damaged");
    let records: Vec<Record> = (0..100)
        .map(|i| record(&format!("https://example.com/{i}"), 1, 1))
        .collect();
    let (bytes, _) = build(&records, usize::MAX, &dir);
    assert!(read(&bytes).is_ok());

    let truncated = &bytes[..bytes.len() - 40];
    assert!(
        read(truncated)
            .unwrap_err()
            .to_string()
            .contains("truncated")
    );

    let mut flipped = bytes.clone();
    flipped[200] ^= 1;
    assert!(read(&flipped).is_err());

    let mut magic = bytes.clone();
    magic[0] = b'X';
    assert!(SnapshotReader::new(&magic[..]).is_err());

    assert!(SnapshotReader::new(&b"short"[..]).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn diff_counts_lists_and_partitions() {
    let old = vec![
        record("https://example.com/a", 1, 1),
        record("https://example.com/b", 1, 1),
        record("https://example.com/c", 1, 2),
        record("https://example.com/d", 1, 2),
    ];
    let new = vec![
        record("https://example.com/a", 1, 1),
        record("https://example.com/b", 2, 1),
        record("https://example.com/d", 1, 3),
        record("https://example.com/e", 1, 3),
    ];
    let ok = |v: &Vec<Record>| v.clone().into_iter().map(Ok);
    let set = diff(ok(&old), ok(&new), 10).unwrap();
    assert_eq!(
        (
            set.added,
            set.removed,
            set.modified,
            set.moved,
            set.unchanged
        ),
        (1, 1, 1, 1, 1)
    );
    assert_eq!(set.added_urls, ["https://example.com/e"]);
    assert_eq!(set.removed_urls, ["https://example.com/c"]);
    assert_eq!(set.modified_urls, ["https://example.com/b"]);
    assert_eq!(
        set.partitions_touched.into_iter().collect::<Vec<_>>(),
        [1, 2, 3]
    );

    let same = diff(ok(&old), ok(&old), 10).unwrap();
    assert!(same.is_empty());
    assert_eq!(same.unchanged, 4);

    let none: Vec<Record> = Vec::new();
    let all_new = diff(ok(&none), ok(&new), 2).unwrap();
    assert_eq!(all_new.added, 4);
    assert_eq!(all_new.added_urls.len(), 2);
    assert!(all_new.truncated);
    assert_eq!(ChangeSet::default(), diff(ok(&none), ok(&none), 1).unwrap());
}

#[test]
fn diff_propagates_read_errors() {
    let broken = vec![Err(io::Error::other("disk"))];
    assert!(diff(broken, Vec::<io::Result<Record>>::new(), 1).is_err());
}

#[test]
fn sequence_digest_depends_on_every_record_and_its_framing() {
    use sitemap_snapshot::SequenceDigest;
    let digest = |records: &[(&str, u8)]| {
        let mut d = SequenceDigest::default();
        for (loc, content) in records {
            d.update(loc, &[*content; 16]);
        }
        (d.records(), d.finish())
    };
    let base = digest(&[("https://a/", 1), ("https://b/", 2)]);
    assert_eq!(base, digest(&[("https://a/", 1), ("https://b/", 2)]));
    assert_ne!(base, digest(&[("https://a/", 1), ("https://b/", 3)]));
    assert_ne!(base, digest(&[("https://b/", 2), ("https://a/", 1)]));
    assert_ne!(base.1, digest(&[("https://a/", 1)]).1);
    // Moving bytes between two locs changes the digest (length framing).
    assert_ne!(
        digest(&[("ab", 1), ("c", 1)]).1,
        digest(&[("a", 1), ("bc", 1)]).1
    );
    assert_eq!(SequenceDigest::default().records(), 0);
}
