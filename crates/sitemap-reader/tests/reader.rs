#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::io::{self, Read, Write};

use flate2::Compression;
use flate2::write::GzEncoder;
use sitemap_model::{DiagnosticCode, SitemapUrl};
use sitemap_reader::{DocumentKind, InspectOptions, InspectReport, Item, ReaderLimits, inspect};
use sitemap_validation::{ValidationMode, ValidationOptions};

const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml" xmlns:image="http://www.google.com/schemas/sitemap-image/1.1">"#;

fn urlset(body: &str) -> String {
    format!("{HEAD}{body}</urlset>")
}

fn run_with(input: &[u8], options: &InspectOptions) -> (InspectReport, Vec<Item>) {
    let mut items = Vec::new();
    let report = inspect(input, options, |item| items.push(item)).unwrap();
    (report, items)
}

fn run(input: &str) -> (InspectReport, Vec<Item>) {
    run_with(input.as_bytes(), &InspectOptions::default())
}

fn codes(report: &InspectReport) -> Vec<&'static str> {
    report.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn reads_the_conformance_fixtures_written_by_the_generator() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/conformance");
    for (file, kind, entries) in [
        ("basic/expected/sitemap.xml", DocumentKind::Urlset, 3),
        ("extensions/expected/sitemap.xml", DocumentKind::Urlset, 1),
        (
            "partitioned/expected/sitemap.xml",
            DocumentKind::SitemapIndex,
            3,
        ),
        (
            "partitioned/expected/sitemap-1.xml",
            DocumentKind::Urlset,
            2,
        ),
    ] {
        let data = std::fs::read(format!("{root}/{file}")).unwrap();
        let (report, items) = run_with(&data, &InspectOptions::default());
        assert!(report.complete, "{file}");
        assert_eq!(report.error_count(), 0, "{file}: {:?}", report.diagnostics);
        assert_eq!(report.kind, Some(kind), "{file}");
        assert_eq!(report.entries, entries, "{file}");
        assert_eq!(items.len() as u64, entries, "{file}");
        assert_eq!(report.xml_bytes, data.len() as u64, "{file}");
    }
}

#[test]
fn extracts_every_modelled_field() {
    let (report, items) = run(&urlset(
        r#"
  <url>
    <loc> https://example.com/a?x=1&amp;y=&#50;</loc>
    <lastmod>2026-01-02</lastmod>
    <changefreq>daily</changefreq>
    <priority>0.8</priority>
    <xhtml:link rel="alternate" hreflang="fr" href="https://example.com/fr/a"/>
    <image:image><image:loc><![CDATA[https://cdn.example.com/a.png]]></image:loc><image:caption>retired</image:caption></image:image>
  </url>"#,
    ));
    assert_eq!(report.error_count(), 0, "{:?}", report.diagnostics);
    let Item::Url(url) = &items[0] else {
        panic!("expected a url")
    };
    assert_eq!(url.loc.as_str(), "https://example.com/a?x=1&y=2");
    assert_eq!(url.lastmod.as_ref().unwrap().to_string(), "2026-01-02");
    assert_eq!(url.priority.unwrap().value(), 0.8);
    assert_eq!(url.alternates.len(), 1);
    assert_eq!(url.alternates[0].href.as_str(), "https://example.com/fr/a");
    assert_eq!(url.images[0].loc.as_str(), "https://cdn.example.com/a.png");
    assert_eq!(
        (
            report.with_lastmod,
            report.with_alternates,
            report.with_images
        ),
        (1, 1, 1)
    );
}

#[test]
fn gzip_input_gives_the_same_result() {
    let xml = urlset("<url><loc>https://example.com/</loc></url>");
    let (plain, plain_items) = run(&xml);
    let (zipped, zipped_items) = run_with(&gzip(xml.as_bytes()), &InspectOptions::default());
    assert!(zipped.compressed && !plain.compressed);
    assert_eq!(zipped.xml_bytes, plain.xml_bytes);
    assert_eq!(zipped_items, plain_items);
    assert!(zipped.complete);
}

#[test]
fn doctype_is_refused_so_entities_never_expand() {
    let billion_laughs = r#"<?xml version="1.0"?>
<!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">]>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>&lol2;</loc></url></urlset>"#;
    let (report, items) = run(billion_laughs);
    assert_eq!(codes(&report), ["XML_DOCTYPE_FORBIDDEN"]);
    assert!(!report.complete);
    assert!(items.is_empty());

    let xxe = r#"<?xml version="1.0"?>
<!DOCTYPE urlset [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>&xxe;</loc></url></urlset>"#;
    assert_eq!(codes(&run(xxe).0), ["XML_DOCTYPE_FORBIDDEN"]);
}

#[test]
fn undeclared_entities_are_a_well_formedness_error() {
    let (report, _) = run(&urlset("<url><loc>https://example.com/&xxe;</loc></url>"));
    assert_eq!(codes(&report), ["XML_MALFORMED"]);
    assert!(report.diagnostics[0].message.contains("undefined entity"));
}

#[test]
fn uncompressed_size_is_capped() {
    let body = "<url><loc>https://example.com/</loc></url>".repeat(100);
    let xml = urlset(&body);
    let options = InspectOptions {
        limits: ReaderLimits {
            max_bytes: 1_000,
            ..ReaderLimits::default()
        },
        dedupe: false,
        ..InspectOptions::default()
    };
    let (report, _) = run_with(xml.as_bytes(), &options);
    assert_eq!(
        report.diagnostics.last().unwrap().code,
        DiagnosticCode::SitemapTooLarge
    );
    assert!(!report.complete);
    assert!(report.xml_bytes <= 1_001);

    // Exactly at the limit is accepted.
    let exact = InspectOptions {
        limits: ReaderLimits {
            max_bytes: xml.len() as u64,
            ..ReaderLimits::default()
        },
        dedupe: false,
        ..InspectOptions::default()
    };
    assert!(run_with(xml.as_bytes(), &exact).0.complete);
}

#[test]
fn gzip_bomb_stops_at_the_uncompressed_limit() {
    // 64 MiB of whitespace inside the root compresses to about 64 KiB.
    let mut xml = Vec::with_capacity(64 << 20);
    xml.extend_from_slice(HEAD.as_bytes());
    xml.resize(64 << 20, b' ');
    xml.extend_from_slice(b"</urlset>");
    let bomb = gzip(&xml);
    assert!(bomb.len() < 1 << 20);
    let options = InspectOptions {
        limits: ReaderLimits {
            max_bytes: 4 << 20,
            ..ReaderLimits::default()
        },
        ..InspectOptions::default()
    };
    let (report, _) = run_with(&bomb, &options);
    assert_eq!(codes(&report), ["SITEMAP_TOO_LARGE"]);
    assert!(report.xml_bytes <= (4 << 20) + 1);
}

#[test]
fn corrupt_gzip_is_reported() {
    let mut data = gzip(urlset("<url><loc>https://example.com/</loc></url>").as_bytes());
    let middle = data.len() / 2;
    for byte in &mut data[10..middle] {
        *byte ^= 0x5a;
    }
    let (report, _) = run_with(&data, &InspectOptions::default());
    assert!(!report.complete);
    let code = report.diagnostics.last().unwrap().code;
    assert!(
        matches!(
            code,
            DiagnosticCode::GzipInvalid | DiagnosticCode::XmlMalformed
        ),
        "{code}"
    );
}

#[test]
fn depth_and_text_length_are_capped() {
    let deep = urlset(&format!(
        "<url><loc>https://example.com/</loc><x:a xmlns:x=\"urn:x\">{}{}</x:a></url>",
        "<x:a>".repeat(40),
        "</x:a>".repeat(40)
    ));
    assert_eq!(codes(&run(&deep).0), ["XML_DEPTH_EXCEEDED"]);

    let long = urlset(&format!(
        "<url><loc>https://example.com/{}</loc></url>",
        "a".repeat(70_000)
    ));
    assert_eq!(codes(&run(&long).0), ["XML_TEXT_TOO_LONG"]);
}

#[test]
fn root_must_be_a_sitemap_element() {
    let (report, _) =
        run(r#"<?xml version="1.0"?><urlset><url><loc>https://example.com/</loc></url></urlset>"#);
    assert_eq!(codes(&report), ["XML_ROOT_INVALID"]);
    let (report, _) = run(r#"<html xmlns="http://www.w3.org/1999/xhtml"/>"#);
    assert_eq!(codes(&report), ["XML_ROOT_INVALID"]);
    assert_eq!(codes(&run("").0), ["XML_MALFORMED"]);
}

#[test]
fn only_utf8_is_accepted() {
    let latin1 = r#"<?xml version="1.0" encoding="ISO-8859-1"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"/>"#;
    assert_eq!(codes(&run(latin1).0), ["XML_ENCODING_UNSUPPORTED"]);

    let mut invalid = urlset("<url><loc>https://example.com/").into_bytes();
    invalid.extend_from_slice(&[0xff, 0xfe]);
    invalid.extend_from_slice(b"</loc></url></urlset>");
    let (report, _) = run_with(&invalid, &InspectOptions::default());
    assert_eq!(codes(&report), ["XML_MALFORMED"]);
}

#[test]
fn malformed_documents_stop_reading() {
    for xml in [
        urlset("<url><loc>https://example.com/</lastmod></url>"),
        format!("{HEAD}<url><loc>https://example.com/</loc></url>"),
        urlset("<url><loc>https://example.com/\u{1}</loc></url>"),
        format!(
            "{}<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"/>",
            urlset("")
        ),
    ] {
        let (report, _) = run(&xml);
        assert_eq!(codes(&report), ["XML_MALFORMED"], "{xml}");
        assert!(!report.complete);
    }
}

#[test]
fn element_order_follows_the_xsd() {
    // The order emitted by the TypeScript library: xhtml:link before lastmod.
    let (report, items) = run(&urlset(
        r#"<url><loc>https://example.com/</loc><xhtml:link rel="alternate" hreflang="en" href="https://example.com/"/><lastmod>2026-01-01</lastmod></url>
<url><priority>0.5</priority><loc>https://example.com/b</loc></url>"#,
    ));
    assert_eq!(codes(&report), ["XML_ELEMENT_ORDER", "XML_ELEMENT_ORDER"]);
    assert_eq!(items.len(), 2, "order problems do not invalidate entries");
}

#[test]
fn invalid_values_reuse_generator_diagnostics() {
    let (report, items) = run(&urlset(
        r#"<url><loc>https://example.com/a</loc><lastmod>2026-13-01</lastmod></url>
<url><loc>https://example.com/b</loc><changefreq>sometimes</changefreq></url>
<url><loc>https://example.com/c</loc><priority>high</priority></url>
<url><loc>https://example.com/d</loc><priority>1.5</priority></url>
<url><loc>ftp://example.com/e</loc></url>
<url><lastmod>2026-01-01</lastmod></url>
<url><loc>https://example.com/ok</loc></url>"#,
    ));
    assert_eq!(
        codes(&report),
        [
            "LASTMOD_INVALID",
            "CHANGEFREQ_INVALID",
            "PRIORITY_INVALID",
            "PRIORITY_OUT_OF_RANGE",
            "URL_UNSUPPORTED_SCHEME",
            "REQUIRED_FIELD_MISSING",
        ]
    );
    assert_eq!(items.len(), 1);
    assert_eq!(report.entries, 7);
    assert_eq!(report.valid_entries, 1);
    assert_eq!(report.diagnostics[2].location.record, Some(3));
}

#[test]
fn strict_profile_applies_to_urls() {
    let options = InspectOptions {
        validation: ValidationOptions {
            mode: ValidationMode::Strict,
            site: Some(SitemapUrl::new("https://example.com/").unwrap()),
            reference_time: None,
        },
        ..InspectOptions::default()
    };
    let xml = urlset("<url><loc>http://other.example/</loc></url>");
    let (report, items) = run_with(xml.as_bytes(), &options);
    let found = codes(&report);
    for code in ["URL_NOT_HTTPS", "HOST_MISMATCH", "LASTMOD_MISSING"] {
        assert!(found.contains(&code), "{found:?}");
    }
    assert!(items.is_empty());
}

#[test]
fn duplicates_and_unknown_elements() {
    let (report, items) = run(&urlset(
        r#"<url><loc>https://example.com/</loc><loc>https://example.com/x</loc><title>no</title></url>
<url><loc>https://example.com/</loc></url>
<video:video xmlns:video="http://www.google.com/schemas/sitemap-video/1.1"/>
<url><loc>https://example.com/v</loc><video:video xmlns:video="http://www.google.com/schemas/sitemap-video/1.1"><video:title>t</video:title></video:video></url>"#,
    ));
    assert_eq!(
        codes(&report),
        [
            "XML_UNEXPECTED_ELEMENT",
            "XML_UNEXPECTED_ELEMENT",
            "DUPLICATE_URL"
        ]
    );
    assert_eq!(items.len(), 2);
    assert_eq!(report.duplicates, 1);
    assert_eq!(
        report.unsupported_extensions["http://www.google.com/schemas/sitemap-video/1.1"],
        2
    );
}

#[test]
fn sitemap_index_entries() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://example.com/s1.xml</loc><lastmod>2026-02-01T10:00:00Z</lastmod></sitemap>
  <sitemap><loc>https://example.com/s2.xml.gz</loc><priority>0.5</priority></sitemap>
  <url><loc>https://example.com/</loc></url>
</sitemapindex>"#;
    let (report, items) = run(xml);
    assert_eq!(report.kind, Some(DocumentKind::SitemapIndex));
    assert_eq!(
        codes(&report),
        ["XML_UNEXPECTED_ELEMENT", "XML_UNEXPECTED_ELEMENT"]
    );
    assert_eq!(items.len(), 2);
    assert!(matches!(&items[1], Item::Sitemap(s) if s.loc.as_str().ends_with("s2.xml.gz")));
}

#[test]
fn more_than_50000_urls_is_reported_once() {
    let body = "<url><loc>https://example.com/</loc></url>".repeat(50_002);
    let options = InspectOptions {
        dedupe: false,
        ..InspectOptions::default()
    };
    let (report, items) = run_with(urlset(&body).as_bytes(), &options);
    assert_eq!(codes(&report), ["SITEMAP_URL_LIMIT_EXCEEDED"]);
    assert_eq!(items.len(), 50_002);
}

#[test]
fn source_io_errors_are_returned() {
    struct Failing;
    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        }
    }
    let error = inspect(Failing, &InspectOptions::default(), |_| {}).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}
