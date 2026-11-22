//! `Engine::doctor`: read-only checks of the output and state directories.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

use advanced_sitemap::{
    CheckStatus, DirectoryLock, DoctorReport, Engine, EngineConfig, FsStore, LOCK_FILE,
    PartitionLimits, RawUrlRecord, RunMode, SnapshotSettings, records,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "advanced-sitemap-doctor-{label}-{}",
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

fn config(state: &Path) -> EngineConfig {
    let mut config = EngineConfig::default();
    config.snapshot = Some(SnapshotSettings::new(state));
    config
}

fn publish(config: &EngineConfig, out: &Path, count: u32) {
    let input: Vec<_> = (0..count)
        .map(|i| RawUrlRecord::with_loc(format!("https://example.com/p/{i}")))
        .collect();
    let mut store = FsStore::new(out);
    let report = Engine::new(config.clone())
        .unwrap()
        .run(records(input), &mut store, RunMode::Publish)
        .unwrap();
    assert!(report.succeeded());
}

fn status(report: &DoctorReport, name: &str) -> CheckStatus {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no {name} check"))
        .status
}

#[test]
fn nothing_published_is_healthy_and_nothing_is_created() {
    let dir = TempDir::new("empty");
    let out = dir.0.join("out");
    let state = dir.0.join("state");
    let report = Engine::new(config(&state)).unwrap().doctor(&out);
    assert!(!report.has_errors() && !report.has_warnings(), "{report:?}");
    assert_eq!(status(&report, "files"), CheckStatus::Skipped);
    assert!(report.state_token.is_some());
    assert!(!out.exists(), "doctor created the output directory");
    assert!(!state.exists(), "doctor created the state directory");
}

#[test]
fn a_fresh_publication_passes_every_check() {
    let dir = TempDir::new("fresh");
    let (out, state) = (dir.0.join("out"), dir.0.join("state"));
    let config = config(&state);
    publish(&config, &out, 3);
    let before = fs::read_dir(&out).unwrap().count();
    let engine = Engine::new(config).unwrap();
    let report = engine.doctor(&out);
    assert!(!report.has_errors() && !report.has_warnings(), "{report:?}");
    assert_eq!(status(&report, "files"), CheckStatus::Ok);
    assert_eq!(status(&report, "snapshot"), CheckStatus::Ok);
    assert_eq!(
        report.state_token.as_deref(),
        Some(engine.state_token(&FsStore::new(&out)).unwrap().as_str())
    );
    assert_eq!(fs::read_dir(&out).unwrap().count(), before);
}

#[test]
fn a_file_changed_outside_the_engine_is_an_error() {
    let dir = TempDir::new("tampered");
    let (out, state) = (dir.0.join("out"), dir.0.join("state"));
    let config = config(&state);
    publish(&config, &out, 3);
    // Same size, different bytes: only the digest can tell.
    let path = out.join("sitemap.xml");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replacen("/p/1<", "/p/7<", 1)).unwrap();
    let report = Engine::new(config.clone()).unwrap().doctor(&out);
    assert_eq!(status(&report, "files"), CheckStatus::Error, "{report:?}");
    assert!(report.has_errors());

    fs::remove_file(&path).unwrap();
    let report = Engine::new(config).unwrap().doctor(&out);
    let files = report.checks.iter().find(|c| c.name == "files").unwrap();
    assert!(files.message.contains("missing"), "{}", files.message);
}

#[test]
fn a_running_process_and_leftover_staging_are_warnings() {
    let dir = TempDir::new("busy");
    let (out, state) = (dir.0.join("out"), dir.0.join("state"));
    let config = config(&state);
    publish(&config, &out, 2);
    fs::create_dir(out.join(".advanced-sitemap-staging-999-0")).unwrap();
    let lock = DirectoryLock::acquire(&out, LOCK_FILE).unwrap();
    let report = Engine::new(config.clone()).unwrap().doctor(&out);
    assert_eq!(status(&report, "lock"), CheckStatus::Warning);
    assert_eq!(status(&report, "staging"), CheckStatus::Warning);
    assert!(!report.has_errors(), "{report:?}");
    drop(lock);
    let report = Engine::new(config).unwrap().doctor(&out);
    assert_eq!(status(&report, "lock"), CheckStatus::Ok);
}

#[test]
fn changed_settings_make_the_snapshot_a_warning() {
    let dir = TempDir::new("settings");
    let (out, state) = (dir.0.join("out"), dir.0.join("state"));
    let mut config = config(&state);
    publish(&config, &out, 2);
    config.limits = PartitionLimits::new(1_000, 10 * 1024 * 1024).unwrap();
    let report = Engine::new(config).unwrap().doctor(&out);
    assert_eq!(
        status(&report, "snapshot"),
        CheckStatus::Warning,
        "{report:?}"
    );
}

#[test]
fn a_snapshot_left_behind_by_an_interrupted_run_is_stale() {
    let dir = TempDir::new("stale");
    let (out, state) = (dir.0.join("out"), dir.0.join("state"));
    let config = config(&state);
    publish(&config, &out, 2);
    let snapshot = state.join("sitemap.snapshot");
    let old = fs::read(&snapshot).unwrap();
    publish(&config, &out, 3);
    // As if the second run published, then stopped before its snapshot.
    fs::write(&snapshot, old).unwrap();
    let report = Engine::new(config).unwrap().doctor(&out);
    let check = report.checks.iter().find(|c| c.name == "snapshot").unwrap();
    assert_eq!(check.status, CheckStatus::Warning);
    assert!(
        check.message.contains("SNAPSHOT_STALE"),
        "{}",
        check.message
    );
}
