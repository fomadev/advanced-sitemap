//! Execution planning: which mode a run should use, what each mode would
//! cost, and why (lot B of the production readiness plan).
//!
//! The modes are `full` (republish every file), `skip-unchanged` (write
//! only files whose bytes changed; same result) and `delta` (`apply`: read
//! back and rewrite only the partitions a change feed touches). A mode is
//! never chosen unless its correctness preconditions hold; the cost model
//! only orders the modes that are eligible.
//!
//! Every cost is an estimate from [`CostModel`], calibrated on the
//! reference machine (see `docs/performance`). On that machine the model
//! reproduces the measured runs within about 30 %; on other hardware the
//! absolute numbers can be far off, but the comparison between modes
//! usually still holds because all modes pay the same disk.

use serde::Serialize;

use super::Quantity;

/// Unit costs used to estimate a run. Times are for one CPU core of the
/// reference machine; adjust them to calibrate on other hardware.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CostModel {
    /// Reading, validating and encoding one record, in microseconds.
    pub pipeline_us_per_url: f64,
    /// Writing one byte of sitemap output, in nanoseconds.
    pub write_ns_per_byte: f64,
    /// Syncing and renaming one published file, in milliseconds.
    pub publish_ms_per_file: f64,
    /// Building, syncing and comparing the URL snapshot, per URL of the
    /// publication, in microseconds.
    pub snapshot_us_per_url: f64,
    /// Reading back, checking and rewriting one URL of a touched partition
    /// (delta mode), in microseconds.
    pub delta_us_per_touched_url: f64,
    /// Snapshot size per URL, in bytes.
    pub snapshot_bytes_per_url: f64,
    /// Memory per deduplicated URL, in bytes.
    pub dedupe_bytes_per_url: f64,
}

impl CostModel {
    /// Calibrated on the reference laptop of `docs/performance` (Intel
    /// Core i5-6300U, slow disk, Windows 11), November 2026:
    ///
    /// * `pipeline`: `generate --skip-unchanged` rerun without changes,
    ///   1 M URLs in 3.3 to 4.7 s;
    /// * `write` and `publish`: `generate` of 1 M URLs (168.6 MB, 20 files)
    ///   in 16.3 s, and of the same URLs in 1,000 files in 37.5 to 41.3 s;
    /// * `snapshot`: `--state-dir` adds 3.0 to 3.4 s at 1 M URLs;
    /// * `delta`: `apply` touching all 24 partitions of 1 M URLs in 21.1 s.
    ///
    /// Checked against runs not used for calibration: `generate` of 5 M
    /// URLs (estimated 81 s, measured 80.4 s) and `apply` of 10,000
    /// operations over 1,000 partitions (estimated 46 s, measured 49.6 to
    /// 54.2 s).
    pub const REFERENCE: CostModel = CostModel {
        pipeline_us_per_url: 4.0,
        write_ns_per_byte: 70.0,
        publish_ms_per_file: 25.0,
        snapshot_us_per_url: 3.3,
        delta_us_per_touched_url: 17.2,
        snapshot_bytes_per_url: 45.0,
        dedupe_bytes_per_url: 32.0,
    };

    /// Name of the calibration, reported with every plan.
    pub const REFERENCE_NAME: &'static str = "reference laptop, 2026-11 (docs/performance)";
}

impl Default for CostModel {
    fn default() -> Self {
        Self::REFERENCE
    }
}

/// One mode considered by the planner.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// `full`, `skip-unchanged`, `delta` or `regenerate`.
    pub mode: &'static str,
    /// The mode's correctness preconditions hold.
    pub eligible: bool,
    /// Preconditions checked, or why the mode is not eligible.
    pub reasons: Vec<String>,
    /// Estimated duration when the least work is needed, in milliseconds.
    pub milliseconds_min: Quantity,
    /// Estimated duration when the most work is needed, in milliseconds.
    pub milliseconds_max: Quantity,
    /// Bytes read (input, published files, snapshot).
    pub bytes_read: Quantity,
    /// Bytes written at most (sitemaps, snapshot).
    pub bytes_written: Quantity,
    /// Peak memory.
    pub memory_bytes: Quantity,
    /// Partitions read or written.
    pub partitions_affected: Quantity,
}

impl Candidate {
    fn ineligible(mode: &'static str, reason: impl Into<String>) -> Self {
        Self {
            mode,
            eligible: false,
            reasons: vec![reason.into()],
            milliseconds_min: Quantity::Unknown,
            milliseconds_max: Quantity::Unknown,
            bytes_read: Quantity::Unknown,
            bytes_written: Quantity::Unknown,
            memory_bytes: Quantity::Unknown,
            partitions_affected: Quantity::Unknown,
        }
    }
}

/// The planner's decision.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlan {
    /// Mode asked for: `full`, `skip-unchanged`, `auto`, or `delta` for a
    /// change feed.
    pub requested: &'static str,
    /// Mode the run would use, or `none` when no eligible mode can run.
    pub chosen: &'static str,
    /// Why.
    pub reason: String,
    /// Every mode considered, eligible or not.
    pub candidates: Vec<Candidate>,
    /// Token of the published state the plan was made for. Pass it to the
    /// run (`--expect-state`) so that it refuses to start if the state
    /// changed since.
    pub state_token: String,
    /// Calibration of the cost model.
    pub cost_model: &'static str,
}

/// What the planner knows about the run.
pub(crate) struct Facts {
    /// Valid URLs expected.
    pub urls: Quantity,
    /// Average encoded `<url>` size.
    pub url_bytes: u64,
    /// Input size.
    pub input_bytes: Quantity,
    /// Sitemap files expected.
    pub files: Quantity,
    /// A previous publication exists (its manifest or main file).
    pub published: bool,
    /// A snapshot is kept.
    pub snapshot: bool,
    /// Sort budget of the snapshot.
    pub snapshot_budget: u64,
    /// Deduplication on.
    pub dedupe: bool,
}

fn ms(model_us: f64) -> u64 {
    (model_us / 1000.0).ceil() as u64
}

/// Plans a `generate` or `validate` run.
pub(crate) fn plan_generate(
    facts: &Facts,
    requested: &'static str,
    model: &CostModel,
    state_token: String,
) -> ExecutionPlan {
    let derive = |q: Quantity, v: f64| -> Quantity {
        match q {
            Quantity::Unknown => Quantity::Unknown,
            _ => Quantity::Estimated(v.ceil() as u64),
        }
    };
    let n = facts.urls.value().unwrap_or(0) as f64;
    let files = facts.files.value().unwrap_or(1) as f64;
    let out_bytes = n * facts.url_bytes as f64;
    let snapshot_us = if facts.snapshot {
        n * model.snapshot_us_per_url
    } else {
        0.0
    };
    let snapshot_bytes = if facts.snapshot {
        n * model.snapshot_bytes_per_url
    } else {
        0.0
    };
    let memory = (if facts.dedupe {
        n * model.dedupe_bytes_per_url
    } else {
        0.0
    }) + if facts.snapshot {
        facts.snapshot_budget as f64
    } else {
        0.0
    };
    let full_us = n * model.pipeline_us_per_url
        + out_bytes * model.write_ns_per_byte / 1000.0
        + files * model.publish_ms_per_file * 1000.0
        + snapshot_us;
    let input_bytes = facts.input_bytes.value().unwrap_or(0) as f64;

    let full = Candidate {
        mode: "full",
        eligible: true,
        reasons: vec!["always correct: every file is written".to_owned()],
        milliseconds_min: derive(facts.urls, ms(full_us) as f64),
        milliseconds_max: derive(facts.urls, ms(full_us) as f64),
        bytes_read: derive(facts.input_bytes, input_bytes),
        bytes_written: derive(facts.urls, out_bytes + snapshot_bytes),
        memory_bytes: derive(facts.urls, memory),
        partitions_affected: facts.files,
    };
    let skip = if facts.published {
        // Best case: nothing changed, every file is compared and none is
        // written. Worst case: every file changed (comparison stops at the
        // first different byte, so it costs about the same as `full`).
        let min_us = n * model.pipeline_us_per_url + snapshot_us;
        Candidate {
            mode: "skip-unchanged",
            eligible: true,
            reasons: vec![
                "a previous publication exists".to_owned(),
                "same bytes as `full`: each new file is compared with the published one".to_owned(),
            ],
            milliseconds_min: derive(facts.urls, ms(min_us) as f64),
            milliseconds_max: derive(facts.urls, ms(full_us) as f64),
            bytes_read: derive(facts.urls, input_bytes + out_bytes),
            bytes_written: derive(facts.urls, out_bytes + snapshot_bytes),
            memory_bytes: derive(facts.urls, memory),
            partitions_affected: facts.files,
        }
    } else {
        Candidate::ineligible(
            "skip-unchanged",
            "nothing is published yet: every file must be written (it would behave as `full`)",
        )
    };
    let delta = Candidate::ineligible(
        "delta",
        "needs a change feed: run `explain --changes FEED` to plan an `apply`",
    );

    let (chosen, reason) = match requested {
        "full" => ("full", "requested".to_owned()),
        "skip-unchanged" if facts.published => ("skip-unchanged", "requested".to_owned()),
        "skip-unchanged" => (
            "skip-unchanged",
            "requested; nothing is published yet, so every file is written".to_owned(),
        ),
        _ if facts.published => (
            "skip-unchanged",
            "auto: a previous publication exists; same result as `full`, without rewriting unchanged files"
                .to_owned(),
        ),
        _ => ("full", "auto: first publication".to_owned()),
    };
    ExecutionPlan {
        requested,
        chosen,
        reason,
        candidates: vec![full, skip, delta],
        state_token,
        cost_model: CostModel::REFERENCE_NAME,
    }
}

/// What the planner knows about an `apply`.
pub(crate) struct DeltaFacts {
    /// From [`crate::engine`]'s read-only delta check.
    pub plan: crate::engine::DeltaPlan,
    /// A source is configured, so a full regeneration is possible.
    pub source_available: bool,
    /// Average encoded `<url>` size of the published files.
    pub url_bytes: u64,
    /// Sort budget of the snapshot.
    pub snapshot_budget: u64,
}

/// Plans an `apply` of a change feed, against regenerating from the source.
pub(crate) fn plan_apply(
    facts: &DeltaFacts,
    model: &CostModel,
    state_token: String,
) -> ExecutionPlan {
    let p = &facts.plan;
    let est = |v: f64| Quantity::Estimated(v.ceil() as u64);
    let total = p.total_urls as f64;
    let snapshot_bytes = total * model.snapshot_bytes_per_url;

    let delta = match &p.refusal {
        Some(reason) => Candidate::ineligible("delta", reason.clone()),
        None => {
            let us = total * model.snapshot_us_per_url
                + p.touched_urls as f64 * model.delta_us_per_touched_url
                + p.touched_partitions as f64 * model.publish_ms_per_file * 1000.0;
            Candidate {
                mode: "delta",
                eligible: true,
                reasons: vec![
                    "hash strategy, snapshot and manifest describe the publication in place"
                        .to_owned(),
                    format!(
                        "{} of {} partitions touched ({} of {} URLs); they are checked against the snapshot when read",
                        p.touched_partitions, p.partitions, p.touched_urls, p.total_urls
                    ),
                ],
                milliseconds_min: est(ms(us) as f64),
                milliseconds_max: est(ms(us) as f64),
                bytes_read: est(p.touched_bytes as f64 + snapshot_bytes),
                bytes_written: est(p.touched_bytes as f64 + snapshot_bytes),
                // The largest touched partition held as entries (about 4
                // times its XML size), the feed, the snapshot sort budget.
                memory_bytes: est(p.largest_touched_bytes as f64 * 4.0
                    + p.operations as f64 * 600.0
                    + facts.snapshot_budget as f64),
                partitions_affected: Quantity::Measured(p.touched_partitions),
            }
        }
    };
    let regenerate = if facts.source_available {
        let pipeline = total * model.pipeline_us_per_url + total * model.snapshot_us_per_url;
        let changed_bytes = p.touched_bytes as f64;
        let us = pipeline
            + changed_bytes * model.write_ns_per_byte / 1000.0
            + p.touched_partitions as f64 * model.publish_ms_per_file * 1000.0;
        Candidate {
            mode: "regenerate",
            eligible: true,
            reasons: vec![
                "a source is configured: `generate --execution-mode auto` rewrites only the files that change".to_owned(),
                "the source must already contain the feed's changes; the planner cannot check it".to_owned(),
            ],
            milliseconds_min: est(ms(us) as f64),
            milliseconds_max: est(ms(us) as f64),
            bytes_read: est(total * facts.url_bytes as f64 + snapshot_bytes),
            bytes_written: est(changed_bytes + snapshot_bytes),
            memory_bytes: est(total * model.dedupe_bytes_per_url + facts.snapshot_budget as f64),
            partitions_affected: Quantity::Measured(p.partitions),
        }
    } else {
        Candidate::ineligible("regenerate", "no source configured")
    };

    let value = |c: &Candidate| c.milliseconds_max.value().unwrap_or(u64::MAX);
    let (chosen, reason) = match (delta.eligible, regenerate.eligible) {
        (true, true) if value(&regenerate) < value(&delta) => (
            "regenerate",
            "estimated faster than applying the feed: run `generate --execution-mode auto` from an up-to-date source (`apply` would still be correct)".to_owned(),
        ),
        (true, _) => ("delta", "`apply` rewrites only the touched partitions".to_owned()),
        (false, true) => (
            "regenerate",
            "`apply` cannot run (see the delta candidate); regenerate from an up-to-date source".to_owned(),
        ),
        (false, false) => (
            "none",
            "`apply` cannot run and no source is configured; nothing can be done safely".to_owned(),
        ),
    };
    ExecutionPlan {
        requested: "delta",
        chosen,
        reason,
        candidates: vec![delta, regenerate],
        state_token,
        cost_model: CostModel::REFERENCE_NAME,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn facts(published: bool) -> Facts {
        Facts {
            urls: Quantity::Measured(1_000_000),
            url_bytes: 169,
            input_bytes: Quantity::Measured(127_000_000),
            files: Quantity::Measured(20),
            published,
            snapshot: false,
            snapshot_budget: 64 << 20,
            dedupe: true,
        }
    }

    /// The reference calibration reproduces the measured `generate` runs.
    #[test]
    fn reference_model_matches_the_measured_runs() {
        let plan = plan_generate(&facts(false), "auto", &CostModel::REFERENCE, String::new());
        let full = &plan.candidates[0];
        let ms = full.milliseconds_max.value().unwrap();
        assert!(
            (14_000..=19_000).contains(&ms),
            "1 M: {ms} ms, measured 16.3 s"
        );
        let mut five = facts(false);
        five.urls = Quantity::Measured(5_000_000);
        five.files = Quantity::Measured(100);
        let plan = plan_generate(&five, "full", &CostModel::REFERENCE, String::new());
        let ms = plan.candidates[0].milliseconds_max.value().unwrap();
        assert!(
            (70_000..=92_000).contains(&ms),
            "5 M: {ms} ms, measured 80.4 s"
        );
    }

    #[test]
    fn auto_uses_skip_unchanged_only_after_a_first_publication() {
        let first = plan_generate(&facts(false), "auto", &CostModel::REFERENCE, String::new());
        assert_eq!(first.chosen, "full");
        assert!(!first.candidates[1].eligible);
        let next = plan_generate(&facts(true), "auto", &CostModel::REFERENCE, String::new());
        assert_eq!(next.chosen, "skip-unchanged");
        assert!(
            next.candidates
                .iter()
                .all(|c| c.mode != "delta" || !c.eligible)
        );
        assert_eq!(
            plan_generate(&facts(true), "full", &CostModel::REFERENCE, String::new()).chosen,
            "full"
        );
    }

    fn delta_facts(touched_partitions: u64, source: bool) -> DeltaFacts {
        DeltaFacts {
            plan: crate::engine::DeltaPlan {
                operations: 100,
                invalid_operations: 0,
                refusal: None,
                total_urls: 1_000_000,
                partitions: 1_000,
                touched_partitions,
                touched_urls: touched_partitions * 1_000,
                touched_bytes: touched_partitions * 169_000,
                largest_touched_bytes: 169_000,
            },
            source_available: source,
            url_bytes: 169,
            snapshot_budget: 64 << 20,
        }
    }

    #[test]
    fn apply_is_preferred_when_few_partitions_are_touched() {
        let plan = plan_apply(&delta_facts(10, true), &CostModel::REFERENCE, String::new());
        assert_eq!(plan.chosen, "delta");
        // Touching everything is cheaper to regenerate.
        let plan = plan_apply(
            &delta_facts(1_000, true),
            &CostModel::REFERENCE,
            String::new(),
        );
        assert_eq!(plan.chosen, "regenerate", "{plan:?}");
        // Without a source, apply is the only possibility.
        let plan = plan_apply(
            &delta_facts(1_000, false),
            &CostModel::REFERENCE,
            String::new(),
        );
        assert_eq!(plan.chosen, "delta");
    }

    #[test]
    fn a_refused_delta_is_never_chosen() {
        let mut facts = delta_facts(1, false);
        facts.plan.refusal = Some("no snapshot yet".to_owned());
        let plan = plan_apply(&facts, &CostModel::REFERENCE, String::new());
        assert_eq!(plan.chosen, "none");
        assert!(!plan.candidates[0].eligible);
        facts.source_available = true;
        let plan = plan_apply(&facts, &CostModel::REFERENCE, String::new());
        assert_eq!(plan.chosen, "regenerate");
    }
}
