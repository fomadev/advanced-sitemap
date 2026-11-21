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

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|it| {
            it.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
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
