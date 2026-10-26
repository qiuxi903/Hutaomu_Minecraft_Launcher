//! 整合包实例
//!
//! 整合包不是单个模组或光影，它自带一整套模组、配置与脚本，
//! 必须作为一个**独立实例**存在，铺进现有实例的目录会让两个整合包互相串。
//!
//! 这里负责实例目录的标记、实例 id 的生成，以及整合包自己的版本描述。

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::core::minecraft::version::VersionJson;

/// 整合包实例的标记文件名
///
/// 放在版本目录里，扫描器看到它就知道这个目录是一个整合包实例，
/// 于是单独列成一条，不跟同 Minecraft 版本的原版合并到一起。
pub const MARKER_FILE: &str = "htmcl-modpack.json";

/// 整合包实例的信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModpackInfo {
    /// 整合包名称
    pub name: String,
    /// Minecraft 版本
    pub mc_version: String,
    /// 加载器名，如 Fabric
    #[serde(default)]
    pub loader: Option<String>,
    /// 加载器版本号，如 0.20.1
    #[serde(default)]
    pub loader_version: Option<String>,
    /// Modrinth 项目 id
    #[serde(default)]
    pub project_id: Option<String>,
    /// Modrinth 上的整合包版本 id
    ///
    /// 记下来才能在版本管理里对同一个整合包重新安装。
    #[serde(default)]
    pub pack_version_id: Option<String>,
    /// 整合包来自哪个平台：`modrinth` 或 `curseforge`
    ///
    /// 重新安装时要回到原来的平台去取文件。
    #[serde(default)]
    pub source: Option<String>,
}

/// 标记文件在版本目录下的位置
pub fn marker_path(version_dir: &Path) -> PathBuf {
    version_dir.join(MARKER_FILE)
}

/// 读取整合包标记，不是整合包实例时返回 None
pub fn read_marker(version_dir: &Path) -> Option<ModpackInfo> {
    let text = std::fs::read_to_string(marker_path(version_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 写入整合包标记
pub fn write_marker(version_dir: &Path, info: &ModpackInfo) -> Result<()> {
    crate::core::paths::ensure_dir(version_dir)?;
    let text = serde_json::to_string_pretty(info)?;
    std::fs::write(marker_path(version_dir), text)?;
    Ok(())
}

/// 这个版本是不是整合包实例
pub fn is_instance_dir(game_dir: &Path, id: &str) -> bool {
    is_instance(&game_dir.join("versions").join(id))
}

/// 这个版本目录是不是一个整合包实例
pub fn is_instance(version_dir: &Path) -> bool {
    marker_path(version_dir).is_file()
}

/// 把项目 slug 变成合法的实例 id
///
/// 只保留字母、数字、横线与下划线，其余字符换成横线，
/// 免得版本目录名里出现路径分隔符之类的怪字符。
pub fn sanitize_id(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "modpack".to_string()
    } else {
        out
    }
}

/// 生成一个不与现有版本目录冲突的实例 id
pub fn unique_id(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// 生成整合包实例自己的版本描述
///
/// 加载器版本描述的副本，只把 id 换成实例 id：
/// 继承链、依赖库、启动参数跟加载器版本完全一致，
/// 而实例目录里的模组与配置又是各自独立的。
pub fn write_instance_json(
    version_dir: &Path,
    instance_id: &str,
    loader_json: &VersionJson,
    name: &str,
) -> Result<()> {
    let mut json = loader_json.clone();
    json.id = instance_id.to_string();
    json.name = name.to_string();
    json.local_path = None;
    crate::core::paths::ensure_dir(version_dir)?;
    let path = version_dir.join(format!("{instance_id}.json"));
    let text = serde_json::to_string_pretty(&json)?;
    std::fs::write(path, text)?;
    Ok(())
}

/// 生成原版整合包的版本描述
///
/// 原版整合包没有加载器，版本描述只声明继承原版版本，
/// 客户端 jar 与依赖库跟原版实例共用一份，不重复下载。
pub fn write_vanilla_json(
    version_dir: &Path,
    instance_id: &str,
    mc_version: &str,
    name: &str,
) -> Result<()> {
    let json = VersionJson {
        id: instance_id.to_string(),
        name: name.to_string(),
        version_type: "release".to_string(),
        inherits_from: Some(mc_version.to_string()),
        ..Default::default()
    };
    crate::core::paths::ensure_dir(version_dir)?;
    let path = version_dir.join(format!("{instance_id}.json"));
    let text = serde_json::to_string_pretty(&json)?;
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_becomes_safe_instance_id() {
        assert_eq!(sanitize_id("All The Mods 9"), "all-the-mods-9");
        assert_eq!(sanitize_id("pack/with\\bad:chars"), "pack-with-bad-chars");
        assert_eq!(sanitize_id("---"), "modpack");
    }

    #[test]
    fn id_avoids_taken_names() {
        let taken = vec!["atm9".to_string(), "atm9-2".to_string()];
        assert_eq!(unique_id("atm9", &taken), "atm9-3");
        assert_eq!(unique_id("other", &taken), "other");
    }

    #[test]
    fn marker_round_trip() {
        let dir = std::env::temp_dir().join("htmcl-modpack-test");
        let _ = std::fs::remove_dir_all(&dir);
        let info = ModpackInfo {
            name: "测试整合包".into(),
            mc_version: "26.2".into(),
            loader: Some("Fabric".into()),
            loader_version: Some("0.20.1".into()),
            project_id: Some("abc".into()),
            pack_version_id: Some("ver1".into()),
            source: Some("curseforge".into()),
        };
        write_marker(&dir, &info).unwrap();
        let read = read_marker(&dir).unwrap();
        assert_eq!(read.name, "测试整合包");
        assert_eq!(read.mc_version, "26.2");
        assert_eq!(read.loader.as_deref(), Some("Fabric"));
        assert_eq!(read.source.as_deref(), Some("curseforge"));
        assert!(is_instance(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
