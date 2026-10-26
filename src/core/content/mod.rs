//! 内容来源平台
//!
//! Modrinth 与 CurseForge 两边的接口形状不一样，
//! 但界面只关心「一个项目有哪些版本、能装到哪儿」，
//! 所以这里把两个平台都转成同一套结构（[`modrinth::Project`] 等），
//! 界面按当前平台选一路走。

pub mod curseforge;
pub mod mcmod;
pub mod modfeature;
pub mod modmeta;
pub mod modpack;
pub mod modrinth;
pub mod translate;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::core::download::{DownloadProgress, Downloader};

pub use modrinth::ContentKind;

/// 内容平台
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
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

    /// 平台的一句话说明，摆在平台切换旁边
    pub fn hint(self) -> &'static str {
        match self {
            Self::Modrinth => "开放平台，不用登录，模组最全",
            Self::CurseForge => "老牌平台，整合包与光影很多，需要 API Key",
        }
    }
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
            modrinth::search(dl, kind, query, mc_version, loader, offset, limit)
        }
        ContentSource::CurseForge => {
            curseforge::search(dl, kind, query, mc_version, loader, offset, limit)
        }
    }
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
