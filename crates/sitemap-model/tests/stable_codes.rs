//! Diagnostic codes are a public contract: a frozen list guards their names
//! and default severities, and every code must be documented.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeMap;

use sitemap_model::{DiagnosticCode, Severity};

fn frozen() -> Vec<(&'static str, &'static str)> {
    include_str!("stable_codes.txt")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| l.split_once(' ').expect("CODE Severity"))
        .collect()
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "Error",
        Severity::Warning => "Warning",
        Severity::Recommendation => "Recommendation",
        Severity::Info => "Info",
    }
}

#[test]
fn no_code_is_renamed_removed_or_downgraded() {
    let current: BTreeMap<&str, &str> = DiagnosticCode::ALL
        .iter()
        .map(|c| (c.as_str(), severity_name(c.default_severity())))
        .collect();
    for (code, severity) in frozen() {
        let found = current
            .get(code)
            .unwrap_or_else(|| panic!("{code} was removed or renamed: a breaking change"));
        assert_eq!(*found, severity, "default severity of {code} changed");
    }
}

#[test]
fn every_code_is_frozen_and_documented() {
    let frozen: Vec<&str> = frozen().into_iter().map(|(code, _)| code).collect();
    let docs = include_str!("../../../docs/concepts/diagnostics.md");
    for code in DiagnosticCode::ALL {
        let code = code.as_str();
        assert!(
            frozen.contains(&code),
            "{code} is new: append it to tests/stable_codes.txt"
        );
        assert!(
            docs.contains(code),
            "{code} is not documented in docs/concepts/diagnostics.md"
        );
    }
}
