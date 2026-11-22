//! Error kinds, causes and retry hints.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::error::Error as _;
use std::io;

use advanced_sitemap::{ConfigDocument, EngineError, Sitemap, SitemapError, SitemapUrl};

#[test]
fn engine_error_kinds_are_stable() {
    let cases = [
        (EngineError::Config("x".into()), "config", false),
        (EngineError::Io(io::Error::other("disk")), "io", false),
        (EngineError::DeltaRefused("x".into()), "delta-refused", false),
        (EngineError::Busy("x".into()), "busy", true),
        (
            EngineError::StateChanged {
                expected: "a".into(),
                actual: "b".into(),
            },
            "state-changed",
            true,
        ),
    ];
    for (error, kind, retryable) in cases {
        assert_eq!(error.kind(), kind, "{error}");
        assert_eq!(error.is_retryable(), retryable, "{error}");
    }
    let protocol = ConfigDocument::from_json("{").unwrap_err();
    assert_eq!(EngineError::from(protocol).kind(), "config");
}

#[test]
fn wrapped_errors_are_exposed_as_sources() {
    let error = EngineError::Io(io::Error::other("disk full"));
    let source = error.source().expect("I/O cause");
    assert_eq!(source.to_string(), "disk full");
    assert!(EngineError::Busy("x".into()).source().is_none());

    let protocol = EngineError::from(ConfigDocument::from_json("{").unwrap_err());
    assert!(protocol.source().is_some());
}

#[test]
fn sitemap_errors_without_cause_have_no_source() {
    let url = SitemapUrl::new("https://example.com/").unwrap();
    let error = Sitemap::builder()
        .url(url.clone())
        .url(url)
        .build()
        .unwrap_err();
    assert!(matches!(error, SitemapError::DuplicateUrl(_)));
    assert!(error.source().is_none());
}
