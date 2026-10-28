//! 内容更新
//!
//! 把本地装着的模组、光影、资源包换成线上的最新版本：
//! 查最新版 → 比对 → 下载 → 换掉旧文件 → 删掉旧的那份。
//!
//! 「已经是最新的」用文件的 sha1 判定，比对文件名准得多：
//! 同一个版本号的包偶尔会被作者重新打包，文件名没变，内容却变了。
//! 线上没给 sha1 时退回来比大小。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};

use crate::core::content::modrinth::{ContentKind, Version};
use crate::core::content::{ContentSource, DownloadProgress, Downloader};
use crate::core::download;

/// 查线上版本时的并发数
///
/// 版本查询是网络往返，一个一个问的话，一百多个模组要三十多秒。
/// 实测冷请求下并发 8 路能把单次从 340 毫秒压到 54 毫秒，
/// 再往上收益就平了（12 路、16 路只快一成）。
/// 别开太大：Modrinth 每分钟限 300 次请求，
/// CurseForge 的接口也容易被限流，之前批量查版本就撞过 403。
pub const CHECK_CONCURRENCY: usize = 8;

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

/// 一条内容查完的结果
#[derive(Debug, Clone)]
pub enum Checked {
    /// 已经是线上那个版本，不用动
    Current(Version),
    /// 线上有更新的版本，要下载替换
    Stale(Version),
    /// 查不到
    Failed(String),
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

/// 并行地检查一批内容
///
/// `check` 对每个目标跑一次，结果按传入顺序返回。
/// 用工作队列而不是平分块：某个目标查得慢时，别的线程能接着往下做，
/// 不用在原地等它。
pub fn check_many<F, T>(
    targets: &[Target],
    concurrency: usize,
    progress: &Arc<DownloadProgress>,
    check: F,
) -> Vec<T>
where
    F: Fn(&Target) -> T + Sync,
    T: Send,
{
    let total = targets.len();
    if total == 0 {
        return Vec::new();
    }
    let workers = concurrency.max(1).min(total);
    let cursor = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<T>>> = (0..total).map(|_| Mutex::new(None)).collect();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = cursor.fetch_add(1, Ordering::SeqCst);
                    if index >= total {
                        break;
                    }
                    let target = &targets[index];
                    let result = check(target);
                    if let Ok(mut slot) = slots[index].lock() {
                        *slot = Some(result);
                    }
                    let count = done.fetch_add(1, Ordering::SeqCst) + 1;
                    progress.set_message(format!(
                        "正在检查 {count}/{total} · {}",
                        target.file_name
                    ));
                }
            });
        }
    });

    slots
        .into_iter()
        .map(|slot| {
            let value = match slot.into_inner() {
                Ok(value) => value,
                Err(err) => err.into_inner(),
            };
            // 每个下标恰好被一个线程领走并写入，这里拿不到值说明代码有 bug
            value.expect("内容检查结果丢失")
        })
        .collect()
}

/// 并行查一批内容的线上最新版，并跟本地文件比对
///
/// 返回顺序跟传入顺序一致。
pub fn check_all(
    dl: &Downloader,
    targets: &[Target],
    mc_version: Option<&str>,
    loader: Option<&str>,
    concurrency: usize,
    progress: &Arc<DownloadProgress>,
) -> Vec<Checked> {
    check_many(targets, concurrency, progress, |target| {
        match latest(
            dl,
            target.source,
            &target.project_id,
            mc_version,
            loader,
        ) {
            Ok(version) => {
                if is_current(&target.path, &version) {
                    Checked::Current(version)
                } else {
                    Checked::Stale(version)
                }
            }
            Err(err) => Checked::Failed(err.to_string()),
        }
    })
}

/// 用已经查好的线上版本替换本地文件
///
/// 旧文件会被删掉——两个版本同时躺在目录里，游戏会把它们都加载一遍。
pub fn apply(
    dl: &Downloader,
    target: &Target,
    version: &Version,
    game_dir: &Path,
    progress: &Arc<DownloadProgress>,
) -> Result<Outcome> {
    if version.primary_file().is_none() {
        return Err(anyhow!("这个版本没有可下载的文件"));
    }
    let dest = crate::core::content::install(
        dl,
        target.source,
        target.kind,
        version,
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
    apply(dl, target, &version, game_dir, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::content::modrinth::VersionFile;
    use std::sync::atomic::AtomicUsize;

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

    fn target_with(name: &str) -> Target {
        Target {
            kind: ContentKind::Mod,
            source: ContentSource::Modrinth,
            project_id: name.to_string(),
            file_name: name.to_string(),
            path: PathBuf::from(name),
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

    #[test]
    fn 并发检查的结果按传入顺序返回() {
        let targets: Vec<Target> = (0..50).map(|i| target_with(&format!("m{i}.jar"))).collect();
        let progress = DownloadProgress::new();
        let results = check_many(&targets, 8, &progress, |target| {
            // 故意让前面的慢一点，好打乱完成顺序
            let n: u64 = target.file_name.trim_start_matches('m').trim_end_matches(".jar").parse().unwrap();
            std::thread::sleep(std::time::Duration::from_millis((50 - n) % 7));
            target.file_name.clone()
        });
        assert_eq!(results.len(), targets.len());
        for (target, result) in targets.iter().zip(results.iter()) {
            assert_eq!(*result, target.file_name);
        }
    }

    #[test]
    fn 并发检查每个目标都做一次() {
        let targets: Vec<Target> = (0..30).map(|i| target_with(&format!("m{i}.jar"))).collect();
        let progress = DownloadProgress::new();
        let calls = AtomicUsize::new(0);
        let results = check_many(&targets, 4, &progress, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            1usize
        });
        assert_eq!(calls.load(Ordering::SeqCst), targets.len());
        assert!(results.iter().all(|r| *r == 1));
    }

    #[test]
    fn 并发检查空列表不报错() {
        let progress = DownloadProgress::new();
        let targets: Vec<Target> = Vec::new();
        let results = check_many(&targets, 8, &progress, |_| 0usize);
        assert!(results.is_empty());
    }
}
