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

/// Modrinth 分类的中文名
///
/// 参考 PCL2 / HMCL / SJMCL 的做法，把平台自带的英文分类显示成中文，
/// 避免界面上出现 technology、optimization 这类英文标签。
pub fn category_label(category: &str) -> String {
    let zh = match category {
        // 加载器
        "fabric" => "Fabric",
        "forge" => "Forge",
        "quilt" => "Quilt",
        "neoforge" => "NeoForge",
        "liteloader" => "LiteLoader",
        "rift" => "Rift",
        "modloader" => "ModLoader",
        // Mod 分类
        "technology" => "科技",
        "gameplay" => "玩法",
        "library" => "功能库",
        "decoration" => "装饰",
        "optimization" => "优化",
        "stylized" => "卡通",
        "realistic" => "写实",
        "worldgen" => "地形",
        "utility" => "工具",
        "game-mechanics" => "机制",
        "adventure" => "冒险",
        "combat" => "战斗",
        "magic" => "魔法",
        "misc" => "综合",
        "social" => "社交",
        "storage" => "存储",
        "visual" => "视觉",
        "audio" => "音效",
        "tweaks" => "调整",
        "data-gen" => "数据生成",
        "lightweight" => "轻量",
        "multiplayer" => "多人",
        "survival" => "生存",
        "family" => "家庭",
        "kitchen" => "厨房",
        "horror" => "恐怖",
        "seasonal" => "节日",
        "comedy" => "搞笑",
        "puzzle" => "解谜",
        "sports" => "体育",
        "education" => "教育",
        "roleplay" => "角色扮演",
        // 光影
        "core" => "核心",
        "shadows" => "阴影",
        "water" => "水面",
        "lighting" => "光照",
        "reflections" => "反射",
        "bloom" => "辉光",
        "ctm" => "连接纹理",
        "fog" => "雾气",
        "particles" => "粒子",
        "post-processing" => "后处理",
        "atmosphere" => "大气",
        "colored-lighting" => "彩色光照",
        "lightmap" => "光照贴图",
        "volumetrics" => "体积光",
        "visual-effects" => "视觉特效",
        "screen-effects" => "屏幕特效",
        "cursed" => "实验性",
        "updating" => "更新中",
        "experimental" => "实验性",
        "legacy" => "旧版",
        // 资源包
        "16x" => "16×",
        "32x" => "32×",
        "64x" => "64×",
        "128x" => "128×",
        "256x" => "256×",
        "512x" => "512×",
        "fantasy" => "奇幻",
        "minimal" => "极简",
        "cute" => "可爱",
        "blocks" => "方块",
        "decorative" => "装饰",
        "menu" => "菜单",
        "panorama" => "全景图",
        "animals" => "动物",
        "mobs" => "生物",
        "medieval" => "中世纪",
        "modern" => "现代",
        "vanilla" => "原版",
        _ => return category.to_string(),
    };
    zh.to_string()
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
    /// 搜索是不是走了社区镜像
    ///
    /// CurseForge 官方搜索端点偶尔会临时拦截请求并返回 403，
    /// 这时改用社区镜像，界面上说一声，免得用户蒙在鼓里。
    #[serde(default)]
    pub via_mirror: bool,
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
    /// CurseForge 的项目 id，Modrinth 的结果没有这个
    #[serde(default)]
    pub cf_id: Option<u32>,
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
    /// CurseForge 的项目 id，Modrinth 的项目没有这个
    #[serde(default)]
    pub cf_id: Option<u32>,
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
    /// CurseForge 的项目 id 与文件 id，Modrinth 的版本没有这两个
    #[serde(default)]
    pub cf_mod_id: Option<u32>,
    #[serde(default)]
    pub cf_file_id: Option<u32>,
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
    /// CurseForge 的项目 id 与文件 id，Modrinth 的文件没有这两个
    #[serde(default)]
    pub cf_mod_id: Option<u32>,
    #[serde(default)]
    pub cf_file_id: Option<u32>,
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

/// 按 slug 查找项目，不存在时返回 None
///
/// 用于把本地文件名反查成 Modrinth 项目，从而给出官网链接。
pub fn find_by_slug(dl: &Downloader, slug: &str) -> Result<Option<Project>> {
    if slug.trim().is_empty() {
        return Ok(None);
    }
    let url = format!("{API}/project/{}", encode(slug));
    match dl.client_for_auth(&url).get(&url).send() {
        Ok(resp) => {
            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !resp.status().is_success() {
                return Err(anyhow::anyhow!("HTTP {}", resp.status()));
            }
            let bytes = resp.bytes().context("读取响应失败")?;
            let project = serde_json::from_slice::<Project>(&bytes)
                .context("解析项目信息失败")?;
            Ok(Some(project))
        }
        Err(err) => Err(anyhow::anyhow!("请求失败: {err}")),
    }
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

/// 整合包声明的 Minecraft 版本与加载器
#[derive(Debug, Clone, Default)]
pub struct ModpackDeps {
    /// Minecraft 版本
    pub minecraft: String,
    /// 加载器名，如 fabric
    pub loader: Option<String>,
    /// 加载器版本号，如 0.20.1
    pub loader_version: Option<String>,
}

/// 下载并解压整合包，返回解压目录与它声明的依赖
///
/// 整合包是一个**独立实例**，不能铺进现有实例的目录里，
/// 否则两个整合包的模组会互相串。这里只把内容准备好，
/// 装到哪个目录由调用方决定。
pub fn fetch_modpack(
    dl: &Downloader,
    version: &Version,
    progress: &Arc<DownloadProgress>,
) -> Result<(PathBuf, ModpackDeps)> {
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

    let index = read_index(&extract_dir)?;
    let deps = deps_from_index(&index);
    Ok((extract_dir, deps))
}

/// 把整合包里的文件装到目标目录
///
/// 目标目录就是整合包自己的实例目录，
/// 模组、配置、脚本都落在里面，不影响别的实例。
pub fn install_modpack_files(
    dl: &Downloader,
    extract_dir: &Path,
    target_dir: &Path,
    progress: &Arc<DownloadProgress>,
) -> Result<()> {
    let index = read_index(extract_dir)?;

    // 下载整合包内文件，全部放进这个实例自己的目录
    paths::ensure_dir(target_dir)?;
    let mut tasks: Vec<FileTask> = Vec::new();
    let mut total = 0u64;
    let mut skipped = 0usize;
    for entry in &index.files {
        // 服务端专用的文件客户端用不上，装进来只是白占磁盘
        if !entry.for_client() {
            skipped += 1;
            continue;
        }
        let dest = target_dir.join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
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
    if skipped > 0 {
        log::info!("整合包里有 {skipped} 个服务端专用文件，客户端不装");
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
            copy_dir(&src, target_dir)?;
        }
    }
    Ok(())
}

/// 读取整合包里的索引文件
fn read_index(extract_dir: &Path) -> Result<MrpackIndex> {
    let index_path = extract_dir.join("modrinth.index.json");
    let index_text = std::fs::read_to_string(&index_path)
        .context("整合包缺少 modrinth.index.json")?;
    Ok(serde_json::from_str(&index_text)?)
}

/// 从索引里取出 Minecraft 版本与加载器
fn deps_from_index(index: &MrpackIndex) -> ModpackDeps {
    let d = &index.dependencies;
    let mc = d.minecraft.clone();
    let pick = |value: &Option<String>| -> Option<String> {
        let raw = value.as_ref()?.trim();
        if raw.is_empty() {
            return None;
        }
        // 有时加载器版本号会带上 Minecraft 版本前缀，例如 1.20.1-47.2.0
        let stripped = raw
            .strip_prefix(&format!("{mc}-"))
            .unwrap_or(raw)
            .trim();
        if stripped.is_empty() {
            None
        } else {
            Some(stripped.to_string())
        }
    };

    let (loader, loader_version): (Option<&str>, Option<String>) =
        if let Some(v) = pick(&d.fabric) {
            (Some("fabric"), Some(v))
        } else if let Some(v) = pick(&d.neoforge) {
            (Some("neoforge"), Some(v))
        } else if let Some(v) = pick(&d.forge) {
            (Some("forge"), Some(v))
        } else if let Some(v) = pick(&d.quilt) {
            (Some("quilt"), Some(v))
        } else {
            (None, None)
        };
    ModpackDeps {
        minecraft: mc,
        loader: loader.map(|l| l.to_string()),
        loader_version,
    }
}

#[derive(Debug, Deserialize)]
struct MrpackIndex {
    #[serde(default)]
    files: Vec<MrpackFile>,
    #[serde(default)]
    dependencies: MrpackDependencies,
}

/// 整合包声明的 Minecraft 版本与加载器
///
/// 加载器字段的写法不统一：Modrinth 规范用 `fabric`，
/// 但不少整合包（多是从 CurseForge 转过来的）写成 `fabric-loader`，
/// 两种都要认，不然会把整合包当成原版装。
#[derive(Debug, Default, Deserialize)]
struct MrpackDependencies {
    #[serde(default)]
    minecraft: String,
    #[serde(default, alias = "fabric-loader")]
    fabric: Option<String>,
    #[serde(default, alias = "quilt-loader")]
    quilt: Option<String>,
    #[serde(default, alias = "forge-loader")]
    forge: Option<String>,
    #[serde(default, alias = "neoforge-loader")]
    neoforge: Option<String>,
}

/// 整合包里单个文件的适用环境
#[derive(Debug, Default, Deserialize)]
struct MrpackEnv {
    #[serde(default)]
    client: String,
    #[serde(default)]
    server: String,
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
    #[serde(default)]
    env: MrpackEnv,
}

impl MrpackFile {
    /// 这个文件要不要装进客户端
    ///
    /// 写了 `client: unsupported` 的是服务端专用文件，客户端用不上，
    /// 装进来只是白占磁盘。
    fn for_client(&self) -> bool {
        self.env.client != "unsupported"
    }
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

/// 项目主页链接
///
/// Modrinth 的路径要跟项目类型对上：`/mod/xxx`、`/shader/xxx`、`/resourcepack/xxx`，
/// 只写 `https://modrinth.com/xxx` 是打不开的（404）。
pub fn project_url(project_type: &str, slug: &str) -> String {
    let kind = match project_type {
        "mod" | "modpack" | "resourcepack" | "shader" | "plugin" | "project" => project_type,
        _ => "project",
    };
    format!("https://modrinth.com/{kind}/{slug}")
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 项目主页的路径要跟项目类型对上，只写 slug 是打不开的
    #[test]
    fn project_url_includes_type_path() {
        assert_eq!(
            project_url("mod", "sodium"),
            "https://modrinth.com/mod/sodium"
        );
        assert_eq!(
            project_url("shader", "complementary-reimagined"),
            "https://modrinth.com/shader/complementary-reimagined"
        );
        assert_eq!(
            project_url("resourcepack", "fancy-world"),
            "https://modrinth.com/resourcepack/fancy-world"
        );
        // 没见过的类型走通用入口
        assert_eq!(project_url("unknown", "foo"), "https://modrinth.com/project/foo");
    }

    #[test]
    fn category_label_translates_known_categories() {
        assert_eq!(category_label("technology"), "科技");
        assert_eq!(category_label("optimization"), "优化");
        assert_eq!(category_label("atmosphere"), "大气");
        // 加载器名保持原样
        assert_eq!(category_label("fabric"), "Fabric");
        // 未收录的分类原样返回
        assert_eq!(category_label("some-new-tag"), "some-new-tag");
    }

    /// 整合包索引里能读出 Minecraft 版本与加载器
    #[test]
    fn reads_modpack_dependencies() {
        let index: MrpackIndex = serde_json::from_str(
            r#"{
                "files": [],
                "dependencies": {
                    "minecraft": "1.20.1",
                    "fabric": "0.15.11"
                }
            }"#,
        )
        .unwrap();
        let deps = deps_from_index(&index);
        assert_eq!(deps.minecraft, "1.20.1");
        assert_eq!(deps.loader.as_deref(), Some("fabric"));
        assert_eq!(deps.loader_version.as_deref(), Some("0.15.11"));
    }

    /// 原版整合包没有加载器
    #[test]
    fn reads_modpack_without_loader() {
        let index: MrpackIndex = serde_json::from_str(
            r#"{
                "files": [],
                "dependencies": { "minecraft": "1.20.1" }
            }"#,
        )
        .unwrap();
        let deps = deps_from_index(&index);
        assert_eq!(deps.minecraft, "1.20.1");
        assert!(deps.loader.is_none());
        assert!(deps.loader_version.is_none());
    }

    /// 不少整合包把加载器写成 fabric-loader，认不出来就会被装成原版
    #[test]
    fn reads_modpack_loader_alias() {
        let index: MrpackIndex = serde_json::from_str(
            r#"{
                "files": [],
                "dependencies": {
                    "minecraft": "26.3",
                    "fabric-loader": "0.19.5"
                }
            }"#,
        )
        .unwrap();
        let deps = deps_from_index(&index);
        assert_eq!(deps.minecraft, "26.3");
        assert_eq!(deps.loader.as_deref(), Some("fabric"));
        assert_eq!(deps.loader_version.as_deref(), Some("0.19.5"));
    }

    /// 加载器版本号带 Minecraft 版本前缀时要拆开
    #[test]
    fn strips_mc_prefix_from_loader_version() {
        let index: MrpackIndex = serde_json::from_str(
            r#"{
                "files": [],
                "dependencies": {
                    "minecraft": "1.20.1",
                    "forge": "1.20.1-47.2.0"
                }
            }"#,
        )
        .unwrap();
        let deps = deps_from_index(&index);
        assert_eq!(deps.loader.as_deref(), Some("forge"));
        assert_eq!(deps.loader_version.as_deref(), Some("47.2.0"));
    }

    /// 服务端专用的文件客户端不该装
    #[test]
    fn skips_server_only_files() {
        let index: MrpackIndex = serde_json::from_str(
            r#"{
                "files": [
                    { "path": "mods/a.jar", "fileSize": 1 },
                    { "path": "mods/b.jar", "fileSize": 1,
                      "env": { "client": "unsupported", "server": "required" } },
                    { "path": "mods/c.jar", "fileSize": 1,
                      "env": { "client": "required", "server": "unsupported" } }
                ],
                "dependencies": { "minecraft": "26.3" }
            }"#,
        )
        .unwrap();
        assert!(index.files[0].for_client());
        assert!(!index.files[1].for_client());
        assert!(index.files[2].for_client());
    }
}
