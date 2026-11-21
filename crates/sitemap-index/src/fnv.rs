//! FNV-1a, 64-bit variant.
//!
//! Used to assign URLs to stable-hash partitions. The function is part of the
//! persisted-output contract: changing it would move URLs between partitions.
//! It is frozen by the test vectors below and must never change.

const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64-bit hash of `bytes`. Not a cryptographic hash.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::fnv1a64;

    /// Reference vectors from the FNV specification test suite.
    #[test]
    fn frozen_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    /// Engine-specific vector, computed independently (Python reference
    /// implementation) when the function was frozen.
    #[test]
    fn frozen_url_vector() {
        assert_eq!(fnv1a64(b"https://example.com/"), 0x0c8b_41cf_dcb3_c914);
    }
}
