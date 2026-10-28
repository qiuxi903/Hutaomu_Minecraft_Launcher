//! 内容更新
//!
//! 把本地装着的模组、光影、资源包换成线上的最新版本：
//! 查最新版 → 比对 → 下载 → 换掉旧文件 → 删掉旧的那份。
//!
//! 「已经是最新的」用文件的 sha1 判定，比对文件名准得多：
//! 同一个版本号的包偶尔会被作者重新打包，文件名没变，内容却变了。
//! 线上没给 sha1 时退回来比大小。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::core::content::modrinth::{ContentKind, Version};
use crate::core::content::{ContentSource, DownloadProgress, Downloader};
use crate::core::download;

/// 一次更新的目标
#[derive(Debug, Clone)]
pub struct Target {
    /// 内容分类
    pub kind: ContentKind,
    /// 平台
    pub source: ContentSource,
    /// 平台项目标识：Modrinth 是 slug，CurseForge 是数字编号
    pub project_id: String,
    /// 本地文件名
    pub file_name: String,
    /// 本地文件路径
    pub path: PathBuf,
}

/// 更新的结果
#[derive(Debug, Clone)]
pub struct Outcome {
    /// 更新后的文件路径
    pub path: PathBuf,
    /// 线上版本号
    pub version: String,
    /// 换下来的旧文件
    pub old_path: Option<PathBuf>,
    /// 是不是真的换了新文件
    pub updated: bool,
}

/// 查线上最新版
///
/// 按当前实例的 MC 版本与加载器筛，筛不到就放宽条件再来一次：
/// 有些包没在版本列表里写全游戏版本，卡太死就永远更新不了。
pub fn latest(
    dl: &Downloader,
    source: ContentSource,
    project_id: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Version> {
    let filtered = crate::core::content::versions(dl, source, project_id, mc_version, loader)?;
    if let Some(version) = filtered.into_iter().next() {
        return Ok(version);
    }
    let loose = crate::core::content::versions(dl, source, project_id, None, None)?;
    loose
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("线上还没有这个内容的可用版本"))
}

/// 本地这个文件是不是已经是线上那个版本
fn is_current(path: &Path, version: &Version) -> bool {
    let Some(file) = version.primary_file() else {
        return false;
    };
    if !path.is_file() {
        return false;
    }
    // 有 sha1 就比 sha1，最准
    if let Some(sha1) = file
        .hashes
        .sha1
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if let Ok(bytes) = std::fs::read(path) {
            return download::sha1_hex(&bytes).eq_ignore_ascii_case(sha1);
        }
    }
    // 线上没给校验值就比大小
    let local = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    file.size > 0 && local == file.size
}

/// 更新一个内容
///
/// 已经是最新版就直接返回，不下载。
/// 换了新版本时，旧文件会被删掉——两个版本同时躺在目录里，
/// 游戏会把它们都加载一遍。
pub fn update(
    dl: &Downloader,
    target: &Target,
    game_dir: &Path,
    mc_version: Option<&str>,
    loader: Option<&str>,
    progress: &Arc<DownloadProgress>,
) -> Result<Outcome> {
    let version = latest(dl, target.source, &target.project_id, mc_version, loader)?;
    if is_current(&target.path, &version) {
        return Ok(Outcome {
            path: target.path.clone(),
            version: version.version_number.clone(),
            old_path: None,
            updated: false,
        });
    }

    let file = version
        .primary_file()
        .ok_or_else(|| anyhow!("这个版本没有可下载的文件"))?;
    let dest = crate::core::content::install(
        dl,
        target.source,
        target.kind,
        &version,
        game_dir,
        progress,
    )?;

    // 换下来的旧文件删掉
    let mut old_path = None;
    if target.path != dest && target.path.is_file() {
        if let Err(err) = std::fs::remove_file(&target.path) {
            log::warn!("旧文件删除失败 {}: {err}", target.path.display());
        } else {
            old_path = Some(target.path.clone());
        }
    }

    Ok(Outcome {
        path: dest,
        version: version.version_number.clone(),
        old_path,
        updated: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::content::modrinth::VersionFile;

    fn version_with(name: &str, size: u64) -> Version {
        Version {
            id: "v1".into(),
            project_id: "demo".into(),
            name: name.into(),
            version_number: "1.0.0".into(),
            version_type: "release".into(),
            game_versions: vec![],
            loaders: vec![],
            date_published: String::new(),
            downloads: 0,
            files: vec![VersionFile {
                url: "https://example.invalid/a.jar".into(),
                filename: name.into(),
                primary: true,
                size,
                hashes: Default::default(),
                cf_mod_id: None,
                cf_file_id: None,
            }],
            dependencies: vec![],
            cf_mod_id: None,
            cf_file_id: None,
        }
    }

    #[test]
    fn 同一个文件就是最新版() {
        let dir = std::env::temp_dir().join("htmcl-update-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("a.jar");
        std::fs::write(&path, b"hello").unwrap();
        let version = version_with("a.jar", 5);
        assert!(is_current(&path, &version));
        // 大小对不上就不是最新版
        let other = version_with("a.jar", 9);
        assert!(!is_current(&path, &other));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn 文件不在就不是最新版() {
        let version = version_with("a.jar", 5);
        assert!(!is_current(
            &std::env::temp_dir().join("htmcl-update-not-exist.jar"),
            &version
        ));
    }
}
