//! Configuration documents: parsing and resolution never panic.
#![no_main]

use advanced_sitemap::{ConfigDocument, resolve_config};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data)
        && let Ok(doc) = ConfigDocument::from_json(text)
    {
        let _ = resolve_config(doc, std::path::Path::new("."));
    }
});
