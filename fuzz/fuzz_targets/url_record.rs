//! URL record decoding and validation: never panics; a rejected record has
//! an error diagnostic; an accepted one encodes and reads back the same.
#![no_main]

use std::io::Cursor;

use advanced_sitemap::inspect::inspect_reader;
use advanced_sitemap::protocol::JsonlReader;
use advanced_sitemap::{
    InspectOptions, Item, Location, Namespaces, ValidationMode, ValidationOptions, Validator, xml,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let validators = [
        Validator::new(ValidationOptions::default()),
        Validator::new(ValidationOptions {
            mode: ValidationMode::Strict,
            ..ValidationOptions::default()
        }),
    ];
    for item in JsonlReader::with_max_line_bytes(Cursor::new(data), 1 << 16) {
        let Ok(record) = item else { continue };
        let Ok(raw) = record.result else { continue };
        for validator in &validators {
            let mut diagnostics = Vec::new();
            match validator.validate(&raw, &Location::record(1, None), &mut diagnostics) {
                None => assert!(diagnostics.iter().any(|d| d.is_error())),
                Some(entry) => {
                    let mut block = Vec::new();
                    xml::encode_url(&entry, &mut block).expect("validated entry does not encode");
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
                    assert_eq!(again, block, "validated entry does not round-trip");
                }
            }
        }
    }
});
