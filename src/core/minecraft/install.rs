use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
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
) -> Result<InstallReport> {
    let resolved = version::resolve(game_dir, id)?;
    let mut report = InstallReport::default();
    paths::ensure_dir(&resolved.version_dir)?;
    paths::ensure_dir(&resolved.libraries_dir)?;
    paths::ensure_dir(&resolved.natives_dir)?;

    // ---------- 第一阶段：主 jar、依赖库、natives、资源索引、日志配置 ----------
    let mut core_tasks: Vec<FileTask> = Vec::new();
    let mut core_total: u64 = 0;

    if resolved.jar_path.is_file() {
        core_total += std::fs::metadata(&resolved.jar_path)
            .map(|m| m.len())
            .unwrap_or(0);
    } else if let Some(client) = resolved
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

    let index_path = resolved.json.asset_index.as_ref().map(|index| {
        resolved
            .assets_dir
            .join("indexes")
            .join(format!("{}.json", index.id))
    });
    if let (Some(index), Some(path)) = (&resolved.json.asset_index, &index_path) {
        if !path.is_file() {
            if let Some(url) = &index.url {
                let size = index.size.unwrap_or(0);
                core_total += size;
                core_tasks.push(
                    FileTask::new(url.clone(), path.clone())
                        .with_size(size)
                        .with_optional_sha1(index.sha1.clone()),
                );
            }
        }
    }

    if let Some(client) = resolved
        .json
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
    {
        if let Some(url) = &client.file.url {
            let dest = resolved
                .assets_dir
                .join("log_configs")
                .join(&client.file.id);
            if !dest.is_file() {
                let size = client.file.size.unwrap_or(0);
                core_total += size;
                core_tasks.push(
                    FileTask::new(url.clone(), dest)
                        .with_size(size)
                        .with_optional_sha1(client.file.sha1.clone()),
                );
            }
        }
    }

    progress.total.store(core_total, Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    progress.set_message(format!("正在下载 {} 个依赖文件 ...", core_tasks.len()));
    dl.download_batch(core_tasks, 8, progress)?;

    if progress.is_cancelled() {
        return Err(anyhow!("已取消"));
    }

    // ---------- 第二阶段：资源文件（允许部分失败） ----------
    let mut virtual_assets: Option<(String, PathBuf)> = None;
    if let (Some(index), Some(path)) = (&resolved.json.asset_index, &index_path) {
        let objects_dir = resolved.assets_dir.join("objects");
        match std::fs::read(path) {
            Ok(data) => match serde_json::from_slice::<AssetIndexFile>(&data) {
                Ok(parsed) => {
                    let mut tasks: Vec<FileTask> = Vec::new();
                    let mut total = 0u64;
                    for object in parsed.objects.values() {
                        if object.hash.len() < 2 {
                            continue;
                        }
                        let (prefix, rest) = object.hash.split_at(2);
                        let dest = objects_dir.join(prefix).join(rest);
                        if dest.is_file() {
                            continue;
                        }
                        total += object.size;
                        tasks.push(
                            FileTask::new(format!("{RESOURCES_BASE}/{prefix}/{rest}"), dest)
                                .with_size(object.size)
                                .with_optional_sha1(Some(object.hash.clone())),
                        );
                    }
                    report.total_assets = parsed.objects.len();
                    if !tasks.is_empty() {
                        progress.set_message(format!("正在下载资源文件（{} 个）...", tasks.len()));
                        progress.total.store(total, Ordering::Relaxed);
                        progress.done.store(0, Ordering::Relaxed);
                        let failed = dl.download_batch_lenient(tasks, 16, progress);
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

    // ---------- 第三阶段：解压 natives ----------
    progress.set_message("正在解压本地库 ...");
    for (jar, exclude) in &native_libs {
        if jar.is_file() {
            let _ = extract_natives(jar, &resolved.natives_dir, exclude.as_deref());
        }
    }

    // ---------- 第四阶段：旧版本虚拟资源目录 ----------
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
                        let (prefix, rest) = object.hash.split_at(2);
                        let src = objects_dir.join(prefix).join(rest);
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
        "版本 {id} 安装完成（缺失资源 {} / {}）",
        report.missing_assets,
        report.total_assets
    );
    Ok(report)
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
