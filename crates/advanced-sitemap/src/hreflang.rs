//! Cross-entry hreflang checks (return links and conflicting codes).
//!
//! Google ignores hreflang annotations that are not confirmed by the target
//! page: if A lists B as an alternate, B must list A. This needs every
//! entry of the run, so it runs after streaming, on compact edges:
//! 128-bit fingerprints plus an interned language id, and a pair map built
//! at the end (about 110 bytes per alternate link, measured). With
//! deduplication on, the set of written `loc` values is shared with it.
//! Only links whose target is also in the run can be checked; the others
//! are counted and reported once.

use std::collections::{HashMap, HashSet};

use sitemap_model::{Diagnostic, DiagnosticCode, Location, UrlEntry};

use crate::engine::fingerprint;

struct Edge {
    from: u128,
    to: u128,
    language: u32,
    record: u64,
}

/// Collects alternate links during a run and checks them at the end.
#[derive(Default)]
pub(crate) struct ReciprocityChecker {
    languages: HashMap<String, u32>,
    names: Vec<String>,
    edges: Vec<Edge>,
    locs: HashSet<u128>,
    /// `loc` text for each fingerprint that is the source of an edge, used
    /// in messages.
    sources: HashMap<u128, String>,
}

impl ReciprocityChecker {
    /// Records one written entry. `track_loc` is false when the caller
    /// already keeps the set of written `loc` fingerprints (deduplication).
    pub(crate) fn record(&mut self, entry: &UrlEntry, record: u64, track_loc: bool) {
        let from = fingerprint(entry.loc.as_str());
        if track_loc {
            self.locs.insert(from);
        }
        if entry.alternates.is_empty() {
            return;
        }
        self.sources
            .entry(from)
            .or_insert_with(|| entry.loc.as_str().to_owned());
        for alternate in &entry.alternates {
            let code = alternate.hreflang.as_str().to_ascii_lowercase();
            let next = self.names.len() as u32;
            let language = *self.languages.entry(code.clone()).or_insert_with(|| {
                self.names.push(code);
                next
            });
            self.edges.push(Edge {
                from,
                to: fingerprint(alternate.href.as_str()),
                language,
                record,
            });
        }
    }

    /// Checks every recorded link. `known` replaces the internal set of
    /// `loc` fingerprints when the caller tracked them. Diagnostics follow
    /// input order.
    pub(crate) fn finish(self, known: Option<&HashSet<u128>>) -> Vec<Diagnostic> {
        let locs = known.unwrap_or(&self.locs);
        let mut by_pair: HashMap<(u128, u128), u32> = HashMap::with_capacity(self.edges.len());
        for edge in &self.edges {
            by_pair.entry((edge.from, edge.to)).or_insert(edge.language);
        }
        let mut out = Vec::new();
        let mut unverifiable = 0u64;
        for edge in &self.edges {
            if edge.from == edge.to {
                continue;
            }
            if !locs.contains(&edge.to) {
                unverifiable += 1;
                continue;
            }
            let location = Location::record(edge.record, None).with_field("alternates");
            let source = self.sources.get(&edge.from).map_or("", String::as_str);
            match by_pair.get(&(edge.to, edge.from)) {
                None => out.push(
                    Diagnostic::new(
                        DiagnosticCode::HreflangReturnLinkMissing,
                        format!(
                            "the {} alternate does not link back to this page",
                            self.names[edge.language as usize]
                        ),
                        location,
                    )
                    .with_value(source)
                    .with_suggestion(
                        "Every language version must list all the others, including this one.",
                    ),
                ),
                Some(&back) => {
                    // How this page names itself versus how the target names it.
                    if let Some(&own) = by_pair.get(&(edge.from, edge.from)) {
                        if own != back {
                            out.push(
                                Diagnostic::new(
                                    DiagnosticCode::HreflangConflict,
                                    format!(
                                        "this page calls itself {} but its {} alternate calls it {}",
                                        self.names[own as usize],
                                        self.names[edge.language as usize],
                                        self.names[back as usize]
                                    ),
                                    location,
                                )
                                .with_value(source),
                            );
                        }
                    }
                }
            }
        }
        if unverifiable > 0 {
            out.push(Diagnostic::new(
                DiagnosticCode::HreflangTargetNotListed,
                format!(
                    "{unverifiable} alternate links point to URLs outside this run; their return links were not checked"
                ),
                Location::default(),
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use sitemap_model::{LanguageTag, SitemapUrl};

    use super::*;

    fn entry(loc: &str, alternates: &[(&str, &str)]) -> UrlEntry {
        alternates.iter().fold(
            UrlEntry::new(SitemapUrl::new(loc).unwrap()),
            |entry, (lang, href)| {
                entry.with_alternate(
                    LanguageTag::parse(lang).unwrap(),
                    SitemapUrl::new(href).unwrap(),
                )
            },
        )
    }

    fn check(entries: &[UrlEntry]) -> Vec<(&'static str, Option<u64>)> {
        let mut checker = ReciprocityChecker::default();
        for (i, e) in entries.iter().enumerate() {
            checker.record(e, i as u64 + 1, true);
        }
        checker
            .finish(None)
            .iter()
            .map(|d| (d.code.as_str(), d.location.record))
            .collect()
    }

    const EN: &str = "https://example.com/en/";
    const FR: &str = "https://example.com/fr/";
    const DE: &str = "https://example.com/de/";

    #[test]
    fn complete_clusters_pass() {
        let pair = [("en", EN), ("fr", FR)];
        assert!(check(&[entry(EN, &pair), entry(FR, &pair)]).is_empty());
    }

    #[test]
    fn missing_return_link_and_unlisted_targets() {
        let found = check(&[
            entry(EN, &[("en", EN), ("fr", FR), ("de", DE)]),
            entry(FR, &[("fr", FR)]),
        ]);
        assert_eq!(
            found,
            [
                ("HREFLANG_RETURN_LINK_MISSING", Some(1)),
                ("HREFLANG_TARGET_NOT_LISTED", None)
            ]
        );
    }

    #[test]
    fn conflicting_codes() {
        let found = check(&[
            entry(EN, &[("en", EN), ("fr", FR)]),
            entry(FR, &[("fr", FR), ("en-GB", EN)]),
        ]);
        assert_eq!(found, [("HREFLANG_CONFLICT", Some(1))]);
    }
}
