//! Read-only health check of an output directory and its state.

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;
use sitemap_snapshot::{digest128, open_file as open_snapshot, to_hex};
use sitemap_storage::{DirectoryLock, FsStore, LOCK_FILE, STAGING_PREFIX};

use super::Engine;
use crate::manifest::Manifest;

/// Outcome of one [`Check`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum CheckStatus {
    /// Nothing to do.
    Ok,
    /// The next run copes with it, but something is off or will be slower.
    Warning,
    /// The published files or the state are not what the engine wrote, or a
    /// run cannot start.
    Error,
    /// Not applicable to this configuration.
    Skipped,
}

impl CheckStatus {
    /// Name of the status, as written in JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Warning => "warning",
            CheckStatus::Error => "error",
            CheckStatus::Skipped => "skipped",
        }
    }
}

/// One check of [`Engine::doctor`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Check {
    /// Stable name: `output-directory`, `lock`, `staging`, `manifest`,
    /// `files`, `snapshot`.
    pub name: &'static str,
    /// Outcome.
    pub status: CheckStatus,
    /// What was found.
    pub message: String,
    /// What to do about it, when something can be done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl Check {
    fn new(name: &'static str, status: CheckStatus, message: impl Into<String>) -> Self {
        Self {
            name,
            status,
            message: message.into(),
            suggestion: None,
        }
    }

    fn suggest(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// Result of [`Engine::doctor`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DoctorReport {
    /// Checks, in a fixed order.
    pub checks: Vec<Check>,
    /// The state token a run would start from (see [`Engine::state_token`]),
    /// when it could be computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_token: Option<String>,
}

impl DoctorReport {
    /// At least one check failed.
    pub fn has_errors(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Error)
    }

    /// At least one check reported a warning.
    pub fn has_warnings(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Warning)
    }
}

impl Engine {
    /// Checks the output directory `output_dir` and the state directory
    /// against this configuration, without writing anything: directory
    /// access, locks held by a running process, staging left by an
    /// interrupted run, the published manifest against the files in place
    /// (sizes and digests, so every published file is read once), and
    /// whether the snapshot describes the current publication and settings.
    pub fn doctor(&self, output_dir: &Path) -> DoctorReport {
        let mut checks = vec![
            check_output_directory(output_dir),
            self.check_locks(output_dir),
            check_staging(output_dir),
        ];
        let manifest_bytes = self.check_manifest(output_dir, &mut checks);
        checks.push(self.check_snapshot(manifest_bytes.as_deref()));
        let state_token = self.state_token(&FsStore::new(output_dir)).ok();
        DoctorReport {
            checks,
            state_token,
        }
    }

    fn check_locks(&self, output_dir: &Path) -> Check {
        let mut held = Vec::new();
        let mut targets = vec![(output_dir.to_path_buf(), LOCK_FILE.to_owned())];
        if let Some(settings) = &self.config.snapshot {
            targets.push((
                settings.directory.clone(),
                format!("{}.lock", self.config.layout.stem),
            ));
        }
        for (dir, name) in targets {
            match DirectoryLock::is_held(&dir, &name) {
                Ok(false) => {}
                Ok(true) => held.push(dir.join(name).display().to_string()),
                Err(error) => {
                    return Check::new(
                        "lock",
                        CheckStatus::Error,
                        format!("{}: {error}", dir.join(name).display()),
                    );
                }
            }
        }
        if held.is_empty() {
            Check::new("lock", CheckStatus::Ok, "no run in progress")
        } else {
            Check::new(
                "lock",
                CheckStatus::Warning,
                format!("a run is in progress: {} is held", held.join(", ")),
            )
            .suggest("A run started now would stop with a busy error; retry when it has finished.")
        }
    }

    /// Checks the manifest and the files it lists; returns the manifest's
    /// bytes when it is published.
    fn check_manifest(&self, output_dir: &Path, checks: &mut Vec<Check>) -> Option<Vec<u8>> {
        if !self.config.manifest {
            checks.push(Check::new(
                "manifest",
                CheckStatus::Skipped,
                "no manifest is written (output.manifest is false)",
            ));
            checks.push(Check::new(
                "files",
                CheckStatus::Skipped,
                "without a manifest, the published files cannot be checked",
            ));
            return None;
        }
        let name = match self.manifest_name() {
            Ok(name) => name,
            Err(error) => {
                checks.push(Check::new(
                    "manifest",
                    CheckStatus::Error,
                    error.to_string(),
                ));
                return None;
            }
        };
        let path = output_dir.join(name.as_str());
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                checks.push(Check::new(
                    "manifest",
                    CheckStatus::Ok,
                    format!("nothing published yet ({name} is absent)"),
                ));
                checks.push(Check::new(
                    "files",
                    CheckStatus::Skipped,
                    "nothing published yet",
                ));
                return None;
            }
            Err(error) => {
                checks.push(Check::new(
                    "manifest",
                    CheckStatus::Error,
                    format!("{}: {error}", path.display()),
                ));
                return None;
            }
        };
        let Some(manifest) = Manifest::parse(&bytes) else {
            checks.push(
                Check::new(
                    "manifest",
                    CheckStatus::Error,
                    format!("{name} is not a valid manifest"),
                )
                .suggest("Run a full generation (--execution-mode full) to publish a new one."),
            );
            return Some(bytes);
        };
        checks.push(Check::new(
            "manifest",
            CheckStatus::Ok,
            format!(
                "{name} lists {} file(s) and {} URL(s)",
                manifest.artifacts.len(),
                manifest.total_urls
            ),
        ));
        checks.push(check_files(output_dir, &manifest));
        Some(bytes)
    }

    fn check_snapshot(&self, manifest: Option<&[u8]>) -> Check {
        let Some(path) = self.config.snapshot_path() else {
            return Check::new(
                "snapshot",
                CheckStatus::Skipped,
                "no state directory is configured",
            );
        };
        let reader = match open_snapshot(&path) {
            Ok(Some(reader)) => reader,
            Ok(None) if manifest.is_some() => {
                return Check::new(
                    "snapshot",
                    CheckStatus::Warning,
                    format!("{} is absent although files are published", path.display()),
                )
                .suggest("The next generate writes it; apply is refused until then.");
            }
            Ok(None) => {
                return Check::new("snapshot", CheckStatus::Ok, "no snapshot yet");
            }
            Err(error) => {
                return Check::new(
                    "snapshot",
                    CheckStatus::Warning,
                    format!("{} cannot be read: {error}", path.display()),
                )
                .suggest("The next generate replaces it; apply is refused until then.");
            }
        };
        if *reader.config() != digest128(self.config.output_settings().as_bytes()) {
            return Check::new(
                "snapshot",
                CheckStatus::Warning,
                "the settings that shape the output changed since the snapshot was written",
            )
            .suggest("Run a full generation; apply is refused until then.");
        }
        match (reader.publication(), manifest) {
            (Some(described), Some(bytes)) if *described != digest128(bytes) => Check::new(
                "snapshot",
                CheckStatus::Warning,
                "the snapshot does not describe the files in place (SNAPSHOT_STALE: a run was interrupted after publishing)",
            )
            .suggest("The next generate replaces it; apply is refused until then."),
            _ => Check::new(
                "snapshot",
                CheckStatus::Ok,
                format!("{} matches the settings and the publication", path.display()),
            ),
        }
    }
}

fn check_output_directory(dir: &Path) -> Check {
    match fs::metadata(dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Check::new(
            "output-directory",
            CheckStatus::Ok,
            format!(
                "{} does not exist yet; the first run creates it",
                dir.display()
            ),
        ),
        Err(error) => Check::new(
            "output-directory",
            CheckStatus::Error,
            format!("{}: {error}", dir.display()),
        ),
        Ok(meta) if !meta.is_dir() => Check::new(
            "output-directory",
            CheckStatus::Error,
            format!("{} is not a directory", dir.display()),
        ),
        Ok(meta) if meta.permissions().readonly() => Check::new(
            "output-directory",
            CheckStatus::Error,
            format!("{} is read-only", dir.display()),
        ),
        Ok(_) => Check::new(
            "output-directory",
            CheckStatus::Ok,
            format!("{} exists", dir.display()),
        ),
    }
}

fn check_staging(dir: &Path) -> Check {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Check::new("staging", CheckStatus::Ok, "no staging directory");
        }
        Err(error) => {
            return Check::new(
                "staging",
                CheckStatus::Error,
                format!("{}: {error}", dir.display()),
            );
        }
    };
    let left = entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(STAGING_PREFIX))
        })
        .count();
    if left == 0 {
        Check::new("staging", CheckStatus::Ok, "no staging directory")
    } else {
        Check::new(
            "staging",
            CheckStatus::Warning,
            format!("{left} staging director(y/ies) left by an interrupted or running process"),
        )
        .suggest(
            "The next run removes them once it holds the lock; nothing was published from them.",
        )
    }
}

fn check_files(dir: &Path, manifest: &Manifest) -> Check {
    let mut problems = Vec::new();
    let mut without_digest = 0usize;
    for artifact in &manifest.artifacts {
        let path = dir.join(&artifact.name);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                problems.push(format!("{} is missing", artifact.name));
                continue;
            }
            Err(error) => {
                problems.push(format!("{}: {error}", artifact.name));
                continue;
            }
        };
        if bytes.len() as u64 != artifact.bytes {
            problems.push(format!(
                "{} has {} bytes, the manifest says {}",
                artifact.name,
                bytes.len(),
                artifact.bytes
            ));
            continue;
        }
        match &artifact.digest {
            Some(expected) if *expected != to_hex(&digest128(&bytes)) => {
                problems.push(format!(
                    "{} differs from the published content",
                    artifact.name
                ));
            }
            Some(_) => {}
            None => without_digest += 1,
        }
    }
    if !problems.is_empty() {
        let shown: Vec<_> = problems.iter().take(10).cloned().collect();
        let more = problems.len().saturating_sub(shown.len());
        let mut message = format!(
            "{} file(s) do not match the manifest: {}",
            problems.len(),
            shown.join("; ")
        );
        if more > 0 {
            message.push_str(&format!("; and {more} more"));
        }
        return Check::new("files", CheckStatus::Error, message).suggest(
            "The files were changed outside the engine. Run a full generation (--execution-mode full) to publish them again.",
        );
    }
    if without_digest > 0 {
        return Check::new(
            "files",
            CheckStatus::Warning,
            format!(
                "{without_digest} file(s) checked by size only: the manifest predates file digests"
            ),
        )
        .suggest("The next generate writes a manifest with digests.");
    }
    Check::new(
        "files",
        CheckStatus::Ok,
        format!(
            "{} file(s) match the manifest (size and digest)",
            manifest.artifacts.len()
        ),
    )
}
