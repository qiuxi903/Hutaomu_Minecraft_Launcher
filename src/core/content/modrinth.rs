use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};

use crate::core::download::{DownloadProgress, Downloader, FileTask};
use crate::core::paths;

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Mod,
    Shader,
    ResourcePack,
    Modpack,
    World,
}

impl ContentKind {
    pub fn all() -> [ContentKind; 5] {
        [
            Self::Mod,
            Self::Shader,
            Self::ResourcePack,
            Self::Modpack,
            Self::World,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mod => "Mod",
            Self::Shader => "光影",
            Self::ResourcePack => "资源包",
            Self::Modpack => "整合包",
            Self::World => "世界",
        }
    }

    pub fn facet(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
            Self::Modpack => "modpack",
            Self::World => "world",
        }
    }

    /// 相对游戏目录的安装位置
    pub fn install_dir(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::Shader => "shaderpacks",
            Self::ResourcePack => "resourcepacks",
            Self::Modpack => "",
            Self::World => "saves",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
            Self::Modpack => "modpack",
            Self::World => "world",
        }
    }

    pub fn supports_loader_filter(self) -> bool {
        matches!(self, Self::Mod | Self::Modpack)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub hits: Vec<SearchHit>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub limit: u32,
    #[serde(default)]
    pub total_hits: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchHit {
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub follows: u64,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub date_modified: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub gallery: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub published: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub wiki_url: Option<String>,
    #[serde(default)]
    pub discord_url: Option<String>,
    #[serde(default)]
    pub gallery: Vec<GalleryImage>,
    #[serde(default)]
    pub license: License,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct License {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GalleryImage {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version_number: String,
    #[serde(default)]
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub date_published: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

impl Version {
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }

    pub fn loader_label(&self) -> String {
        if self.loaders.is_empty() {
            "通用".into()
        } else {
            self.loaders.join(" / ")
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub hashes: Hashes,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Hashes {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub dependency_type: String,
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

pub fn search(
    dl: &Downloader,
    kind: ContentKind,
    query: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
    offset: u32,
    limit: u32,
) -> Result<SearchResponse> {
    let mut facets: Vec<String> = vec![format!("[\"project_type:{}\"]", kind.facet())];
    if let Some(version) = mc_version {
        if !version.is_empty() {
            facets.push(format!("[\"versions:{version}\"]"));
        }
    }
    if let Some(loader) = loader {
        if !loader.is_empty() && kind.supports_loader_filter() {
            facets.push(format!("[\"categories:{loader}\"]"));
        }
    }
    let facets_json = format!("[{}]", facets.join(","));

    let mut url = format!(
        "{API}/search?query={}&limit={}&offset={}&index=relevance&facets={}",
        encode(query),
        limit.clamp(1, 50),
        offset,
        encode(&facets_json)
    );
    if query.trim().is_empty() {
        url = url.replace("index=relevance", "index=downloads");
    }

    dl.get_json_direct(&url)
        .with_context(|| "搜索失败，请检查网络连接".to_string())
}

pub fn project(dl: &Downloader, id: &str) -> Result<Project> {
    dl.get_json_direct(&format!("{API}/project/{id}"))
}

pub fn versions(
    dl: &Downloader,
    project_id: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<Version>> {
    let mut url = format!("{API}/project/{project_id}/version");
    let mut params: Vec<String> = Vec::new();
    if let Some(version) = mc_version {
        if !version.is_empty() {
            params.push(format!("game_versions={}", encode(&format!("[\"{version}\"]"))));
        }
    }
    if let Some(loader) = loader {
        if !loader.is_empty() {
            params.push(format!("loaders={}", encode(&format!("[\"{loader}\"]"))));
        }
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }
    dl.get_json_direct(&url)
}

/// 把内容文件下载到对应目录，返回落地路径
pub fn install(
    dl: &Downloader,
    kind: ContentKind,
    version: &Version,
    game_dir: &Path,
    progress: &Arc<DownloadProgress>,
) -> Result<PathBuf> {
    let file = version
        .primary_file()
        .ok_or_else(|| anyhow!("该版本没有可下载的文件"))?;
    let dir = if kind.install_dir().is_empty() {
        game_dir.to_path_buf()
    } else {
        game_dir.join(kind.install_dir())
    };
    paths::ensure_dir(&dir)?;
    let dest = dir.join(&file.filename);
    progress.set_message(format!("正在下载 {} ...", file.filename));
    progress.add_total(file.size);
    dl.download(
        &FileTask::new(file.url.clone(), dest.clone())
            .with_size(file.size)
            .with_optional_sha1(file.hashes.sha1.clone()),
        progress,
    )?;
    Ok(dest)
}

/// 安装整合包（.mrpack）
pub fn install_modpack(
    dl: &Downloader,
    version: &Version,
    game_dir: &Path,
    progress: &Arc<DownloadProgress>,
) -> Result<()> {
    let file = version
        .primary_file()
        .ok_or_else(|| anyhow!("该整合包没有可下载的文件"))?;
    let cache = paths::cache_dir().join("modpacks");
    paths::ensure_dir(&cache)?;
    let archive_path = cache.join(&file.filename);

    progress.set_message("正在下载整合包 ...");
    progress.add_total(file.size);
    dl.download(
        &FileTask::new(file.url.clone(), archive_path.clone())
            .with_size(file.size)
            .with_optional_sha1(file.hashes.sha1.clone()),
        progress,
    )?;

    progress.set_message("正在解压整合包 ...");
    let extract_dir = cache.join(format!("extract-{}", version.id));
    if extract_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&extract_dir);
    }
    paths::ensure_dir(&extract_dir)?;

    let archive_file = std::fs::File::open(&archive_path)?;
    let mut archive = zip::ZipArchive::new(archive_file)?;
    archive.extract(&extract_dir)?;

    let index_path = extract_dir.join("modrinth.index.json");
    let index_text = std::fs::read_to_string(&index_path)
        .context("整合包缺少 modrinth.index.json")?;
    let index: MrpackIndex = serde_json::from_str(&index_text)?;

    // 下载整合包内文件
    let mut tasks: Vec<FileTask> = Vec::new();
    let mut total = 0u64;
    for entry in &index.files {
        let dest = game_dir.join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let url = entry.downloads.first().cloned().unwrap_or_default();
        if url.is_empty() {
            continue;
        }
        total += entry.file_size;
        tasks.push(
            FileTask::new(url, dest)
                .with_size(entry.file_size)
                .with_optional_sha1(entry.hashes.sha1.clone()),
        );
    }
    progress.set_message(format!("正在下载整合包内容（{} 个文件）...", tasks.len()));
    progress.total.store(total, std::sync::atomic::Ordering::Relaxed);
    progress.done.store(0, std::sync::atomic::Ordering::Relaxed);
    if !tasks.is_empty() {
        dl.download_batch(tasks, 8, progress)?;
    }

    // 覆盖 overrides / client-overrides
    progress.set_message("正在写入整合包配置 ...");
    for name in ["overrides", "client-overrides"] {
        let src = extract_dir.join(name);
        if src.is_dir() {
            copy_dir(&src, game_dir)?;
        }
    }

    let _ = std::fs::remove_dir_all(&extract_dir);
    Ok(())
}

#[derive(Debug, Deserialize)]
struct MrpackIndex {
    #[serde(default)]
    files: Vec<MrpackFile>,
}

#[derive(Debug, Deserialize)]
struct MrpackFile {
    #[serde(default)]
    path: String,
    #[serde(default)]
    hashes: Hashes,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default, rename = "fileSize")]
    file_size: u64,
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let relative = path.strip_prefix(src).unwrap_or(path);
        let target = dest.join(relative);
        if entry.file_type().is_dir() {
            paths::ensure_dir(&target)?;
        } else {
            paths::ensure_parent(&target)?;
            std::fs::copy(path, &target)?;
        }
    }
    Ok(())
}

/// 递归收集必须的前置依赖（仅一层，避免无限递归）
pub fn required_dependencies(
    dl: &Downloader,
    version: &Version,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Vec<Version> {
    let mut result = Vec::new();
    for dep in &version.dependencies {
        if dep.dependency_type != "required" {
            continue;
        }
        let Some(project_id) = &dep.project_id else {
            continue;
        };
        let Ok(list) = versions(dl, project_id, mc_version, loader) else {
            continue;
        };
        if let Some(first) = list.into_iter().next() {
            result.push(first);
        }
    }
    result
}

pub fn format_downloads(count: u64) -> String {
    if count >= 10_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 10_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}
