use std::cmp::Ordering;
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
                .filter_map(|v| {
                    let rest = v.strip_prefix(&prefix)?.trim().to_string();
                    if rest.is_empty() {
                        return None;
                    }
                    Some(LoaderVersion {
                        version: rest,
                        stable: false,
                    })
                })
                .collect();
            sort_versions_desc(&mut versions);
            Ok(versions)
        }
        LoaderKind::NeoForge => {
            // 1.20.1 用的是过渡产物 net.neoforged:forge，其余版本用 net.neoforged:neoforge
            let (url, artifact_prefix) = if mc_version == "1.20.1" {
                (
                    dl.source
                        .neoforge_maven("net/neoforged/forge/maven-metadata.xml"),
                    format!("{mc_version}-"),
                )
            } else {
                (
                    dl.source
                        .neoforge_maven("net/neoforged/neoforge/maven-metadata.xml"),
                    String::new(),
                )
            };
            let xml = String::from_utf8_lossy(&dl.get_bytes_direct(&url)?).to_string();
            Ok(filter_neoforge_versions(
                extract_xml_versions(&xml),
                mc_version,
                &artifact_prefix,
            ))
        }
    }
}

/// 从 Maven 元数据里挑出某个 Minecraft 版本可用的 NeoForge 构建
///
/// `artifact_prefix` 非空表示用的是 1.20.1 的过渡产物 net.neoforged:forge，
/// 版本号形如 `1.20.1-47.1.106`；否则用 net.neoforged:neoforge，
/// 版本号形如 `20.4.253`，同一个仓库里混着各个 Minecraft 版本的构建。
fn filter_neoforge_versions(
    raw: Vec<String>,
    mc_version: &str,
    artifact_prefix: &str,
) -> Vec<LoaderVersion> {
    let mc_prefix = if artifact_prefix.is_empty() {
        neoforge_version_prefix(mc_version)
    } else {
        None
    };
    let mut versions: Vec<LoaderVersion> = raw
        .into_iter()
        .filter_map(|v| {
            let v = v.trim().to_string();
            if artifact_prefix.is_empty() {
                // 必须按目标 Minecraft 版本过滤，否则自动选择会挑到给别的版本用的构建
                let prefix = mc_prefix.as_ref()?;
                if !v.starts_with(&format!("{prefix}.")) {
                    return None;
                }
                // 预发布版本（带 - 后缀）不参与选择
                if v.contains('-') {
                    return None;
                }
                Some(v)
            } else {
                // 过渡产物的版本号形如 1.20.1-47.1.106，
                // 先去掉前缀再看剩下的是不是预发布版本
                let rest = v.strip_prefix(artifact_prefix)?.trim().to_string();
                if rest.is_empty() || rest.contains('-') {
                    return None;
                }
                Some(rest)
            }
        })
        .map(|version| LoaderVersion {
            version,
            stable: false,
        })
        .collect();
    sort_versions_desc(&mut versions);
    versions
}

/// NeoForge 版本号的前两段对应 Minecraft 的次版本与补丁号
///
/// 1.20.4 → `20.4`，1.21 → `21.0`，1.21.1 → `21.1`，26.1 → `26.1`
fn neoforge_version_prefix(mc_version: &str) -> Option<String> {
    let rest = mc_version.strip_prefix("1.").unwrap_or(mc_version);
    let mut parts = rest.split('.');
    let major = parts.next()?.trim();
    if major.is_empty() {
        return None;
    }
    match parts.next().map(|s| s.trim()) {
        Some(minor) if !minor.is_empty() => Some(format!("{major}.{minor}")),
        _ => Some(format!("{major}.0")),
    }
}

/// 版本号按数字逐段比较，从大到小排序
fn sort_versions_desc(versions: &mut [LoaderVersion]) {
    versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
}

/// 比较两个版本号
///
/// 逐段比较：数字段按数值比，数字段大于非数字段（预发布），
/// 段数多的一方更大，除非多出来的都是非数字段（此时它是预发布版本，更小）。
fn compare_versions(a: &str, b: &str) -> Ordering {
    let sa: Vec<&str> = a.split(['.', '-']).filter(|s| !s.is_empty()).collect();
    let sb: Vec<&str> = b.split(['.', '-']).filter(|s| !s.is_empty()).collect();

    let common = sa.len().min(sb.len());
    for i in 0..common {
        let x = sa[i];
        let y = sb[i];
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(p), Ok(q)) => p.cmp(&q),
            (Ok(_), Err(_)) => Ordering::Greater,
            (Err(_), Ok(_)) => Ordering::Less,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }

    if sa.len() == sb.len() {
        return Ordering::Equal;
    }
    // 段数不同：多出来的那一段如果是预发布标记，说明带后缀的其实是更旧的版本
    let longer_is_prerelease = if sa.len() > sb.len() {
        sa[common..].iter().all(|s| s.parse::<u64>().is_err())
    } else {
        sb[common..].iter().all(|s| s.parse::<u64>().is_err())
    };
    let sa_longer = sa.len() > sb.len();
    match (longer_is_prerelease, sa_longer) {
        (true, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Greater,
        (false, false) => Ordering::Less,
    }
}

/// 加载器安装器在 Maven 仓库中的坐标
///
/// 安装器文件名里用的是「Maven 版本号」，与安装完成后 versions 目录里的
/// 版本 id 并不相同，把两者混为一谈会拼出 404 的地址：
/// - Forge 1.12.2 + 14.23.0.2486 → Maven `1.12.2-14.23.0.2486`，版本 id `1.12.2-forge-14.23.0.2486`
/// - NeoForge 1.20.1 + 47.1.106 → Maven `1.20.1-47.1.106`，版本 id `1.20.1-forge-47.1.106`
/// - NeoForge 21.1.172 → Maven `21.1.172`，版本 id `neoforge-21.1.172`
#[derive(Debug, Clone)]
struct InstallerArtifact {
    /// Maven 版本号
    maven_version: String,
    /// 安装完成后生成的版本 id
    version_id: String,
    /// 相对仓库根目录的路径
    path: String,
}

fn installer_artifact(kind: LoaderKind, mc_version: &str, loader_version: &str) -> InstallerArtifact {
    match kind {
        LoaderKind::Forge => {
            let maven_version = format!("{mc_version}-{loader_version}");
            InstallerArtifact {
                path: format!(
                    "net/minecraftforge/forge/{maven_version}/forge-{maven_version}-installer.jar"
                ),
                version_id: format!("{mc_version}-forge-{loader_version}"),
                maven_version,
            }
        }
        LoaderKind::NeoForge if mc_version == "1.20.1" => {
            // 1.20.1 走 net.neoforged:forge 过渡产物
            let maven_version = format!("{mc_version}-{loader_version}");
            InstallerArtifact {
                path: format!(
                    "net/neoforged/forge/{maven_version}/forge-{maven_version}-installer.jar"
                ),
                version_id: format!("{mc_version}-forge-{loader_version}"),
                maven_version,
            }
        }
        LoaderKind::NeoForge => {
            let maven_version = loader_version.to_string();
            InstallerArtifact {
                path: format!(
                    "net/neoforged/neoforge/{maven_version}/neoforge-{maven_version}-installer.jar"
                ),
                version_id: format!("neoforge-{loader_version}"),
                maven_version,
            }
        }
        LoaderKind::Fabric | LoaderKind::Quilt => {
            unreachable!("Fabric / Quilt 不使用安装器")
        }
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
        LoaderKind::Forge | LoaderKind::NeoForge => {
            let artifact = installer_artifact(kind, mc_version, loader_version);
            let url = match kind {
                LoaderKind::Forge => dl.source.forge_maven(&artifact.path),
                _ => dl.source.neoforge_maven(&artifact.path),
            };
            log::info!(
                "下载 {} 安装器 {} -> {}",
                kind.label(),
                artifact.maven_version,
                artifact.path
            );
            run_installer(
                dl,
                &url,
                game_dir,
                java,
                progress,
                &artifact.version_id,
                loader_version,
                mc_version,
            )
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
    keyword: &str,
    mc_version: &str,
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

    // 1.7 ~ 1.16.5 的 Forge 安装器沿用官方启动器的安装逻辑，
    // 必须先有 launcher_profiles.json，否则它会直接报错退出。
    let mut prepared = false;
    if needs_launcher_profiles(mc_version) {
        ensure_launcher_profiles(game_dir)?;
        prepared = true;
    }

    // 安装器会自己新建版本目录，跑之前先记下已有的版本，跑完再比对，
    // 这样才能认出老版本 Forge 那种不规则的目录名。
    let before = version::installed_versions(game_dir);

    loop {
        let (status, output) = spawn_installer(&jar, game_dir, java, progress)?;
        if status.success() {
            break;
        }
        // 安装器因为缺少启动器配置而失败时，补一份再试一次
        if !prepared && output.contains("launcher profile") {
            prepared = true;
            log::warn!("安装器缺少启动器配置，补一份 launcher_profiles.json 后重试");
            ensure_launcher_profiles(game_dir)?;
            continue;
        }
        return Err(anyhow!(
            "安装器执行失败（退出码 {:?}），请检查网络或改用其他 Java",
            status.code()
        ));
    }

    let after = version::installed_versions(game_dir);
    detect_installed_version(&before, &after, expected_id, keyword)
        .ok_or_else(|| anyhow!("安装器已结束，但未找到生成的版本目录"))
}

/// 该 Minecraft 版本是否需要 launcher_profiles.json
///
/// 1.7 ~ 1.16.5 的 Forge 安装器沿用官方启动器的安装逻辑，
/// 少了这个文件会直接报错退出；1.17 起换了新的安装器，不再需要。
fn needs_launcher_profiles(mc_version: &str) -> bool {
    let parts: Vec<u32> = mc_version
        .split(['.', '-'])
        .filter_map(|p| p.parse().ok())
        .collect();
    match parts.as_slice() {
        [1, minor, patch] => (*minor, *patch) <= (16, 5),
        [1, minor] => *minor <= 16,
        _ => false,
    }
}

/// 补一份官方启动器格式的 launcher_profiles.json
///
/// 老版本 Forge 安装器靠它确认游戏目录有效，已存在时不动。
fn ensure_launcher_profiles(game_dir: &Path) -> Result<()> {
    let path = game_dir.join("launcher_profiles.json");
    if path.is_file() {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let json = serde_json::json!({
        "profiles": {},
        "selectedProfile": null,
        "clientToken": uuid::Uuid::new_v4().to_string(),
        "authenticationDatabase": {},
        "launcherVersion": {
            "name": "htmcl",
            "id": 21,
            "user": "nobody",
            "uuid": "00000000000000000000000000000000",
            "version": "1.0",
            "javaVersion": { "minimumLauncherVersion": 21 },
            "created": { "date": now },
            "lastUsed": { "date": now }
        },
        "clientSettings": {
            "locale": "zh_cn",
            "banner": "minecraft",
            "username": "",
            "showLauncherSkin": false,
            "hasSeenDataTransferWarning": true,
            "hasSeenMultiplayerWarning": true,
            "hideServers": false
        }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&json)?)
        .with_context(|| format!("写入 launcher_profiles.json 失败: {}", path.display()))?;
    log::info!("已生成 launcher_profiles.json: {}", path.display());
    Ok(())
}

/// 给安装器的 JVM 带上系统代理
///
/// 返回形如 `-Dhttp.proxyHost=127.0.0.1` 的参数。
/// SOCKS 代理走 `socksProxyHost`，由 Java 统一接管所有连接。
fn proxy_jvm_args() -> Vec<String> {
    if !crate::core::download::use_system_proxy() {
        return Vec::new();
    }
    let Some(proxy) = crate::core::download::system_proxy() else {
        return Vec::new();
    };
    let Ok(url) = reqwest::Url::parse(&proxy) else {
        return Vec::new();
    };
    let Some(host) = url.host_str() else {
        return Vec::new();
    };
    let Some(port) = url.port_or_known_default() else {
        return Vec::new();
    };
    let scheme = url.scheme();
    if scheme.starts_with("socks") {
        vec![
            format!("-DsocksProxyHost={host}"),
            format!("-DsocksProxyPort={port}"),
            "-DsocksProxyVersion=5".to_string(),
        ]
    } else {
        vec![
            format!("-Dhttp.proxyHost={host}"),
            format!("-Dhttp.proxyPort={port}"),
            format!("-Dhttps.proxyHost={host}"),
            format!("-Dhttps.proxyPort={port}"),
        ]
    }
}

/// 运行安装器，返回退出状态与它的输出
fn spawn_installer(
    jar: &Path,
    game_dir: &Path,
    java: &JavaInfo,
    progress: &Arc<DownloadProgress>,
) -> Result<(std::process::ExitStatus, String)> {
    progress.set_message("正在运行安装器（可能需要几分钟）...");
    let mut cmd = Command::new(&java.path);
    // 安装器是独立的 Java 进程，不会继承启动器的代理设置，
    // 直连 Maven 中央仓库会超时，导致「这些库下载失败」而安装中断。
    // 这些是 JVM 参数，必须放在 -jar 之前。
    for arg in proxy_jvm_args() {
        cmd.arg(arg);
    }
    cmd.arg("-jar")
        .arg(jar)
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

    // 安装器的输出既用于显示进度，也要留着判断失败原因
    let collected = Arc::new(std::sync::Mutex::new(String::new()));
    let progress_out = progress.clone();
    let out_text = collected.clone();
    let handle_out = stdout.map(|s| std::thread::spawn(move || stream_lines(s, progress_out, out_text)));
    let progress_err = progress.clone();
    let err_text = collected.clone();
    let handle_err = stderr.map(|s| std::thread::spawn(move || stream_lines(s, progress_err, err_text)));

    let status = child.wait().context("等待安装器结束失败")?;
    if let Some(h) = handle_out {
        let _ = h.join();
    }
    if let Some(h) = handle_err {
        let _ = h.join();
    }

    let text = collected.lock().map(|t| t.clone()).unwrap_or_default();
    Ok((status, text))
}

/// 从安装前后版本目录的差异里找出安装器生成的版本
fn detect_installed_version(
    before: &[String],
    after: &[String],
    expected_id: &str,
    keyword: &str,
) -> Option<String> {
    let created: Vec<&String> = after.iter().filter(|v| !before.contains(v)).collect();

    // 1. 新建的目录里精确命中期望的版本 id
    if let Some(v) = created.iter().find(|v| v.eq_ignore_ascii_case(expected_id)) {
        return Some((*v).clone());
    }
    // 2. 新建的目录里包含加载器版本号。老版本 Forge 的目录名形如
    //    `1.7.10-Forge10.13.4.1614-1.7.10`，与拼出来的 id 对不上，只能按版本号片段找
    if !keyword.is_empty() {
        if let Some(v) = created.iter().find(|v| v.contains(keyword)) {
            return Some((*v).clone());
        }
    }
    // 3. 安装器没有新建目录（之前装过同一个版本），回退到期望的版本 id
    if let Some(v) = after.iter().find(|v| v.eq_ignore_ascii_case(expected_id)) {
        return Some(v.clone());
    }
    // 4. 兜底：版本目录里带加载器名字的
    after
        .iter()
        .find(|v| v.to_lowercase().contains("forge"))
        .cloned()
}

fn stream_lines<R: std::io::Read + Send + 'static>(
    stream: R,
    progress: Arc<DownloadProgress>,
    collected: Arc<std::sync::Mutex<String>>,
) {
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
        if let Ok(mut buffer) = collected.lock() {
            buffer.push_str(trimmed);
            buffer.push('\n');
        }
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

    /// Forge 的安装器在 Maven 上是 `{mc}-{forge版本}`，
    /// 拼成 `{mc}-forge-{forge版本}` 会得到 404
    #[test]
    fn forge_installer_uses_maven_version_layout() {
        let artifact = installer_artifact(LoaderKind::Forge, "1.12.2", "14.23.0.2486");
        assert_eq!(
            artifact.path,
            "net/minecraftforge/forge/1.12.2-14.23.0.2486/forge-1.12.2-14.23.0.2486-installer.jar"
        );
        assert_eq!(artifact.version_id, "1.12.2-forge-14.23.0.2486");
        assert_eq!(artifact.maven_version, "1.12.2-14.23.0.2486");
    }

    #[test]
    fn forge_installer_layout_holds_for_modern_versions() {
        let artifact = installer_artifact(LoaderKind::Forge, "1.20.1", "47.3.0");
        assert_eq!(
            artifact.path,
            "net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar"
        );
        assert_eq!(artifact.version_id, "1.20.1-forge-47.3.0");
    }

    /// 1.20.1 的 NeoForge 走 net.neoforged:forge 过渡产物
    #[test]
    fn neoforge_1201_uses_forge_artifact() {
        let artifact = installer_artifact(LoaderKind::NeoForge, "1.20.1", "47.1.106");
        assert_eq!(
            artifact.path,
            "net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar"
        );
        assert_eq!(artifact.version_id, "1.20.1-forge-47.1.106");
    }

    #[test]
    fn neoforge_modern_uses_neoforge_artifact() {
        let artifact = installer_artifact(LoaderKind::NeoForge, "1.21.1", "21.1.172");
        assert_eq!(
            artifact.path,
            "net/neoforged/neoforge/21.1.172/neoforge-21.1.172-installer.jar"
        );
        assert_eq!(artifact.version_id, "neoforge-21.1.172");
    }

    #[test]
    fn neoforge_version_prefix_maps_mc_versions() {
        assert_eq!(neoforge_version_prefix("1.20.2").as_deref(), Some("20.2"));
        assert_eq!(neoforge_version_prefix("1.20.4").as_deref(), Some("20.4"));
        assert_eq!(neoforge_version_prefix("1.21").as_deref(), Some("21.0"));
        assert_eq!(neoforge_version_prefix("1.21.1").as_deref(), Some("21.1"));
        assert_eq!(neoforge_version_prefix("26.1").as_deref(), Some("26.1"));
    }

    #[test]
    fn compare_versions_orders_numerically() {
        assert_eq!(compare_versions("20.4.190", "20.4.189"), Ordering::Greater);
        assert_eq!(compare_versions("26.3.0.95", "26.3.0.9"), Ordering::Greater);
        assert_eq!(compare_versions("20.4.190", "21.0.167"), Ordering::Less);
        assert_eq!(compare_versions("14.23.5.2860", "14.23.5.2860"), Ordering::Equal);
    }

    /// 预发布版本排在正式版之后（更小）
    #[test]
    fn compare_versions_treats_prerelease_as_older() {
        assert_eq!(compare_versions("20.4.190-beta", "20.4.190"), Ordering::Less);
    }

    #[test]
    fn sort_versions_desc_puts_newest_first() {
        let mut list = vec![
            LoaderVersion {
                version: "20.4.151".into(),
                stable: false,
            },
            LoaderVersion {
                version: "20.4.253".into(),
                stable: false,
            },
            LoaderVersion {
                version: "20.4.190".into(),
                stable: false,
            },
        ];
        sort_versions_desc(&mut list);
        assert_eq!(list[0].version, "20.4.253");
        assert_eq!(list[1].version, "20.4.190");
        assert_eq!(list[2].version, "20.4.151");
    }

    /// 老版本 Forge 生成的目录名不规则，要能认出来
    #[test]
    fn detects_legacy_forge_version_directory() {
        let before = vec!["1.7.10".to_string()];
        let after = vec![
            "1.7.10".to_string(),
            "1.7.10-Forge10.13.4.1614-1.7.10".to_string(),
        ];
        let found = detect_installed_version(
            &before,
            &after,
            "1.7.10-forge-10.13.4.1614-1.7.10",
            "10.13.4.1614-1.7.10",
        );
        assert_eq!(found.as_deref(), Some("1.7.10-Forge10.13.4.1614-1.7.10"));
    }

    #[test]
    fn detects_exact_version_directory() {
        let before = vec!["1.12.2".to_string()];
        let after = vec![
            "1.12.2".to_string(),
            "1.12.2-forge-14.23.0.2486".to_string(),
        ];
        let found = detect_installed_version(
            &before,
            &after,
            "1.12.2-forge-14.23.0.2486",
            "14.23.0.2486",
        );
        assert_eq!(found.as_deref(), Some("1.12.2-forge-14.23.0.2486"));
    }

    /// 1.20.1 的过渡产物版本号带 - 前缀，不能被当成预发布版本筛掉
    #[test]
    fn neoforge_1201_transitional_artifact_is_kept() {
        let raw = vec![
            "1.20.1-47.1.106".to_string(),
            "1.20.1-47.1.7".to_string(),
            "1.20.1-47.2.0-beta".to_string(),
        ];
        let versions = filter_neoforge_versions(raw, "1.20.1", "1.20.1-");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "47.1.106");
        assert_eq!(versions[1].version, "47.1.7");
    }

    /// 同一个仓库里混着各个 Minecraft 版本的构建，必须按目标版本过滤
    #[test]
    fn neoforge_versions_are_filtered_by_mc_version() {
        let raw = vec![
            "20.4.253".to_string(),
            "20.4.190-beta".to_string(),
            "21.1.264".to_string(),
            "26.3.0.97".to_string(),
        ];
        let versions = filter_neoforge_versions(raw, "1.20.4", "");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "20.4.253");
    }

    #[test]
    fn neoforge_versions_for_26x_are_filtered() {
        let raw = vec![
            "26.3.0.97".to_string(),
            "26.1.2.122".to_string(),
            "21.1.264".to_string(),
        ];
        let versions = filter_neoforge_versions(raw, "26.1", "");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "26.1.2.122");
    }

    /// 老版本 Forge 安装器（1.7 ~ 1.16.5）需要 launcher_profiles.json
    #[test]
    fn launcher_profiles_needed_only_for_legacy_forge() {        assert!(needs_launcher_profiles("1.7.10"));
        assert!(needs_launcher_profiles("1.12.2"));
        assert!(needs_launcher_profiles("1.16.5"));
        assert!(!needs_launcher_profiles("1.17.1"));
        assert!(!needs_launcher_profiles("1.20.1"));
        assert!(!needs_launcher_profiles("26.1"));
    }

    /// 安装器没新建目录时（重复安装同一个版本）也要能找到
    #[test]
    fn detects_existing_version_directory() {
        let before = vec!["1.12.2".to_string()];
        let after = vec![
            "1.12.2".to_string(),
            "1.12.2-forge-14.23.0.2486".to_string(),
        ];
        let found = detect_installed_version(
            &before,
            &after,
            "1.12.2-forge-14.23.0.2486",
            "14.23.0.2486",
        );
        assert_eq!(found.as_deref(), Some("1.12.2-forge-14.23.0.2486"));
    }
}
