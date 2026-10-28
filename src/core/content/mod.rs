//! 内容来源平台
//!
//! Modrinth 与 CurseForge 两边的接口形状不一样，
//! 但界面只关心「一个项目有哪些版本、能装到哪儿」，
//! 所以这里把两个平台都转成同一套结构（[`modrinth::Project`] 等），
//! 界面按当前平台选一路走。

pub mod cache;
pub mod curseforge;
pub mod mcmod;
pub mod modfeature;
pub mod modmeta;
pub mod modpack;
pub mod modrinth;
pub mod translate;
pub mod update;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::core::download::{DownloadProgress, Downloader};

pub use modrinth::ContentKind;
pub use modrinth::RemoteProject;

/// 内容平台
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContentSource {
    /// Modrinth 开放平台，不需要 API Key
    #[default]
    Modrinth,
    /// CurseForge 官方平台，需要 API Key
    CurseForge,
}

impl ContentSource {
    pub fn all() -> [ContentSource; 2] {
        [Self::Modrinth, Self::CurseForge]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Modrinth => "Modrinth",
            Self::CurseForge => "CurseForge",
        }
    }

    /// 平台标识，写进整合包标记里
    pub fn id(self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::CurseForge => "curseforge",
        }
    }

    /// 按标识还原平台，认不出来时按 Modrinth 处理
    pub fn from_id(id: &str) -> Self {
        match id.trim().to_lowercase().as_str() {
            "curseforge" | "cf" => Self::CurseForge,
            _ => Self::Modrinth,
        }
    }

    /// 这个平台收不收这类内容
    ///
    /// Modrinth 不收数据包，也不收世界。数据包那个筛选它会直接忽略，
    /// 返回一堆不相干的模组；世界则是老老实实返回 0 条。
    /// 所以这两类内容只给 CurseForge。
    pub fn supports(self, kind: ContentKind) -> bool {
        match (self, kind) {
            (ContentSource::Modrinth, ContentKind::DataPack | ContentKind::World) => false,
            _ => true,
        }
    }
}

/// 某个分类能在哪些平台上搜
///
/// 顺序固定，界面上照这个顺序摆平台选项。
pub fn sources_for(kind: ContentKind) -> Vec<ContentSource> {
    ContentSource::all()
        .into_iter()
        .filter(|source| source.supports(kind))
        .collect()
}

/// 按平台搜索内容
pub fn search(
    dl: &Downloader,
    source: ContentSource,
    kind: ContentKind,
    query: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
    offset: u32,
    limit: u32,
) -> Result<modrinth::SearchResponse> {
    match source {
        ContentSource::Modrinth => {
            // Modrinth 不收录数据包，别拿不存在的分类去搜
            if kind == ContentKind::DataPack {
                return Err(anyhow::anyhow!(
                    "Modrinth 没有数据包分类，切到 CurseForge 平台就能搜"
                ));
            }
            modrinth::search(dl, kind, query, mc_version, loader, offset, limit)
        }
        ContentSource::CurseForge => {
            curseforge::search(dl, kind, query, mc_version, loader, offset, limit)
        }
    }
}

/// 双平台搜索的结果
#[derive(Debug, Default)]
pub struct UnifiedSearch {
    /// 去重合并后的结果，按下载量从多到少排
    pub hits: Vec<modrinth::SearchHit>,
    /// 合并后的总条数
    pub total: u32,
    /// 有没有哪一路走了社区镜像
    pub via_mirror: bool,
    /// 哪一路失败了（键是平台，值是原因）
    pub errors: Vec<(ContentSource, String)>,
}

/// 双平台一起搜，结果去重合并
///
/// 两个平台各搜一遍，同一个项目只留一条：
/// 先用 MCMod 的对照表配对（它记着同一个模组在两边的 slug），
/// 没有对照的再按「标题与作者都一样」兜底。
/// 两边都有时优先用 `preferred` 那边的名字、图标与版本。
pub fn search_unified(
    dl: &Downloader,
    kind: ContentKind,
    query: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
    limit: u32,
    preferred: ContentSource,
    mcmod: &mcmod::Database,
) -> UnifiedSearch {
    let platforms = sources_for(kind);
    let mut results: Vec<(ContentSource, Result<modrinth::SearchResponse, String>)> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = platforms
            .iter()
            .map(|&source| {
                let dl = dl.clone();
                let query = query.to_string();
                let mc = mc_version.map(|s| s.to_string());
                let loader = loader.map(|s| s.to_string());
                scope.spawn(move || {
                    let result = search(
                        &dl,
                        source,
                        kind,
                        &query,
                        mc.as_deref(),
                        loader.as_deref(),
                        0,
                        limit,
                    );
                    (source, result.map_err(|e| format!("{e:#}")))
                })
            })
            .collect();
        for handle in handles {
            if let Ok(pair) = handle.join() {
                results.push(pair);
            }
        }
    });

    let mut unified = UnifiedSearch::default();
    let mut by_platform: HashMap<ContentSource, Vec<modrinth::SearchHit>> = HashMap::new();
    for (source, result) in results {
        match result {
            Ok(response) => {
                if response.via_mirror {
                    unified.via_mirror = true;
                }
                by_platform.insert(source, response.hits);
            }
            Err(err) => {
                log::warn!("{} 搜索失败: {err}", source.label());
                unified.errors.push((source, err));
            }
        }
    }

    let first = by_platform.remove(&preferred).unwrap_or_default();
    let second: Vec<modrinth::SearchHit> =
        by_platform.values().flat_map(|hits| hits.clone()).collect();
    unified.hits = merge_hits(&first, &second, mcmod);
    unified.total = unified.hits.len() as u32;
    unified
}

/// 把两路搜索结果去重合并
///
/// 判断两边是不是同一个项目：
/// 1. MCMod 对照表（curseforge_slug ↔ modrinth_slug）；
/// 2. 标题与作者归一化后都相同（两边的作者都非空时才比作者）。
///
/// 配上对的：`preferred` 那边做底，补上另一边的平台项目编号；
/// 没配上的各自保留。结果按下载量从多到少排。
fn merge_hits(
    preferred: &[modrinth::SearchHit],
    other: &[modrinth::SearchHit],
    db: &mcmod::Database,
) -> Vec<modrinth::SearchHit> {
    let mut merged: Vec<modrinth::SearchHit> = Vec::with_capacity(preferred.len() + other.len());
    let mut taken_other = vec![false; other.len()];

    for mut hit in preferred.to_vec() {
        // 在另一边找同一个项目
        let partner = other.iter().enumerate().position(|(index, candidate)| {
            if taken_other[index] {
                return false;
            }
            if same_project(&db, &hit, candidate) {
                return true;
            }
            false
        });
        if let Some(index) = partner {
            taken_other[index] = true;
            let mate = &other[index];
            // 两边的平台编号都留住，装哪个平台由详情页决定
            if hit.cf_id.is_none() {
                hit.cf_id = mate.cf_id;
            }
        }
        merged.push(hit);
    }
    for (index, hit) in other.iter().enumerate() {
        if !taken_other[index] {
            merged.push(hit.clone());
        }
    }
    merged.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    merged
}

/// 两边的搜索结果是不是同一个项目
fn same_project(
    db: &crate::core::content::mcmod::Database,
    a: &modrinth::SearchHit,
    b: &modrinth::SearchHit,
) -> bool {
    if a.source == b.source {
        return false;
    }
    // MCMod 对照表最可靠
    let (mr_slug, cf_slug) = if a.source == ContentSource::Modrinth {
        (&a.slug, &b.slug)
    } else {
        (&b.slug, &a.slug)
    };
    if let Some(entry) = db.lookup_modrinth(mr_slug) {
        if !entry.curseforge_slug.is_empty() && entry.curseforge_slug.eq_ignore_ascii_case(cf_slug)
        {
            return true;
        }
    }
    // 兜底：标题与作者都一样
    if normalize_title(&a.title) == normalize_title(&b.title) && !a.title.is_empty() {
        let (author_a, author_b) = (&a.author, &b.author);
        if author_a.trim().is_empty() || author_b.trim().is_empty() {
            return true;
        }
        if author_a.trim().eq_ignore_ascii_case(author_b.trim()) {
            return true;
        }
    }
    false
}

/// 标题归一化：小写、只留字母与数字
fn normalize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 项目详情
pub fn project(
    dl: &Downloader,
    source: ContentSource,
    kind: ContentKind,
    id: &str,
) -> Result<modrinth::Project> {
    match source {
        ContentSource::Modrinth => modrinth::project(dl, id),
        ContentSource::CurseForge => curseforge::project(dl, id, kind),
    }
}

/// 项目的可安装版本
pub fn versions(
    dl: &Downloader,
    source: ContentSource,
    project_id: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<modrinth::Version>> {
    match source {
        ContentSource::Modrinth => modrinth::versions(dl, project_id, mc_version, loader),
        ContentSource::CurseForge => curseforge::versions(dl, project_id, mc_version, loader),
    }
}

/// 把内容文件装到实例里
pub fn install(
    dl: &Downloader,
    source: ContentSource,
    kind: ContentKind,
    version: &modrinth::Version,
    game_dir: &Path,
    progress: &Arc<DownloadProgress>,
) -> Result<PathBuf> {
    match source {
        ContentSource::Modrinth => modrinth::install(dl, kind, version, game_dir, progress),
        ContentSource::CurseForge => curseforge::install(dl, kind, version, game_dir, progress),
    }
}

/// 下载并解压整合包
pub fn fetch_modpack(
    dl: &Downloader,
    source: ContentSource,
    version: &modrinth::Version,
    progress: &Arc<DownloadProgress>,
) -> Result<(PathBuf, modrinth::ModpackDeps)> {
    match source {
        ContentSource::Modrinth => modrinth::fetch_modpack(dl, version, progress),
        ContentSource::CurseForge => curseforge::fetch_modpack(dl, version, progress),
    }
}

/// 把整合包里的文件铺到实例目录
pub fn install_modpack_files(
    dl: &Downloader,
    source: ContentSource,
    extract_dir: &Path,
    target_dir: &Path,
    progress: &Arc<DownloadProgress>,
) -> Result<()> {
    match source {
        ContentSource::Modrinth => modrinth::install_modpack_files(dl, extract_dir, target_dir, progress),
        ContentSource::CurseForge => {
            curseforge::install_modpack_files(dl, extract_dir, target_dir, progress)
        }
    }
}

/// 前置依赖
///
/// Modrinth 的依赖直接写在版本数据里，CurseForge 要单独问接口。
pub fn required_dependencies(
    dl: &Downloader,
    source: ContentSource,
    version: &modrinth::Version,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Vec<modrinth::Version> {
    match source {
        ContentSource::Modrinth => {
            modrinth::required_dependencies(dl, version, mc_version, loader)
        }
        ContentSource::CurseForge => {
            let (Some(mod_id), Some(file_id)) = (version.cf_mod_id, version.cf_file_id) else {
                return Vec::new();
            };
            let deps = match curseforge::dependencies(dl, mod_id, file_id) {
                Ok(deps) => deps,
                Err(err) => {
                    log::warn!("获取 CurseForge 依赖失败: {err}");
                    return Vec::new();
                }
            };
            let mut out = Vec::new();
            for dep in deps {
                if dep.relation_type != curseforge::RELATION_REQUIRED {
                    continue;
                }
                match curseforge::versions(dl, &dep.mod_id.to_string(), mc_version, loader) {
                    Ok(list) => {
                        // 取最新那个
                        if let Some(found) = list.into_iter().next() {
                            out.push(found);
                        }
                    }
                    Err(err) => {
                        log::warn!("获取 CurseForge 依赖版本失败: {err}");
                    }
                }
            }
            out
        }
    }
}

/// 项目主页地址
pub fn project_url(source: ContentSource, kind: ContentKind, project: &modrinth::Project) -> String {
    match source {
        ContentSource::Modrinth => modrinth::project_url(kind.facet(), &project.slug),
        ContentSource::CurseForge => curseforge::project_url(kind, &project.slug),
    }
}

/// 用文件指纹反查 CurseForge 项目
///
/// CurseForge 给每个文件算了一个指纹，
/// 本地装着的模组拿指纹去问接口，就能知道它在 CurseForge 上属于哪个项目。
///
/// 返回的是「文件名 → 线上项目」，项目里带着 slug 与图标地址。
pub fn curseforge_projects(
    dl: &Downloader,
    files: &[(String, PathBuf)],
) -> Result<std::collections::HashMap<String, RemoteProject>> {
    let mut out = std::collections::HashMap::new();
    if files.is_empty() {
        return Ok(out);
    }
    // 先算指纹，一个文件算一次就够
    let fingerprints = compute_fingerprints(files);
    let mut owners: Vec<(String, u32)> = Vec::new();
    let mut list: Vec<u32> = Vec::new();
    for ((name, _), fingerprint) in files.iter().zip(fingerprints) {
        if let Some(fingerprint) = fingerprint {
            owners.push((name.clone(), fingerprint));
            list.push(fingerprint);
        }
    }
    if list.is_empty() {
        return Ok(out);
    }

    // 指纹换项目编号
    let hits = curseforge::fingerprints(dl, &list)?;
    let mut project_of: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for (fingerprint, project_id) in hits {
        project_of.insert(fingerprint, project_id);
    }
    if project_of.is_empty() {
        return Ok(out);
    }

    // 项目编号换项目资料，项目页地址与图标都要用它
    let mut ids: Vec<u32> = project_of.values().copied().collect();
    ids.sort_unstable();
    ids.dedup();
    let projects: std::collections::HashMap<u32, RemoteProject> =
        curseforge::projects_by_ids(dl, &ids)?.into_iter().collect();

    for (name, fingerprint) in owners {
        let Some(project_id) = project_of.get(&fingerprint) else {
            continue;
        };
        if let Some(project) = projects.get(project_id) {
            out.insert(name, project.clone());
        }
    }
    Ok(out)
}

/// 并行算文件的指纹
///
/// 算指纹要把整个文件读一遍，模组多了很费时间，
/// 分几路同时算能快不少。
fn compute_fingerprints(files: &[(String, PathBuf)]) -> Vec<Option<u32>> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 8);
    if threads <= 1 || files.len() < 4 {
        return files
            .iter()
            .map(|(_, path)| curseforge::file_fingerprint(path))
            .collect();
    }
    let chunk_size = files.len().div_ceil(threads);
    let mut out: Vec<Option<u32>> = Vec::with_capacity(files.len());
    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|(_, path)| curseforge::file_fingerprint(path))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            match handle.join() {
                Ok(part) => out.extend(part),
                // 哪一路出错就当没算出来，不影响别的
                Err(_) => out.extend(std::iter::repeat_n(None, chunk_size)),
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 数据包只有_curseforge_收() {
        assert!(!ContentSource::Modrinth.supports(ContentKind::DataPack));
        assert!(ContentSource::CurseForge.supports(ContentKind::DataPack));
    }

    #[test]
    fn 常见分类两个平台都能搜() {
        for kind in [
            ContentKind::Mod,
            ContentKind::Shader,
            ContentKind::ResourcePack,
            ContentKind::Modpack,
        ] {
            assert_eq!(
                sources_for(kind).len(),
                2,
                "{} 应该两个平台都能搜",
                kind.label()
            );
        }
    }

    #[test]
    fn 每个分类至少留一个平台() {
        for kind in ContentKind::all() {
            assert!(
                !sources_for(kind).is_empty(),
                "{} 没有能搜的平台",
                kind.label()
            );
        }
    }

    fn hit(source: ContentSource, slug: &str, title: &str, downloads: u64) -> modrinth::SearchHit {
        modrinth::SearchHit {
            project_id: slug.to_string(),
            slug: slug.to_string(),
            title: title.to_string(),
            description: String::new(),
            author: "作者".to_string(),
            icon_url: None,
            downloads,
            follows: 0,
            categories: vec![],
            versions: vec![],
            project_type: "mod".to_string(),
            date_modified: String::new(),
            license: String::new(),
            gallery: vec![],
            cf_id: if source == ContentSource::CurseForge {
                Some(42)
            } else {
                None
            },
            source,
        }
    }

    #[test]
    fn 两边都有的项目只留一条() {
        let db = mcmod::Database::parse(
            "mcmod_id,curseforge_slug,modrinth_slug,name,subname,abbr
459,jei,jei,JEI,Just Enough Items,JEI
",
        );
        let mr = vec![hit(ContentSource::Modrinth, "jei", "Just Enough Items (JEI)", 100)];
        let cf = vec![hit(ContentSource::CurseForge, "jei", "Just Enough Items (JEI)", 90)];
        let merged = merge_hits(&mr, &cf, &db);
        assert_eq!(merged.len(), 1, "MCMod 对照表配上对的应该合并成一条");
        // 优先平台（这里是 Modrinth）的数据做底
        assert_eq!(merged[0].source, ContentSource::Modrinth);
        // 另一边的平台编号留了下来
        assert_eq!(merged[0].cf_id, Some(42));
    }

    #[test]
    fn 标题作者都一样的也算同一个项目() {
        let db = mcmod::Database::default();
        let mr = vec![hit(ContentSource::Modrinth, "some-mod", "Some Mod!", 100)];
        let cf = vec![hit(ContentSource::CurseForge, "some-mod-cf", "Some  Mod", 90)];
        let merged = merge_hits(&mr, &cf, &db);
        assert_eq!(merged.len(), 1, "标题与作者归一化后相同应该合并");
    }

    #[test]
    fn 不同项目各自保留() {
        let db = mcmod::Database::default();
        let mr = vec![hit(ContentSource::Modrinth, "aaa", "Alpha", 100)];
        let cf = vec![hit(ContentSource::CurseForge, "bbb", "Beta", 90)];
        let merged = merge_hits(&mr, &cf, &db);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn 合并结果按下载量排() {
        let db = mcmod::Database::default();
        let mr = vec![
            hit(ContentSource::Modrinth, "small", "Small", 10),
            hit(ContentSource::Modrinth, "big", "Big", 999),
        ];
        let cf = vec![hit(ContentSource::CurseForge, "mid", "Mid", 100)];
        let merged = merge_hits(&mr, &cf, &db);
        let downloads: Vec<u64> = merged.iter().map(|hit| hit.downloads).collect();
        let mut sorted = downloads.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(downloads, sorted, "应该按下载量从多到少排");
    }

    #[test]
    fn 标题归一化忽略大小写与符号() {
        assert_eq!(normalize_title("Some Mod!"), normalize_title("some  mod"));
        assert!(normalize_title("!!!").is_empty());
    }
}
