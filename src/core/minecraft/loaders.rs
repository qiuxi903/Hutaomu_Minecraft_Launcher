use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::core::download::{DownloadProgress, Downloader, FileTask};
use crate::core::java::JavaInfo;
use crate::core::minecraft::{install, version, VersionJson};
use crate::core::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderKind {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl LoaderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Fabric => "轻量、更新快，适合绝大多数模组",
            Self::Quilt => "Fabric 的分支，兼容大部分 Fabric 模组",
            Self::Forge => "老牌加载器，模组生态庞大",
            Self::NeoForge => "Forge 分支，1.20.2 之后的主流选择",
        }
    }

    pub fn all() -> [LoaderKind; 4] {
        [Self::Fabric, Self::Quilt, Self::Forge, Self::NeoForge]
    }

    /// 配置文件中使用的稳定标识
    pub fn id(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    /// 由配置标识还原加载器类型
    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_lowercase().as_str() {
            "fabric" => Some(Self::Fabric),
            "quilt" => Some(Self::Quilt),
            "forge" => Some(Self::Forge),
            "neoforge" | "neo_forge" | "neo-forge" => Some(Self::NeoForge),
            _ => None,
        }
    }
}

/// 加载器版本选择策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderPolicy {
    /// 推荐稳定版
    Stable,
    /// 最新版
    Latest,
    /// 每次由用户手动选择
    Ask,
}

impl LoaderPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => "推荐稳定版",
            Self::Latest => "最新版",
            Self::Ask => "每次询问",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Latest => "latest",
            Self::Ask => "ask",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id.trim().to_lowercase().as_str() {
            "latest" => Self::Latest,
            "ask" => Self::Ask,
            _ => Self::Stable,
        }
    }

    pub fn all() -> [LoaderPolicy; 3] {
        [Self::Stable, Self::Latest, Self::Ask]
    }
}

/// 按策略从版本列表中挑选一个版本
///
/// 列表约定按「新到旧」排列，因此第一个即为最新版本。
pub fn pick_version(list: &[LoaderVersion], policy: LoaderPolicy) -> Option<String> {
    if list.is_empty() {
        return None;
    }
    match policy {
        LoaderPolicy::Stable => list
            .iter()
            .find(|v| v.stable)
            .or_else(|| list.first())
            .map(|v| v.version.clone()),
        LoaderPolicy::Latest => list.first().map(|v| v.version.clone()),
        LoaderPolicy::Ask => None,
    }
}

#[derive(Debug, Clone)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderInfo,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderInfo {
    version: String,
    #[serde(default)]
    stable: bool,
}

/// 列出某个 Minecraft 版本可用的加载器版本
pub fn list_versions(
    dl: &Downloader,
    kind: LoaderKind,
    mc_version: &str,
) -> Result<Vec<LoaderVersion>> {
    match kind {
        LoaderKind::Fabric => {
            let url = dl
                .source
                .fabric_meta(&format!("/v2/versions/loader/{mc_version}"));
            let list: Vec<FabricLoaderEntry> = dl.get_json_direct(&url)?;
            Ok(list
                .into_iter()
                .map(|e| LoaderVersion {
                    version: e.loader.version,
                    stable: e.loader.stable,
                })
                .collect())
        }
        LoaderKind::Quilt => {
            let url = dl
                .source
                .quilt_meta(&format!("/v3/versions/loader/{mc_version}"));
            let list: Vec<FabricLoaderEntry> = dl.get_json_direct(&url)?;
            Ok(list
                .into_iter()
                .map(|e| LoaderVersion {
                    version: e.loader.version,
                    stable: e.loader.stable,
                })
                .collect())
        }
        LoaderKind::Forge => {
            let url = dl
                .source
                .forge_maven("net/minecraftforge/forge/maven-metadata.xml");
            let xml = String::from_utf8_lossy(&dl.get_bytes_direct(&url)?).to_string();
            let prefix = format!("{mc_version}-");
            let mut versions: Vec<LoaderVersion> = extract_xml_versions(&xml)
                .into_iter()
                .filter_map(|v| v.strip_prefix(&prefix).map(|s| s.to_string()))
                .map(|version| LoaderVersion {
                    version,
                    stable: false,
                })
                .collect();
            versions.reverse();
            Ok(versions)
        }
        LoaderKind::NeoForge => {
            let (url, prefix) = neoforge_endpoint(dl, mc_version);
            let xml = String::from_utf8_lossy(&dl.get_bytes_direct(&url)?).to_string();
            let mut versions: Vec<LoaderVersion> = extract_xml_versions(&xml)
                .into_iter()
                .filter_map(|v| {
                    let stripped = v.strip_prefix(&prefix)?.to_string();
                    Some(LoaderVersion {
                        version: stripped,
                        stable: false,
                    })
                })
                .collect();
            versions.reverse();
            Ok(versions)
        }
    }
}

/// NeoForge 在 1.20.1 使用 net/neoforged/forge，之后使用 net/neoforged/neoforge
fn neoforge_endpoint(dl: &Downloader, mc_version: &str) -> (String, String) {
    if mc_version == "1.20.1" {
        (
            dl.source
                .neoforge_maven("net/neoforged/forge/maven-metadata.xml"),
            "1.20.1-".to_string(),
        )
    } else {
        (
            dl.source
                .neoforge_maven("net/neoforged/neoforge/maven-metadata.xml"),
            String::new(),
        )
    }
}

fn extract_xml_versions(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + 9..];
        let Some(end) = after.find("</version>") else {
            break;
        };
        out.push(after[..end].trim().to_string());
        rest = &after[end + 10..];
    }
    out
}

/// 安装加载器，返回安装后的版本 id
pub fn install(
    dl: &Downloader,
    kind: LoaderKind,
    mc_version: &str,
    loader_version: &str,
    game_dir: &Path,
    java: &JavaInfo,
    progress: &Arc<DownloadProgress>,
    opts: install::InstallOptions,
) -> Result<String> {
    match kind {
        LoaderKind::Fabric => install_profile_json(
            dl,
            game_dir,
            &dl.source.fabric_meta(&format!(
                "/v2/versions/loader/{mc_version}/{loader_version}/profile/json"
            )),
            progress,
            opts,
        ),
        LoaderKind::Quilt => install_profile_json(
            dl,
            game_dir,
            &dl.source.quilt_meta(&format!(
                "/v3/versions/loader/{mc_version}/{loader_version}/profile/json"
            )),
            progress,
            opts,
        ),
        LoaderKind::Forge => {
            let name = format!("{mc_version}-forge-{loader_version}");
            let path = format!("net/minecraftforge/forge/{name}/forge-{name}-installer.jar");
            let url = dl.source.forge_maven(&path);
            run_installer(dl, &url, game_dir, java, progress, &name)
        }
        LoaderKind::NeoForge => {
            let (path, expected) = if mc_version == "1.20.1" {
                let name = format!("1.20.1-forge-{loader_version}");
                (
                    format!("net/neoforged/forge/{name}/forge-{name}-installer.jar"),
                    name,
                )
            } else {
                (
                    format!(
                        "net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
                    ),
                    format!("neoforge-{loader_version}"),
                )
            };
            let url = dl.source.neoforge_maven(&path);
            run_installer(dl, &url, game_dir, java, progress, &expected)
        }
    }
}

fn install_profile_json(
    dl: &Downloader,
    game_dir: &Path,
    url: &str,
    progress: &Arc<DownloadProgress>,
    opts: install::InstallOptions,
) -> Result<String> {
    progress.set_message("正在获取加载器信息 ...");
    let json: VersionJson = dl
        .get_json_direct(url)
        .with_context(|| format!("获取加载器描述失败: {url}"))?;
    let id = json.id.clone();
    if id.is_empty() {
        return Err(anyhow!("加载器描述缺少版本号"));
    }
    version::write_local_json(game_dir, &json)?;
    let report = install::install_version(dl, game_dir, &id, progress, opts)?;
    log::info!("加载器 {id} 安装完成：{}", report.summary());
    Ok(id)
}

/// 只安装加载器自身的依赖库，不重复下载继承自原版的文件
///
/// 原版的客户端 jar 与依赖库由另一条安装流程负责，这里仅处理加载器
/// 自己在版本 JSON 中声明的库，从而让两者可以并行执行。
pub fn install_own(
    dl: &Downloader,
    kind: LoaderKind,
    mc_version: &str,
    loader_version: &str,
    game_dir: &Path,
    progress: &Arc<DownloadProgress>,
    opts: install::InstallOptions,
) -> Result<String> {
    let url = match kind {
        LoaderKind::Fabric => dl.source.fabric_meta(&format!(
            "/v2/versions/loader/{mc_version}/{loader_version}/profile/json"
        )),
        LoaderKind::Quilt => dl.source.quilt_meta(&format!(
            "/v3/versions/loader/{mc_version}/{loader_version}/profile/json"
        )),
        _ => return Err(anyhow!("{} 不支持按自身依赖方式安装", kind.label())),
    };

    progress.set_message(format!("正在获取 {} 加载器信息 ...", kind.label()));
    let json: VersionJson = dl
        .get_json_direct(&url)
        .with_context(|| format!("获取加载器描述失败: {url}"))?;
    let id = json.id.clone();
    if id.is_empty() {
        return Err(anyhow!("加载器描述缺少版本号"));
    }
    version::write_local_json(game_dir, &json)?;
    install::install_own_libraries(dl, game_dir, &id, progress, opts)?;
    log::info!("加载器 {id} 安装完成");
    Ok(id)
}

/// 下载并运行 Forge / NeoForge 安装器
fn run_installer(
    dl: &Downloader,
    url: &str,
    game_dir: &Path,
    java: &JavaInfo,
    progress: &Arc<DownloadProgress>,
    expected_id: &str,
) -> Result<String> {
    paths::ensure_dir(game_dir)?;
    let cache = paths::cache_dir().join("installers");
    paths::ensure_dir(&cache)?;
    let file_name = url
        .rsplit('/')
        .next()
        .unwrap_or("installer.jar")
        .to_string();
    let jar = cache.join(&file_name);

    if !jar.is_file() {
        progress.set_message("正在下载安装器 ...");
        progress.add_total(1);
        dl.download(&FileTask::new(url, jar.clone()), progress)
            .with_context(|| format!("下载安装器失败: {url}"))?;
    }

    progress.set_message("正在运行安装器（可能需要几分钟）...");
    let mut cmd = Command::new(&java.path);
    cmd.arg("-jar")
        .arg(&jar)
        .arg("--installClient")
        .arg(game_dir);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.current_dir(game_dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().context("启动安装器失败")?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let progress_out = progress.clone();
    let handle_out = stdout.map(|s| std::thread::spawn(move || stream_lines(s, progress_out)));
    let progress_err = progress.clone();
    let handle_err = stderr.map(|s| std::thread::spawn(move || stream_lines(s, progress_err)));

    let status = child.wait().context("等待安装器结束失败")?;
    if let Some(h) = handle_out {
        let _ = h.join();
    }
    if let Some(h) = handle_err {
        let _ = h.join();
    }

    if !status.success() {
        return Err(anyhow!(
            "安装器执行失败（退出码 {:?}），请检查网络或改用其他 Java",
            status.code()
        ));
    }

    let versions = version::installed_versions(game_dir);
    if versions.iter().any(|v| v == expected_id) {
        return Ok(expected_id.to_string());
    }
    let keyword = expected_id
        .to_lowercase()
        .replace("neoforge-", "")
        .replace("forge-", "");
    if let Some(found) = versions
        .iter()
        .find(|v| {
            let lower = v.to_lowercase();
            lower.contains("forge") && lower.contains(&keyword)
        })
    {
        return Ok(found.clone());
    }
    versions
        .iter()
        .find(|v| v.to_lowercase().contains("forge"))
        .cloned()
        .ok_or_else(|| anyhow!("安装器已结束，但未找到生成的版本目录"))
}

fn stream_lines<R: std::io::Read + Send + 'static>(stream: R, progress: Arc<DownloadProgress>) {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(stream);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        log::debug!("[installer] {trimmed}");
        let short: String = trimmed.chars().take(120).collect();
        progress.set_message(short);
    }
}

/// 该 Minecraft 版本可用的加载器（部分加载器不支持全部版本）
pub fn supports(kind: LoaderKind, mc_version: &str) -> bool {
    let parts: Vec<u32> = mc_version
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect();
    if parts.len() < 2 {
        return false;
    }
    // 2026 起 Mojang 改用 26.x 这类版本号，全部视为现代版本
    if parts[0] != 1 {
        return true;
    }
    let minor = parts[1];
    let patch = parts.get(2).copied().unwrap_or(0);
    match kind {
        LoaderKind::Fabric => minor >= 14,
        LoaderKind::Quilt => minor >= 14,
        LoaderKind::Forge => minor >= 1,
        LoaderKind::NeoForge => (minor == 20 && patch >= 1) || minor >= 21,
    }
}

pub fn loader_version_dir(game_dir: &Path, id: &str) -> PathBuf {
    version::version_dir(game_dir, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(version: &str, stable: bool) -> LoaderVersion {
        LoaderVersion {
            version: version.to_string(),
            stable,
        }
    }

    /// 列表按「新到旧」排列
    #[test]
    fn pick_stable_prefers_marked_version() {
        let list = vec![v("0.16.9", false), v("0.16.8", true)];
        assert_eq!(
            pick_version(&list, LoaderPolicy::Stable).as_deref(),
            Some("0.16.8")
        );
    }

    #[test]
    fn pick_stable_falls_back_to_latest() {
        let list = vec![v("0.16.9", false)];
        assert_eq!(
            pick_version(&list, LoaderPolicy::Stable).as_deref(),
            Some("0.16.9")
        );
    }

    #[test]
    fn pick_latest_takes_first_entry() {
        let list = vec![v("0.16.9", false), v("0.16.8", true)];
        assert_eq!(
            pick_version(&list, LoaderPolicy::Latest).as_deref(),
            Some("0.16.9")
        );
    }

    #[test]
    fn pick_ask_leaves_choice_to_user() {
        let list = vec![v("0.16.9", true)];
        assert_eq!(pick_version(&list, LoaderPolicy::Ask), None);
    }

    #[test]
    fn pick_on_empty_list_returns_none() {
        assert_eq!(pick_version(&[], LoaderPolicy::Stable), None);
    }

    #[test]
    fn loader_kind_id_roundtrip() {
        for kind in LoaderKind::all() {
            assert_eq!(LoaderKind::from_id(kind.id()), Some(kind));
        }
        assert_eq!(LoaderKind::from_id("none"), None);
        assert_eq!(LoaderKind::from_id(""), None);
    }

    #[test]
    fn loader_policy_id_roundtrip() {
        for policy in LoaderPolicy::all() {
            assert_eq!(LoaderPolicy::from_id(policy.id()), policy);
        }
        // 未知值回落到推荐稳定版
        assert_eq!(LoaderPolicy::from_id("unknown"), LoaderPolicy::Stable);
    }
}
