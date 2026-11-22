//! Phase D: the delta mode (ADR-0013).
//!
//! Invariant I1: after any change feed, the published files are exactly
//! those of a full generation from the same URLs sorted by `loc`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;
use std::io;
use std::num::NonZeroU32;

use advanced_sitemap::protocol::ChangeRecord;
use advanced_sitemap::{
    ArtifactName, ArtifactStore, Engine, EngineConfig, EngineError, GenerationReport, MemoryStore,
    PartitionLimits, PartitionStrategy, PublicBaseUrl, RawImage, RawUrlRecord, RunMode, RunStatus,
    SitemapUrl, SnapshotSettings, SourceRecord, records,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

struct StateDir(std::path::PathBuf);

impl StateDir {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "advanced-sitemap-delta-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for StateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config(strategy: PartitionStrategy, state: &StateDir) -> EngineConfig {
    let mut config = EngineConfig {
        strategy,
        ..EngineConfig::default()
    };
    config.layout.public_base_url = Some(
        PublicBaseUrl::new(SitemapUrl::new("https://example.com/sitemaps/").unwrap()).unwrap(),
    );
    let mut settings = SnapshotSettings::new(&state.0);
    settings.memory_budget = 4_096; // force spilling
    config.snapshot = Some(settings);
    config
}

fn record(id: u64, version: u64) -> RawUrlRecord {
    RawUrlRecord {
        loc: Some(format!("https://example.com/p/{id}")),
        lastmod: Some(format!(
            "2026-{:02}-{:02}",
            1 + version % 12,
            1 + version % 28
        )),
        images: if version % 3 == 1 {
            vec![RawImage {
                loc: Some(format!("https://example.com/img/{id}-{version}.png")),
            }]
        } else {
            Vec::new()
        },
        ..RawUrlRecord::default()
    }
}

/// Full generation from the URLs sorted by `loc`: the reference output.
fn full(
    config: &EngineConfig,
    pages: &BTreeMap<u64, u64>,
    store: &mut dyn ArtifactStore,
) -> GenerationReport {
    let mut input: Vec<RawUrlRecord> = pages.iter().map(|(id, v)| record(*id, *v)).collect();
    input.sort_by(|a, b| a.loc.cmp(&b.loc));
    Engine::new(config.clone())
        .unwrap()
        .run(records(input), store, RunMode::Publish)
        .unwrap()
}

fn feed(changes: Vec<ChangeRecord>) -> Vec<io::Result<SourceRecord<ChangeRecord>>> {
    changes
        .into_iter()
        .enumerate()
        .map(|(i, change)| {
            Ok(SourceRecord {
                record: i as u64 + 1,
                line: Some(i as u64 + 1),
                result: Ok(change),
            })
        })
        .collect()
}

fn apply(
    config: &EngineConfig,
    changes: Vec<ChangeRecord>,
    store: &mut dyn ArtifactStore,
) -> Result<GenerationReport, EngineError> {
    Engine::new(config.clone())
        .unwrap()
        .apply_changes(feed(changes), store, RunMode::Publish)
}

fn current_files(report: &GenerationReport, store: &MemoryStore) -> BTreeMap<String, Vec<u8>> {
    let published = store.published().unwrap();
    report
        .artifacts
        .iter()
        .map(|a| (a.name.clone(), published[&a.name].clone()))
        .collect()
}

fn check_equivalence(strategy: PartitionStrategy, seed: u64) {
    let state = StateDir::new(&format!("oracle-{seed}"));
    let reference_state = StateDir::new(&format!("reference-{seed}"));
    let config = config(strategy, &state);
    let reference_config = {
        let mut c = config.clone();
        c.snapshot = Some(SnapshotSettings::new(&reference_state.0));
        c
    };
    let mut rng = Rng(seed);
    let mut pages: BTreeMap<u64, u64> = (0..120).map(|id| (id, rng.below(5))).collect();
    let store = MemoryStore::new();
    full(&config, &pages, &mut store.clone());

    for step in 1..=30 {
        let before = pages.clone();
        let mut changes = Vec::new();
        for _ in 0..1 + rng.below(6) {
            let id = rng.below(200);
            if rng.below(4) == 0 {
                pages.remove(&id);
                changes.push(ChangeRecord::Delete {
                    loc: format!("https://example.com/p/{id}"),
                });
            } else {
                let version = step * 10 + rng.below(10);
                pages.insert(id, version);
                changes.push(ChangeRecord::Upsert {
                    url: record(id, version),
                });
            }
        }
        if pages.is_empty() {
            pages.insert(0, step);
            changes.push(ChangeRecord::Upsert {
                url: record(0, step),
            });
        }

        let report = apply(&config, changes, &mut store.clone()).unwrap();
        assert_eq!(
            report.status,
            RunStatus::Published,
            "step {step}: {:?}",
            report.diagnostics
        );
        let fresh = MemoryStore::new();
        let _ = std::fs::remove_dir_all(&reference_state.0);
        let expected = full(&reference_config, &pages, &mut fresh.clone());
        assert_eq!(
            current_files(&report, &store),
            current_files(&expected, &fresh),
            "step {step}: delta output differs from a full sorted generation"
        );
        assert_eq!(report.stats.urls_written, pages.len() as u64, "step {step}");

        // The change report equals the known changes.
        let c = report.changes.as_ref().unwrap();
        let added = pages.keys().filter(|k| !before.contains_key(k)).count() as u64;
        let removed = before.keys().filter(|k| !pages.contains_key(k)).count() as u64;
        let modified = pages
            .iter()
            .filter(|(k, v)| {
                before
                    .get(k)
                    .is_some_and(|old| record(**k, *old) != record(**k, **v))
            })
            .count() as u64;
        assert_eq!(
            (c.added, c.removed, c.modified, c.moved),
            (added, removed, modified, 0),
            "step {step}"
        );
    }
}

#[test]
fn delta_equals_a_full_sorted_generation_consistent_hash() {
    let strategy = PartitionStrategy::ConsistentHash {
        partitions: NonZeroU32::new(7).unwrap(),
    };
    for seed in [1, 2, 0xde17a] {
        check_equivalence(strategy, seed);
    }
}

#[test]
fn delta_equals_a_full_sorted_generation_stable_hash() {
    let strategy = PartitionStrategy::StableHash {
        partitions: NonZeroU32::new(4).unwrap(),
    };
    for seed in [3, 0xfeed] {
        check_equivalence(strategy, seed);
    }
}

#[test]
fn delta_rewrites_only_the_touched_partitions() {
    let state = StateDir::new("touched");
    let config = config(
        PartitionStrategy::ConsistentHash {
            partitions: NonZeroU32::new(8).unwrap(),
        },
        &state,
    );
    let pages: BTreeMap<u64, u64> = (0..400).map(|id| (id, 0)).collect();
    let store = MemoryStore::new();
    full(&config, &pages, &mut store.clone());

    let report = apply(
        &config,
        vec![ChangeRecord::Upsert { url: record(7, 99) }],
        &mut store.clone(),
    )
    .unwrap();
    let rewritten: Vec<&str> = report
        .artifacts
        .iter()
        .filter(|a| !a.unchanged)
        .map(|a| a.name.as_str())
        .collect();
    let partition = config
        .strategy
        .hash_partition("https://example.com/p/7")
        .unwrap();
    assert_eq!(
        rewritten,
        // The manifest is identical: same URL count and same size.
        [format!("sitemap-{partition}.xml").as_str(), "sitemap.xml"],
        "{:?}",
        report.artifacts
    );
    assert_eq!(report.stats.records, 1);

    // The same feed again changes nothing.
    let again = apply(
        &config,
        vec![ChangeRecord::Upsert { url: record(7, 99) }],
        &mut store.clone(),
    )
    .unwrap();
    assert!(
        again.artifacts.iter().all(|a| a.unchanged),
        "{:?}",
        again.artifacts
    );
}

fn refused(result: Result<GenerationReport, EngineError>) -> String {
    match result {
        Err(EngineError::DeltaRefused(reason)) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn delta_is_refused_when_it_cannot_match_a_full_generation() {
    let state = StateDir::new("refused");
    let strategy = PartitionStrategy::ConsistentHash {
        partitions: NonZeroU32::new(3).unwrap(),
    };
    let config = config(strategy, &state);
    let pages: BTreeMap<u64, u64> = (0..30).map(|id| (id, 0)).collect();
    let change = || vec![ChangeRecord::Upsert { url: record(1, 5) }];

    // No snapshot yet.
    let store = MemoryStore::new();
    assert!(refused(apply(&config, change(), &mut store.clone())).contains("no snapshot"));
    full(&config, &pages, &mut store.clone());

    // Sequential placement depends on the source order.
    let mut sequential = config.clone();
    sequential.strategy = PartitionStrategy::Sequential;
    assert!(refused(apply(&sequential, change(), &mut store.clone())).contains("strategy"));

    // Settings changed since the snapshot (I7).
    let mut other = config.clone();
    other.limits = PartitionLimits::new(40_000, 52_428_800).unwrap();
    assert!(refused(apply(&other, change(), &mut store.clone())).contains("settings changed"));

    // No state directory.
    let mut stateless = config.clone();
    stateless.snapshot = None;
    assert!(refused(apply(&stateless, change(), &mut store.clone())).contains("state.directory"));

    // A touched file changed outside the engine.
    let partition = strategy.hash_partition("https://example.com/p/1").unwrap();
    let name = ArtifactName::new(format!("sitemap-{partition}.xml")).unwrap();
    let published = store.published().unwrap()[name.as_str()].clone();
    let tampered = String::from_utf8(published.clone())
        .unwrap()
        .replace("2026-01-01", "2026-01-02");
    let mut handle = store.clone();
    let mut writer = handle.create(&name).unwrap();
    writer.write_all(tampered.as_bytes()).unwrap();
    writer.finish().unwrap();
    handle
        .publish(&[advanced_sitemap::Publication {
            staged: name.clone(),
            target: name.clone(),
        }])
        .unwrap();
    let before = store.published().unwrap();
    assert!(
        refused(apply(&config, change(), &mut store.clone()))
            .contains("changed outside the engine")
    );
    assert_eq!(
        store.published().unwrap(),
        before,
        "a refused delta wrote something"
    );
}

#[test]
fn invalid_operations_follow_the_invalid_policy() {
    let state = StateDir::new("invalid");
    let config = config(
        PartitionStrategy::StableHash {
            partitions: NonZeroU32::new(2).unwrap(),
        },
        &state,
    );
    let pages: BTreeMap<u64, u64> = (0..10).map(|id| (id, 0)).collect();
    let store = MemoryStore::new();
    full(&config, &pages, &mut store.clone());
    let before = store.published().unwrap();
    let bad = vec![
        ChangeRecord::Upsert { url: record(50, 1) },
        ChangeRecord::Delete {
            loc: "not a url".to_owned(),
        },
    ];
    let report = apply(&config, bad.clone(), &mut store.clone()).unwrap();
    assert_eq!(report.status, RunStatus::Failed);
    assert_eq!(store.published().unwrap(), before);

    let mut skip = config.clone();
    skip.on_invalid = advanced_sitemap::InvalidPolicy::Skip;
    let report = apply(&skip, bad, &mut store.clone()).unwrap();
    assert_eq!(report.status, RunStatus::Published);
    assert_eq!(
        (report.stats.valid_records, report.stats.invalid_records),
        (1, 1)
    );
    assert_eq!(report.stats.urls_written, 11);
}
