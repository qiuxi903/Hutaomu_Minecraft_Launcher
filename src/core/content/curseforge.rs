//! CurseForge 平台支持
//!
//! CurseForge 的官方 API 必须带 `x-api-key`，不带就直接 403，
//! 所以这里的所有请求都走 [`Downloader::get_json_curseforge`]。
//!
//! key 的来源有三个，优先级从高到低：
//! 1. 设置里用户自己填的
//! 2. 环境变量 `CURSEFORGE_API_KEY`
//! 3. 编译期注入的内置 key [`BUILTIN_API_KEY`]
//!
//! 内置 key 不写在源码里（仓库公开，写进去等于公开），
//! 由 `build.rs` 从本地文件或环境变量注入，细节见 `build.rs`。
//!
//! CurseForge 返回的数据会被转成和 Modrinth 一样的结构
//! （[`Project`] / [`Version`] / [`SearchHit`]），
//! 这样下载页一套界面就能同时显示两个平台的内容。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};

use crate::core::content::modrinth::{
    ContentKind, GalleryImage, Hashes, License, ModpackDeps, Project, RemoteProject, SearchHit,
    SearchResponse, Version, VersionFile,
};
use crate::core::download::{DownloadProgress, Downloader, FileTask};
use crate::core::paths;

/// CurseForge 官方 API 根地址
pub const API: &str = "https://api.curseforge.com";

// 编译期注入的 key，由 build.rs 生成
include!(concat!(env!("OUT_DIR"), "/curseforge_key.rs"));

/// Minecraft 在 CurseForge 上的游戏 id
pub const GAME_ID: u32 = 432;

/// 作者内置的 API Key
///
/// 编译期注入，源码里不写：仓库是公开的，源码里出现就等于公开。
/// `build.rs` 从本地文件 `secret/curseforge_key.txt`（不入库）
/// 或环境变量 `HTMCL_CURSEFORGE_KEY` 取，取到就编进 exe，
/// 官方发布的版本因此开箱即用；自己从仓库编译则是空串，
/// 用户可以在「设置 → 内容平台」里填自己的 key。
///
/// 官方搜索端点偶尔会对请求做临时拦截或限流，返回 403，
/// 但同一个 key 过一会儿又能正常拿到数据，并非 key 权限问题。
/// 碰上这种时候 [`search`] 会自动改走社区镜像兜底。
pub const BUILTIN_API_KEY: &str = BUILD_CURSEFORGE_KEY;

/// 项目页地址
pub fn project_url(class: ContentKind, slug: &str) -> String {
    let category = match class {
        ContentKind::Mod => "mc-mods",
        ContentKind::Modpack => "modpacks",
        ContentKind::ResourcePack => "texture-packs",
        ContentKind::Shader => "shaders",
        ContentKind::DataPack => "data-packs",
        ContentKind::World => "worlds",
    };
    format!("https://www.curseforge.com/minecraft/{category}/{slug}")
}

// ------------------------------------------------------------------ 常量映射

/// 内容类型对应的 CurseForge 分类 id
///
/// 取值与 CurseForge 官网的分类一致，见 PCL-CE 的 `ModComp.CompFile`。
pub fn class_id(kind: ContentKind) -> u32 {
    match kind {
        ContentKind::Mod => 6,
        ContentKind::Modpack => 4471,
        ContentKind::Shader => 6552,
        ContentKind::ResourcePack => 12,
        ContentKind::DataPack => 6945,
        ContentKind::World => 17,
    }
}

/// CurseForge 的加载器编号
///
/// 0 = 任意，1 = Forge，3 = LiteLoader，4 = Fabric，5 = Quilt，6 = NeoForge。
pub fn mod_loader_type(loader: &str) -> Option<u32> {
    match loader.trim().to_lowercase().as_str() {
        "forge" => Some(1),
        "liteloader" => Some(3),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

/// 加载器编号对应的名字
pub fn loader_name(loader_type: u32) -> Option<&'static str> {
    match loader_type {
        1 => Some("Forge"),
        3 => Some("LiteLoader"),
        4 => Some("Fabric"),
        5 => Some("Quilt"),
        6 => Some("NeoForge"),
        _ => None,
    }
}

/// 文件发布类型 → 统一的版本类型
///
/// CurseForge：1 = 正式版，2 = 测试版，3 = 预览版。
pub fn release_type_label(release_type: u32) -> &'static str {
    match release_type {
        2 => "beta",
        3 => "alpha",
        _ => "release",
    }
}

/// 依赖关系类型
pub const RELATION_REQUIRED: u32 = 3;

// ------------------------------------------------------------------ 接口原始结构

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfLinks {
    #[serde(default)]
    pub website_url: Option<String>,
    /// 接口有时叫 `issueTrackerUrl`，有时叫 `issuesUrl`，两个都认
    #[serde(rename = "issueTrackerUrl", alias = "issuesUrl", default)]
    pub issue_tracker_url: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub wiki_url: Option<String>,
    #[serde(default)]
    pub discord_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfCategory {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfAuthor {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfPicture {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CfHash {
    #[serde(default)]
    pub value: String,
    /// 1 = Sha1，2 = Md5
    #[serde(default)]
    pub algo: u32,
}

/// 文件的校验值表
///
/// 接口里 `hashes` 通常是一个数组，但也有地方只给单个对象，两种都收下。
#[derive(Debug, Clone, Default)]
pub struct CfHashes(pub Vec<CfHash>);

impl<'de> Deserialize<'de> for CfHashes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let items = match value {
            serde_json::Value::Array(items) => items,
            serde_json::Value::Object(_) => vec![value],
            _ => return Ok(CfHashes::default()),
        };
        Ok(CfHashes(
            items
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect(),
        ))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub game_id: u32,
    #[serde(default)]
    pub mod_id: u32,
    #[serde(default)]
    pub is_available: bool,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub file_name: String,
    /// 1 = 正式版，2 = 测试版，3 = 预览版
    #[serde(default)]
    pub release_type: u32,
    #[serde(default)]
    pub file_status: u32,
    #[serde(default)]
    pub hashes: CfHashes,
    #[serde(default)]
    pub file_date: String,
    #[serde(default)]
    pub file_length: u64,
    #[serde(default)]
    pub download_count: u64,
    #[serde(default)]
    pub download_url: Option<String>,
    /// 文件在 CDN 上的直链，部分接口会带
    #[serde(default)]
    pub cdn_url: Option<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    /// 该文件面向的加载器，部分接口会带这个字段
    #[serde(default)]
    pub mod_loader_type: Option<u32>,
    /// CurseForge 自己算的文件指纹（MurmurHash2）
    #[serde(default)]
    pub file_fingerprint: u32,
    #[serde(default)]
    pub is_server_pack: bool,
    /// 这个文件的前置依赖，CurseForge v1 直接跟在文件里
    #[serde(default)]
    pub dependencies: Vec<CfDependency>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfProject {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub game_id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub links: CfLinks,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub status: u32,
    #[serde(default)]
    pub download_count: u64,
    #[serde(default)]
    pub is_featured: bool,
    #[serde(default)]
    pub primary_category_id: u32,
    #[serde(default)]
    pub categories: Vec<CfCategory>,
    #[serde(default)]
    pub class_id: u32,
    #[serde(default)]
    pub authors: Vec<CfAuthor>,
    #[serde(default)]
    pub logo: Option<CfPicture>,
    #[serde(default)]
    pub screenshots: Vec<CfPicture>,
    #[serde(default)]
    pub main_file_id: u32,
    #[serde(default)]
    pub latest_files: Vec<CfFile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfDependency {
    #[serde(default)]
    pub mod_id: u32,
    #[serde(default)]
    pub file_id: u32,
    /// 1 = 内嵌库，2 = 可选依赖，3 = 必须依赖，4 = 工具，5 = 不兼容，6 = 包含
    #[serde(default)]
    pub relation_type: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfPagination {
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub page_size: u32,
    #[serde(default)]
    pub result_count: u32,
    #[serde(default)]
    pub total_count: u32,
}

/// 单个数据的响应
#[derive(Debug, Clone, Deserialize)]
pub struct CfData<T> {
    #[serde(default = "Vec::new")]
    pub data: Vec<T>,
    #[serde(default)]
    pub pagination: Option<CfPagination>,
}

/// 单个值的数据响应
#[derive(Debug, Clone, Deserialize)]
pub struct CfValue<T> {
    #[serde(default)]
    pub data: T,
}

/// 列表数据的响应
#[derive(Debug, Clone, Deserialize)]
pub struct CfList<T> {
    #[serde(default = "Vec::new")]
    pub data: Vec<T>,
    #[serde(default)]
    pub pagination: Option<CfPagination>,
}

// ------------------------------------------------------------------ 整合包清单

/// CurseForge 整合包里的 `manifest.json`
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfModpackManifest {
    #[serde(default)]
    pub minecraft: Option<CfMinecraft>,
    #[serde(default)]
    pub manifest_type: Option<String>,
    #[serde(default)]
    pub manifest_version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub files: Vec<CfModpackFile>,
    /// overrides 目录名，默认 `overrides`
    #[serde(default)]
    pub overrides: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfMinecraft {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub mod_loaders: Vec<CfModLoader>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfModLoader {
    /// 形如 `forge-47.2.0`、`fabric-0.15.7`
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfModpackFile {
    /// 清单里写的是 `projectID`，大写 D，不能靠驼峰规则转
    #[serde(rename = "projectID", default)]
    pub project_id: u32,
    /// 清单里写的是 `fileID`
    #[serde(rename = "fileID", default)]
    pub file_id: u32,
    #[serde(default)]
    pub required: bool,
}

// ------------------------------------------------------------------ 转换

fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

/// 从 CurseForge 的哈希表里挑出 Sha1
fn sha1_of(file: &CfFile) -> Option<String> {
    file.hashes
        .0
        .iter()
        .find(|h| h.algo == 1)
        .map(|h| h.value.clone())
}

/// CurseForge 的文件对象 → 统一的项目详情
fn project_to_unified(project: &CfProject, kind: ContentKind) -> Project {
    let mut categories: Vec<String> = project
        .categories
        .iter()
        .map(|c| c.slug.clone())
        .filter(|s| !s.is_empty())
        .collect();
    if categories.is_empty() {
        if let Some(primary) = project.categories.first() {
            categories.push(primary.name.clone());
        }
    }

    // 加载器与支持的游戏版本从最新文件里取
    let mut loaders: Vec<String> = Vec::new();
    let mut game_versions: Vec<String> = Vec::new();
    for file in &project.latest_files {
        if let Some(loader_type) = file.mod_loader_type {
            if let Some(name) = loader_name(loader_type) {
                if !loaders.iter().any(|l| l == name) {
                    loaders.push(name.to_string());
                }
            }
        }
        for game in &file.game_versions {
            if !game_versions.contains(game) {
                game_versions.push(game.clone());
            }
        }
    }

    let mut gallery: Vec<GalleryImage> = project
        .screenshots
        .iter()
        .filter(|s| !s.url.is_empty())
        .map(|s| GalleryImage {
            url: s.url.clone(),
            featured: false,
            title: None,
        })
        .collect();
    if let Some(logo) = &project.logo {
        if !logo.url.is_empty() {
            gallery.insert(
                0,
                GalleryImage {
                    url: logo.url.clone(),
                    featured: true,
                    title: None,
                },
            );
        }
    }

    Project {
        id: project.id.to_string(),
        slug: project.slug.clone(),
        title: project.name.clone(),
        description: project.summary.clone(),
        body: project.summary.clone(),
        icon_url: project.logo.as_ref().map(|l| l.url.clone()),
        downloads: project.download_count,
        followers: 0,
        categories,
        loaders,
        game_versions,
        project_type: kind.facet().to_string(),
        published: String::new(),
        updated: String::new(),
        source_url: project
            .links
            .source_url
            .clone()
            .or_else(|| Some(project_url(kind, &project.slug))),
        issues_url: project.links.issue_tracker_url.clone(),
        wiki_url: project.links.wiki_url.clone(),
        discord_url: project.links.discord_url.clone(),
        gallery,
        license: License {
            id: "curseforge".into(),
            name: "CurseForge 项目页".into(),
        },
        cf_id: Some(project.id),
    }
}

/// CurseForge 的项目 → 统一的搜索结果
fn project_to_hit(project: &CfProject, kind: ContentKind) -> SearchHit {
    let mut versions: Vec<String> = Vec::new();
    for file in &project.latest_files {
        for game in &file.game_versions {
            if !versions.contains(game) {
                versions.push(game.clone());
            }
        }
    }
    SearchHit {
        project_id: project.id.to_string(),
        slug: project.slug.clone(),
        title: project.name.clone(),
        description: project.summary.clone(),
        author: project
            .authors
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        icon_url: project.logo.as_ref().map(|l| l.url.clone()),
        downloads: project.download_count,
        follows: 0,
        categories: project
            .categories
            .iter()
            .map(|c| c.slug.clone())
            .filter(|s| !s.is_empty())
            .collect(),
        versions,
        project_type: kind.facet().to_string(),
        date_modified: String::new(),
        license: String::new(),
        gallery: project
            .screenshots
            .iter()
            .map(|s| s.url.clone())
            .collect(),
        cf_id: Some(project.id),
    }
}

/// CurseForge 的文件 → 统一的版本
fn file_to_unified(project_id: &str, file: &CfFile) -> Version {
    let (mc_versions, loaders) = split_game_versions(&file.game_versions);
    let loaders = match file.mod_loader_type.and_then(loader_name) {
        // 接口给了加载器编号就以它为准
        Some(name) => vec![name.to_string()],
        None => loaders,
    };
    let mut hashes = Hashes::default();
    hashes.sha1 = sha1_of(file);
    Version {
        id: file.id.to_string(),
        project_id: project_id.to_string(),
        name: file.display_name.clone(),
        version_number: file.display_name.clone(),
        version_type: release_type_label(file.release_type).to_string(),
        game_versions: mc_versions,
        loaders,
        date_published: file.file_date.clone(),
        downloads: file.download_count,
        files: vec![VersionFile {
            url: String::new(),
            filename: file.file_name.clone(),
            primary: true,
            size: file.file_length,
            hashes,
            cf_mod_id: Some(file.mod_id),
            cf_file_id: Some(file.id),
        }],
        dependencies: Vec::new(),
        cf_mod_id: Some(file.mod_id),
        cf_file_id: Some(file.id),
    }
}

/// 把 gameVersions 拆成游戏版本与加载器
///
/// CurseForge 把加载器名也塞在 `gameVersions` 里，
/// 形如 `["Fabric", "Client", "1.20.1", "Quilt"]`：
/// 数字开头的是 Minecraft 版本，其余是加载器或标记。
fn split_game_versions(versions: &[String]) -> (Vec<String>, Vec<String>) {
    let mut mc = Vec::new();
    let mut loaders = Vec::new();
    for value in versions {
        let name = value.trim();
        if name.is_empty() {
            continue;
        }
        if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            mc.push(name.to_string());
        } else if let Some(loader) = loader_from_name(name) {
            loaders.push(loader.to_string());
        }
    }
    (mc, loaders)
}

/// gameVersions 里的加载器名 → 统一的加载器名
fn loader_from_name(name: &str) -> Option<&'static str> {
    match name.trim().to_lowercase().as_str() {
        "fabric" => Some("Fabric"),
        "quilt" => Some("Quilt"),
        "forge" => Some("Forge"),
        "neoforge" => Some("NeoForge"),
        "liteloader" => Some("LiteLoader"),
        "rift" => Some("Rift"),
        _ => None,
    }
}

// ------------------------------------------------------------------ 接口调用

/// 搜索接口的备用地址
///
/// 官方搜索端点偶尔会对请求做临时拦截或限流，返回 403，
/// 详情、文件、下载这些端点却都正常，说明不是 key 权限问题。
/// 这时改用社区镜像，路径与返回结构都跟官方一致，不用改解析。
const SEARCH_MIRROR: &str = "https://api.curse.tools/v1";

/// 拼搜索用的查询串
///
/// 官方与镜像的参数完全一样，拼一次两边都用。
fn search_query(
    kind: ContentKind,
    query: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
    offset: u32,
    limit: u32,
) -> String {
    let mut url = format!(
        "gameId={GAME_ID}&classId={}&sortOrder=desc&pageSize={}&index={}&sortField=6",
        class_id(kind),
        limit.clamp(1, 50),
        offset
    );
    if let Some(version) = mc_version.filter(|v| !v.trim().is_empty()) {
        url.push_str(&format!("&gameVersion={}", encode(version.trim())));
    }
    // 数据包不分加载器，按加载器筛会把它们全筛掉
    if kind != ContentKind::DataPack {
        if let Some(loader_type) = loader
            .filter(|l| !l.trim().is_empty())
            .and_then(mod_loader_type)
        {
            url.push_str(&format!("&modLoaderType={loader_type}"));
        }
    }
    let keyword = query.trim();
    if !keyword.is_empty() {
        url.push_str(&format!("&searchFilter={}", encode(keyword)));
    }
    url
}

/// 搜索内容
///
/// 先打官方接口，官方拒绝（403）就换社区镜像。
pub fn search(
    dl: &Downloader,
    kind: ContentKind,
    query: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
    offset: u32,
    limit: u32,
) -> Result<SearchResponse> {
    let params = search_query(kind, query, mc_version, loader, offset, limit);
    let official = format!("{API}/v1/mods/search?{params}");

    let (response, via_mirror): (CfData<CfProject>, bool) =
        match dl.get_json_curseforge_allow_denied(&official)? {
            Some(response) => (response, false),
            None => {
                // 官方搜索被临时拦截或限流，换社区镜像兜底
                log::info!("CurseForge 官方搜索被拒，改用社区镜像");
                let mirror = format!("{SEARCH_MIRROR}/mods/search?{params}");
                // 镜像不需要 Key，也不该把 Key 发给第三方
                let response = dl
                    .get_json_with_headers(&mirror, &[])
                    .with_context(|| {
                        "CurseForge 搜索失败：官方接口与备用镜像都没给出结果".to_string()
                    })?;
                (response, true)
            }
        };
    let hits = response
        .data
        .iter()
        .map(|p| project_to_hit(p, kind))
        .collect();
    Ok(SearchResponse {
        hits,
        offset,
        limit,
        via_mirror,
        total_hits: response
            .pagination
            .map(|p| p.total_count)
            .unwrap_or_default(),
    })
}

/// 项目详情
pub fn project(dl: &Downloader, id: &str, kind: ContentKind) -> Result<Project> {
    let url = format!("{API}/v1/mods/{id}");
    // 单个项目的 `data` 是个对象，不是数组
    let response: CfValue<CfProject> = dl
        .get_json_curseforge(&url)
        .with_context(|| "获取 CurseForge 项目详情失败".to_string())?;
    Ok(project_to_unified(&response.data, kind))
}

/// 项目的全部文件（可安装的版本）
pub fn versions(
    dl: &Downloader,
    project_id: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<Version>> {
    let mut all: Vec<CfFile> = Vec::new();
    // `index` 是偏移量不是页码，每页 50 个就往后挪 50
    const PAGE_SIZE: u32 = 50;
    let mut offset = 0u32;
    for _ in 0..20 {
        let url = format!("{API}/v1/mods/{project_id}/files?pageSize={PAGE_SIZE}&index={offset}");
        let response: CfList<CfFile> = dl
            .get_json_curseforge(&url)
            .with_context(|| "获取 CurseForge 文件列表失败".to_string())?;
        let count = response.data.len();
        all.extend(response.data);
        let total = response.pagination.map(|p| p.total_count).unwrap_or(0);
        if count == 0 || all.len() as u32 >= total {
            break;
        }
        offset += PAGE_SIZE;
    }

    let want_loader = loader
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_lowercase());
    let mut list: Vec<Version> = all
        .into_iter()
        .filter(|f| f.is_available)
        .filter(|f| !f.is_server_pack)
        .filter(|f| {
            // 游戏版本只认数字开头的那部分，加载器名不算
            let (mc_versions, _) = split_game_versions(&f.game_versions);
            match mc_version {
                Some(mc) if !mc.trim().is_empty() => {
                    mc_versions.iter().any(|g| g == mc.trim())
                }
                _ => true,
            }
        })
        .filter(|f| match (&want_loader, f.mod_loader_type) {
            // 接口给了加载器编号就按编号比
            (Some(want), Some(have)) => {
                loader_name(have).is_some_and(|name| name.to_lowercase() == *want)
            }
            // 没给编号就从 gameVersions 里认
            (Some(want), None) => {
                let (_, loaders) = split_game_versions(&f.game_versions);
                loaders.iter().any(|l| l.to_lowercase() == *want)
            }
            (None, _) => true,
        })
        .map(|f| file_to_unified(project_id, &f))
        .collect();

    // 最新的排前面
    list.sort_by(|a, b| b.date_published.cmp(&a.date_published));
    Ok(list)
}

/// 文件的真实下载地址
///
/// CurseForge 的下载地址要先向接口换一次，CDN 地址直接拼不出来。
pub fn download_url(dl: &Downloader, mod_id: u32, file_id: u32) -> Result<String> {
    let url = format!("{API}/v1/mods/{mod_id}/files/{file_id}/download-url");
    let response: CfValue<String> = dl
        .get_json_curseforge(&url)
        .with_context(|| "获取 CurseForge 下载地址失败".to_string())?;
    let address = response.data;
    if address.trim().is_empty() {
        return Err(anyhow!("CurseForge 没有给出下载地址"));
    }
    Ok(address)
}

/// 文件的前置依赖
///
/// CurseForge v1 没有单独的依赖接口，依赖关系直接挂在文件对象上，
/// 所以这里取一次文件详情，从里面读依赖列表。
pub fn dependencies(dl: &Downloader, mod_id: u32, file_id: u32) -> Result<Vec<CfDependency>> {
    let url = format!("{API}/v1/mods/{mod_id}/files/{file_id}");
    let response: CfValue<CfFile> = match dl.get_json_curseforge(&url) {
        Ok(response) => response,
        Err(err) => {
            log::warn!("获取 CurseForge 依赖失败: {err}");
            return Ok(Vec::new());
        }
    };
    Ok(response.data.dependencies)
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
    let (mod_id, file_id) = match (version.cf_mod_id, version.cf_file_id) {
        (Some(m), Some(f)) => (m, f),
        _ => return Err(anyhow!("这个文件不是来自 CurseForge")),
    };
    let dir = if kind.install_dir().is_empty() {
        game_dir.to_path_buf()
    } else {
        game_dir.join(kind.install_dir())
    };
    paths::ensure_dir(&dir)?;
    let dest = dir.join(&file.filename);
    let url = download_url(dl, mod_id, file_id)?;

    progress.set_message(format!("正在下载 {} ...", file.filename));
    progress.add_total(file.size);
    let mut task = FileTask::new(url, dest.clone())
        .with_size(file.size)
        .with_optional_sha1(file.hashes.sha1.clone());
    if let Some(key) = dl.curseforge_key() {
        task = task.with_header("x-api-key", key);
    }
    dl.download(&task, progress)?;
    Ok(dest)
}

/// 下载并解压整合包，返回解压目录与它声明的依赖
pub fn fetch_modpack(
    dl: &Downloader,
    version: &Version,
    progress: &Arc<DownloadProgress>,
) -> Result<(PathBuf, ModpackDeps)> {
    let file = version
        .primary_file()
        .ok_or_else(|| anyhow!("该整合包没有可下载的文件"))?;
    let (mod_id, file_id) = match (version.cf_mod_id, version.cf_file_id) {
        (Some(m), Some(f)) => (m, f),
        _ => return Err(anyhow!("这个文件不是来自 CurseForge")),
    };
    let cache = paths::cache_dir().join("modpacks");
    paths::ensure_dir(&cache)?;
    let archive_path = cache.join(&file.filename);

    progress.set_message("正在下载整合包 ...");
    progress.add_total(file.size);
    let url = download_url(dl, mod_id, file_id)?;
    let mut task = FileTask::new(url, archive_path.clone())
        .with_size(file.size)
        .with_optional_sha1(file.hashes.sha1.clone());
    if let Some(key) = dl.curseforge_key() {
        task = task.with_header("x-api-key", key);
    }
    dl.download(&task, progress)?;

    progress.set_message("正在解压整合包 ...");
    let extract_dir = cache.join(format!("cf-extract-{}", version.id));
    if extract_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&extract_dir);
    }
    paths::ensure_dir(&extract_dir)?;

    let archive_file = std::fs::File::open(&archive_path)?;
    let mut archive = zip::ZipArchive::new(archive_file)?;
    archive.extract(&extract_dir)?;

    let manifest = read_manifest(&extract_dir)?;
    let deps = deps_from_manifest(&manifest);
    Ok((extract_dir, deps))
}

/// 读取整合包里的清单文件
pub fn read_manifest(extract_dir: &Path) -> Result<CfModpackManifest> {
    let path = extract_dir.join("manifest.json");
    let text = std::fs::read_to_string(&path).context("整合包缺少 manifest.json")?;
    Ok(serde_json::from_str(&text)?)
}

/// 从清单里取出 Minecraft 版本与加载器
pub fn deps_from_manifest(manifest: &CfModpackManifest) -> ModpackDeps {
    let mut deps = ModpackDeps::default();
    let Some(minecraft) = &manifest.minecraft else {
        return deps;
    };
    deps.minecraft = minecraft.version.clone();
    // 主加载器优先，没有标主加载器就用第一个
    let loader = minecraft
        .mod_loaders
        .iter()
        .find(|l| l.primary)
        .or_else(|| minecraft.mod_loaders.first());
    if let Some(loader) = loader {
        let (kind, version) = split_loader_id(&loader.id);
        deps.loader = kind;
        deps.loader_version = version;
    }
    deps
}

/// 拆开整合包里的加载器标识
///
/// 形如 `forge-47.2.0`，拆成加载器名与版本号。
pub fn split_loader_id(id: &str) -> (Option<String>, Option<String>) {
    let id = id.trim();
    if id.is_empty() {
        return (None, None);
    }
    for (prefix, name) in [
        ("neoforge-", "neoforge"),
        ("forge-", "forge"),
        ("fabric-", "fabric"),
        ("quilt-", "quilt"),
        ("liteloader-", "liteloader"),
    ] {
        if let Some(version) = id.strip_prefix(prefix) {
            let version = version.trim();
            return (
                Some(name.to_string()),
                if version.is_empty() {
                    None
                } else {
                    Some(version.to_string())
                },
            );
        }
    }
    match id.to_lowercase().as_str() {
        "forge" => (Some("forge".into()), None),
        "neoforge" => (Some("neoforge".into()), None),
        "fabric" => (Some("fabric".into()), None),
        "quilt" => (Some("quilt".into()), None),
        "liteloader" => (Some("liteloader".into()), None),
        _ => (None, None),
    }
}

/// 把整合包里的文件装到目标目录
///
/// 清单里列的模组放进 `mods/`，`overrides` 目录的内容铺在实例根目录。
pub fn install_modpack_files(
    dl: &Downloader,
    extract_dir: &Path,
    target_dir: &Path,
    progress: &Arc<DownloadProgress>,
) -> Result<()> {
    let manifest = read_manifest(extract_dir)?;
    paths::ensure_dir(target_dir)?;
    let mods_dir = target_dir.join("mods");
    paths::ensure_dir(&mods_dir)?;

    let entries: Vec<CfModpackFile> = manifest
        .files
        .iter()
        .filter(|f| f.file_id > 0 && f.project_id > 0)
        .cloned()
        .collect();
    if entries.is_empty() {
        copy_overrides(extract_dir, target_dir, &manifest)?;
        return Ok(());
    }

    // 一个整合包常有上百个依赖，逐个查接口要花不少时间，
    // 这里并发查，顺便把文件大小与校验值一起拿到，进度条才算得准。
    let resolved = resolve_files(dl, &entries, progress);

    let mut tasks: Vec<FileTask> = Vec::new();
    let mut total = 0u64;
    let mut failed = 0usize;
    for (_entry, resolved) in entries.iter().zip(resolved.iter()) {
        let Some(resolved) = resolved else {
            failed += 1;
            continue;
        };
        let file_name = if resolved.file_name.is_empty() {
            file_name_from_url(&resolved.url)
        } else {
            resolved.file_name.clone()
        };
        if file_name.is_empty() {
            failed += 1;
            continue;
        }
        let dest = mods_dir.join(&file_name);
        total += resolved.size;
        let mut task = FileTask::new(resolved.url.clone(), dest)
            .with_size(resolved.size)
            .with_optional_sha1(resolved.sha1.clone());
        if let Some(key) = dl.curseforge_key() {
            task = task.with_header("x-api-key", key);
        }
        tasks.push(task);
    }

    if failed > 0 {
        log::warn!("整合包里有 {failed} 个文件没取到下载地址");
    }
    if !tasks.is_empty() {
        progress.set_message(format!("正在下载整合包内容（{} 个文件）...", tasks.len()));
        progress.total.store(total, std::sync::atomic::Ordering::Relaxed);
        progress.done.store(0, std::sync::atomic::Ordering::Relaxed);
        dl.download_batch(tasks, 8, progress)?;
    }

    copy_overrides(extract_dir, target_dir, &manifest)?;
    Ok(())
}

/// 一个依赖文件查出来的信息
#[derive(Debug, Clone, Default)]
struct ResolvedFile {
    url: String,
    file_name: String,
    size: u64,
    sha1: Option<String>,
}

/// 并发查每个依赖文件的下载地址
///
/// 同一个项目只查一次文件列表，列表里能拿到的直接用，
/// 拿不到再单独换下载地址。
fn resolve_files(
    dl: &Downloader,
    entries: &[CfModpackFile],
    progress: &Arc<DownloadProgress>,
) -> Vec<Option<ResolvedFile>> {
    // 按项目分组，一个项目一次请求
    let mut projects: Vec<u32> = Vec::new();
    for entry in entries {
        if !projects.contains(&entry.project_id) {
            projects.push(entry.project_id);
        }
    }
    let workers = num_cpus::get().clamp(2, 8);
    let chunk = projects.len().div_ceil(workers).max(1);

    let mut lists: Vec<Vec<CfFile>> = vec![Vec::new(); projects.len()];
    std::thread::scope(|scope| {
        let handles: Vec<_> = projects
            .chunks(chunk)
            .enumerate()
            .map(|(index, group)| {
                let dl = dl.clone();
                let progress = progress.clone();
                scope.spawn(move || -> Vec<(usize, Vec<CfFile>)> {
                    let mut out = Vec::new();
                    for (offset, project_id) in group.iter().enumerate() {
                        if progress.is_cancelled() {
                            break;
                        }
                        let slot = index * chunk + offset;
                        match project_files(&dl, *project_id) {
                            Ok(files) => out.push((slot, files)),
                            Err(err) => {
                                log::warn!("取 CurseForge 项目 {project_id} 的文件列表失败: {err}");
                            }
                        }
                    }
                    out
                })
            })
            .collect();
        for handle in handles {
            for (slot, files) in handle.join().unwrap_or_default() {
                lists[slot] = files;
            }
        }
    });

    let mut out: Vec<Option<ResolvedFile>> = vec![None; entries.len()];
    for (slot, entry) in entries.iter().enumerate() {
        let project_index = projects
            .iter()
            .position(|p| *p == entry.project_id)
            .unwrap_or(0);
        let file = lists[project_index]
            .iter()
            .find(|f| f.id == entry.file_id)
            .cloned();
        let mut resolved = match &file {
            Some(file) => ResolvedFile {
                url: String::new(),
                file_name: file.file_name.clone(),
                size: file.file_length,
                sha1: sha1_of(file),
            },
            None => ResolvedFile::default(),
        };
        // 文件列表里的 cdnUrl 能直接用就省一次请求
        let direct = file.as_ref().and_then(|f| f.cdn_url.clone());
        resolved.url = match direct {
            Some(url) if !url.trim().is_empty() => url,
            _ => match download_url(dl, entry.project_id, entry.file_id) {
                Ok(url) => url,
                Err(err) => {
                    log::warn!(
                        "取不到 CurseForge 文件 {}/{} 的下载地址: {err}",
                        entry.project_id,
                        entry.file_id
                    );
                    continue;
                }
            },
        };
        out[slot] = Some(resolved);
    }
    out
}

/// 取某个项目的文件列表
fn project_files(dl: &Downloader, project_id: u32) -> Result<Vec<CfFile>> {
    let url = format!("{API}/v1/mods/{project_id}/files?pageSize=50");
    let response: CfList<CfFile> = dl
        .get_json_curseforge(&url)
        .with_context(|| format!("获取 CurseForge 项目 {project_id} 的文件列表失败"))?;
    Ok(response.data)
}

/// 把 overrides 目录的内容铺到实例根目录
fn copy_overrides(
    extract_dir: &Path,
    target_dir: &Path,
    manifest: &CfModpackManifest,
) -> Result<()> {
    let override_name = manifest.overrides.clone().unwrap_or_else(|| "overrides".into());
    for name in [override_name.as_str(), "client-overrides"] {
        let src = extract_dir.join(name);
        if src.is_dir() {
            copy_dir(&src, target_dir)?;
        }
    }
    Ok(())
}

/// 从下载地址里取出文件名
fn file_name_from_url(url: &str) -> String {
    let last = url.split('/').next_back().unwrap_or_default();
    percent_encoding::percent_decode_str(last)
        .decode_utf8_lossy()
        .to_string()
}

// ------------------------------------------------------------------ 指纹反查

/// 本地文件的 CurseForge 指纹
///
/// CurseForge 给每个文件算了一个 32 位指纹（MurmurHash2，种子为 1），
/// 拿这个指纹就能反查文件在 CurseForge 上属于哪个项目，
/// 本地装着的模组也能认出是哪一页。
///
/// 算之前要先把制表符、换行、回车、空格滤掉，
/// 不然同样的文件换个平台重新打包就对不上了。
pub fn fingerprint_of(data: &[u8]) -> u32 {
    let filtered: Vec<u8> = data
        .iter()
        .copied()
        .filter(|b| !matches!(b, 9 | 10 | 13 | 32))
        .collect();
    murmur2::murmur2(&filtered, 1)
}

/// 读文件算指纹
pub fn file_fingerprint(path: &Path) -> Option<u32> {
    let data = std::fs::read(path).ok()?;
    Some(fingerprint_of(&data))
}

/// 问接口要某个文件自己的指纹
///
/// CurseForge 在文件信息里就带着指纹，
/// 拿它跟本地算出来的对一下，能验证算法有没有算错。
pub fn file_fingerprint_of(dl: &Downloader, mod_id: u32, file_id: u32) -> Option<u32> {
    let url = format!("{API}/v1/mods/{mod_id}/files/{file_id}");
    let response: CfValue<CfFile> = dl.get_json_curseforge(&url).ok()?;
    let fingerprint = response.data.file_fingerprint;
    if fingerprint == 0 {
        None
    } else {
        Some(fingerprint)
    }
}

/// 指纹反查的结果
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfFingerprintFile {
    #[serde(default)]
    pub id: u32,
    /// 文件的指纹
    #[serde(default)]
    pub file_fingerprint: u32,
    /// 文件在项目里的编号
    #[serde(default)]
    pub mod_id: u32,
}

/// 指纹命中的项目
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfFingerprintHit {
    /// 项目的编号
    #[serde(default)]
    pub id: u32,
    /// 命中的文件
    #[serde(default)]
    pub file: Option<CfFingerprintFile>,
}

/// 指纹反查的响应
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFingerprintResponse {
    #[serde(default)]
    pub data: CfFingerprintData,
}

/// 指纹反查的正文
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFingerprintData {
    /// 完全匹配上的文件
    #[serde(default)]
    pub exact_matches: Vec<CfFingerprintHit>,
}

/// 请求指纹反查的正文
#[derive(Debug, Clone, Serialize)]
struct CfFingerprintRequest {
    fingerprints: Vec<u32>,
}

/// 一次最多查多少个指纹
const FINGERPRINT_BATCH: usize = 100;

/// 用指纹反查项目
///
/// 返回的是「指纹 → 项目编号」的对应关系。
/// 接口一次最多吃 100 个，超了自动分批。
pub fn fingerprints(dl: &Downloader, hashes: &[u32]) -> Result<Vec<(u32, u32)>> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for chunk in hashes.chunks(FINGERPRINT_BATCH) {
        if chunk.is_empty() {
            continue;
        }
        let url = format!("{API}/v1/fingerprints/{GAME_ID}");
        let body = CfFingerprintRequest {
            fingerprints: chunk.to_vec(),
        };
        let response: CfFingerprintResponse = match dl.post_json_curseforge(&url, &body) {
            Ok(response) => response,
            Err(err) => {
                // 一个批次查不动不影响别的批次，记下来继续
                log::warn!("CurseForge 指纹反查失败: {err}");
                continue;
            }
        };
        for hit in &response.data.exact_matches {
            let Some(file) = &hit.file else {
                continue;
            };
            if file.file_fingerprint == 0 {
                continue;
            }
            let project_id = if hit.id != 0 { hit.id } else { file.mod_id };
            if project_id == 0 {
                continue;
            }
            out.push((file.file_fingerprint, project_id));
        }
    }
    Ok(out)
}

/// 批量取项目的简介
///
/// 指纹反查只给得出项目编号，项目页地址要用项目自己的 slug，
/// 显示图标还要项目自己的 logo，这里一次把几个项目都取回来。
pub fn projects_by_ids(dl: &Downloader, ids: &[u32]) -> Result<Vec<(u32, RemoteProject)>> {
    let mut out: Vec<(u32, RemoteProject)> = Vec::new();
    for chunk in ids.chunks(50) {
        if chunk.is_empty() {
            continue;
        }
        // 字段名接口只认驼峰，写成 mod_ids 会直接 400
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct IdsRequest<'a> {
            mod_ids: &'a [u32],
            game_id: u32,
        }
        let body = IdsRequest {
            mod_ids: chunk,
            game_id: GAME_ID,
        };
        let response: CfList<CfProject> = match dl.post_json_curseforge(&format!("{API}/v1/mods"), &body)
        {
            Ok(response) => response,
            Err(err) => {
                log::warn!("CurseForge 批量取项目失败: {err}");
                continue;
            }
        };
        for project in &response.data {
            if project.id == 0 || project.slug.is_empty() {
                continue;
            }
            out.push((
                project.id,
                RemoteProject {
                    slug: project.slug.clone(),
                    // CurseForge 的接口只认数字编号，查版本列表时要用它
                    project_id: Some(project.id.to_string()),
                    // 缩略图比原图小得多，列表里的小图标用它更合适
                    icon_url: project.logo.as_ref().map(|logo| {
                        logo.thumbnail_url
                            .clone()
                            .filter(|url| !url.is_empty())
                            .unwrap_or_else(|| logo.url.clone())
                    }),
                },
            ));
        }
    }
    Ok(out)
}

/// 复制目录
fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    paths::ensure_dir(dst)?;
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = match entry.path().strip_prefix(src) {
            Ok(rel) => rel,
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            paths::ensure_dir(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                paths::ensure_dir(parent)?;
            }
            std::fs::copy(entry.path(), &target).with_context(|| {
                format!("复制整合包文件失败: {}", entry.path().display())
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_kind_maps_to_class_id() {
        assert_eq!(class_id(ContentKind::Mod), 6);
        assert_eq!(class_id(ContentKind::Modpack), 4471);
        assert_eq!(class_id(ContentKind::Shader), 6552);
        assert_eq!(class_id(ContentKind::ResourcePack), 12);
        assert_eq!(class_id(ContentKind::World), 17);
    }

    #[test]
    fn loader_names_map_to_type() {
        assert_eq!(mod_loader_type("Forge"), Some(1));
        assert_eq!(mod_loader_type("fabric"), Some(4));
        assert_eq!(mod_loader_type("Quilt"), Some(5));
        assert_eq!(mod_loader_type("neoforge"), Some(6));
        assert_eq!(mod_loader_type("原版"), None);
        assert_eq!(mod_loader_type(""), None);
    }

    #[test]
    fn release_type_maps_to_version_type() {
        assert_eq!(release_type_label(1), "release");
        assert_eq!(release_type_label(2), "beta");
        assert_eq!(release_type_label(3), "alpha");
    }

    #[test]
    fn manifest_gives_minecraft_and_loader() {
        let manifest: CfModpackManifest = serde_json::from_str(
            r#"{
                "minecraft": {
                    "version": "1.20.1",
                    "modLoaders": [{"id": "forge-47.2.0", "primary": true}]
                },
                "manifestType": "minecraftModpack",
                "manifestVersion": 1,
                "name": "测试整合包",
                "version": "1.0.0",
                "author": "作者",
                "files": [
                    {"projectID": 238222, "fileID": 4593548, "required": true}
                ],
                "overrides": "overrides"
            }"#,
        )
        .unwrap();
        let deps = deps_from_manifest(&manifest);
        assert_eq!(deps.minecraft, "1.20.1");
        assert_eq!(deps.loader.as_deref(), Some("forge"));
        assert_eq!(deps.loader_version.as_deref(), Some("47.2.0"));
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].project_id, 238222);
    }

    #[test]
    fn manifest_without_loader_only_reports_minecraft() {
        let manifest: CfModpackManifest = serde_json::from_str(
            r#"{"minecraft":{"version":"1.16.5","modLoaders":[]},"files":[]}"#,
        )
        .unwrap();
        let deps = deps_from_manifest(&manifest);
        assert_eq!(deps.minecraft, "1.16.5");
        assert!(deps.loader.is_none());
    }

    #[test]
    fn loader_id_split_handles_bare_names() {
        assert_eq!(
            split_loader_id("fabric-0.15.7"),
            (Some("fabric".to_string()), Some("0.15.7".to_string()))
        );
        assert_eq!(
            split_loader_id("neoforge-47.1.84"),
            (Some("neoforge".to_string()), Some("47.1.84".to_string()))
        );
        assert_eq!(split_loader_id("forge"), (Some("forge".to_string()), None));
        assert_eq!(split_loader_id(""), (None, None));
    }

    #[test]
    fn file_name_is_taken_from_download_url() {
        assert_eq!(
            file_name_from_url("https://mediafilez.forgecdn.net/files/238222/4593548/all-the-mods-9.jar"),
            "all-the-mods-9.jar"
        );
        assert_eq!(
            file_name_from_url("https://mediafilez.forgecdn.net/files/1/2/my%20mod.jar"),
            "my mod.jar"
        );
    }

    #[test]
    fn project_converts_to_unified_shape() {
        let project: CfProject = serde_json::from_str(
            r#"{
                "id": 238222,
                "gameId": 432,
                "name": "All The Mods 9",
                "slug": "all-the-mods-9",
                "summary": "一个整合包",
                "status": 4,
                "downloadCount": 123456,
                "classId": 4471,
                "authors": [{"id": 1, "name": "作者"}],
                "logo": {"url": "https://mediafilez.forgecdn.net/logo.png"},
                "categories": [{"id": 4, "name": "Adventure", "slug": "adventure"}],
                "latestFiles": [{
                    "id": 4593548,
                    "modId": 238222,
                    "isAvailable": true,
                    "displayName": "9.0.0",
                    "fileName": "all-the-mods-9.zip",
                    "releaseType": 1,
                    "fileLength": 1234,
                    "gameVersions": ["1.20.1"],
                    "hashes": [{"value": "ABCDEF", "algo": 1}]
                }]
            }"#,
        )
        .unwrap();
        let unified = project_to_unified(&project, ContentKind::Modpack);
        assert_eq!(unified.id, "238222");
        assert_eq!(unified.title, "All The Mods 9");
        assert_eq!(unified.downloads, 123456);
        assert_eq!(unified.cf_id, Some(238222));
        assert_eq!(unified.game_versions, vec!["1.20.1".to_string()]);

        let version = file_to_unified("238222", &project.latest_files[0]);
        assert_eq!(version.id, "4593548");
        assert_eq!(version.cf_mod_id, Some(238222));
        assert_eq!(version.cf_file_id, Some(4593548));
        let file = version.primary_file().unwrap();
        assert_eq!(file.filename, "all-the-mods-9.zip");
        assert_eq!(file.size, 1234);
        assert_eq!(file.hashes.sha1.as_deref(), Some("ABCDEF"));
    }

    #[test]
    fn search_response_maps_to_hits() {
        let project: CfProject = serde_json::from_str(
            r#"{
                "id": 238222,
                "name": "All The Mods 9",
                "slug": "all-the-mods-9",
                "summary": "一个整合包",
                "downloadCount": 10,
                "classId": 4471,
                "authors": [{"id": 1, "name": "作者"}],
                "categories": [{"id": 4, "name": "Adventure", "slug": "adventure"}]
            }"#,
        )
        .unwrap();
        let hit = project_to_hit(&project, ContentKind::Modpack);
        assert_eq!(hit.project_id, "238222");
        assert_eq!(hit.author, "作者");
        assert_eq!(hit.cf_id, Some(238222));
        assert_eq!(hit.categories, vec!["adventure".to_string()]);
    }

    #[test]
    fn single_project_response_has_object_data() {
        // 单个项目的 data 是对象，列表的 data 才是数组
        let response: CfValue<CfProject> = serde_json::from_str(
            r#"{"data": {"id": 238222, "name": "整合包", "slug": "pack", "summary": "说明"}}"#,
        )
        .unwrap();
        assert_eq!(response.data.id, 238222);
        assert_eq!(response.data.name, "整合包");
    }

    #[test]
    fn list_response_has_pagination() {
        let response: CfList<CfProject> = serde_json::from_str(
            r#"{"data": [{"id": 1, "name": "甲"}], "pagination": {"index": 0, "pageSize": 24, "resultCount": 1, "totalCount": 320}}"#,
        )
        .unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.pagination.map(|p| p.total_count), Some(320));
    }

    #[test]
    fn hashes_accept_object_or_array() {
        let array_form: CfFile = serde_json::from_str(
            r#"{"id": 1, "modId": 2, "fileName": "a.jar", "hashes": [{"value": "AA", "algo": 1}]}"#,
        )
        .unwrap();
        assert_eq!(sha1_of(&array_form).as_deref(), Some("AA"));

        let object_form: CfFile = serde_json::from_str(
            r#"{"id": 1, "modId": 2, "fileName": "a.jar", "hashes": {"value": "BB", "algo": 1}}"#,
        )
        .unwrap();
        assert_eq!(sha1_of(&object_form).as_deref(), Some("BB"));

        let md5_form: CfFile = serde_json::from_str(
            r#"{"id": 1, "modId": 2, "fileName": "a.jar", "hashes": [{"value": "CC", "algo": 2}]}"#,
        )
        .unwrap();
        assert_eq!(sha1_of(&md5_form), None);
    }

    #[test]
    fn links_accept_issue_url_aliases() {
        let with_issues: CfProject = serde_json::from_str(
            r#"{"id": 1, "name": "甲", "links": {"issuesUrl": "https://example.com/issues"}}"#,
        )
        .unwrap();
        assert_eq!(
            with_issues.links.issue_tracker_url.as_deref(),
            Some("https://example.com/issues")
        );

        let with_tracker: CfProject = serde_json::from_str(
            r#"{"id": 1, "name": "甲", "links": {"issueTrackerUrl": "https://example.com/bugs"}}"#,
        )
        .unwrap();
        assert_eq!(
            with_tracker.links.issue_tracker_url.as_deref(),
            Some("https://example.com/bugs")
        );
    }

    /// MurmurHash2 的参考实现
    ///
    /// 照着 Austin Appleby 的 C 版抄的，用来核对算法本身对不对，
    /// 免得换依赖的时候算出来的东西对不上 CurseForge。
    fn reference_murmur2(data: &[u8], seed: u32) -> u32 {
        const M: u32 = 0x5bd1e995;
        const R: u32 = 24;
        let mut h = seed ^ (data.len() as u32);
        let mut chunks = data.chunks_exact(4);
        for c in &mut chunks {
            let mut k = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
            k = k.wrapping_mul(M);
            k ^= k >> R;
            k = k.wrapping_mul(M);
            h = h.wrapping_mul(M);
            h ^= k;
        }
        let tail = chunks.remainder();
        match tail.len() {
            3 => {
                h ^= (tail[2] as u32) << 16;
                h ^= (tail[1] as u32) << 8;
                h ^= tail[0] as u32;
                h = h.wrapping_mul(M);
            }
            2 => {
                h ^= (tail[1] as u32) << 8;
                h ^= tail[0] as u32;
                h = h.wrapping_mul(M);
            }
            1 => {
                h ^= tail[0] as u32;
                h = h.wrapping_mul(M);
            }
            _ => {}
        }
        h ^= h >> 13;
        h = h.wrapping_mul(M);
        h ^= h >> 15;
        h
    }

    #[test]
    fn murmur_matches_reference_implementation() {
        // 各种长度都过一遍，尾巴不足 4 字节的情况也要对
        for len in 0..64usize {
            let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(37).wrapping_add(11)).collect();
            assert_eq!(
                murmur2::murmur2(&data, 1),
                reference_murmur2(&data, 1),
                "长度 {len} 对不上"
            );
        }
    }

    #[test]
    fn fingerprint_ignores_whitespace_bytes() {
        // 制表符、换行、回车、空格都不参与计算
        let base = b"hello world";
        let padded = b"h e l l o\tworld\r\n";
        assert_eq!(fingerprint_of(base), fingerprint_of(padded));

        // 别的字节还是要算进去
        assert_ne!(fingerprint_of(b"hello world"), fingerprint_of(b"hello_world"));
    }

    #[test]
    fn fingerprint_matches_filtered_reference() {
        let data: Vec<u8> = (0..97u32).map(|i| i as u8).collect();
        let filtered: Vec<u8> = data
            .iter()
            .copied()
            .filter(|b| !matches!(b, 9 | 10 | 13 | 32))
            .collect();
        assert_eq!(fingerprint_of(&data), reference_murmur2(&filtered, 1));
    }

    #[test]
    fn game_versions_split_loaders_from_versions() {
        // CurseForge 把加载器名也塞在 gameVersions 里
        let (mc, loaders) = split_game_versions(&[
            "Fabric".to_string(),
            "Client".to_string(),
            "1.20.1".to_string(),
            "Quilt".to_string(),
            "26.3".to_string(),
        ]);
        assert_eq!(mc, vec!["1.20.1".to_string(), "26.3".to_string()]);
        assert_eq!(loaders, vec!["Fabric".to_string(), "Quilt".to_string()]);

        // 没有加载器时就只有版本
        let (mc, loaders) = split_game_versions(&["1.20.1".to_string()]);
        assert_eq!(mc, vec!["1.20.1".to_string()]);
        assert!(loaders.is_empty());
    }

    #[test]
    fn search_query_uses_offset_not_page() {
        // index 是偏移量，翻页时要往后挪一整页
        let query = search_query(ContentKind::Mod, "sodium", Some("1.20.1"), Some("fabric"), 50, 25);
        assert!(query.contains("index=50"), "{query}");
        assert!(query.contains("pageSize=25"), "{query}");
        assert!(query.contains("classId=6"), "{query}");
        // 版本号里的点会被转义成 %2E
        assert!(query.contains("gameVersion=1%2E20%2E1"), "{query}");
        assert!(query.contains("modLoaderType=4"), "{query}");
        assert!(query.contains("searchFilter=sodium"), "{query}");
    }

    #[test]
    fn fingerprint_response_is_parsed() {        let response: CfFingerprintResponse = serde_json::from_str(
            r#"{
                "data": {
                    "exactMatches": [
                        {
                            "id": 238222,
                            "file": {"id": 4593548, "modId": 238222, "fileFingerprint": 123456}
                        }
                    ],
                    "partialMatches": []
                }
            }"#,
        )
        .unwrap();
        assert_eq!(response.data.exact_matches.len(), 1);
        let hit = &response.data.exact_matches[0];
        assert_eq!(hit.id, 238222);
        let file = hit.file.as_ref().unwrap();
        assert_eq!(file.file_fingerprint, 123456);
        assert_eq!(file.mod_id, 238222);
    }
}
