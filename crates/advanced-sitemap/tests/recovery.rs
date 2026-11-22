//! Production hardening: concurrent runs, leftovers of interrupted runs and
//! file system errors, on the real file system store.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use advanced_sitemap::protocol::ChangeRecord;
use advanced_sitemap::{
    DirectoryLock, Engine, EngineConfig, EngineError, FsStore, PartitionStrategy, PublicBaseUrl,
    RawUrlRecord, RunMode, RunStatus, SitemapUrl, SnapshotSettings, SourceRecord, records,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "advanced-sitemap-recovery-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config(state: Option<&Path>) -> EngineConfig {
    let mut config = EngineConfig::default();
    config.strategy = PartitionStrategy::ConsistentHash {
        partitions: NonZeroU32::new(4).unwrap(),
    };
    config.layout.public_base_url =
        Some(PublicBaseUrl::new(SitemapUrl::new("https://example.com/").unwrap()).unwrap());
    config.snapshot = state.map(SnapshotSettings::new);
    config
}

fn input(versions: &BTreeMap<u32, u32>) -> Vec<RawUrlRecord> {
    let mut records: Vec<RawUrlRecord> = versions
        .iter()
        .map(|(id, v)| {
            let mut this = RawUrlRecord::default();
            this.loc = Some(format!("https://example.com/p/{id:04}"));
            this.lastmod = Some(format!("2026-01-{:02}", 1 + v % 28));
            this
        })
        .collect();
    records.sort_by(|a, b| a.loc.cmp(&b.loc));
    records
}

fn generate(
    config: &EngineConfig,
    out: &Path,
    versions: &BTreeMap<u32, u32>,
) -> Result<RunStatus, EngineError> {
    Engine::new(config.clone())
        .unwrap()
        .run(
            records(input(versions)),
            &mut FsStore::new(out),
            RunMode::Publish,
        )
        .map(|r| r.status)
}

fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        // The lock file is not part of the output; on Windows it cannot even
        // be read while a run holds it.
        .filter(|e| e.file_type().unwrap().is_file() && e.file_name() != ".advanced-sitemap.lock")
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_second_run_on_the_same_output_is_refused_while_one_is_running() {
    let tmp = TempDir::new("busy");
    let out = tmp.0.join("out");
    let versions: BTreeMap<u32, u32> = (0..50).map(|i| (i, 0)).collect();
    generate(&config(None), &out, &versions).unwrap();
    let before = files(&out);

    // A run in progress holds the output directory's lock.
    let held = DirectoryLock::acquire(&out, ".advanced-sitemap.lock").unwrap();
    let changed: BTreeMap<u32, u32> = (0..50).map(|i| (i, 1)).collect();
    match generate(&config(None), &out, &changed) {
        Err(EngineError::Busy(message)) => assert!(message.contains("locked"), "{message}"),
        other => panic!("expected Busy, got {other:?}"),
    }
    assert_eq!(files(&out), before, "a refused run wrote something");
    drop(held);
    assert_eq!(
        generate(&config(None), &out, &changed).unwrap(),
        RunStatus::Published
    );
}

#[test]
fn a_shared_state_directory_is_locked_too() {
    let tmp = TempDir::new("state-lock");
    let state = tmp.0.join("state");
    let versions: BTreeMap<u32, u32> = (0..20).map(|i| (i, 0)).collect();
    let held = DirectoryLock::acquire(&state, "sitemap.lock").unwrap();
    // Different output directories, same state directory and stem.
    match generate(&config(Some(&state)), &tmp.0.join("out"), &versions) {
        Err(EngineError::Busy(_)) => {}
        other => panic!("expected Busy, got {other:?}"),
    }
    drop(held);
    generate(&config(Some(&state)), &tmp.0.join("out"), &versions).unwrap();
}

#[test]
fn leftovers_of_an_interrupted_run_do_not_block_the_next_one() {
    let tmp = TempDir::new("leftovers");
    let (out, state) = (tmp.0.join("out"), tmp.0.join("state"));
    let versions: BTreeMap<u32, u32> = (0..60).map(|i| (i, 0)).collect();
    generate(&config(Some(&state)), &out, &versions).unwrap();

    // What a run killed in the middle leaves behind.
    let staging = out.join(".advanced-sitemap-staging-424242-7");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("sitemap-2.xml"), b"<?xml half written").unwrap();
    fs::write(state.join("sitemap.snapshot.tmp"), b"ASNAPv1\ntruncated").unwrap();

    let changed: BTreeMap<u32, u32> = (0..60).map(|i| (i, i % 3)).collect();
    assert_eq!(
        generate(&config(Some(&state)), &out, &changed).unwrap(),
        RunStatus::Published
    );
    assert!(!staging.exists(), "stale staging directory not removed");
    assert!(!state.join("sitemap.snapshot.tmp").exists());

    // The result equals a clean run.
    let clean = TempDir::new("leftovers-clean");
    generate(&config(None), &clean.0, &changed).unwrap();
    assert_eq!(files(&out), files(&clean.0));
}

/// A crash after the sitemaps were published but before the snapshot was
/// replaced leaves an older snapshot. The delta mode must then refuse to
/// touch a partition that changed, rather than build on stale state.
#[test]
fn a_snapshot_older_than_the_published_files_makes_apply_refuse() {
    let tmp = TempDir::new("stale-snapshot");
    let (out, state) = (tmp.0.join("out"), tmp.0.join("state"));
    let versions: BTreeMap<u32, u32> = (0..80).map(|i| (i, 0)).collect();
    generate(&config(Some(&state)), &out, &versions).unwrap();
    let snapshot = fs::read(state.join("sitemap.snapshot")).unwrap();

    // Publish new content, then put the old snapshot back (the rename of
    // the new one never happened).
    let changed: BTreeMap<u32, u32> = (0..80).map(|i| (i, 5)).collect();
    generate(&config(Some(&state)), &out, &changed).unwrap();
    fs::write(state.join("sitemap.snapshot"), snapshot).unwrap();
    let before = files(&out);

    let feed = vec![Ok(SourceRecord::new(
        1,
        Some(1),
        Ok(ChangeRecord::Upsert {
            url: input(&BTreeMap::from([(3, 9)])).remove(0),
        }),
    ))];
    let result = Engine::new(config(Some(&state))).unwrap().apply_changes(
        feed,
        &mut FsStore::new(&out),
        RunMode::Publish,
    );
    match result {
        Err(EngineError::DeltaRefused(reason)) => {
            // Caught before any file is read: the snapshot records the
            // manifest of the publication it describes.
            assert!(reason.contains("another publication"), "{reason}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(files(&out), before);
}

#[test]
fn an_unusable_output_directory_fails_without_publishing() {
    let tmp = TempDir::new("io-error");
    // The output "directory" is a file: every write fails.
    let out = tmp.0.join("out");
    fs::write(&out, b"not a directory").unwrap();
    let versions: BTreeMap<u32, u32> = (0..10).map(|i| (i, 0)).collect();
    match generate(&config(None), &out, &versions) {
        Err(EngineError::Io(_)) => {}
        other => panic!("expected an I/O error, got {other:?}"),
    }
    assert_eq!(fs::read(&out).unwrap(), b"not a directory");
}

#[test]
fn the_manifest_lists_the_digest_of_every_published_file() {
    let tmp = TempDir::new("digests");
    let out = tmp.0.join("out");
    let versions: BTreeMap<u32, u32> = (0..90).map(|i| (i, 0)).collect();
    let report = Engine::new(config(None))
        .unwrap()
        .run(
            records(input(&versions)),
            &mut FsStore::new(&out),
            RunMode::Publish,
        )
        .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("sitemap.manifest.json")).unwrap()).unwrap();
    let artifacts = manifest["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 5); // 4 partitions and the index
    for artifact in artifacts {
        let name = artifact["name"].as_str().unwrap();
        // Recomputed from the file, independently of the streaming hash.
        let expected = advanced_sitemap::to_hex(&advanced_sitemap::digest128(
            &fs::read(out.join(name)).unwrap(),
        ));
        assert_eq!(artifact["digest"], expected.as_str(), "{name}");
    }
    for artifact in &report.artifacts {
        let expected = advanced_sitemap::to_hex(&advanced_sitemap::digest128(
            &fs::read(out.join(&artifact.name)).unwrap(),
        ));
        assert_eq!(
            artifact.digest.as_deref(),
            Some(expected.as_str()),
            "{}",
            artifact.name
        );
    }
}

/// A run interrupted after publishing, before the snapshot is replaced,
/// leaves a snapshot that describes another publication: the next change
/// report says so.
#[test]
fn a_stale_snapshot_is_reported_by_the_next_run() {
    let tmp = TempDir::new("stale-report");
    let (out, state) = (tmp.0.join("out"), tmp.0.join("state"));
    let versions: BTreeMap<u32, u32> = (0..40).map(|i| (i, 0)).collect();
    generate(&config(Some(&state)), &out, &versions).unwrap();
    let snapshot = fs::read(state.join("sitemap.snapshot")).unwrap();
    let changed: BTreeMap<u32, u32> = (0..40).map(|i| (i, 2)).collect();
    generate(&config(Some(&state)), &out, &changed).unwrap();
    fs::write(state.join("sitemap.snapshot"), snapshot).unwrap();

    let report = Engine::new(config(Some(&state)))
        .unwrap()
        .run(
            records(input(&changed)),
            &mut FsStore::new(&out),
            RunMode::Publish,
        )
        .unwrap();
    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert!(codes.contains(&"SNAPSHOT_STALE"), "{codes:?}");
    // The run replaced the snapshot: the next one is clean.
    let next = Engine::new(config(Some(&state)))
        .unwrap()
        .run(
            records(input(&changed)),
            &mut FsStore::new(&out),
            RunMode::Publish,
        )
        .unwrap();
    assert!(
        next.diagnostics
            .iter()
            .all(|d| d.code.as_str() != "SNAPSHOT_STALE")
    );
}
