//! Bounded sitemap reader (plain or gzip): never panics, respects its byte
//! limit, and every accepted entry encodes to a block that reads back the
//! same.
#![no_main]

use advanced_sitemap::inspect::inspect_reader;
use advanced_sitemap::{InspectOptions, Item, Namespaces, xml};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut options = InspectOptions::default();
    options.limits.max_bytes = 1 << 20;
    let mut blocks = Vec::new();
    let Ok(report) = inspect_reader("fuzz", data, &options, |item| {
        if let Item::Url(entry) = item {
            let mut block = Vec::new();
            xml::encode_url(&entry, &mut block).expect("accepted entry does not encode");
            blocks.push(block);
        }
    }) else {
        return;
    };
    assert!(report.xml_bytes <= (1 << 20) + 64 * 1024);
    for block in blocks {
        let mut document = xml::urlset_header(Namespaces::ALL).into_bytes();
        document.extend_from_slice(&block);
        document.extend_from_slice(xml::URLSET_FOOTER.as_bytes());
        let mut again = Vec::new();
        inspect_reader("again", &document[..], &InspectOptions::default(), |item| {
            if let Item::Url(entry) = item {
                xml::encode_url(&entry, &mut again).expect("re-encoding failed");
            }
        })
        .expect("re-reading failed");
        assert_eq!(again, block, "accepted entry does not round-trip");
    }
});
