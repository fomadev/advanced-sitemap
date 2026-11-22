//! Reproducible fuzzing on stable Rust (lot D).
//!
//! Each target mutates valid inputs (fixtures, generated records) many
//! times with a seeded generator and checks that the engine never panics
//! and that strong properties hold on whatever it accepts. The number of
//! iterations per target is `FUZZ_ITERATIONS` (default 3,000), and
//! `FUZZ_SEED` changes the seed; a failure prints both, so it can be
//! replayed exactly. Coverage-guided fuzzing with `cargo fuzz` uses the
//! same entry points (see `fuzz/`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use advanced_sitemap::inspect::inspect_reader;
use advanced_sitemap::protocol::{ChangeRecord, JsonlReader};
use advanced_sitemap::{
    ConfigDocument, InspectOptions, Item, LanguageTag, Location, Namespaces, ReaderLimits,
    RobotsConfig, RobotsGroup, SitemapUrl, ValidationMode, ValidationOptions, Validator,
    W3cDateTime, xml,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64: every seed, including 0, gives a good sequence.
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// Fragments that exercise parsers: markup, entities, encodings, numbers.
const TOKENS: &[&[u8]] = &[
    b"<",
    b">",
    b"&",
    b"&amp;",
    b"&#0;",
    b"&#xFFFFFF;",
    b"&lt;",
    b"]]>",
    b"<![CDATA[x]]>",
    b"<!DOCTYPE x [<!ENTITY a \"aaaa\">]>",
    b"&a;",
    b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>",
    b"<url>",
    b"</url>",
    b"<loc>",
    b"</loc>",
    b"<urlset>",
    b"</urlset>",
    b"<sitemapindex>",
    b"xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"",
    b"<image:image>",
    b"<video:video>",
    b"<news:news>",
    b"<xhtml:link rel=\"alternate\" hreflang=\"fr\" href=\"https://e.com/\"/>",
    b"\x00",
    b"\xff\xfe",
    b"\xc3\x28",
    b"\xf0\x9f\x98\x80",
    b"\r\n",
    b"\t",
    b" ",
    b"\"",
    b"'",
    b"{",
    b"}",
    b"[",
    b"]",
    b",",
    b":",
    b"null",
    b"true",
    b"-1",
    b"1e400",
    b"99999999999999999999",
    b"https://",
    b"http://user:pw@e.com/",
    b"https://e.com/#frag",
    b"%00",
    b"%zz",
    b"..",
    b"2026-02-30",
    b"2026-13-01T25:61:61+99:99",
    b"9999-12-31T23:59:59.999999999Z",
    b"zh-Hant-TW",
    b"x-default",
    b"\\u0000",
    b"\\ud800",
];

fn mutate(rng: &mut Rng, input: &[u8]) -> Vec<u8> {
    let mut data = input.to_vec();
    for _ in 0..1 + rng.below(8) {
        let len = data.len();
        match rng.below(8) {
            0 if len > 0 => {
                let i = rng.below(len);
                data[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len);
                let n = rng.below((len - i).min(64) + 1);
                data.drain(i..i + n);
            }
            2 => {
                let i = rng.below(len + 1);
                let token = TOKENS[rng.below(TOKENS.len())];
                data.splice(i..i, token.iter().copied());
            }
            3 if len > 0 => {
                let i = rng.below(len);
                let n = rng.below((len - i).min(256) + 1);
                let copy = data[i..i + n].to_vec();
                let at = rng.below(data.len() + 1);
                data.splice(at..at, copy);
            }
            4 => data.truncate(rng.below(len + 1)),
            5 => {
                let i = rng.below(len + 1);
                let n = rng.below(16);
                let bytes: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
                data.splice(i..i, bytes);
            }
            6 if len > 0 => {
                let i = rng.below(len);
                data[i] = TOKENS[rng.below(TOKENS.len())][0];
            }
            _ => {}
        }
    }
    data
}

fn iterations() -> usize {
    std::env::var("FUZZ_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_000)
}

fn seed(target: u64) -> u64 {
    let base = std::env::var("FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x5eed);
    base ^ target.wrapping_mul(0x9e37_79b9)
}

/// Runs `check` on `iterations()` mutations of the seeds; on a panic,
/// reports the seed, the iteration and the input so it can be replayed.
fn fuzz(target: u64, name: &str, seeds: &[Vec<u8>], check: impl Fn(&[u8])) {
    let mut rng = Rng(seed(target));
    for i in 0..iterations() {
        let base = &seeds[rng.below(seeds.len())];
        let input = mutate(&mut rng, base);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(&input)));
        if outcome.is_err() {
            let path = std::env::temp_dir().join(format!("fuzz-{name}-crash.bin"));
            let _ = fs::write(&path, &input);
            panic!(
                "{name}: iteration {i} (FUZZ_SEED={}) panicked; input saved to {}",
                std::env::var("FUZZ_SEED").unwrap_or_else(|_| "default".into()),
                path.display()
            );
        }
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/conformance");
    let mut out = Vec::new();
    for dir in fs::read_dir(root).unwrap() {
        let expected = dir.unwrap().path().join("expected");
        if let Ok(files) = fs::read_dir(expected) {
            for file in files {
                let path = file.unwrap().path();
                if path.extension().is_some_and(|e| e == "xml") {
                    out.push(fs::read(path).unwrap());
                }
            }
        }
    }
    assert!(out.len() >= 6);
    out
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

/// Reader property: never panics, respects its limits, and every entry it
/// accepts encodes to a block that reads back to the same block.
fn check_reader(input: &[u8]) {
    let options = InspectOptions {
        limits: ReaderLimits {
            max_bytes: 1 << 20,
            ..ReaderLimits::default()
        },
        ..InspectOptions::default()
    };
    let mut blocks = Vec::new();
    let report = inspect_reader("fuzz", input, &options, |item| {
        if let Item::Url(entry) = item {
            let mut block = Vec::new();
            xml::encode_url(&entry, &mut block).unwrap();
            blocks.push(block);
        }
    });
    let Ok(report) = report else { return };
    assert!(
        report.xml_bytes <= (1 << 20) + 64 * 1024,
        "read past the limit"
    );
    // Round trip of everything accepted.
    for block in blocks {
        let mut document = xml::urlset_header(Namespaces::ALL).into_bytes();
        document.extend_from_slice(&block);
        document.extend_from_slice(xml::URLSET_FOOTER.as_bytes());
        let mut again = Vec::new();
        inspect_reader("again", &document[..], &InspectOptions::default(), |item| {
            if let Item::Url(entry) = item {
                xml::encode_url(&entry, &mut again).unwrap();
            }
        })
        .unwrap();
        assert_eq!(again, block, "an accepted entry does not round-trip");
    }
}

#[test]
fn fuzz_xml_reader() {
    fuzz(1, "xml-reader", &fixtures(), check_reader);
}

#[test]
fn fuzz_gzip_reader() {
    let seeds: Vec<Vec<u8>> = fixtures().iter().map(|f| gzip(f)).collect();
    fuzz(2, "gzip-reader", &seeds, check_reader);
}

#[test]
fn decompression_bombs_stop_at_the_limit() {
    // 64 MiB of zeros compress to about 64 KiB.
    let zeros = vec![b' '; 64 << 20];
    let bomb = gzip(&zeros);
    assert!(bomb.len() < 1 << 20);
    let options = InspectOptions {
        limits: ReaderLimits {
            max_bytes: 1 << 20,
            ..ReaderLimits::default()
        },
        ..InspectOptions::default()
    };
    let report = inspect_reader("bomb", &bomb[..], &options, |_| {}).unwrap();
    assert!(!report.complete);
    assert!(
        report.xml_bytes <= (1 << 20) + 64 * 1024,
        "{}",
        report.xml_bytes
    );
}

fn jsonl_seeds() -> Vec<Vec<u8>> {
    vec![
        br#"{"loc":"https://example.com/a","lastmod":"2026-11-22","changefreq":"daily","priority":0.5}"#.to_vec(),
        br#"{"loc":"https://example.com/b","alternates":[{"hreflang":"en","href":"https://example.com/b"},{"hreflang":"fr","href":"https://example.com/fr/b"}],"images":[{"loc":"https://example.com/i.png"}]}"#.to_vec(),
        br#"{"loc":"https://example.com/v","videos":[{"thumbnailLoc":"https://example.com/t.jpg","title":"T","description":"D","contentLoc":"https://example.com/v.mp4","duration":60,"rating":4.5,"tags":["a"],"restriction":{"relationship":"allow","countries":["FR"]}}]}"#.to_vec(),
        br#"{"loc":"https://example.com/n","news":{"publication":{"name":"N","language":"fr"},"publicationDate":"2026-11-22T10:00:00Z","title":"T"}}"#.to_vec(),
    ]
}

/// Decoding, validation and encoding of URL records: no panic, and every
/// accepted record encodes to a block that the reader reads back exactly.
#[test]
fn fuzz_url_records() {
    let strict = Validator::new(ValidationOptions {
        mode: ValidationMode::Strict,
        ..ValidationOptions::default()
    });
    let standard = Validator::new(ValidationOptions::default());
    fuzz(3, "url-records", &jsonl_seeds(), |input| {
        let mut lines = input.to_vec();
        lines.push(b'\n');
        for item in JsonlReader::with_max_line_bytes(Cursor::new(&lines), 1 << 16) {
            let Ok(record) = item else { continue };
            let Ok(raw) = record.result else { continue };
            for validator in [&standard, &strict] {
                let mut diagnostics = Vec::new();
                if let Some(entry) =
                    validator.validate(&raw, &Location::record(1, None), &mut diagnostics)
                {
                    let mut block = Vec::new();
                    xml::encode_url(&entry, &mut block).unwrap();
                    let mut document = xml::urlset_header(Namespaces::ALL).into_bytes();
                    document.extend_from_slice(&block);
                    document.extend_from_slice(xml::URLSET_FOOTER.as_bytes());
                    check_reader(&document);
                } else {
                    assert!(
                        diagnostics.iter().any(|d| d.is_error()),
                        "rejected without an error"
                    );
                }
            }
        }
    });
}

#[test]
fn fuzz_change_feed() {
    let seeds = vec![
        br#"{"op":"upsert","url":{"loc":"https://example.com/a","lastmod":"2026-11-22"}}"#.to_vec(),
        br#"{"op":"delete","loc":"https://example.com/b"}"#.to_vec(),
    ];
    fuzz(4, "change-feed", &seeds, |input| {
        let mut lines = input.to_vec();
        lines.push(b'\n');
        for item in JsonlReader::<_, ChangeRecord>::typed(Cursor::new(&lines), 1 << 16) {
            let _ = item.map(|r| r.result);
        }
    });
}

#[test]
fn fuzz_configuration_documents() {
    let seeds = vec![
        br#"{"schemaVersion":"1.1","site":"https://example.com","publicBaseUrl":"https://example.com/s/","partitioning":{"strategy":"consistent-hash","partitions":8,"maxUrls":40000},"output":{"directory":"dist","publish":"auto","extensions":["video","news"]},"state":{"directory":"state","maxFeedOperations":10},"robots":{"groups":[{"userAgents":["*"],"disallow":["/admin"]}]},"source":{"path":"u.jsonl"}}"#.to_vec(),
        br#"{"schemaVersion":"1.0","urls":[{"loc":"https://example.com/"}]}"#.to_vec(),
    ];
    fuzz(5, "config", &seeds, |input| {
        if let Ok(text) = std::str::from_utf8(input)
            && let Ok(doc) = ConfigDocument::from_json(text)
        {
            let _ = advanced_sitemap::resolve_config(doc, Path::new("."));
        }
    });
}

/// URL, date and language tag parsing: no panic, and what is accepted is
/// stable (parsing the normalized form gives it back).
#[test]
fn fuzz_scalar_parsers() {
    let seeds: Vec<Vec<u8>> = [
        "https://example.com/a/b?c=d&e=%20f",
        "http://EXAMPLE.com:8080/x/../y",
        "https://xn--caf-dma.example/é?q=日本",
        "2026-11-22T10:00:00.123+05:30",
        "2026-11-22",
        "zh-Hant-TW",
        "x-default",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    fuzz(6, "scalars", &seeds, |input| {
        let text = String::from_utf8_lossy(input);
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
}

#[test]
fn fuzz_robots() {
    let seeds = vec![
        b"*|/admin|/public".to_vec(),
        b"Googlebot|/private?x|".to_vec(),
    ];
    fuzz(7, "robots", &seeds, |input| {
        let text = String::from_utf8_lossy(input);
        let mut parts = text.split('|').map(str::to_owned);
        let config = RobotsConfig {
            groups: vec![RobotsGroup {
                user_agents: parts.next().into_iter().collect(),
                disallow: parts.next().into_iter().collect(),
                allow: parts.next().into_iter().collect(),
            }],
            sitemaps: parts.collect(),
        };
        let (built, _) = config.build();
        if let Some(robots) = built {
            let (text, _) = robots.render_checked();
            assert!(text.is_ascii() || text.chars().all(|c| c != '\r'));
        }
    });
}

/// Regression (found by `fuzz_url_records`, seeds 1 and 3): a control
/// character in a news or video text passed validation, then failed in the
/// XML writer, stopping the run with an internal error.
#[test]
fn control_characters_in_extension_texts_are_diagnostics() {
    let validator = Validator::new(ValidationOptions::default());
    let cases = [
        (
            r#"{"loc":"https://example.com/n","news":{"publication":{"name":"N","language":"fr"},"publicationDate":"2026-11-22T10:00:00Z","title":"T\u0000"}}"#,
            "NEWS_TEXT_INVALID",
        ),
        (
            r#"{"loc":"https://example.com/n","news":{"publication":{"name":"N\u0007","language":"fr"},"publicationDate":"2026-11-22T10:00:00Z","title":"T"}}"#,
            "NEWS_TEXT_INVALID",
        ),
        (
            r#"{"loc":"https://example.com/v","videos":[{"thumbnailLoc":"https://example.com/t.jpg","title":"T\u001f","description":"D","contentLoc":"https://example.com/v.mp4"}]}"#,
            "VIDEO_TEXT_INVALID",
        ),
        (
            r#"{"loc":"https://example.com/v","videos":[{"thumbnailLoc":"https://example.com/t.jpg","title":"T","description":"D","contentLoc":"https://example.com/v.mp4","tags":["a\u000bb"]}]}"#,
            "VIDEO_TEXT_INVALID",
        ),
        (
            r#"{"loc":"https://example.com/v","videos":[{"thumbnailLoc":"https://example.com/t.jpg","title":"T","description":"D","contentLoc":"https://example.com/v.mp4","uploader":{"name":"U\u0000"}}]}"#,
            "VIDEO_UPLOADER_INVALID",
        ),
    ];
    for (json, code) in cases {
        let raw: advanced_sitemap::RawUrlRecord = serde_json::from_str(json).unwrap();
        let mut diagnostics = Vec::new();
        let entry = validator.validate(&raw, &Location::record(1, None), &mut diagnostics);
        assert!(entry.is_none(), "{json}");
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&code), "{json}: {codes:?}");
    }
}
