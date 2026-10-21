use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::download::{DownloadSource, Downloader};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    #[serde(default)]
    pub time: String,
    #[serde(rename = "releaseTime", default)]
    pub release_time: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(rename = "complianceLevel", default)]
    pub compliance_level: Option<u32>,
}

impl VersionEntry {
    pub fn is_release(&self) -> bool {
        self.version_type == "release"
    }

    pub fn is_snapshot(&self) -> bool {
        self.version_type == "snapshot"
    }

    pub fn is_old(&self) -> bool {
        self.version_type == "old_beta" || self.version_type == "old_alpha"
    }

    pub fn type_label(&self) -> &'static str {
        match self.version_type.as_str() {
            "release" => "正式版",
            "snapshot" => "快照",
            "old_beta" => "远古 Beta",
            "old_alpha" => "远古 Alpha",
            _ => "其他",
        }
    }
}

pub fn fetch_manifest(source: DownloadSource) -> Result<VersionManifest> {
    let dl = Downloader::new(source, 30)?;
    let url = source.version_manifest();
    let manifest: VersionManifest = dl
        .get_json(&url)
        .with_context(|| format!("获取版本清单失败: {url}"))?;
    Ok(manifest)
}

pub fn find<'a>(manifest: &'a VersionManifest, id: &str) -> Option<&'a VersionEntry> {
    manifest.versions.iter().find(|v| v.id == id)
}
