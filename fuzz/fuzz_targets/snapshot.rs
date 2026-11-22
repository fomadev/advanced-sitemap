//! Snapshot decoder and diff never panic on any input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use sitemap_snapshot::{SnapshotReader, diff};

fuzz_target!(|data: &[u8]| {
    if let Ok(reader) = SnapshotReader::new(data) {
        let _ = reader.collect::<std::io::Result<Vec<_>>>();
    }
    if let (Ok(a), Ok(b)) = (SnapshotReader::new(data), SnapshotReader::new(data)) {
        let _ = diff(a, b, 10);
    }
});
