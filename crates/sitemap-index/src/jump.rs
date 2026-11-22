//! Jump consistent hash (Lamping and Veach, 2014, "A Fast, Minimal Memory,
//! Consistent Hash Algorithm", figure 1).
//!
//! Used by the `consistent-hash` strategy (ADR-0012). When the bucket count
//! grows from N to N+1, a key either keeps its bucket or moves to the new
//! bucket N; about 1/(N+1) of the keys move. The function is part of the
//! persisted-output contract and is frozen by the test vectors below.
//!
//! The loop uses one `f64` division and one multiplication per step. Both
//! are correctly rounded IEEE 754 operations on every target Rust supports
//! with SSE2 or an equivalent FPU, so the result does not depend on the
//! platform; the vectors are checked in CI on Linux, Windows and macOS.

/// Bucket of `key` among `buckets` buckets, in `0..buckets`.
///
/// Returns 0 when `buckets` is 0 or 1.
pub fn jump_consistent_hash(mut key: u64, buckets: u32) -> u32 {
    let mut bucket: i64 = -1;
    let mut next: i64 = 0;
    while next < i64::from(buckets) {
        bucket = next;
        key = key.wrapping_mul(2_862_933_555_777_941_757).wrapping_add(1);
        let scale = f64::from(1u32 << 31) / ((key >> 33) + 1) as f64;
        next = ((bucket + 1) as f64 * scale) as i64;
    }
    // `bucket` is in `0..buckets` here, or -1 when `buckets` is 0.
    u32::try_from(bucket.max(0)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::jump_consistent_hash as jump;
    use crate::fnv1a64;

    /// Vectors computed with an independent Python transcription of the
    /// paper's figure 1 when the function was frozen.
    #[test]
    fn frozen_vectors() {
        assert_eq!(jump(0, 1), 0);
        assert_eq!(jump(0, 10), 0);
        assert_eq!(jump(1, 10), 6);
        assert_eq!(jump(0xdead_beef, 1000), 285);
        assert_eq!(jump(u64::MAX, 7), 2);
        assert_eq!(jump(12_345_678_901_234_567_890, 100_000), 46_485);
        assert_eq!(jump(fnv1a64(b"https://example.com/"), 20), 9);
        assert_eq!(jump(fnv1a64(b"https://example.com/a"), 8), 6);
    }

    #[test]
    fn degenerate_counts() {
        assert_eq!(jump(42, 0), 0);
        assert_eq!(jump(42, 1), 0);
    }

    /// Growing from N to N+1 moves a key only to the new bucket, and about
    /// 1/(N+1) of the keys move.
    #[test]
    fn growth_moves_keys_only_to_the_new_bucket() {
        for n in [1u32, 4, 20, 99] {
            let mut moved = 0u32;
            for i in 0..20_000u64 {
                let key = fnv1a64(format!("https://example.com/p/{i}").as_bytes());
                let (before, after) = (jump(key, n), jump(key, n + 1));
                assert!(before < n && after < n + 1);
                if before != after {
                    assert_eq!(after, n, "key {i} moved between old buckets");
                    moved += 1;
                }
            }
            let expected = 20_000.0 / f64::from(n + 1);
            let ratio = f64::from(moved) / expected;
            assert!((0.85..1.15).contains(&ratio), "n={n}: {moved} moved");
        }
    }
}
