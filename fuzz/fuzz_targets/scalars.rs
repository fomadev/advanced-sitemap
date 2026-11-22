//! URL, date and language tag parsers: never panic, and parsing a
//! normalized value gives it back.
#![no_main]

use advanced_sitemap::{LanguageTag, SitemapUrl, W3cDateTime};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(url) = SitemapUrl::new(&text) {
        assert!(url.as_str().chars().count() <= 2_047);
        let again = SitemapUrl::new(url.as_str()).expect("normalized URL rejected");
        assert_eq!(again.as_str(), url.as_str());
    }
    if let Ok(date) = W3cDateTime::parse(&text) {
        let again = W3cDateTime::parse(date.as_str()).expect("normalized date rejected");
        assert_eq!(again.as_str(), date.as_str());
    }
    if let Ok(tag) = LanguageTag::parse(&text) {
        let again = LanguageTag::parse(tag.as_str()).expect("normalized tag rejected");
        assert_eq!(again.as_str(), tag.as_str());
    }
});
