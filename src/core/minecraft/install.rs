use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::core::download::{DownloadProgress, Downloader, FileTask};
use crate::core::minecraft::manifest::VersionEntry;
use crate::core::minecraft::version::{self, VersionJson};
use crate::core::paths;

const RESOURCES_BASE: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
struct AssetIndexFile {
    #[serde(default)]
    objects: HashMap<String, AssetObject>,
    #[serde(default)]
    #[serde(rename = "map_to_resources")]
    map_to_resources: bool,
    #[serde(default)]
    #[serde(rename = "virtual")]
    is_virtual: bool,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    #[serde(default)]
    size: u64,
}

/// 安装结果：资源文件允许缺失（可稍后补齐），依赖库与主 jar 不允许
#[derive(Debug, Clone, Default)]
pub struct InstallReport {
    pub missing_assets: usize,
    pub total_assets: usize,
}

impl InstallReport {
    pub fn is_complete(&self) -> bool {
        self.missing_assets == 0
    }

    pub fn summary(&self) -> String {
        if self.is_complete() {
            "安装完成".to_string()
        } else {
            format!(
                "安装完成，但有 {} / {} 个资源文件下载失败，可稍后重新安装补齐",
                self.missing_assets, self.total_assets
            )
        }
    }
}

/// 安装选项：并发数与是否下载资源文件
#[derive(Debug, Clone, Copy)]
pub struct InstallOptions {
    /// 并发下载线程数
    pub threads: usize,
    /// 是否下载资源文件（音效、语言、材质等，占下载量的大头）
    pub install_assets: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            threads: 8,
            install_assets: true,
        }
    }
}

/// 下载并保存版本的 JSON 描述文件
pub fn ensure_version_json(
    dl: &Downloader,
    game_dir: &Path,
    entry: &VersionEntry,
) -> Result<VersionJson> {
    let path = version::version_json_path(game_dir, &entry.id);
    if path.is_file() {
        if let Ok(json) = version::read_local_json(game_dir, &entry.id) {
            return Ok(json);
        }
    }
    let json: VersionJson = dl
        .get_json(&entry.url)
        .with_context(|| format!("下载版本描述失败: {}", entry.id))?;
    version::write_local_json(game_dir, &json)?;
    Ok(json)
}

/// 写入加载器生成的版本 JSON（带 inheritsFrom）
pub fn write_loader_json(game_dir: &Path, json: &VersionJson) -> Result<()> {
    version::write_local_json(game_dir, json)
}

/// 完整安装一个版本（客户端、依赖库、资源文件、natives）
pub fn install_version(
    dl: &Downloader,
    game_dir: &Path,
    id: &str,
    progress: &Arc<DownloadProgress>,
    opts: InstallOptions,
) -> Result<InstallReport> {
    install_core(dl, game_dir, id, progress, opts)?;
    install_assets(dl, game_dir, id, progress, opts)
}

/// 第一阶段：主 jar、依赖库、natives、资源索引、日志配置
///
/// 进度采用累加语义，不会覆盖同一任务里其它阶段已经计入的字节，
/// 因此加载器安装可以与本阶段并行执行。
pub fn install_core(
    dl: &Downloader,
    game_dir: &Path,
    id: &str,
    progress: &Arc<DownloadProgress>,
    opts: InstallOptions,
) -> Result<()> {
    let resolved = version::resolve(game_dir, id)?;
    paths::ensure_dir(&resolved.version_dir)?;
    paths::ensure_dir(&resolved.libraries_dir)?;
    paths::ensure_dir(&resolved.natives_dir)?;

    // ---------- 主 jar、依赖库、资源索引、日志配置 ----------
    let mut core_tasks: Vec<FileTask> = Vec::new();
    let mut core_total: u64 = 0;

    // 客户端 jar：交给统一下载逻辑判断是否已存在且校验通过
    if let Some(client) = resolved
        .json
        .downloads
        .as_ref()
        .and_then(|d| d.client.as_ref())
    {
        if let Some(url) = &client.url {
            let size = client.size.unwrap_or(0);
            core_total += size;
            core_tasks.push(
                FileTask::new(url.clone(), resolved.jar_path.clone())
                    .with_size(size)
                    .with_optional_sha1(client.sha1.clone()),
            );
        }
    }

    let mut native_libs: Vec<(PathBuf, Option<Vec<String>>)> = Vec::new();
    for lib in &resolved.json.libraries {
        if let Some((rel, artifact)) = version::library_artifact(lib) {
            if let Some(url) = &artifact.url {
                let dest = resolved
                    .libraries_dir
                    .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                let size = artifact.size.unwrap_or(0);
                core_total += size;
                core_tasks.push(
                    FileTask::new(url.clone(), dest)
                        .with_size(size)
                        .with_optional_sha1(artifact.sha1.clone()),
                );
            }
        }
        if let Some((rel, artifact)) = version::library_native(lib) {
            if let Some(url) = &artifact.url {
                let dest = resolved
                    .libraries_dir
                    .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                let size = artifact.size.unwrap_or(0);
                core_total += size;
                core_tasks.push(
                    FileTask::new(url.clone(), dest.clone())
                        .with_size(size)
                        .with_optional_sha1(artifact.sha1.clone()),
                );
                let exclude = lib.extract.as_ref().map(|e| e.exclude.clone());
                native_libs.push((dest, exclude));
            }
        }
    }

    if let Some(index) = &resolved.json.asset_index {
        let path = resolved
            .assets_dir
            .join("indexes")
            .join(format!("{}.json", index.id));
        if let Some(url) = &index.url {
            let size = index.size.unwrap_or(0);
            core_total += size;
            core_tasks.push(
                FileTask::new(url.clone(), path)
                    .with_size(size)
                    .with_optional_sha1(index.sha1.clone()),
            );
        }
    }

    if let Some(client) = resolved
        .json
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
    {
        if let Some(url) = &client.file.url {
            let dest = resolved.assets_dir.join("log_configs").join(&client.file.id);
            let size = client.file.size.unwrap_or(0);
            core_total += size;
            core_tasks.push(
                FileTask::new(url.clone(), dest)
                    .with_size(size)
                    .with_optional_sha1(client.file.sha1.clone()),
            );
        }
    }

    progress.add_total(core_total);
    progress.set_message(format!("正在下载 {} 个依赖文件 ...", core_tasks.len()));
    dl.download_batch(core_tasks, opts.threads, progress)?;

    if progress.is_cancelled() {
        return Err(anyhow!("已取消"));
    }

    // ---------- 解压 natives ----------
    progress.set_message("正在解压本地库 ...");
    for (jar, exclude) in &native_libs {
        if jar.is_file() {
            let _ = extract_natives(jar, &resolved.natives_dir, exclude.as_deref());
        }
    }

    log::info!("版本 {id} 核心文件安装完成");
    Ok(())
}

/// 第二阶段：资源文件（允许部分失败）
pub fn install_assets(
    dl: &Downloader,
    game_dir: &Path,
    id: &str,
    progress: &Arc<DownloadProgress>,
    opts: InstallOptions,
) -> Result<InstallReport> {
    let mut report = InstallReport::default();
    if !opts.install_assets {
        log::info!("版本 {id} 已跳过资源文件下载");
        return Ok(report);
    }

    let resolved = version::resolve(game_dir, id)?;
    let mut virtual_assets: Option<(String, PathBuf)> = None;

    if let Some(index) = &resolved.json.asset_index {
        let path = resolved
            .assets_dir
            .join("indexes")
            .join(format!("{}.json", index.id));
        let objects_dir = resolved.assets_dir.join("objects");
        match std::fs::read(&path) {
            Ok(data) => match serde_json::from_slice::<AssetIndexFile>(&data) {
                Ok(parsed) => {
                    let mut tasks: Vec<FileTask> = Vec::new();
                    let mut total = 0u64;
                    for object in parsed.objects.values() {
                        if object.hash.len() < 2 {
                            continue;
                        }
                        // 资源在 Mojang CDN 上的路径是「前两位目录 / 完整 hash 文件名」，
                        // 本地存放位置同样如此（游戏按这个规则查找）
                        let prefix = &object.hash[..2];
                        let dest = objects_dir.join(prefix).join(&object.hash);
                        if dest.is_file() {
                            continue;
                        }
                        total += object.size;
                        tasks.push(
                            FileTask::new(
                                format!("{RESOURCES_BASE}/{prefix}/{}", object.hash),
                                dest,
                            )
                            .with_size(object.size)
                            .with_optional_sha1(Some(object.hash.clone())),
                        );
                    }
                    report.total_assets = parsed.objects.len();
                    if !tasks.is_empty() {
                        progress.set_message(format!("正在下载资源文件（{} 个）...", tasks.len()));
                        progress.add_total(total);
                        let failed = dl.download_batch_lenient(tasks, opts.threads, progress);
                        report.missing_assets = failed.len();
                        if !failed.is_empty() {
                            log::warn!("{} 个资源文件下载失败（可稍后重试）", failed.len());
                        }
                    }
                    if parsed.is_virtual || parsed.map_to_resources {
                        virtual_assets = Some((index.id.clone(), objects_dir.clone()));
                    }
                }
                Err(err) => log::warn!("资源索引解析失败: {err}"),
            },
            Err(err) => log::warn!("资源索引读取失败: {err}"),
        }
    }

    if progress.is_cancelled() {
        return Err(anyhow!("已取消"));
    }

    // ---------- 旧版本虚拟资源目录 ----------
    if let Some((index_id, objects_dir)) = virtual_assets {
        let virtual_dir = resolved.assets_dir.join("virtual").join(&index_id);
        if !virtual_dir.is_dir() {
            progress.set_message("正在整理旧版资源目录 ...");
            if let Ok(data) = std::fs::read(
                resolved
                    .assets_dir
                    .join("indexes")
                    .join(format!("{index_id}.json")),
            ) {
                if let Ok(parsed) = serde_json::from_slice::<AssetIndexFile>(&data) {
                    for (name, object) in &parsed.objects {
                        if object.hash.len() < 2 {
                            continue;
                        }
                        let prefix = &object.hash[..2];
                        let src = objects_dir.join(prefix).join(&object.hash);
                        let dest = virtual_dir.join(name);
                        if src.is_file() && !dest.is_file() {
                            let _ = paths::ensure_parent(&dest);
                            let _ = std::fs::copy(&src, &dest);
                        }
                    }
                }
            }
        }
    }

    log::info!(
        "版本 {id} 资源安装完成（缺失资源 {} / {}）",
        report.missing_assets,
        report.total_assets
    );
    Ok(report)
}

/// 只安装版本自身声明的依赖库（不包含继承链上原版的那些）
///
/// 加载器版本通过 inheritsFrom 继承原版，若按合并后的版本安装，
/// 原版的客户端 jar 与依赖库会被重复下载一次。这里只取版本自身的库，
/// 让加载器可以与原版并行安装而不产生重复流量。
pub fn install_own_libraries(
    dl: &Downloader,
    game_dir: &Path,
    id: &str,
    progress: &Arc<DownloadProgress>,
    opts: InstallOptions,
) -> Result<()> {
    let json = version::read_local_json(game_dir, id)?;
    let libraries_dir = paths::libraries_dir(game_dir);
    paths::ensure_dir(&libraries_dir)?;

    let mut tasks: Vec<FileTask> = Vec::new();
    let mut total = 0u64;
    let mut native_libs: Vec<(PathBuf, Option<Vec<String>>)> = Vec::new();

    for lib in &json.libraries {
        if let Some((rel, artifact)) = version::library_artifact(lib) {
            if let Some(url) = &artifact.url {
                let dest = libraries_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                let size = artifact.size.unwrap_or(0);
                total += size;
                tasks.push(
                    FileTask::new(url.clone(), dest)
                        .with_size(size)
                        .with_optional_sha1(artifact.sha1.clone()),
                );
            }
        }
        if let Some((rel, artifact)) = version::library_native(lib) {
            if let Some(url) = &artifact.url {
                let dest = libraries_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                let size = artifact.size.unwrap_or(0);
                total += size;
                tasks.push(
                    FileTask::new(url.clone(), dest.clone())
                        .with_size(size)
                        .with_optional_sha1(artifact.sha1.clone()),
                );
                let exclude = lib.extract.as_ref().map(|e| e.exclude.clone());
                native_libs.push((dest, exclude));
            }
        }
    }

    if tasks.is_empty() {
        // 仍要继承原版的本地库，否则老版本（如 1.7.10 + Forge）会缺少 natives
        extract_inherited_natives(game_dir, id);
        return Ok(());
    }

    progress.add_total(total);
    progress.set_message(format!("正在下载加载器依赖（{} 个）...", tasks.len()));
    dl.download_batch(tasks, opts.threads, progress)?;

    if progress.is_cancelled() {
        return Err(anyhow!("已取消"));
    }

    let natives_dir = version::version_dir(game_dir, id).join("natives");
    for (jar, exclude) in &native_libs {
        if jar.is_file() {
            let _ = extract_natives(jar, &natives_dir, exclude.as_deref());
        }
    }
    extract_inherited_natives(game_dir, id);
    Ok(())
}

/// 把继承链上的本地库解压到该版本的 natives 目录
///
/// 加载器版本通过 inheritsFrom 继承原版，老版本（LWJGL 2）的本地库必须解压到
/// 加载器版本自己的 natives 目录，否则启动时找不到 dll。
fn extract_inherited_natives(game_dir: &Path, id: &str) {
    let Ok(resolved) = version::resolve(game_dir, id) else {
        return;
    };
    let natives_dir = version::version_dir(game_dir, id).join("natives");
    let _ = paths::ensure_dir(&natives_dir);
    for lib in &resolved.json.libraries {
        if let Some((rel, _)) = version::library_native(lib) {
            let jar = resolved
                .libraries_dir
                .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if jar.is_file() {
                let exclude = lib.extract.as_ref().map(|e| e.exclude.clone());
                let _ = extract_natives(&jar, &natives_dir, exclude.as_deref());
            }
        }
    }
}

/// 解压 native 库到指定目录，跳过 META-INF 等排除项
pub fn extract_natives(jar: &Path, dest: &Path, exclude: Option<&[String]>) -> Result<()> {
    paths::ensure_dir(dest)?;
    let file =
        std::fs::File::open(jar).with_context(|| format!("打开本地库失败: {}", jar.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("解析本地库失败: {}", jar.display()))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(name) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let name_str = name.to_string_lossy().replace('\\', "/");
        if entry.is_dir() {
            continue;
        }
        if name_str.starts_with("META-INF/") {
            continue;
        }
        if let Some(excludes) = exclude {
            if excludes.iter().any(|e| name_str.starts_with(e.as_str())) {
                continue;
            }
        }
        let out = dest.join(&name);
        paths::ensure_parent(&out)?;
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        std::fs::write(&out, data)?;
    }
    Ok(())
}

/// 判断版本是否已完整安装（客户端 jar + 版本 json 存在）
pub fn is_version_ready(game_dir: &Path, id: &str) -> bool {
    match version::resolve(game_dir, id) {
        Ok(resolved) => resolved.jar_path.is_file() || resolved.json.inherits_from.is_some(),
        Err(_) => false,
    }
}
