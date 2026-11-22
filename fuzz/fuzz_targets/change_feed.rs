//! Change-feed decoding never panics.
#![no_main]

use std::io::Cursor;

use advanced_sitemap::protocol::{ChangeRecord, JsonlReader};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for item in JsonlReader::<_, ChangeRecord>::typed(Cursor::new(data), 1 << 16) {
        let _ = item.map(|record| record.result);
    }
});
