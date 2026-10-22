use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionJson {
    pub id: String,
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
    /// 仅用于本地 UI 显示
    #[serde(skip)]
    pub local_path: Option<PathBuf>,
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

fn merge(parent: &VersionJson, child: &VersionJson) -> VersionJson {
    let mut libraries = parent.libraries.clone();
    libraries.extend(child.libraries.clone());

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

#[derive(Debug, Clone)]
pub struct LocalVersion {
    pub id: String,
    pub version_type: String,
    pub loader: Option<String>,
    pub mc_version: String,
    pub size: u64,
    pub last_modified: Option<std::time::SystemTime>,
    pub has_jar: bool,
    pub icon_data_url: Option<String>,
}

/// 扫描本地已安装版本，并推断加载器类型与对应 MC 版本
pub fn scan_local(game_dir: &Path) -> Vec<LocalVersion> {
    let mut result = Vec::new();
    for id in installed_versions(game_dir) {
        let json = match read_local_json(game_dir, &id) {
            Ok(j) => j,
            Err(_) => continue,
        };
        let loader = detect_loader(&id, &json);
        let mc_version = detect_mc_version(game_dir, &id, &json);
        let dir = version_dir(game_dir, &id);
        let size = paths::dir_size(&dir);
        let last_modified = std::fs::metadata(dir.join(format!("{id}.json")))
            .and_then(|m| m.modified())
            .ok();
        let has_jar = version_jar_path(game_dir, &id).is_file()
            || version_jar_path(game_dir, &json.inherits_from.clone().unwrap_or_default()).is_file();
        result.push(LocalVersion {
            id,
            version_type: json.version_type.clone(),
            loader,
            mc_version,
            size,
            last_modified,
            has_jar,
            icon_data_url: None,
        });
    }
    result.sort_by(|a, b| {
        b.last_modified
            .cmp(&a.last_modified)
            .then_with(|| b.id.cmp(&a.id))
    });
    result
}

fn detect_loader(id: &str, json: &VersionJson) -> Option<String> {
    let lower = id.to_lowercase();
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
}
