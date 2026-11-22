//! Lot B: execution modes, state tokens and the plan of an `apply`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use advanced_sitemap::plan::{CostModel, explain, explain_apply};
use advanced_sitemap::protocol::ChangeRecord;
use advanced_sitemap::{
    Engine, EngineConfig, EngineError, FsStore, InputSpec, PartitionStrategy, PublicBaseUrl,
    PublishPolicy, RawUrlRecord, ResolvedConfig, RunMode, SitemapUrl, SnapshotSettings,
    SourceRecord, records,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "advanced-sitemap-planner-{label}-{}",
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

fn config(state: Option<&Path>, publish: PublishPolicy) -> EngineConfig {
    let mut config = EngineConfig {
        strategy: PartitionStrategy::ConsistentHash {
            partitions: NonZeroU32::new(6).unwrap(),
        },
        publish,
        ..EngineConfig::default()
    };
    config.layout.public_base_url =
        Some(PublicBaseUrl::new(SitemapUrl::new("https://example.com/").unwrap()).unwrap());
    config.snapshot = state.map(SnapshotSettings::new);
    config
}

fn input(versions: &BTreeMap<u32, u32>) -> Vec<RawUrlRecord> {
    let mut records: Vec<RawUrlRecord> = versions
        .iter()
        .map(|(id, v)| RawUrlRecord {
            loc: Some(format!("https://example.com/p/{id:04}")),
            lastmod: Some(format!("2026-01-{:02}", 1 + v % 28)),
            ..RawUrlRecord::default()
        })
        .collect();
    records.sort_by(|a, b| a.loc.cmp(&b.loc));
    records
}

fn run(
    config: &EngineConfig,
    out: &Path,
    versions: &BTreeMap<u32, u32>,
) -> Result<advanced_sitemap::GenerationReport, EngineError> {
    Engine::new(config.clone()).unwrap().run(
        records(input(versions)),
        &mut FsStore::new(out),
        RunMode::Publish,
    )
}

fn resolved(config: &EngineConfig, out: &Path, versions: &BTreeMap<u32, u32>) -> ResolvedConfig {
    let values = input(versions)
        .into_iter()
        .map(|r| serde_json::json!({"loc": r.loc, "lastmod": r.lastmod}))
        .collect();
    ResolvedConfig {
        engine: config.clone(),
        input: Some(InputSpec::Inline(values)),
        output_dir: out.to_path_buf(),
    }
}

#[test]
fn auto_mode_writes_everything_first_then_only_what_changed() {
    let tmp = TempDir::new("auto");
    let out = tmp.0.join("out");
    let config = config(None, PublishPolicy::Auto);
    let versions: BTreeMap<u32, u32> = (0..300).map(|i| (i, 0)).collect();

    let first = run(&config, &out, &versions).unwrap();
    let e = first.execution.as_ref().unwrap();
    assert_eq!((e.requested, e.chosen), ("auto", "full"), "{}", e.reason);
    assert!(first.artifacts.iter().all(|a| !a.unchanged));

    let second = run(&config, &out, &versions).unwrap();
    let e = second.execution.as_ref().unwrap();
    assert_eq!(e.chosen, "skip-unchanged", "{}", e.reason);
    assert!(second.artifacts.iter().all(|a| a.unchanged));

    // The plan says the same before each run.
    let plan = explain(&resolved(&config, &out, &versions), 50).unwrap();
    assert_eq!(plan.execution.chosen, "skip-unchanged");
    let modes: Vec<(&str, bool)> = plan
        .execution
        .candidates
        .iter()
        .map(|c| (c.mode, c.eligible))
        .collect();
    assert_eq!(
        modes,
        [("full", true), ("skip-unchanged", true), ("delta", false)]
    );
}

#[test]
fn a_run_planned_on_an_older_state_is_refused() {
    let tmp = TempDir::new("state");
    let (out, state) = (tmp.0.join("out"), tmp.0.join("state"));
    let config = config(Some(&state), PublishPolicy::Auto);
    let versions: BTreeMap<u32, u32> = (0..100).map(|i| (i, 0)).collect();
    run(&config, &out, &versions).unwrap();

    let token = explain(&resolved(&config, &out, &versions), 50)
        .unwrap()
        .execution
        .state_token;
    assert_eq!(token.len(), 32);
    // Planning again without any change gives the same token.
    let again = explain(&resolved(&config, &out, &versions), 50).unwrap();
    assert_eq!(again.execution.state_token, token);

    // Another run publishes in between.
    let changed: BTreeMap<u32, u32> = (0..100).map(|i| (i, 1)).collect();
    run(&config, &out, &changed).unwrap();
    let before: Vec<u8> = fs::read(out.join("sitemap.manifest.json")).unwrap();

    let mut planned = config.clone();
    planned.expected_state = Some(token.clone());
    match run(&planned, &out, &versions) {
        Err(EngineError::StateChanged { expected, actual }) => {
            assert_eq!(expected, token);
            assert_ne!(actual, token);
        }
        other => panic!("expected StateChanged, got {other:?}"),
    }
    assert_eq!(fs::read(out.join("sitemap.manifest.json")).unwrap(), before);

    // With the current token, the run goes ahead.
    let current = explain(&resolved(&config, &out, &changed), 50)
        .unwrap()
        .execution
        .state_token;
    planned.expected_state = Some(current);
    run(&planned, &out, &versions).unwrap();
}

fn feed(changes: Vec<ChangeRecord>) -> Vec<std::io::Result<SourceRecord<ChangeRecord>>> {
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

#[test]
fn explain_plans_an_apply_without_writing() {
    let tmp = TempDir::new("apply-plan");
    let (out, state) = (tmp.0.join("out"), tmp.0.join("state"));
    let config = config(Some(&state), PublishPolicy::Auto);
    let versions: BTreeMap<u32, u32> = (0..600).map(|i| (i, 0)).collect();
    let changes = || {
        feed(vec![
            ChangeRecord::Upsert {
                url: input(&BTreeMap::from([(7, 3)])).remove(0),
            },
            ChangeRecord::Delete {
                loc: "https://example.com/p/0008".to_owned(),
            },
        ])
    };
    let mut without_source = resolved(&config, &out, &versions);
    without_source.input = None;

    // Nothing published yet: the delta mode is refused, and nothing is
    // possible without a source.
    let plan = explain_apply(&without_source, changes(), &CostModel::REFERENCE).unwrap();
    assert_eq!(plan.chosen, "none", "{plan:?}");
    assert!(plan.candidates[0].reasons[0].contains("no snapshot"));
    assert!(!out.exists() || fs::read_dir(&out).unwrap().count() == 0);

    run(&config, &out, &versions).unwrap();
    let before = fs::read(state.join("sitemap.snapshot")).unwrap();
    let plan = explain_apply(&without_source, changes(), &CostModel::REFERENCE).unwrap();
    assert_eq!(plan.chosen, "delta", "{plan:?}");
    let delta = &plan.candidates[0];
    let touched: std::collections::BTreeSet<u32> =
        ["https://example.com/p/0007", "https://example.com/p/0008"]
            .iter()
            .filter_map(|loc| config.strategy.hash_partition(loc))
            .collect();
    assert_eq!(
        delta.partitions_affected.value(),
        Some(touched.len() as u64)
    );
    assert_eq!(
        fs::read(state.join("sitemap.snapshot")).unwrap(),
        before,
        "explain wrote state"
    );

    // The token from the plan lets the apply run.
    let mut planned = config.clone();
    planned.expected_state = Some(plan.state_token.clone());
    let report = Engine::new(planned)
        .unwrap()
        .apply_changes(changes(), &mut FsStore::new(&out), RunMode::Publish)
        .unwrap();
    let c = report.changes.unwrap();
    assert_eq!((c.modified, c.removed), (1, 1));
}
