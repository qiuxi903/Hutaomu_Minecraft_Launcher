//! 统一安装管线：一次任务里把游戏本体与加载器一起装好。
//!
//! 与旧流程的区别：
//! - 旧流程要先到「游戏」页装本体，再到「加载器」页单独装加载器，两次独立操作。
//! - 本模块把两者合成一次任务，��载器可以在游戏下载过程中并行安装。
//!
//! 并行安全性：
//! - Fabric / Quilt 只下载自身声明的依赖库，与原版库路径不同，可以从一开始就并行。
//! - Forge / NeoForge 需要运行官方安装器，安装器会自行处理原版文件，
//!   因此必须等原版核心文件就位后再启动，但可以与资源文件下载并行。

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::core::download::{DownloadProgress, Downloader};
use crate::core::java::JavaInfo;
use crate::core::minecraft::install::{self, InstallOptions};
use crate::core::minecraft::loaders::{self, LoaderKind, LoaderPolicy};
use crate::core::minecraft::manifest::VersionEntry;

/// 加载器安装时机
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderTiming {
    /// 与游戏本体同时下载安装
    Parallel,
    /// 游戏下载完成后再安装
    After,
    /// 不自动安装，需要到「下载 - 加载器」页手动触发
    Manual,
}

impl LoaderTiming {
    pub fn label(self) -> &'static str {
        match self {
            Self::Parallel => "与游戏同时",
            Self::After => "下载完成后",
            Self::Manual => "不自动安装",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Parallel => "parallel",
            Self::After => "after",
            Self::Manual => "manual",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id.trim().to_lowercase().as_str() {
            "after" => Self::After,
            "manual" => Self::Manual,
            _ => Self::Parallel,
        }
    }

    pub fn all() -> [LoaderTiming; 3] {
        [Self::Parallel, Self::After, Self::Manual]
    }
}

/// 一次安装任务的完整参数
#[derive(Debug, Clone)]
pub struct InstallRequest {
    /// Minecraft 版本号
    pub mc_version: String,
    /// 该版本在清单中的条目（用于取版本 JSON 地址）
    pub entry: VersionEntry,
    /// 需要安装的加载器，None 表示只装原版
    pub loader: Option<LoaderKind>,
    /// 指定的加载器版本，None 表示按策略自动挑选
    pub loader_version: Option<String>,
    /// 自动挑选加载器版本时的策略
    pub loader_policy: LoaderPolicy,
    /// 加载器安装时机
    pub timing: LoaderTiming,
    /// 游戏目录
    pub game_dir: PathBuf,
    /// 运行 Forge / NeoForge 安装器所需的 Java
    pub java: Option<JavaInfo>,
    /// 下载选项
    pub opts: InstallOptions,
}

/// 安装结果
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    /// 原版版本 id
    pub game_id: String,
    /// 加载器版本 id（未安装加载器时为 None）
    pub loader_id: Option<String>,
    /// 资源文件安装报告
    pub report: install::InstallReport,
}

/// 执行一次「游戏 + 加载器」的完整安装
pub fn install_bundle(
    dl: &Downloader,
    req: &InstallRequest,
    progress: &Arc<DownloadProgress>,
) -> Result<InstallOutcome> {
    // 1. 先落盘原版版本 JSON：加载器的 inheritsFrom 依赖它
    install::ensure_version_json(dl, &req.game_dir, &req.entry)?;

    // 2. 确定加载器版本
    let (kind, loader_version) = match req.loader {
        None => (None, None),
        Some(kind) => {
            let version = match &req.loader_version {
                Some(v) if !v.trim().is_empty() => v.trim().to_string(),
                _ => {
                    let list = loaders::list_versions(dl, kind, &req.mc_version)
                        .map_err(|e| anyhow!("获取 {} 加载器版本失败: {e}", kind.label()))?;
                    loaders::pick_version(&list, req.loader_policy).ok_or_else(|| {
                        anyhow!("没有找到适用于 Minecraft {} 的 {} 加载器", req.mc_version, kind.label())
                    })?
                }
            };
            (Some(kind), Some(version))
        }
    };

    // 3. 按时机执行
    let Some(kind) = kind else {
        let report = install::install_version(dl, &req.game_dir, &req.mc_version, progress, req.opts)?;
        return Ok(InstallOutcome {
            game_id: req.mc_version.clone(),
            loader_id: None,
            report,
        });
    };

    let loader_version = loader_version
        .ok_or_else(|| anyhow!("没有找到适用于 Minecraft {} 的 {} 加载器", req.mc_version, kind.label()))?;
    install_with_loader(dl, req, kind, &loader_version, progress)
}

/// 带加载器的安装
fn install_with_loader(
    dl: &Downloader,
    req: &InstallRequest,
    kind: LoaderKind,
    loader_version: &str,
    progress: &Arc<DownloadProgress>,
) -> Result<InstallOutcome> {
    // 手动选择加载器视为明确请求，此时不再受「不自动安装」限制
    let timing = match req.timing {
        LoaderTiming::Manual => LoaderTiming::After,
        other => other,
    };

    // Forge / NeoForge 要运行官方安装器，安装器会自行处理原版文件，
    // 必须等原版核心文件就位后才能启动。
    let needs_vanilla_first = matches!(kind, LoaderKind::Forge | LoaderKind::NeoForge);

    if timing == LoaderTiming::Parallel && !needs_vanilla_first {
        // Fabric / Quilt：加载器依赖与原版核心文件同时下载
        let handle = spawn_loader_install(dl, req, kind, loader_version, progress);
        let report = install::install_version(
            dl,
            &req.game_dir,
            &req.mc_version,
            progress,
            req.opts,
        )?;
        let loader_id = join_loader_install(handle)?;
        return Ok(InstallOutcome {
            game_id: req.mc_version.clone(),
            loader_id: Some(loader_id),
            report,
        });
    }

    // 先装原版核心文件（客户端 jar、依赖库、natives、资源索引）
    install::install_core(dl, &req.game_dir, &req.mc_version, progress, req.opts)?;

    if timing == LoaderTiming::Parallel {
        // 原版核心就绪后，加载器与资源文件并行
        let handle = spawn_loader_install(dl, req, kind, loader_version, progress);
        let report = install::install_assets(dl, &req.game_dir, &req.mc_version, progress, req.opts)?;
        let loader_id = join_loader_install(handle)?;
        return Ok(InstallOutcome {
            game_id: req.mc_version.clone(),
            loader_id: Some(loader_id),
            report,
        });
    }

    // 顺序安装：原版全部完成后再装加载器
    let report = install::install_assets(dl, &req.game_dir, &req.mc_version, progress, req.opts)?;
    let loader_id = install_loader_now(dl, req, kind, loader_version, progress)?;
    Ok(InstallOutcome {
        game_id: req.mc_version.clone(),
        loader_id: Some(loader_id),
        report,
    })
}

/// 在后台线程安装加载器
fn spawn_loader_install(
    dl: &Downloader,
    req: &InstallRequest,
    kind: LoaderKind,
    loader_version: &str,
    progress: &Arc<DownloadProgress>,
) -> std::thread::JoinHandle<Result<String>> {
    let dl = dl.clone();
    let req = req.clone();
    let loader_version = loader_version.to_string();
    let progress = progress.clone();
    std::thread::spawn(move || install_loader_now(&dl, &req, kind, &loader_version, &progress))
}

/// 等待加载器安装线程结束
fn join_loader_install(handle: std::thread::JoinHandle<Result<String>>) -> Result<String> {
    match handle.join() {
        Ok(result) => result,
        Err(_) => Err(anyhow!("加载器安装线程异常退出")),
    }
}

/// 实际执行加载器安装
fn install_loader_now(
    dl: &Downloader,
    req: &InstallRequest,
    kind: LoaderKind,
    loader_version: &str,
    progress: &Arc<DownloadProgress>,
) -> Result<String> {
    match kind {
        LoaderKind::Fabric | LoaderKind::Quilt => {
            // 只需要加载器自身的依赖库，原版的由另一条线程负责
            loaders::install_own(
                dl,
                kind,
                &req.mc_version,
                loader_version,
                &req.game_dir,
                progress,
                req.opts,
            )
        }
        LoaderKind::Forge | LoaderKind::NeoForge => {
            let java = req
                .java
                .as_ref()
                .ok_or_else(|| anyhow!("没有找到可用的 Java，运行安装器需要 Java 8 及以上版本"))?;
            loaders::install(
                dl,
                kind,
                &req.mc_version,
                loader_version,
                &req.game_dir,
                java,
                progress,
                req.opts,
            )
        }
    }
}
