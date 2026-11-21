//! Integration tests of the engine: conformance fixtures, policies,
//! determinism, publication and well-formedness.

// Test helpers outside #[test] functions may panic on failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use advanced_sitemap::{
    ConfigDocument, Engine, EngineConfig, FsStore, InvalidPolicy, MemoryStore, NullStore,
    PartitionLimits, PublicBaseUrl, RawUrlRecord, RunMode, RunStatus, SitemapUrl, open_input,
    records, resolve_config,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/conformance")
}

fn run_fixture(
    name: &str,
) -> (
    advanced_sitemap::GenerationReport,
    BTreeMap<String, Vec<u8>>,
) {
    let dir = fixtures().join(name);
    let text = fs::read_to_string(dir.join("config.json")).unwrap();
    let resolved = resolve_config(ConfigDocument::from_json(&text).unwrap(), &dir).unwrap();
    let engine = Engine::new(resolved.engine).unwrap();
    let input = open_input(resolved.input.as_ref().unwrap()).unwrap();
    let mut store = MemoryStore::new();
    let report = engine.run(input, &mut store, RunMode::Publish).unwrap();
    (report, store.published().unwrap())
}

fn codes(report: &advanced_sitemap::GenerationReport) -> Vec<&'static str> {
    report.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn conformance_fixtures() {
    let mut checked = 0;
    for entry in fs::read_dir(fixtures()).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let (report, published) = run_fixture(&name);

        let expected_dir = dir.join("expected");
        let codes_file = dir.join("expected-codes.txt");
        if codes_file.exists() {
            assert_eq!(report.status, RunStatus::Failed, "{name}");
            assert!(published.is_empty(), "{name}: nothing may be published");
            let expected: Vec<String> = fs::read_to_string(codes_file)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect();
            assert_eq!(codes(&report), expected, "{name}");
        } else {
            assert_eq!(
                report.status,
                RunStatus::Published,
                "{name}: {:?}",
                report.diagnostics
            );
            let mut expected = BTreeMap::new();
            for file in fs::read_dir(&expected_dir).unwrap() {
                let path = file.unwrap().path();
                let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
                expected.insert(file_name, fs::read(&path).unwrap());
            }
            for (file_name, bytes) in &expected {
                let actual = published
                    .get(file_name)
                    .unwrap_or_else(|| panic!("{name}: {file_name} not published"));
                assert_eq!(
                    String::from_utf8_lossy(actual),
                    String::from_utf8_lossy(bytes),
                    "{name}/{file_name}"
                );
            }
            let names: Vec<&String> = published.keys().collect();
            let expected_names: Vec<&String> = expected.keys().collect();
            assert_eq!(names, expected_names, "{name}: unexpected artifacts");
        }
        checked += 1;
    }
    assert!(checked >= 4);
}

#[test]
fn partitioned_fixture_builds_a_consistent_index() {
    let (report, published) = run_fixture("partitioned");
    let index = String::from_utf8(published["sitemap.xml"].clone()).unwrap();
    for i in 1..=3 {
        assert!(index.contains(&format!(
            "<loc>https://example.com/sitemaps/sitemap-{i}.xml</loc>"
        )));
        assert!(published.contains_key(&format!("sitemap-{i}.xml")));
    }
    // Partition 1 lastmod is the latest of its two entries; partition 2 has none.
    assert!(index.contains("<lastmod>2026-03-01T00:00:00Z</lastmod>"));
    assert_eq!(index.matches("<lastmod>").count(), 2);
    assert_eq!(report.stats.urls_written, 5);
}

#[test]
fn output_is_deterministic() {
    let (_, a) = run_fixture("partitioned");
    let (_, b) = run_fixture("partitioned");
    assert_eq!(a, b);
}

#[test]
fn skip_policy_publishes_valid_records() {
    let config = EngineConfig {
        on_invalid: InvalidPolicy::Skip,
        ..EngineConfig::default()
    };
    let engine = Engine::new(config).unwrap();
    let mut store = MemoryStore::new();
    let report = engine
        .run(
            records([
                RawUrlRecord::with_loc("https://example.com/a"),
                RawUrlRecord::with_loc("not a url"),
                RawUrlRecord::with_loc("https://example.com/b"),
            ]),
            &mut store,
            RunMode::Publish,
        )
        .unwrap();
    assert_eq!(report.status, RunStatus::Published);
    assert_eq!(report.stats.invalid_records, 1);
    assert_eq!(report.stats.urls_written, 2);
    assert_eq!(report.severity_counts.error, 1);
}

#[test]
fn dry_run_publishes_nothing() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let mut store = MemoryStore::new();
    let report = engine
        .run(
            records([RawUrlRecord::with_loc("https://example.com/")]),
            &mut store,
            RunMode::DryRun,
        )
        .unwrap();
    assert_eq!(report.status, RunStatus::Valid);
    assert!(store.published().unwrap().is_empty());
    assert_eq!(store.staged_count().unwrap(), 0);

    let mut null = NullStore::new();
    let report = engine
        .run(
            records([RawUrlRecord::with_loc("bad")]),
            &mut null,
            RunMode::DryRun,
        )
        .unwrap();
    assert_eq!(report.status, RunStatus::Invalid);
}

#[test]
fn missing_public_base_url_fails_without_publishing() {
    let config = EngineConfig {
        limits: PartitionLimits::new(1, 52_428_800).unwrap(),
        ..EngineConfig::default()
    };
    let engine = Engine::new(config).unwrap();
    let mut store = MemoryStore::new();
    let report = engine
        .run(
            records([
                RawUrlRecord::with_loc("https://example.com/a"),
                RawUrlRecord::with_loc("https://example.com/b"),
            ]),
            &mut store,
            RunMode::Publish,
        )
        .unwrap();
    assert_eq!(report.status, RunStatus::Failed);
    assert_eq!(codes(&report), ["PUBLIC_BASE_URL_MISSING"]);
    assert!(store.published().unwrap().is_empty());
}

#[test]
fn stale_artifacts_are_reported_not_deleted() {
    let mut config = EngineConfig {
        limits: PartitionLimits::new(1, 52_428_800).unwrap(),
        ..EngineConfig::default()
    };
    config.layout.public_base_url =
        Some(PublicBaseUrl::new(SitemapUrl::new("https://example.com/").unwrap()).unwrap());
    let engine = Engine::new(config).unwrap();
    let mut store = MemoryStore::new();
    let three = (1..=3).map(|i| RawUrlRecord::with_loc(format!("https://example.com/{i}")));
    engine
        .run(records(three), &mut store, RunMode::Publish)
        .unwrap();
    assert!(store.published().unwrap().contains_key("sitemap-3.xml"));

    let two = (1..=2).map(|i| RawUrlRecord::with_loc(format!("https://example.com/{i}")));
    let report = engine
        .run(records(two), &mut store, RunMode::Publish)
        .unwrap();
    assert_eq!(codes(&report), ["STALE_ARTIFACT"]);
    assert!(
        store.published().unwrap().contains_key("sitemap-3.xml"),
        "never deleted"
    );
}

#[test]
fn diagnostics_are_capped_with_exact_counts() {
    let config = EngineConfig {
        max_diagnostics: 5,
        on_invalid: InvalidPolicy::Skip,
        ..EngineConfig::default()
    };
    let engine = Engine::new(config).unwrap();
    let mut store = MemoryStore::new();
    let bad = (0..20).map(|i| RawUrlRecord::with_loc(format!("ftp://example.com/{i}")));
    let report = engine
        .run(records(bad), &mut store, RunMode::Publish)
        .unwrap();
    assert_eq!(report.diagnostics.len(), 6);
    assert_eq!(report.diagnostics[5].code.as_str(), "DIAGNOSTICS_TRUNCATED");
    assert_eq!(report.diagnostic_counts["URL_UNSUPPORTED_SCHEME"], 20);
    assert_eq!(report.diagnostic_counts["EMPTY_SITEMAP"], 1);
}

#[test]
fn file_store_end_to_end_and_well_formed() {
    let dir = std::env::temp_dir().join(format!("advanced-sitemap-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let mut config = EngineConfig::default();
    config.layout.public_base_url =
        Some(PublicBaseUrl::new(SitemapUrl::new("https://example.com/").unwrap()).unwrap());
    let engine = Engine::new(config).unwrap();
    let mut store = FsStore::new(&dir);
    let input = (0..120_000)
        .map(|i| RawUrlRecord::with_loc(format!("https://example.com/p/{i}?a=1&b=<{i}>")));
    let report = engine
        .run(records(input), &mut store, RunMode::Publish)
        .unwrap();
    assert_eq!(report.status, RunStatus::Published);

    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "sitemap-1.xml",
            "sitemap-2.xml",
            "sitemap-3.xml",
            "sitemap.manifest.json",
            "sitemap.xml"
        ]
    );

    let mut total = 0;
    for name in [
        "sitemap-1.xml",
        "sitemap-2.xml",
        "sitemap-3.xml",
        "sitemap.xml",
    ] {
        let bytes = fs::read(dir.join(name)).unwrap();
        assert!(bytes.len() <= 52_428_800);
        let mut reader = quick_xml::Reader::from_reader(bytes.as_slice());
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(e)) if e.name().0 == "url" => total += 1,
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => {}
                Err(error) => panic!("{name} is not well-formed: {error}"),
            }
            buf.clear();
        }
    }
    assert_eq!(total, 120_000);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn invalid_robots_configuration_is_refused() {
    let config = EngineConfig {
        robots: Some(advanced_sitemap::RobotsConfig {
            groups: vec![advanced_sitemap::RobotsGroup {
                user_agents: vec!["*".into()],
                disallow: vec!["private".into()],
                allow: vec![],
            }],
            sitemaps: vec![],
        }),
        ..EngineConfig::default()
    };
    let error = Engine::new(config).unwrap_err().to_string();
    assert!(error.contains("ROBOTS_PATH_INVALID"), "{error}");
}

#[test]
fn robots_without_public_base_url_warns() {
    let config = EngineConfig {
        robots: Some(advanced_sitemap::RobotsConfig::default()),
        ..EngineConfig::default()
    };
    let engine = Engine::new(config).unwrap();
    let mut store = MemoryStore::new();
    let report = engine
        .run(
            records([RawUrlRecord::with_loc("https://example.com/")]),
            &mut store,
            RunMode::Publish,
        )
        .unwrap();
    assert!(report.succeeded());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == "ROBOTS_SITEMAP_NOT_LISTED")
    );
    assert_eq!(store.published().unwrap()["robots.txt"], b"");
}

#[test]
fn hreflang_reciprocity_through_the_protocol() {
    let text = r#"{
      "schemaVersion": "1.1",
      "validation": { "hreflangReciprocity": true },
      "output": { "manifest": false },
      "urls": [
        { "loc": "https://example.com/en/", "alternates": [
          { "hreflang": "en", "href": "https://example.com/en/" },
          { "hreflang": "fr", "href": "https://example.com/fr/" },
          { "hreflang": "de", "href": "https://example.de/" } ] },
        { "loc": "https://example.com/fr/", "alternates": [
          { "hreflang": "fr", "href": "https://example.com/fr/" } ] }
      ]
    }"#;
    let resolved =
        resolve_config(ConfigDocument::from_json(text).unwrap(), Path::new(".")).unwrap();
    assert!(resolved.engine.hreflang_reciprocity);
    let engine = Engine::new(resolved.engine.clone()).unwrap();
    let source = open_input(resolved.input.as_ref().unwrap()).unwrap();
    let report = engine
        .run(source, &mut NullStore::new(), RunMode::DryRun)
        .unwrap();
    assert_eq!(report.status, RunStatus::Valid, "warnings do not block");
    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes,
        ["HREFLANG_RETURN_LINK_MISSING", "HREFLANG_TARGET_NOT_LISTED"]
    );
    assert_eq!(report.diagnostics[0].location.record, Some(1));

    // Off by default: no cross-entry diagnostics.
    let off = text.replace(r#""hreflangReciprocity": true"#, r#""dedupe": true"#);
    let resolved =
        resolve_config(ConfigDocument::from_json(&off).unwrap(), Path::new(".")).unwrap();
    let engine = Engine::new(resolved.engine.clone()).unwrap();
    let source = open_input(resolved.input.as_ref().unwrap()).unwrap();
    let report = engine
        .run(source, &mut NullStore::new(), RunMode::DryRun)
        .unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}
