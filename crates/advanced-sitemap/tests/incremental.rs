//! Phase A of the lifecycle design: `PublishPolicy::Changed`.
//!
//! Invariant I1: publishing only changed files gives exactly the bytes of a
//! full publication. Invariants I3/I4: after a failure in the middle of a
//! publication, the next run converges to the same result.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;
use std::io;
use std::num::NonZeroU32;

use advanced_sitemap::{
    ArtifactName, ArtifactStore, ArtifactWriter, Engine, EngineConfig, GenerationReport,
    MemoryStore, PartitionLimits, PartitionStrategy, PublicBaseUrl, Publication, PublishPolicy,
    RawUrlRecord, RunMode, SitemapUrl, StoreDescription, records,
};

/// Deterministic xorshift generator: the test needs no dependency and is
/// reproducible.
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

fn config(strategy: PartitionStrategy, publish: PublishPolicy) -> EngineConfig {
    let mut config = EngineConfig {
        strategy,
        publish,
        limits: PartitionLimits::new(7, 52_428_800).unwrap(),
        ..EngineConfig::default()
    };
    config.layout.public_base_url = Some(
        PublicBaseUrl::new(SitemapUrl::new("https://example.com/sitemaps/").unwrap()).unwrap(),
    );
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
        ..RawUrlRecord::default()
    }
}

fn run(
    config: &EngineConfig,
    pages: &BTreeMap<u64, u64>,
    store: &mut dyn ArtifactStore,
) -> io::Result<GenerationReport> {
    let engine = Engine::new(config.clone()).unwrap();
    let input: Vec<RawUrlRecord> = pages.iter().map(|(id, v)| record(*id, *v)).collect();
    engine
        .run(records(input), store, RunMode::Publish)
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Files of the current publication, read from a store.
fn current_files(report: &GenerationReport, store: &MemoryStore) -> BTreeMap<String, Vec<u8>> {
    let published = store.published().unwrap();
    report
        .artifacts
        .iter()
        .map(|a| (a.name.clone(), published[&a.name].clone()))
        .collect()
}

fn check_equivalence(strategy: PartitionStrategy, seed: u64) {
    let mut rng = Rng(seed);
    let mut pages: BTreeMap<u64, u64> = (0..40).map(|id| (id, 0)).collect();
    let incremental_config = config(strategy, PublishPolicy::Changed);
    let full_config = config(strategy, PublishPolicy::All);
    let incremental = MemoryStore::new();

    for step in 0..60 {
        // A few random changes: add, remove, or modify pages.
        for _ in 0..rng.below(4) {
            let id = rng.below(80);
            match rng.below(3) {
                0 => {
                    pages.insert(id, step);
                }
                1 => {
                    pages.remove(&id);
                }
                _ => {
                    if let Some(version) = pages.get_mut(&id) {
                        *version += 1;
                    }
                }
            }
        }
        if pages.is_empty() {
            pages.insert(0, step);
        }

        // Everything already published, stale files from older runs included.
        let before = incremental.published().unwrap();
        let report = run(&incremental_config, &pages, &mut incremental.clone()).unwrap();
        let fresh = MemoryStore::new();
        let full_report = run(&full_config, &pages, &mut fresh.clone()).unwrap();

        let got = current_files(&report, &incremental);
        let expected = current_files(&full_report, &fresh);
        assert_eq!(got, expected, "seed {seed}, step {step}: I1 broken");

        for artifact in &report.artifacts {
            let same_as_before = before.get(&artifact.name) == got.get(&artifact.name);
            assert_eq!(
                artifact.unchanged, same_as_before,
                "seed {seed}, step {step}: {} unchanged flag",
                artifact.name
            );
        }
    }
}

#[test]
fn changed_policy_equals_full_publication_sequential() {
    for seed in [1, 2, 3, 0x5eed] {
        check_equivalence(PartitionStrategy::Sequential, seed);
    }
}

#[test]
fn changed_policy_equals_full_publication_stable_hash() {
    let strategy = PartitionStrategy::StableHash {
        partitions: NonZeroU32::new(6).unwrap(),
    };
    for seed in [7, 8, 9, 0xbeef] {
        check_equivalence(strategy, seed);
    }
}

#[test]
fn rerun_without_changes_publishes_nothing() {
    let strategy = PartitionStrategy::StableHash {
        partitions: NonZeroU32::new(4).unwrap(),
    };
    let config = config(strategy, PublishPolicy::Changed);
    let pages: BTreeMap<u64, u64> = (0..20).map(|id| (id, 0)).collect();
    let store = MemoryStore::new();
    let first = run(&config, &pages, &mut store.clone()).unwrap();
    assert!(first.artifacts.iter().all(|a| !a.unchanged));
    let second = run(&config, &pages, &mut store.clone()).unwrap();
    assert!(
        second.artifacts.iter().all(|a| a.unchanged),
        "{:?}",
        second.artifacts
    );

    // One page changes: its partition, the index (its lastmod) and the
    // manifest are republished; the other partitions are not.
    let mut changed = pages.clone();
    changed.insert(3, 5);
    let third = run(&config, &changed, &mut store.clone()).unwrap();
    let republished: Vec<&str> = third
        .artifacts
        .iter()
        .filter(|a| !a.unchanged)
        .map(|a| a.name.as_str())
        .collect();
    assert!(republished.contains(&"sitemap.xml"), "{republished:?}");
    assert_eq!(
        republished
            .iter()
            .filter(|n| n.starts_with("sitemap-"))
            .count(),
        1,
        "{republished:?}"
    );
}

/// Publishes only the first `limit` artifacts of the plan, then fails, as if
/// the process stopped in the middle of a publication.
struct FailingStore {
    inner: MemoryStore,
    limit: usize,
}

impl ArtifactStore for FailingStore {
    fn create(&mut self, name: &ArtifactName) -> io::Result<Box<dyn ArtifactWriter>> {
        self.inner.create(name)
    }

    fn read_published(&self, name: &ArtifactName) -> io::Result<Option<Vec<u8>>> {
        self.inner.read_published(name)
    }

    fn publish(&mut self, plan: &[Publication]) -> io::Result<()> {
        let cut = self.limit.min(plan.len());
        self.inner.publish(&plan[..cut])?;
        if cut < plan.len() {
            return Err(io::Error::other("simulated failure"));
        }
        Ok(())
    }

    fn discard(&mut self) -> io::Result<()> {
        self.inner.discard()
    }

    fn staged_matches_published(
        &self,
        staged: &ArtifactName,
        target: &ArtifactName,
    ) -> io::Result<bool> {
        self.inner.staged_matches_published(staged, target)
    }

    fn describe(&self) -> StoreDescription {
        self.inner.describe()
    }
}

#[test]
fn interrupted_publication_converges_on_the_next_run() {
    let strategy = PartitionStrategy::StableHash {
        partitions: NonZeroU32::new(5).unwrap(),
    };
    let config = config(strategy, PublishPolicy::Changed);
    let before: BTreeMap<u64, u64> = (0..30).map(|id| (id, 0)).collect();
    let after: BTreeMap<u64, u64> = (0..30).map(|id| (id, id % 3)).chain([(99, 1)]).collect();

    let fresh = MemoryStore::new();
    let expected_report = run(&config, &after, &mut fresh.clone()).unwrap();
    let expected = current_files(&expected_report, &fresh);

    for limit in 0..8 {
        let store = MemoryStore::new();
        run(&config, &before, &mut store.clone()).unwrap();
        let mut failing = FailingStore {
            inner: store.clone(),
            limit,
        };
        let interrupted = run(&config, &after, &mut failing);
        let report = run(&config, &after, &mut store.clone()).unwrap();
        assert_eq!(
            current_files(&report, &store),
            expected,
            "limit {limit}: did not converge (first attempt: {:?})",
            interrupted.as_ref().map(|r| r.status)
        );
    }
}

#[test]
fn changed_policy_on_the_file_system_equals_full_publication() {
    use advanced_sitemap::FsStore;
    let dir = std::env::temp_dir().join(format!(
        "advanced-sitemap-incremental-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for strategy in [
        PartitionStrategy::Sequential,
        PartitionStrategy::StableHash {
            partitions: NonZeroU32::new(5).unwrap(),
        },
    ] {
        let mut rng = Rng(42);
        let mut pages: BTreeMap<u64, u64> = (0..40).map(|id| (id, 0)).collect();
        let incremental_config = config(strategy, PublishPolicy::Changed);
        let full_config = config(strategy, PublishPolicy::All);
        let _ = std::fs::remove_dir_all(&dir);
        for step in 0..25 {
            for _ in 0..rng.below(4) {
                let id = rng.below(80);
                match rng.below(3) {
                    0 => {
                        pages.insert(id, step);
                    }
                    1 => {
                        pages.remove(&id);
                    }
                    _ => {
                        if let Some(v) = pages.get_mut(&id) {
                            *v += 1;
                        }
                    }
                }
            }
            if pages.is_empty() {
                pages.insert(0, step);
            }
            let report = run(&incremental_config, &pages, &mut FsStore::new(&dir)).unwrap();
            let fresh = MemoryStore::new();
            let full_report = run(&full_config, &pages, &mut fresh.clone()).unwrap();
            let expected = current_files(&full_report, &fresh);
            let got: BTreeMap<String, Vec<u8>> = report
                .artifacts
                .iter()
                .map(|a| (a.name.clone(), std::fs::read(dir.join(&a.name)).unwrap()))
                .collect();
            assert_eq!(got, expected, "step {step}: I1 broken on the file system");
            let leftover = std::fs::read_dir(&dir).unwrap().any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".advanced-sitemap-staging")
            });
            assert!(!leftover, "step {step}: staging directory left behind");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
