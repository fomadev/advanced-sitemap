//! Tests of the `advanced-sitemap` binary: commands, exit codes, outputs.

// Test helpers outside #[test] functions may panic on failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_advanced-sitemap"))
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/conformance")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "advanced-sitemap-cli-{label}-{}",
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

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// Entries of an output directory, without the (empty, hidden) lock file
/// that every writing run keeps there.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|it| {
            it.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|n| n != ".advanced-sitemap.lock")
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn generate_from_config_matches_fixture() {
    let tmp = TempDir::new("generate");
    let output = bin()
        .args(["generate", "--config"])
        .arg(fixtures().join("partitioned/config.json"))
        .arg("--out")
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let expected_dir = fixtures().join("partitioned/expected");
    assert_eq!(listing(&tmp.0), listing(&expected_dir));
    for name in listing(&expected_dir) {
        assert_eq!(
            fs::read(tmp.0.join(&name)).unwrap(),
            fs::read(expected_dir.join(&name)).unwrap(),
            "{name}"
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Status       published"), "{stdout}");
}

#[test]
fn generate_from_jsonl_with_flags_and_json_report() {
    let tmp = TempDir::new("jsonl");
    let input = tmp.0.join("urls.jsonl");
    let lines: String = (0..25)
        .map(|i| format!("{{\"loc\":\"https://example.com/{i}\"}}\n"))
        .collect();
    fs::write(&input, lines).unwrap();
    let out = tmp.0.join("out");
    let output = bin()
        .args([
            "--format",
            "json",
            "generate",
            "--max-urls",
            "10",
            "--public-base-url",
            "https://example.com/",
        ])
        .arg("--input")
        .arg(&input)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(code(&output), 0);
    let report = json(&output);
    assert_eq!(report["status"], "published");
    assert_eq!(report["schemaVersion"], "1.0");
    assert_eq!(report["stats"]["urlsWritten"], 25);
    assert_eq!(
        listing(&out),
        [
            "sitemap-1.xml",
            "sitemap-2.xml",
            "sitemap-3.xml",
            "sitemap.manifest.json",
            "sitemap.xml"
        ]
    );
}

#[test]
fn invalid_input_exits_1_and_writes_nothing() {
    let tmp = TempDir::new("invalid");
    let output = bin()
        .args(["--format", "json", "generate", "--config"])
        .arg(fixtures().join("invalid-fail/config.json"))
        .arg("--out")
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(code(&output), 1);
    let report = json(&output);
    assert_eq!(report["status"], "failed");
    assert_eq!(report["severityCounts"]["error"], 4);
    assert_eq!(report["diagnostics"][4]["location"]["record"], 4);
    assert!(
        listing(&tmp.0).is_empty(),
        "no file and no staging directory left"
    );
}

#[test]
fn validate_writes_nothing() {
    let tmp = TempDir::new("validate");
    let config = tmp.0.join("config.json");
    fs::write(
        &config,
        r#"{"schemaVersion":"1.0","output":{"directory":"out"},"urls":[{"loc":"http://example.com/"}]}"#,
    )
    .unwrap();
    let output = bin()
        .args(["validate", "--config"])
        .arg(&config)
        .output()
        .unwrap();
    assert_eq!(code(&output), 0);
    assert_eq!(listing(&tmp.0), ["config.json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("URL_NOT_HTTPS"), "{stdout}");

    let strict = bin()
        .args(["validate", "--strict", "--config"])
        .arg(&config)
        .output()
        .unwrap();
    assert_eq!(code(&strict), 1);

    let deny = bin()
        .args([
            "validate",
            "--deny-warnings",
            "--site",
            "https://other.example",
            "--config",
        ])
        .arg(&config)
        .output()
        .unwrap();
    assert_eq!(
        code(&deny),
        1,
        "HOST_MISMATCH is a warning; --deny-warnings fails"
    );
}

#[test]
fn explain_reports_estimates_without_writing() {
    let tmp = TempDir::new("explain");
    let input = tmp.0.join("urls.jsonl");
    let lines: String = (0..5000)
        .map(|i| format!("{{\"loc\":\"https://example.com/products/{i:05}\"}}\n"))
        .collect();
    fs::write(&input, lines).unwrap();
    let output = bin()
        .args([
            "--format",
            "json",
            "explain",
            "--sample",
            "100",
            "--max-urls",
            "1000",
            "--strategy",
            "stable-hash",
            "--partitions",
            "2",
        ])
        .arg("--input")
        .arg(&input)
        .output()
        .unwrap();
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = json(&output);
    assert_eq!(plan["input"]["records"]["kind"], "estimated");
    let estimate = plan["input"]["records"]["value"].as_u64().unwrap();
    assert!((4900..=5100).contains(&estimate), "{estimate}");
    assert_eq!(plan["partitioning"]["suggestedPartitions"], 7);
    assert_eq!(plan["outputLayout"], "index");
    let notes = plan["notes"].to_string();
    assert!(notes.contains("PARTITION_LIMIT_EXCEEDED"), "{notes}");
    assert!(notes.contains("PUBLIC_BASE_URL_MISSING"), "{notes}");
    assert_eq!(listing(&tmp.0), ["urls.jsonl"]);
}

#[test]
fn usage_and_config_errors_exit_2() {
    let no_input = bin().args(["generate"]).output().unwrap();
    assert_eq!(code(&no_input), 2);

    let bad_flag = bin().args(["generate", "--nope"]).output().unwrap();
    assert_eq!(code(&bad_flag), 2);

    let tmp = TempDir::new("config");
    let config = tmp.0.join("config.json");
    fs::write(&config, r#"{"schemaVersion":"2.0"}"#).unwrap();
    let output = bin()
        .args(["--format", "json", "validate", "--config"])
        .arg(&config)
        .output()
        .unwrap();
    assert_eq!(code(&output), 2);
    assert_eq!(json(&output)["error"]["kind"], "config");
}

#[test]
fn missing_input_file_exits_3() {
    let output = bin()
        .args(["validate", "--input", "does-not-exist.jsonl"])
        .output()
        .unwrap();
    assert_eq!(code(&output), 3);
}

#[test]
fn inspect_reads_generated_files_and_reports_json() {
    let dir = fixtures().join("partitioned/expected");
    let output = bin()
        .args(["--format", "json", "inspect"])
        .arg(dir.join("sitemap.xml"))
        .arg(dir.join("sitemap-1.xml"))
        .output()
        .unwrap();
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json(&output);
    assert_eq!(report["status"], "valid");
    assert_eq!(report["files"][0]["kind"], "sitemapindex");
    assert_eq!(report["files"][0]["entries"], 3);
    assert_eq!(report["files"][1]["kind"], "urlset");
}

#[test]
fn inspect_exit_codes() {
    let temp = TempDir::new("inspect");
    let bad = temp.0.join("bad.xml");
    fs::write(
        &bad,
        r#"<!DOCTYPE x [<!ENTITY a "b">]><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"/>"#,
    )
    .unwrap();
    let output = bin()
        .args(["--format", "json", "inspect"])
        .arg(&bad)
        .output()
        .unwrap();
    assert_eq!(code(&output), 1);
    let report = json(&output);
    assert_eq!(report["status"], "invalid");
    assert_eq!(report["files"][0]["complete"], false);
    assert_eq!(
        report["files"][0]["diagnostics"][0]["code"],
        "XML_DOCTYPE_FORBIDDEN"
    );

    let warn = temp.0.join("warn.xml");
    fs::write(
        &warn,
        r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>https://example.com/</loc></url><url><loc>https://example.com/</loc></url></urlset>"#,
    )
    .unwrap();
    assert_eq!(code(&bin().arg("inspect").arg(&warn).output().unwrap()), 0);
    let denied = bin()
        .args(["inspect", "--deny-warnings"])
        .arg(&warn)
        .output()
        .unwrap();
    assert_eq!(code(&denied), 1);

    let missing = bin()
        .arg("inspect")
        .arg(temp.0.join("missing.xml"))
        .output()
        .unwrap();
    assert_eq!(code(&missing), 3);
    let usage = bin()
        .args(["inspect", "--site", "not a url", "x.xml"])
        .output()
        .unwrap();
    assert_eq!(code(&usage), 2);
}

#[test]
fn skip_unchanged_leaves_identical_files_untouched() {
    let temp = TempDir::new("skip-unchanged");
    let out = temp.0.join("out");
    let config = fixtures().join("partitioned/config.json");
    let generate = |extra: &[&str]| {
        let output = bin()
            .args(["--format", "json", "generate", "--config"])
            .arg(&config)
            .arg("--out")
            .arg(&out)
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(
            code(&output),
            0,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        json(&output)
    };
    let first = generate(&[]);
    let unchanged = |report: &serde_json::Value| {
        report["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["unchanged"] == true)
            .count()
    };
    assert_eq!(unchanged(&first), 0);
    let mtime = |name: &str| fs::metadata(out.join(name)).unwrap().modified().unwrap();
    let before = mtime("sitemap-1.xml");
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let second = generate(&["--skip-unchanged"]);
    let total = second["artifacts"].as_array().unwrap().len();
    assert_eq!(unchanged(&second), total, "{second}");
    assert_eq!(mtime("sitemap-1.xml"), before, "file was rewritten");
    assert!(
        !listing(&out)
            .iter()
            .any(|n| n.starts_with(".advanced-sitemap-staging"))
    );

    // Without the flag every file is republished.
    let third = generate(&[]);
    assert_eq!(unchanged(&third), 0);
    assert_ne!(mtime("sitemap-1.xml"), before);
}

#[test]
fn diff_compares_published_directories() {
    let temp = TempDir::new("diff");
    let old = temp.0.join("old");
    let new = temp.0.join("new");
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    let head = r#"<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#;
    fs::write(
        old.join("sitemap.xml"),
        format!("{head}<url><loc>https://example.com/a</loc></url><url><loc>https://example.com/b</loc><lastmod>2026-01-01</lastmod></url></urlset>"),
    )
    .unwrap();
    // Same `a` (different formatting), `b` modified, `c` added, split in two files.
    fs::write(
        new.join("sitemap-1.xml"),
        format!("{head}\n  <url>\n    <loc> https://example.com/a </loc>\n  </url>\n</urlset>"),
    )
    .unwrap();
    fs::write(
        new.join("sitemap-2.xml"),
        format!("{head}<url><loc>https://example.com/b</loc><lastmod>2026-02-01</lastmod></url><url><loc>https://example.com/c</loc></url></urlset>"),
    )
    .unwrap();

    let output = bin()
        .args(["--format", "json", "diff"])
        .arg(&old)
        .arg(&new)
        .output()
        .unwrap();
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json(&output);
    assert_eq!(report["status"], "different");
    let c = &report["changes"];
    assert_eq!(
        (
            c["added"].as_u64(),
            c["removed"].as_u64(),
            c["modified"].as_u64(),
            c["unchanged"].as_u64()
        ),
        (Some(1), Some(0), Some(1), Some(1))
    );
    assert_eq!(c["addedUrls"][0], "https://example.com/c");
    assert_eq!(c["modifiedUrls"][0], "https://example.com/b");
    assert_eq!(report["new"]["files"].as_array().unwrap().len(), 2);

    let strict = bin()
        .args(["diff", "--fail-on-change"])
        .arg(&old)
        .arg(&new)
        .output()
        .unwrap();
    assert_eq!(code(&strict), 1);
    let same = bin()
        .args(["diff", "--fail-on-change"])
        .arg(&old)
        .arg(&old)
        .output()
        .unwrap();
    assert_eq!(code(&same), 0);
    let empty = temp.0.join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert_eq!(
        code(&bin().arg("diff").arg(&old).arg(&empty).output().unwrap()),
        2
    );
}

#[test]
fn explain_measures_migration_cost_from_the_snapshot() {
    let temp = TempDir::new("migration");
    let input = temp.0.join("urls.jsonl");
    let lines: String = (0..500)
        .map(|i| format!("{{\"loc\":\"https://example.com/p/{i}\"}}\n"))
        .collect();
    fs::write(&input, lines).unwrap();
    let config = temp.0.join("config.json");
    fs::write(
        &config,
        r#"{"schemaVersion": "1.1", "publicBaseUrl": "https://example.com/",
            "partitioning": {"strategy": "consistent-hash", "partitions": 4},
            "source": {"path": "urls.jsonl"}, "output": {"directory": "out"},
            "state": {"directory": "state"}}"#,
    )
    .unwrap();
    let run = |command: &str, extra: &[&str]| {
        let output = bin()
            .args(["--format", "json", command, "--config"])
            .arg(&config)
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(
            code(&output),
            0,
            "{command} {extra:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        json(&output)
    };
    // No snapshot yet: no migration section.
    assert!(run("explain", &[]).get("migration").is_none());
    run("generate", &[]);

    let same = run("explain", &[]);
    let m = &same["migration"];
    assert_eq!(m["previousUrls"], 500);
    assert_eq!(m["previousPartitions"], 4);
    assert_eq!(m["configChanged"], false);
    assert_eq!(
        m["moved"],
        serde_json::json!({"kind": "measured", "value": 0})
    );

    // N -> N+1 with jump hash: about 1/5 of the URLs move.
    let grown = run("explain", &["--partitions", "5"]);
    assert_eq!(grown["migration"]["configChanged"], true);
    let moved = grown["migration"]["moved"]["value"].as_u64().unwrap();
    assert!((60..=140).contains(&moved), "{moved}");
    let notes = grown["notes"].to_string();
    assert!(notes.contains("PARTITION_MIGRATION_COST"), "{notes}");

    // Switching to stable-hash moves almost everything.
    let switched = run(
        "explain",
        &["--strategy", "stable-hash", "--partitions", "5"],
    );
    assert!(switched["migration"]["moved"]["value"].as_u64().unwrap() > 300);

    // A tight URL limit is flagged from the previous URLs.
    let tight = run("explain", &["--max-urls", "130"]);
    assert!(
        tight["notes"]
            .to_string()
            .contains("PARTITION_HEADROOM_LOW")
    );
}

#[test]
fn apply_rewrites_only_the_touched_partition() {
    let temp = TempDir::new("apply");
    let mut locs: Vec<String> = (0..300)
        .map(|i| format!("https://example.com/p/{i}"))
        .collect();
    locs.sort();
    let lines: String = locs
        .iter()
        .map(|loc| format!("{{\"loc\":\"{loc}\",\"lastmod\":\"2026-01-01\"}}\n"))
        .collect();
    fs::write(temp.0.join("urls.jsonl"), lines).unwrap();
    let config = temp.0.join("config.json");
    fs::write(
        &config,
        r#"{"schemaVersion": "1.1", "publicBaseUrl": "https://example.com/",
            "partitioning": {"strategy": "consistent-hash", "partitions": 6},
            "source": {"path": "urls.jsonl"}, "output": {"directory": "out"},
            "state": {"directory": "state"}}"#,
    )
    .unwrap();
    let feed = temp.0.join("changes.jsonl");
    fs::write(
        &feed,
        "{\"op\":\"upsert\",\"url\":{\"loc\":\"https://example.com/p/5\",\"lastmod\":\"2026-02-02\"}}\n{\"op\":\"delete\",\"loc\":\"https://example.com/p/6\"}\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        bin()
            .args(["--format", "json"])
            .args(args)
            .arg("--config")
            .arg(&config)
            .output()
            .unwrap()
    };

    // Before any publication there is no snapshot: refused, exit 2.
    let early = run(&["apply", "--changes", feed.to_str().unwrap()]);
    assert_eq!(code(&early), 2);
    assert_eq!(json(&early)["error"]["kind"], "delta-refused");

    assert_eq!(code(&run(&["generate"])), 0);
    let out = temp.0.join("out");
    let before: std::collections::BTreeMap<String, Vec<u8>> = listing(&out)
        .into_iter()
        .map(|n| (n.clone(), fs::read(out.join(&n)).unwrap()))
        .collect();

    // The feed changes two URLs: over a limit of one.
    let limited = run(&[
        "apply",
        "--max-operations",
        "1",
        "--changes",
        feed.to_str().unwrap(),
    ]);
    assert_eq!(code(&limited), 2);
    assert_eq!(json(&limited)["error"]["kind"], "delta-refused");

    let dry = run(&["apply", "--dry-run", "--changes", feed.to_str().unwrap()]);
    assert_eq!(code(&dry), 0, "{}", String::from_utf8_lossy(&dry.stdout));
    assert_eq!(json(&dry)["status"], "valid");
    for (name, bytes) in &before {
        assert_eq!(
            &fs::read(out.join(name)).unwrap(),
            bytes,
            "dry run changed {name}"
        );
    }

    let applied = run(&["apply", "--changes", feed.to_str().unwrap()]);
    assert_eq!(
        code(&applied),
        0,
        "{}",
        String::from_utf8_lossy(&applied.stdout)
    );
    let report = json(&applied);
    assert_eq!(report["status"], "published");
    assert_eq!(report["stats"]["urlsWritten"], 299);
    assert_eq!(report["changes"]["modified"], 1);
    assert_eq!(report["changes"]["removed"], 1);
    let changed: Vec<String> = listing(&out)
        .into_iter()
        .filter(|n| fs::read(out.join(n)).unwrap() != before[n])
        .collect();
    let rewritten = report["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["unchanged"] == false)
        .count();
    assert_eq!(changed.len(), rewritten, "{changed:?}");
    assert!(changed.len() <= 5, "{changed:?}"); // at most 2 partitions, index, manifest
    assert!(changed.contains(&"sitemap.xml".to_owned()), "{changed:?}");
}

#[test]
fn execution_mode_state_token_and_apply_plan() {
    let temp = TempDir::new("planner");
    let mut locs: Vec<String> = (0..200)
        .map(|i| format!("https://example.com/p/{i}"))
        .collect();
    locs.sort();
    let lines: String = locs
        .iter()
        .map(|loc| format!("{{\"loc\":\"{loc}\"}}\n"))
        .collect();
    fs::write(temp.0.join("urls.jsonl"), lines).unwrap();
    let config = temp.0.join("config.json");
    fs::write(
        &config,
        r#"{"schemaVersion": "1.1", "publicBaseUrl": "https://example.com/",
            "partitioning": {"strategy": "consistent-hash", "partitions": 5},
            "source": {"path": "urls.jsonl"},
            "output": {"directory": "out", "publish": "auto"},
            "state": {"directory": "state"}}"#,
    )
    .unwrap();
    let feed = temp.0.join("changes.jsonl");
    fs::write(
        &feed,
        "{\"op\":\"delete\",\"loc\":\"https://example.com/p/3\"}\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        bin()
            .args(["--format", "json"])
            .args(args)
            .arg("--config")
            .arg(&config)
            .output()
            .unwrap()
    };

    let first = json(&run(&["generate"]));
    assert_eq!(first["execution"]["chosen"], "full");
    let plan = json(&run(&["explain"]));
    assert_eq!(plan["execution"]["chosen"], "skip-unchanged");
    let token = plan["execution"]["stateToken"].as_str().unwrap().to_owned();

    // A wrong token is refused before anything is written.
    let stale = run(&[
        "generate",
        "--expect-state",
        "00000000000000000000000000000000",
    ]);
    assert_eq!(code(&stale), 2);
    assert_eq!(json(&stale)["error"]["kind"], "state-changed");
    let ok = run(&[
        "generate",
        "--execution-mode",
        "auto",
        "--expect-state",
        &token,
    ]);
    assert_eq!(code(&ok), 0, "{}", String::from_utf8_lossy(&ok.stdout));
    assert_eq!(json(&ok)["execution"]["chosen"], "skip-unchanged");

    // Plan of an apply: the delta mode is eligible and touches one partition.
    let apply_plan = run(&["explain", "--changes", feed.to_str().unwrap()]);
    assert_eq!(
        code(&apply_plan),
        0,
        "{}",
        String::from_utf8_lossy(&apply_plan.stdout)
    );
    let apply_plan = json(&apply_plan);
    assert_eq!(apply_plan["requested"], "delta");
    assert_eq!(apply_plan["candidates"][0]["mode"], "delta");
    assert_eq!(apply_plan["candidates"][0]["eligible"], true);
    assert_eq!(
        apply_plan["candidates"][0]["partitionsAffected"]["value"],
        1
    );
    let token = apply_plan["stateToken"].as_str().unwrap();
    let applied = run(&[
        "apply",
        "--changes",
        feed.to_str().unwrap(),
        "--expect-state",
        token,
    ]);
    assert_eq!(
        code(&applied),
        0,
        "{}",
        String::from_utf8_lossy(&applied.stdout)
    );
    assert_eq!(json(&applied)["changes"]["removed"], 1);

    // explain plans for the mode asked on its command line.
    let full = json(&run(&["explain", "--execution-mode", "full"]));
    assert_eq!(full["execution"]["chosen"], "full");

    // Human output names the decision.
    let human = bin()
        .arg("explain")
        .arg("--config")
        .arg(&config)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("Execution") && text.contains("State token"),
        "{text}"
    );
}

#[test]
fn doctor_reports_checks_and_exit_codes() {
    let temp = TempDir::new("doctor");
    let out = temp.0.join("out");
    let config = fixtures().join("partitioned/config.json");
    let doctor = |extra: &[&str]| {
        bin()
            .args(["--format", "json", "doctor", "--config"])
            .arg(&config)
            .arg("--out")
            .arg(&out)
            .args(extra)
            .output()
            .unwrap()
    };

    let empty = doctor(&[]);
    assert_eq!(code(&empty), 0);
    assert_eq!(json(&empty)["status"], "ok");
    assert!(!out.exists(), "doctor wrote the output directory");

    let generated = bin()
        .args(["generate", "--config"])
        .arg(&config)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(code(&generated), 0);
    let healthy = json(&doctor(&[]));
    assert_eq!(healthy["status"], "ok", "{healthy}");
    assert!(healthy["stateToken"].is_string());

    fs::create_dir(out.join(".advanced-sitemap-staging-1-0")).unwrap();
    assert_eq!(code(&doctor(&[])), 0);
    assert_eq!(code(&doctor(&["--deny-warnings"])), 1);
    fs::remove_dir(out.join(".advanced-sitemap-staging-1-0")).unwrap();

    fs::write(out.join("sitemap-1.xml"), "changed").unwrap();
    let broken = doctor(&[]);
    assert_eq!(code(&broken), 1);
    let report = json(&broken);
    assert_eq!(report["status"], "error");
    let files = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "files")
        .unwrap();
    assert_eq!(files["status"], "error", "{files}");
}
