//! `advanced-sitemap diff`: compares two sets of published sitemap files.
//!
//! Each side is read with the bounded reader ([`sitemap_reader`]), every
//! valid `<url>` entry is re-encoded in the engine's canonical form and
//! digested, and the two sides are compared through sorted snapshots (the
//! same external sort and streaming diff as the change report). Formatting
//! differences (whitespace, attribute quotes, element order) are therefore
//! ignored; any change of a value is detected. Which file a URL is in is
//! ignored too.

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sitemap_reader::{InspectOptions, Item};
use sitemap_snapshot::{Record, SnapshotBuilder, SnapshotReader, diff, digest128};

use crate::inspect::{FileInspection, inspect_reader};
use crate::report::ChangesReport;

/// One side of a comparison.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSide {
    /// Files read, in order.
    pub files: Vec<FileInspection>,
    /// Valid `<url>` entries compared.
    pub urls: u64,
}

impl DiffSide {
    /// `true` when every file was read completely without error diagnostics.
    pub fn is_valid(&self) -> bool {
        self.files.iter().all(FileInspection::is_valid)
    }
}

/// Result of [`diff_files`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    /// The old side.
    pub old: DiffSide,
    /// The new side.
    pub new: DiffSide,
    /// Differences (`moved` is always 0 and `partitionsTouched` empty: file
    /// placement is not compared).
    pub changes: ChangesReport,
}

/// Lists the sitemap files of a path: the file itself, or every `.xml` and
/// `.xml.gz` file of a directory (not recursive), sorted by name.
pub fn sitemap_files(path: &Path) -> io::Result<Vec<PathBuf>> {
    if !path.is_dir() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(path)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            p.is_file() && (name.ends_with(".xml") || name.ends_with(".xml.gz"))
        })
        .collect();
    files.sort();
    Ok(files)
}

fn read_side(
    files: &[PathBuf],
    options: &InspectOptions,
    temp_dir: &Path,
    memory_budget: usize,
    out: &Path,
) -> io::Result<DiffSide> {
    let mut builder = SnapshotBuilder::new(temp_dir, memory_budget);
    let mut inspections = Vec::with_capacity(files.len());
    let mut block = Vec::new();
    let mut failure: Option<io::Error> = None;
    for path in files {
        let file = std::fs::File::open(path)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
        let inspection = inspect_reader(path.display().to_string(), file, options, |item| {
            if failure.is_some() {
                return;
            }
            if let Item::Url(entry) = item {
                block.clear();
                let pushed = sitemap_xml::encode_url(&entry, &mut block)
                    .map_err(|e| io::Error::other(e.to_string()))
                    .and_then(|()| {
                        builder.push(Record {
                            loc: entry.loc.as_str().to_owned(),
                            content: digest128(&block),
                            partition: 0,
                        })
                    });
                if let Err(error) = pushed {
                    failure = Some(error);
                }
            }
        })?;
        inspections.push(inspection);
    }
    if let Some(error) = failure {
        return Err(error);
    }
    let urls = builder.len();
    let mut writer = io::BufWriter::new(std::fs::File::create(out)?);
    builder.finish(&[0; 16], &mut writer)?;
    io::Write::flush(&mut writer)?;
    Ok(DiffSide {
        files: inspections,
        urls,
    })
}

/// Compares the sitemap files of `old` with those of `new`.
///
/// Snapshots are built in memory up to `memory_budget` bytes each and
/// spilled to `temp_dir` beyond. Deduplication follows `options.dedupe`.
pub fn diff_files(
    old: &[PathBuf],
    new: &[PathBuf],
    options: &InspectOptions,
    temp_dir: &Path,
    memory_budget: usize,
    max_listed: usize,
) -> io::Result<DiffReport> {
    std::fs::create_dir_all(temp_dir)?;
    let id = std::process::id();
    let temp = TempFiles(vec![
        temp_dir.join(format!(".diff-old-{id}.snapshot")),
        temp_dir.join(format!(".diff-new-{id}.snapshot")),
    ]);
    let old_side = read_side(old, options, temp_dir, memory_budget, &temp.0[0])?;
    let new_side = read_side(new, options, temp_dir, memory_budget, &temp.0[1])?;
    let open = |path: &Path| -> io::Result<_> {
        SnapshotReader::new(io::BufReader::with_capacity(
            256 * 1024,
            std::fs::File::open(path)?,
        ))
    };
    let set = diff(open(&temp.0[0])?, open(&temp.0[1])?, max_listed)?;
    let mut changes = ChangesReport::new(set, true, false);
    changes.partitions_touched.clear();
    Ok(DiffReport {
        old: old_side,
        new: new_side,
        changes,
    })
}

/// Removes the temporary snapshots, whatever happens.
struct TempFiles(Vec<PathBuf>);

impl Drop for TempFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}
