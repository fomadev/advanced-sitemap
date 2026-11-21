use serde::{Deserialize, Serialize};

use crate::report::{ArtifactKind, ArtifactReport};

/// The manifest published next to the sitemaps (`<stem>.manifest.json`).
///
/// It lists the artifacts of the last publication. It contains no timestamp,
/// so identical inputs produce identical manifests. The next run reads it to
/// report artifacts that are no longer produced (`STALE_ARTIFACT`); stale files
/// are never deleted automatically.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Manifest format version.
    pub schema_version: String,
    /// Producer, e.g. `advanced-sitemap 0.1.0`.
    pub generator: String,
    /// Total URLs across sitemap files.
    pub total_urls: u64,
    /// Artifacts, in publication order (the manifest itself excluded).
    pub artifacts: Vec<ManifestArtifact>,
}

/// One artifact in the manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestArtifact {
    /// File name.
    pub name: String,
    /// `sitemap` or `index`.
    pub kind: String,
    /// Number of entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries: Option<u64>,
    /// Size in bytes.
    pub bytes: u64,
}

impl Manifest {
    pub(crate) fn from_artifacts(artifacts: &[ArtifactReport], total_urls: u64) -> Self {
        Self {
            schema_version: "1.0".to_owned(),
            generator: format!("advanced-sitemap {}", crate::ENGINE_VERSION),
            total_urls,
            artifacts: artifacts
                .iter()
                .filter(|a| a.kind != ArtifactKind::Manifest)
                .map(|a| ManifestArtifact {
                    name: a.name.clone(),
                    kind: match a.kind {
                        ArtifactKind::Sitemap => "sitemap",
                        ArtifactKind::Index => "index",
                        ArtifactKind::Manifest => "manifest",
                    }
                    .to_owned(),
                    entries: a.entries,
                    bytes: a.bytes,
                })
                .collect(),
        }
    }

    /// Pretty JSON with a trailing newline.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// Parses a manifest; returns `None` if the bytes are not a manifest.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}
