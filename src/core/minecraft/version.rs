use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::minecraft::modpack::{self, ModpackInfo};
use crate::core::minecraft::version_icon::{self, VersionCategory};
use crate::core::paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionJson {
    pub id: String,
    /// 版本显示名，Minecraft 主菜单左下角显示的就是它
    ///
    /// 官方版本 JSON 多数不带这个字段，游戏会退回内置名称；
    /// 我们统一写成启动器品牌名，让主菜单左下角显示品牌而不是版本号。
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub version_type: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    #[serde(rename = "minimumLauncherVersion")]
    pub minimum_launcher_version: Option<u32>,
    #[serde(rename = "releaseTime")]
    pub release_time: Option<String>,
    pub time: Option<String>,
    pub assets: Option<String>,
    #[serde(rename = "assetIndex")]
    pub asset_index: Option<AssetIndex>,
    pub downloads: Option<Downloads>,
    pub libraries: Vec<Library>,
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
    #[serde(rename = "complianceLevel")]
    pub compliance_level: Option<u32>,
    pub jar: Option<String>,
    /// Fabric / Quilt 的加载器信息
    pub loader: Option<LoaderInfo>,
    /// 仅用于本地 UI 显示
    #[serde(skip)]
    pub local_path: Option<PathBuf>,
}

/// Fabric / Quilt 版本 JSON 里的加载器信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LoaderInfo {
    /// 加载器版本号，如 0.20.1
    pub version: String,
    /// 版本号与 Minecraft 版本之间的分隔符
    pub separator: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetIndex {
    pub id: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    #[serde(rename = "totalSize")]
    pub total_size: Option<u64>,
    pub url: Option<String>,
    #[serde(rename = "map_to_resources")]
    pub map_to_resources: Option<bool>,
    #[serde(rename = "virtual")]
    pub is_virtual: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Downloads {
    pub client: Option<DownloadArtifact>,
    pub server: Option<DownloadArtifact>,
    #[serde(rename = "client_mappings")]
    pub client_mappings: Option<DownloadArtifact>,
    #[serde(rename = "server_mappings")]
    pub server_mappings: Option<DownloadArtifact>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadArtifact {
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub natives: Option<HashMap<String, String>>,
    pub rules: Option<Vec<Rule>>,
    pub extract: Option<Extract>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryDownloads {
    pub artifact: Option<DownloadArtifact>,
    pub classifiers: Option<HashMap<String, DownloadArtifact>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Extract {
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Arguments {
    pub game: Vec<Value>,
    pub jvm: Vec<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JavaVersion {
    pub component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Logging {
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
    #[serde(rename = "type")]
    pub log_type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: Option<String>,
}

/// 合并后的完整版本信息
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub id: String,
    /// 组装 classpath 时实际使用的客户端 jar 所属版本
    pub jar_id: String,
    pub json: VersionJson,
    pub version_dir: PathBuf,
    pub jar_path: PathBuf,
    pub game_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub natives_dir: PathBuf,
}

pub fn version_dir(game_dir: &Path, id: &str) -> PathBuf {
    paths::versions_dir(game_dir).join(id)
}

pub fn version_json_path(game_dir: &Path, id: &str) -> PathBuf {
    version_dir(game_dir, id).join(format!("{id}.json"))
}

pub fn version_jar_path(game_dir: &Path, id: &str) -> PathBuf {
    version_dir(game_dir, id).join(format!("{id}.jar"))
}

pub fn read_local_json(game_dir: &Path, id: &str) -> Result<VersionJson> {
    let path = version_json_path(game_dir, id);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("读取版本文件失败: {}", path.display()))?;
    let mut json: VersionJson = serde_json::from_str(&text)
        .with_context(|| format!("解析版本文件失败: {}", path.display()))?;
    if json.id.is_empty() {
        json.id = id.to_string();
    }
    json.local_path = Some(path);
    Ok(json)
}

pub fn write_local_json(game_dir: &Path, json: &VersionJson) -> Result<()> {
    let dir = version_dir(game_dir, &json.id);
    paths::ensure_dir(&dir)?;
    let path = version_json_path(game_dir, &json.id);
    let text = serde_json::to_string_pretty(json)?;
    std::fs::write(path, text)?;
    Ok(())
}

/// 递归加载继承链并合并（返回值已包含父版本的全部内容）
pub fn resolve(game_dir: &Path, id: &str) -> Result<ResolvedVersion> {
    let mut chain: Vec<VersionJson> = Vec::new();
    let mut current = id.to_string();
    for _ in 0..8 {
        let json = read_local_json(game_dir, &current)?;
        let parent = json.inherits_from.clone();
        chain.push(json);
        match parent {
            Some(p) if !p.is_empty() => current = p,
            _ => break,
        }
    }

    if chain.is_empty() {
        return Err(anyhow!("版本 {id} 不存在"));
    }

    let base_id = chain.last().map(|j| j.id.clone()).unwrap_or_else(|| id.into());
    // 从最顶层父版本开始向下合并
    let mut merged = chain.last().cloned().unwrap_or_default();
    for child in chain.iter().rev().skip(1) {
        merged = merge(&merged, child);
    }
    merged.id = id.to_string();

    let jar_id = merged
        .jar
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| base_id.clone());

    let vdir = version_dir(game_dir, id);
    let natives_dir = vdir.join("natives");

    Ok(ResolvedVersion {
        id: id.to_string(),
        jar_path: version_jar_path(game_dir, &jar_id),
        jar_id,
        json: merged,
        version_dir: vdir,
        game_dir: game_dir.to_path_buf(),
        libraries_dir: paths::libraries_dir(game_dir),
        assets_dir: paths::assets_dir(game_dir),
        natives_dir,
    })
}

/// 依赖库的去重键：group、artifact 与 classifier
///
/// 版本号不进键里：同一个库的版本不同就该由更后来的那份顶上。
///
/// classifier 一定要进键里。Minecraft 1.20.5 之后把 natives 拆成了单独的库条目，
/// `org.lwjgl:lwjgl:3.4.3` 和 `org.lwjgl:lwjgl:3.4.3:natives-windows`
/// 看着像同一个库，其实是两个文件，少了 classifier 就会把主 jar 顶掉，
/// 到了游戏里就变成 `ClassNotFoundException`。
fn library_key(lib: &Library) -> String {
    let parts: Vec<&str> = lib.name.split(':').collect();
    match parts.len() {
        0..=1 => lib.name.clone(),
        2 => format!("{}:{}", parts[0], parts[1]),
        _ => {
            // group:artifact:version[:classifier]
            let classifier = parts.get(3).copied().unwrap_or("");
            format!("{}:{}:{}", parts[0], parts[1], classifier)
        }
    }
}

fn merge(parent: &VersionJson, child: &VersionJson) -> VersionJson {
    let mut libraries = parent.libraries.clone();
    libraries.extend(child.libraries.clone());
    // 同名库以子版本为准
    //
    // 加载器会带上自己那份更新的依赖（比如 Forge 带的是 log4j 2.15，
    // Minecraft 1.16.5 自带的是 2.8.1）。两份都留着的话，
    // 旧的那份在 classpath 靠前会先被加载，Forge 的日志转换器就会
    // 因为 API 对不上而直接报错退出。这里按 group:artifact 去重。
    let mut merged: Vec<Library> = Vec::with_capacity(libraries.len());
    for lib in libraries {
        match merged.iter().position(|old| library_key(old) == library_key(&lib)) {
            Some(pos) => merged[pos] = lib,
            None => merged.push(lib),
        }
    }
    let libraries = merged;

    let arguments = match (&parent.arguments, &child.arguments) {
        (None, None) => None,
        (Some(p), None) => Some(p.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(p), Some(c)) => Some(Arguments {
            game: p.game.iter().chain(c.game.iter()).cloned().collect(),
            jvm: p.jvm.iter().chain(c.jvm.iter()).cloned().collect(),
        }),
    };

    VersionJson {
        id: child.id.clone(),
        name: if child.name.is_empty() {
            parent.name.clone()
        } else {
            child.name.clone()
        },
        version_type: if child.version_type.is_empty() {
            parent.version_type.clone()
        } else {
            child.version_type.clone()
        },
        main_class: if child.main_class.is_empty() {
            parent.main_class.clone()
        } else {
            child.main_class.clone()
        },
        inherits_from: child.inherits_from.clone(),
        minimum_launcher_version: child
            .minimum_launcher_version
            .or(parent.minimum_launcher_version),
        release_time: child.release_time.clone().or(parent.release_time.clone()),
        time: child.time.clone().or(parent.time.clone()),
        assets: child.assets.clone().or(parent.assets.clone()),
        asset_index: child.asset_index.clone().or(parent.asset_index.clone()),
        downloads: child.downloads.clone().or(parent.downloads.clone()),
        libraries,
        arguments,
        minecraft_arguments: child
            .minecraft_arguments
            .clone()
            .or(parent.minecraft_arguments.clone()),
        java_version: child.java_version.clone().or(parent.java_version.clone()),
        logging: child.logging.clone().or(parent.logging.clone()),
        compliance_level: child.compliance_level.or(parent.compliance_level),
        jar: child.jar.clone().or(parent.jar.clone()),
        loader: child.loader.clone().or(parent.loader.clone()),
        local_path: child.local_path.clone().or(parent.local_path.clone()),
    }
}

pub const OS_NAME: &str = "windows";
pub const OS_ARCH: &str = "x86_64";

/// 解析库的规则，判断当前平台是否启用
pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn rule_matches(rule: &Rule) -> bool {
    if let Some(os) = &rule.os {
        if let Some(name) = &os.name {
            if !name.eq_ignore_ascii_case(OS_NAME) {
                return false;
            }
        }
        if let Some(arch) = &os.arch {
            let arch = arch.to_lowercase();
            if arch == "x86" && OS_ARCH != "x86" {
                return false;
            }
        }
        if let Some(version) = &os.version {
            if !match_os_version(version) {
                return false;
            }
        }
    }
    if let Some(features) = &rule.features {
        for (_, enabled) in features {
            // 启动器不启用 demo / quick play 等特性
            if *enabled {
                return false;
            }
        }
    }
    true
}

fn match_os_version(pattern: &str) -> bool {
    // 常见形式 ^10\. 或 ^10\.
    let cleaned = pattern.trim_start_matches('^').trim_end_matches('\\').trim_end_matches('.');
    if cleaned.is_empty() {
        return true;
    }
    let version = os_version_string();
    version.starts_with(cleaned)
}

fn os_version_string() -> String {
    #[cfg(windows)]
    {
        // Windows 10/11 统一按 10 处理，与官方启动器行为一致
        "10.0".to_string()
    }
    #[cfg(not(windows))]
    {
        "0".to_string()
    }
}

/// group:artifact:version[:classifier] -> 相对路径
pub fn maven_path(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    Some(format!("{group}/{artifact}/{version}/{file}"))
}

/// 判断某个库在当前平台是否启用
pub fn library_allowed(lib: &Library) -> bool {
    match &lib.rules {
        Some(rules) => rules_allow(rules),
        None => true,
    }
}

/// 计算需要下载的库文件
pub fn library_artifact(lib: &Library) -> Option<(String, DownloadArtifact)> {
    if !library_allowed(lib) {
        return None;
    }
    match &lib.downloads {
        Some(downloads) => {
            // 明确给出了 downloads，就只认其中的 artifact。
            // 旧版本的一些库只有 classifiers（natives）没有 artifact，
            // 此时若按 Maven 规则拼地址会得到一个 404 的不存在的文件。
            let artifact = downloads.artifact.as_ref()?;
            if artifact.url.is_none() {
                return None;
            }
            let path = artifact
                .path
                .clone()
                .or_else(|| maven_path(&lib.name))
                .unwrap_or_default();
            if path.is_empty() {
                return None;
            }
            Some((path, artifact.clone()))
        }
        // 没有 downloads 字段时，按 Maven 规则拼接
        None => {
            let path = maven_path(&lib.name)?;
            let base = lib
                .url
                .clone()
                .unwrap_or_else(|| "https://libraries.minecraft.net/".to_string());
            let url = format!("{}/{}", base.trim_end_matches('/'), path);
            Some((
                path,
                DownloadArtifact {
                    url: Some(url),
                    ..Default::default()
                },
            ))
        }
    }
}

/// 计算需要解压的 native 库
pub fn library_native(lib: &Library) -> Option<(String, DownloadArtifact)> {
    if !library_allowed(lib) {
        return None;
    }
    let natives = lib.natives.as_ref()?;
    let classifier = natives
        .get("windows")
        .or_else(|| natives.get("windows-10"))
        .or_else(|| natives.get("osx"))?;
    let classifier = classifier
        .replace("${arch}", if OS_ARCH == "x86_64" { "64" } else { "32" });
    let downloads = lib.downloads.as_ref()?;
    let classifiers = downloads.classifiers.as_ref()?;
    let artifact = classifiers.get(&classifier)?;
    let path = artifact
        .path
        .clone()
        .unwrap_or_else(|| maven_path(&format!("{}:{}", lib.name, classifier)).unwrap_or_default());
    Some((path, artifact.clone()))
}

/// 读取版本目录下的所有已安装版本 id
pub fn installed_versions(game_dir: &Path) -> Vec<String> {
    let dir = paths::versions_dir(game_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.path().join(format!("{name}.json")).is_file() {
            ids.push(name);
        }
    }
    ids.sort_by_key(|s| s.to_lowercase());
    ids
}

#[derive(Debug, Clone, Default)]
pub struct LocalVersion {
    pub id: String,
    pub version_type: String,
    pub loader: Option<String>,
    /// 加载器版本号，如 0.20.1
    pub loader_version: Option<String>,
    pub mc_version: String,
    pub size: u64,
    pub last_modified: Option<std::time::SystemTime>,
    pub has_jar: bool,
    pub icon_data_url: Option<String>,
    /// 整合包实例的信息，普通版本为 None
    pub modpack: Option<ModpackInfo>,
    /// 版本分类：正式版、快照、旧版、愚人节
    pub category: VersionCategory,
    /// 版本目录里的自定义图标
    pub custom_icon: Option<PathBuf>,
}

/// 扫描本地已安装版本，并推断加载器类型与对应 MC 版本
pub fn scan_local(game_dir: &Path) -> Vec<LocalVersion> {
    let mut result = Vec::new();
    for id in installed_versions(game_dir) {
        let json = match read_local_json(game_dir, &id) {
            Ok(j) => j,
            Err(_) => continue,
        };
        let dir = version_dir(game_dir, &id);
        // 整合包实例有自己的目录与名字，跟普通版本分开
        let marker = modpack::read_marker(&dir);
        let loader = marker
            .as_ref()
            .and_then(|m| m.loader.clone())
            .or_else(|| detect_loader(&id, &json));
        let mc_version = if marker.is_some() {
            // 整合包实例自己成一条，不跟原版版本合并
            id.clone()
        } else {
            detect_mc_version(game_dir, &id, &json)
        };
        let loader_version = marker
            .as_ref()
            .and_then(|m| m.loader_version.clone())
            .filter(|v| !v.is_empty())
            .or_else(|| {
                json.loader
                    .as_ref()
                    .map(|l| l.version.clone())
                    .filter(|v| !v.is_empty())
            })
            .or_else(|| loader_version_from_id(&id, &mc_version));
        let size = paths::dir_size(&dir);
        let last_modified = std::fs::metadata(dir.join(format!("{id}.json")))
            .and_then(|m| m.modified())
            .ok();
        let has_jar = version_jar_path(game_dir, &id).is_file()
            || version_jar_path(game_dir, &json.inherits_from.clone().unwrap_or_default()).is_file();
        let category = version_icon::classify(&json.version_type, &id, json.release_time.as_deref());
        let custom_icon = version_icon::custom_icon(&dir);
        result.push(LocalVersion {
            id,
            version_type: json.version_type.clone(),
            loader,
            loader_version,
            mc_version,
            size,
            last_modified,
            has_jar,
            icon_data_url: None,
            modpack: marker,
            category,
            custom_icon,
        });
    }
    result.sort_by(|a, b| {
        b.last_modified
            .cmp(&a.last_modified)
            .then_with(|| b.id.cmp(&a.id))
    });
    result
}

/// 从版本目录 id 里解析加载器版本号
///
/// 版本目录的命名各加载器不一样，这里按已知的几种前缀还原：
/// - `fabric-loader-0.20.1-26.2` → `0.20.1`
/// - `1.12.2-forge-14.23.0.2486` → `14.23.0.2486`
/// - `neoforge-21.1.172` → `21.1.172`
pub fn loader_version_from_id(id: &str, mc_version: &str) -> Option<String> {
    let mut rest = id.to_string();
    if let Some(stripped) = rest.strip_prefix(&format!("{mc_version}-")) {
        rest = stripped.to_string();
    }
    for prefix in [
        "fabric-loader-",
        "quilt-loader-",
        "neoforge-",
        "forge-",
        "loader-",
    ] {
        let Some(version) = rest.strip_prefix(prefix) else {
            continue;
        };
        let version = version
            .strip_suffix(&format!("-{mc_version}"))
            .unwrap_or(version);
        if version.is_empty() {
            continue;
        }
        return Some(version.to_string());
    }
    None
}

/// 变体的显示名：原版或「加载器 版本号」
impl LocalVersion {
    pub fn variant_label(&self) -> String {
        match (&self.loader, &self.loader_version) {
            (Some(loader), Some(version)) => format!("{loader} {version}"),
            (Some(loader), None) => loader.clone(),
            _ => "原版".to_string(),
        }
    }
}

/// 一个游戏实例：一个 Minecraft 版本及其加载器变体
///
/// 版本列表按实例展示：原版与各加载器版本合成一条，
/// 模组、光影、存档都挂在这个实例上，不再各占一份。
#[derive(Debug, Clone, Default)]
pub struct GameInstance {
    /// Minecraft 版本号，如 26.2
    ///
    /// 整合包实例这里放的是整合包自己的实例 id，
    /// 它跟 Minecraft 版本号无关，纯粹用来区分实例。
    pub mc_version: String,
    /// 该实例下的变体，对应真实的版本目录
    pub variants: Vec<LocalVersion>,
    /// 当前使用的变体 id
    pub active: String,
    /// 实例的显示名
    ///
    /// 普通实例显示 Minecraft 版本号，整合包实例显示整合包名字。
    pub display_name: String,
    /// 是不是整合包实例
    pub is_modpack: bool,
    /// 版本分类，决定默认图标
    pub category: VersionCategory,
    /// 版本目录里的自定义图标
    pub custom_icon: Option<PathBuf>,
}

impl GameInstance {
    /// 当前使用的变体
    pub fn active_version(&self) -> Option<&LocalVersion> {
        self.variants.iter().find(|v| v.id == self.active)
    }

    /// 实例实际对应的 Minecraft 版本
    ///
    /// 整合包实例的 id 是它自己的名字，真实版本要看标记文件里记的那个。
    pub fn real_mc_version(&self) -> String {
        self.variants
            .iter()
            .find_map(|v| v.modpack.as_ref().map(|m| m.mc_version.clone()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.mc_version.clone())
    }

    /// 原版变体
    pub fn vanilla(&self) -> Option<&LocalVersion> {
        self.variants.iter().find(|v| v.loader.is_none())
    }

    /// 当前变体的加载器名，原版显示「原版」
    pub fn loader_label(&self) -> String {
        self.active_version()
            .and_then(|v| v.loader.clone())
            .unwrap_or_else(|| "原版".to_string())
    }

    /// 当前变体的加载器名，原版为 None
    pub fn loader_label_opt(&self) -> Option<String> {
        self.active_version().and_then(|v| v.loader.clone())
    }

    /// 实例内所有变体占用的空间
    pub fn size(&self) -> u64 {
        self.variants.iter().map(|v| v.size).sum()
    }

    /// 实例内最近一次修改的时间
    pub fn last_modified(&self) -> Option<std::time::SystemTime> {
        self.variants.iter().filter_map(|v| v.last_modified).max()
    }

    /// 变体数量
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }
}

/// 把版本目录聚合成游戏实例
///
/// 同一个 Minecraft 版本下的原版与各加载器版本合成一条，
/// 模组、光影、存档因此只有一份，不会因为换了加载器就各分一份。
pub fn group_instances(versions: &[LocalVersion], selected: Option<&str>) -> Vec<GameInstance> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<LocalVersion>> = HashMap::new();
    for v in versions {
        if !groups.contains_key(&v.mc_version) {
            order.push(v.mc_version.clone());
        }
        groups
            .entry(v.mc_version.clone())
            .or_default()
            .push(v.clone());
    }

    let mut instances: Vec<GameInstance> = order
        .into_iter()
        .filter_map(|mc| {
            let variants = groups.remove(&mc)?;
            // 当前变体：优先用全局选中的那个，否则取最近修改过的
            let active = variants
                .iter()
                .find(|v| Some(v.id.as_str()) == selected)
                .map(|v| v.id.clone())
                .unwrap_or_else(|| {
                    variants
                        .iter()
                        .max_by_key(|v| v.last_modified)
                        .map(|v| v.id.clone())
                        .unwrap_or_default()
                });
            let active_version = variants.iter().find(|v| v.id == active);
            Some(GameInstance {
                display_name: variants
                    .iter()
                    .find_map(|v| v.modpack.as_ref().map(|m| m.name.clone()))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| mc.clone()),
                is_modpack: variants.iter().any(|v| v.modpack.is_some()),
                // 分类取当前变体的，图标则整个实例里谁有就用谁的
                category: active_version
                    .map(|v| v.category)
                    .unwrap_or_default(),
                custom_icon: variants.iter().find_map(|v| v.custom_icon.clone()),
                mc_version: mc,
                variants,
                active,
            })
        })
        .collect();

    // 按最近修改排序，与原来的版本列表顺序保持一致
    instances.sort_by(|a, b| b.last_modified().cmp(&a.last_modified()));
    instances
}

fn detect_loader(id: &str, json: &VersionJson) -> Option<String> {    let lower = id.to_lowercase();
    if lower.starts_with("fabric-loader") || lower.contains("fabric") {
        return Some("Fabric".into());
    }
    if lower.starts_with("quilt-loader") || lower.contains("quilt") {
        return Some("Quilt".into());
    }
    if lower.contains("neoforge") {
        return Some("NeoForge".into());
    }
    if lower.contains("forge") {
        return Some("Forge".into());
    }
    if lower.contains("optifine") {
        return Some("OptiFine".into());
    }
    if json.main_class.contains("fabric") {
        return Some("Fabric".into());
    }
    if json.main_class.contains("quilt") {
        return Some("Quilt".into());
    }
    if json.main_class.contains("forge") {
        return Some("Forge".into());
    }
    None
}

pub fn detect_mc_version(game_dir: &Path, id: &str, json: &VersionJson) -> String {
    if json.inherits_from.is_some() {
        // 沿继承链找到最底层版本
        let mut current = json.inherits_from.clone();
        let mut guard = 0;
        while let Some(parent) = current {
            guard += 1;
            if guard > 8 {
                break;
            }
            match read_local_json(game_dir, &parent) {
                Ok(p) => {
                    if p.inherits_from.is_some() {
                        current = p.inherits_from.clone();
                    } else {
                        return p.id;
                    }
                }
                Err(_) => return parent,
            }
        }
    }
    // 从 id 中提取版本号，例如 1.20.1-forge-47.2.0
    let parts: Vec<&str> = id.split('-').collect();
    for part in parts {
        if part.matches('.').count() == 2 && part.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return part.to_string();
        }
    }
    id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧版本的一些库只有 classifiers（natives）没有 artifact，
    /// 此时不能按 Maven 规则拼出一个根本不存在的地址（404）
    #[test]
    fn classifier_only_library_has_no_artifact() {
        let lib: Library = serde_json::from_str(
            r#"{
                "name": "tv.twitch:twitch-platform:5.16",
                "downloads": {
                    "classifiers": {
                        "natives-windows": {
                            "url": "https://libraries.minecraft.net/tv/twitch/twitch-platform/5.16/twitch-platform-5.16-natives-windows.jar",
                            "size": 12345
                        }
                    }
                },
                "natives": { "windows": "natives-windows" },
                "extract": { "exclude": ["META-INF/"] }
            }"#,
        )
        .unwrap();
        assert!(library_artifact(&lib).is_none());
        // natives 那一份要能取到
        let (path, artifact) = library_native(&lib).expect("应当能取到 natives");
        assert!(path.ends_with("twitch-platform-5.16-natives-windows.jar"));
        assert!(artifact.url.is_some());
    }

    /// 正常带 artifact 的库不受影响
    #[test]
    fn normal_library_returns_artifact() {
        let lib: Library = serde_json::from_str(
            r#"{
                "name": "com.google.guava:guava:17.0",
                "downloads": {
                    "artifact": {
                        "url": "https://libraries.minecraft.net/com/google/guava/guava/17.0/guava-17.0.jar",
                        "size": 100
                    }
                }
            }"#,
        )
        .unwrap();
        let (path, artifact) = library_artifact(&lib).expect("应当能取到 artifact");
        assert_eq!(path, "com/google/guava/guava/17.0/guava-17.0.jar");
        assert!(artifact.url.is_some());
    }

    /// 没有 downloads 字段时仍按 Maven 规则拼接
    #[test]
    fn library_without_downloads_uses_maven_path() {
        let lib: Library = serde_json::from_str(
            r#"{
                "name": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10",
                "url": "https://maven.minecraftforge.net/"
            }"#,
        )
        .unwrap();
        let (path, artifact) = library_artifact(&lib).expect("应当能拼出地址");
        assert!(path.ends_with("forge-1.7.10-10.13.4.1614-1.7.10.jar"));
        assert!(artifact.url.unwrap().starts_with("https://maven.minecraftforge.net/"));
    }
    /// 从版本目录 id 里还原加载器版本号
    #[test]
    fn parses_loader_version_from_id() {
        assert_eq!(
            loader_version_from_id("fabric-loader-0.20.1-26.2", "26.2").as_deref(),
            Some("0.20.1")
        );
        assert_eq!(
            loader_version_from_id("quilt-loader-0.20.1-1.20.1", "1.20.1").as_deref(),
            Some("0.20.1")
        );
        assert_eq!(
            loader_version_from_id("1.12.2-forge-14.23.0.2486", "1.12.2").as_deref(),
            Some("14.23.0.2486")
        );
        assert_eq!(
            loader_version_from_id("1.20.1-forge-47.1.106", "1.20.1").as_deref(),
            Some("47.1.106")
        );
        assert_eq!(
            loader_version_from_id("neoforge-21.1.172", "1.21.1").as_deref(),
            Some("21.1.172")
        );
        // 原版没有加载器版本
        assert_eq!(loader_version_from_id("26.2", "26.2"), None);
    }

    /// 原版与加载器版本合成同一个实例
    #[test]
    fn groups_loader_variants_into_one_instance() {
        let make = |id: &str, loader: Option<&str>, mc: &str| LocalVersion {
            id: id.to_string(),
            version_type: "release".into(),
            loader: loader.map(|l| l.to_string()),
            loader_version: None,
            mc_version: mc.to_string(),
            size: 1,
            last_modified: None,
            has_jar: true,
            icon_data_url: None,
            ..Default::default()
        };
        let versions = vec![
            make("26.2", None, "26.2"),
            make("fabric-loader-0.20.1-26.2", Some("Fabric"), "26.2"),
            make("1.20.1", None, "1.20.1"),
        ];
        let instances = group_instances(&versions, Some("fabric-loader-0.20.1-26.2"));
        assert_eq!(instances.len(), 2);
        let inst = instances
            .iter()
            .find(|i| i.mc_version == "26.2")
            .expect("应当有 26.2 实例");
        assert_eq!(inst.variant_count(), 2);
        assert_eq!(inst.active, "fabric-loader-0.20.1-26.2");
        assert_eq!(inst.loader_label(), "Fabric");
        assert!(inst.vanilla().is_some());
        // 另一个实例没有加载器
        let other = instances.iter().find(|i| i.mc_version == "1.20.1").unwrap();
        assert_eq!(other.loader_label(), "原版");
    }

    /// 没选中时用最近修改的变体
    #[test]
    fn instance_active_falls_back_to_latest_modified() {
        let mut vanilla = LocalVersion {
            id: "26.2".to_string(),
            version_type: "release".into(),
            loader: None,
            loader_version: None,
            mc_version: "26.2".to_string(),
            size: 1,
            last_modified: Some(std::time::SystemTime::UNIX_EPOCH),
            has_jar: true,
            icon_data_url: None,
            ..Default::default()
        };
        let mut fabric = LocalVersion {
            id: "fabric-loader-0.20.1-26.2".to_string(),
            version_type: "release".into(),
            loader: Some("Fabric".to_string()),
            loader_version: Some("0.20.1".to_string()),
            mc_version: "26.2".to_string(),
            size: 1,
            last_modified: Some(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(60)),
            has_jar: true,
            icon_data_url: None,
            ..Default::default()
        };
        vanilla.last_modified = Some(std::time::SystemTime::UNIX_EPOCH);
        fabric.last_modified =
            Some(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(60));
        let instances = group_instances(&[vanilla, fabric], None);
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].active, "fabric-loader-0.20.1-26.2");
    }

    /// 整合包的版本 id 是它自己的名字，但 Minecraft 版本要看继承的那一层
    ///
    /// 拿不到就是最底层的原版版本
    #[test]
    fn detects_mc_version_through_inheritance() {
        let dir = std::env::temp_dir().join(format!("htmcl-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let vanilla = VersionJson {
            id: "26.3".into(),
            version_type: "release".into(),
            ..Default::default()
        };
        write_local_json(&dir, &vanilla).unwrap();
        let pack = VersionJson {
            id: "fps".into(),
            inherits_from: Some("26.3".into()),
            main_class: "net.fabricmc.loader.impl.launch.knot.KnotClient".into(),
            ..Default::default()
        };
        write_local_json(&dir, &pack).unwrap();

        let resolved = resolve(&dir, "fps").expect("应能解析整合包版本");
        let mc = detect_mc_version(&dir, "fps", &resolved.json);
        assert_eq!(mc, "26.3", "整合包要认出它底层的 Minecraft 版本");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 加载器带来的同名库要替换掉原版那份
    ///
    /// Forge 带的是 log4j 2.15，Minecraft 1.16.5 自带的是 2.8.1，
    /// 两份都在的话旧的那份先被加载，Forge 的日志转换器会直接崩。
    #[test]
    fn child_library_replaces_parent_one() {
        let parent = VersionJson {
            libraries: vec![
                Library {
                    name: "org.apache.logging.log4j:log4j-core:2.8.1".into(),
                    ..Default::default()
                },
                Library {
                    name: "com.mojang:text2speech:1.11.3".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let child = VersionJson {
            libraries: vec![Library {
                name: "org.apache.logging.log4j:log4j-core:2.15.0".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let merged = merge(&parent, &child);
        assert_eq!(merged.libraries.len(), 2, "同名库不该出现两份");
        let core = merged
            .libraries
            .iter()
            .find(|l| l.name.contains("log4j-core"))
            .expect("应有 log4j-core");
        assert_eq!(core.name, "org.apache.logging.log4j:log4j-core:2.15.0");
    }

    /// 主 jar 与 natives jar 是两个文件，不能当成同一个库合并
    #[test]
    fn main_jar_is_not_merged_with_natives() {
        let parent = VersionJson {
            libraries: vec![Library {
                name: "org.lwjgl:lwjgl-freetype:3.4.3".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let child = VersionJson {
            libraries: vec![Library {
                name: "org.lwjgl:lwjgl-freetype:3.4.3:natives-windows".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let merged = merge(&parent, &child);
        assert_eq!(merged.libraries.len(), 2, "主 jar 与 natives jar 都得留着");
        assert!(merged
            .libraries
            .iter()
            .any(|l| l.name == "org.lwjgl:lwjgl-freetype:3.4.3"));
        assert!(merged
            .libraries
            .iter()
            .any(|l| l.name == "org.lwjgl:lwjgl-freetype:3.4.3:natives-windows"));
    }

    /// 子版本没写 name 时沿用父版本的名字
    #[test]
    fn merge_takes_name_from_child_or_parent() {
        let parent = VersionJson {
            id: "26.2".into(),
            name: "HTMCL".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            ..Default::default()
        };
        let child = VersionJson {
            id: "fabric-loader-0.20.1-26.2".into(),
            inherits_from: Some("26.2".into()),
            ..Default::default()
        };
        let merged = merge(&parent, &child);
        assert_eq!(merged.name, "HTMCL");

        let child_named = VersionJson {
            name: "别的名字".into(),
            ..child.clone()
        };
        assert_eq!(merge(&parent, &child_named).name, "别的名字");
    }

    /// 写版本 JSON 时原样保留 name 字段
    #[test]
    fn write_local_json_keeps_name() {
        let dir = std::env::temp_dir().join(format!("htmcl-brand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let json = VersionJson {
            id: "26.2".into(),
            name: "1.0".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            ..Default::default()
        };
        write_local_json(&dir, &json).unwrap();
        let saved = read_local_json(&dir, "26.2").unwrap();
        assert_eq!(saved.name, "1.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 整合包实例自己成一条，不跟同 Minecraft 版本的原版合并
    #[test]
    fn modpack_instance_stays_separate() {
        let dir = std::env::temp_dir().join(format!("htmcl-modpack-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // 原版版本
        write_local_json(
            &dir,
            &VersionJson {
                id: "26.2".into(),
                version_type: "release".into(),
                ..Default::default()
            },
        )
        .unwrap();

        // 整合包实例：自己的版本描述 + 标记文件
        let pack_dir = version_dir(&dir, "my-pack");
        crate::core::paths::ensure_dir(&pack_dir).unwrap();
        write_local_json(
            &dir,
            &VersionJson {
                id: "my-pack".into(),
                inherits_from: Some("26.2".into()),
                version_type: "release".into(),
                ..Default::default()
            },
        )
        .unwrap();
        crate::core::minecraft::modpack::write_marker(
            &pack_dir,
            &ModpackInfo {
                name: "我的整合包".into(),
                mc_version: "26.2".into(),
                loader: Some("Fabric".into()),
                loader_version: Some("0.20.1".into()),
                project_id: None,
                pack_version_id: None,
                source: None,
            },
        )
        .unwrap();

        let versions = scan_local(&dir);
        assert_eq!(versions.len(), 2);

        let pack = versions.iter().find(|v| v.id == "my-pack").unwrap();
        // 关键：整合包的 mc_version 是自己的 id，因此不会跟 26.2 归到一起
        assert_eq!(pack.mc_version, "my-pack");
        assert_eq!(pack.loader.as_deref(), Some("Fabric"));
        assert_eq!(pack.loader_version.as_deref(), Some("0.20.1"));
        assert!(pack.modpack.is_some());

        let instances = group_instances(&versions, None);
        assert_eq!(instances.len(), 2);
        let pack_inst = instances.iter().find(|i| i.is_modpack).unwrap();
        assert_eq!(pack_inst.mc_version, "my-pack");
        assert_eq!(pack_inst.display_name, "我的整合包");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
