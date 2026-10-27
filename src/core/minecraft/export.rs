//! 导出实例
//!
//! 把实例里的东西打包成一个 zip，可以直接发给朋友。
//!
//! 打包结构跟实例目录保持一致，导出的 zip 解压后放进
//! `.minecraft` 里就能用：
//!
//! ```text
//! 我的实例/
//!   versions/<实例 id>/<实例 id>.json   游戏本体
//!   mods/…                             模组
//!   resourcepacks/…                    资源包
//!   shaderpacks/…                      光影
//!   saves/…                            存档
//!   servers.dat                        服务器列表
//!   config/…                           其它文件夹
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// 导出时要带上的内容
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    /// 游戏本体：实例的版本描述
    pub game: bool,
    /// 模组
    pub mods: bool,
    /// 资源包
    pub resourcepacks: bool,
    /// 光影包
    pub shaderpacks: bool,
    /// 数据包
    pub datapacks: bool,
    /// 单人存档
    pub saves: bool,
    /// 服务器列表
    pub servers: bool,
    /// 其它文件夹：配置、脚本等
    pub others: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            game: true,
            mods: true,
            resourcepacks: false,
            shaderpacks: false,
            datapacks: false,
            saves: false,
            servers: false,
            others: true,
        }
    }
}

impl ExportOptions {
    /// 是否一项都没勾
    pub fn is_empty(&self) -> bool {
        !self.game
            && !self.mods
            && !self.resourcepacks
            && !self.shaderpacks
            && !self.datapacks
            && !self.saves
            && !self.servers
            && !self.others
    }

    /// 勾选的项目名，界面上照着列
    pub fn items(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("游戏本体", self.game),
            ("模组", self.mods),
            ("资源包", self.resourcepacks),
            ("光影包", self.shaderpacks),
            ("数据包", self.datapacks),
            ("单人存档", self.saves),
            ("服务器列表", self.servers),
            ("其它文件夹", self.others),
        ]
    }
}

/// 导出结果
#[derive(Debug, Clone, Default)]
pub struct ExportSummary {
    /// 打包进去的文件数
    pub files: usize,
    /// 打包进去的总大小
    pub bytes: u64,
}

/// 「其它文件夹」包含的目录
const OTHER_FOLDERS: [&str; 4] = ["config", "scripts", "kubejs", "defaultconfigs"];

/// 导出实例
///
/// `game_dir` 是游戏根目录，`instance_dir` 是这个实例自己的目录
/// （开着版本隔离时是 `versions/<MC 版本>`，里面放着模组、配置与存档）。
/// `instance_id` 是实例在版本目录里的名字，`instance_name` 用来当 zip 的顶层目录名。
pub fn export_instance(
    game_dir: &Path,
    instance_dir: &Path,
    instance_id: &str,
    instance_name: &str,
    opts: ExportOptions,
    dest: &Path,
) -> Result<ExportSummary> {
    if opts.is_empty() {
        anyhow::bail!("至少要选一项内容再导出");
    }
    if let Some(parent) = dest.parent() {
        crate::core::paths::ensure_dir(parent)?;
    }
    let file = std::fs::File::create(dest)
        .with_context(|| format!("创建导出文件失败: {}", dest.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut summary = ExportSummary::default();
    let root = instance_name.trim();
    let root = if root.is_empty() { instance_id } else { root };

    // ---- 游戏本体 ----
    if opts.game {
        // 版本描述可能放在版本目录里，也可能就在实例目录里，两边都找一下
        let json = [
            game_dir
                .join("versions")
                .join(instance_id)
                .join(format!("{instance_id}.json")),
            instance_dir.join(format!("{instance_id}.json")),
        ]
        .into_iter()
        .find(|p| p.is_file());
        if let Some(json) = json {
            let file_name = json
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let name = format!("{root}/versions/{instance_id}/{file_name}");
            add_file(&mut zip, &json, &name, &options, &mut summary)?;
            // 版本描述里可能继承别的版本，那份也得带上
            if let Some(parent_id) = inherited_from(&json) {
                let parent_json = game_dir
                    .join("versions")
                    .join(&parent_id)
                    .join(format!("{parent_id}.json"));
                if parent_json.is_file() {
                    let name = format!("{root}/versions/{parent_id}/{parent_id}.json");
                    add_file(&mut zip, &parent_json, &name, &options, &mut summary)?;
                }
            }
        }
    }

    // ---- 模组、光影、资源包、数据包、存档 ----
    for (enabled, folder) in [
        (opts.mods, "mods"),
        (opts.resourcepacks, "resourcepacks"),
        (opts.shaderpacks, "shaderpacks"),
        (opts.datapacks, "datapacks"),
        (opts.saves, "saves"),
    ] {
        if !enabled {
            continue;
        }
        let src = instance_dir.join(folder);
        if !src.is_dir() {
            continue;
        }
        add_dir(&mut zip, &src, &format!("{root}/{folder}"), &options, &mut summary)?;
    }

    // ---- 服务器列表 ----
    if opts.servers {
        let src = instance_dir.join("servers.dat");
        if src.is_file() {
            let name = format!("{root}/servers.dat");
            add_file(&mut zip, &src, &name, &options, &mut summary)?;
        }
    }

    // ---- 其它文件夹 ----
    if opts.others {
        for folder in OTHER_FOLDERS {
            let src = instance_dir.join(folder);
            if !src.is_dir() {
                continue;
            }
            add_dir(&mut zip, &src, &format!("{root}/{folder}"), &options, &mut summary)?;
        }
    }

    zip.finish().context("写入导出文件失败")?;
    Ok(summary)
}

/// 版本描述里继承的是哪个版本
fn inherited_from(json_path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(json_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("inheritsFrom")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// 把一个文件加进压缩包
fn add_file(
    zip: &mut zip::ZipWriter<std::fs::File>,
    src: &Path,
    name: &str,
    options: &zip::write::SimpleFileOptions,
    summary: &mut ExportSummary,
) -> Result<()> {
    let data = std::fs::read(src).with_context(|| format!("读取失败: {}", src.display()))?;
    zip.start_file(name, *options)
        .with_context(|| format!("写入压缩包失败: {name}"))?;
    zip.write_all(&data)?;
    summary.files += 1;
    summary.bytes += data.len() as u64;
    Ok(())
}

/// 把一个目录整个加进压缩包
fn add_dir(
    zip: &mut zip::ZipWriter<std::fs::File>,
    src: &Path,
    prefix: &str,
    options: &zip::write::SimpleFileOptions,
    summary: &mut ExportSummary,
) -> Result<()> {
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = match entry.path().strip_prefix(src) {
            Ok(rel) => rel,
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() || !entry.file_type().is_file() {
            continue;
        }
        let rel_text = rel.to_string_lossy().replace('\\', "/");
        let name = format!("{prefix}/{rel_text}");
        add_file(zip, entry.path(), &name, options, summary)?;
    }
    Ok(())
}

/// 默认的导出文件名
pub fn default_file_name(instance_name: &str) -> String {
    let safe: String = instance_name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let safe = safe.trim_matches('-').to_string();
    let safe = if safe.is_empty() { "实例" } else { &safe };
    format!("{safe}.zip")
}

/// 导出文件放在哪
pub fn default_dest(instance_name: &str) -> PathBuf {
    crate::core::paths::launcher_dir()
        .join("导出")
        .join(default_file_name(instance_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_default_covers_core_content() {
        let opts = ExportOptions::default();
        assert!(opts.game);
        assert!(opts.mods);
        assert!(opts.others);
        assert!(!opts.is_empty());
    }

    #[test]
    fn empty_options_are_detected() {
        let opts = ExportOptions {
            game: false,
            mods: false,
            resourcepacks: false,
            shaderpacks: false,
            datapacks: false,
            saves: false,
            servers: false,
            others: false,
        };
        assert!(opts.is_empty());
    }

    #[test]
    fn file_name_is_safe() {
        // 中文名里的字符会被换成横线，首尾的横线再去掉
        assert_eq!(default_file_name("我的实例"), "实例.zip");
        assert_eq!(default_file_name("atm9"), "atm9.zip");
        assert_eq!(default_file_name("  "), "实例.zip");
    }

    #[test]
    fn inherited_version_is_read_from_json() {
        let dir = std::env::temp_dir().join("htmcl-export-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.json");
        std::fs::write(&path, r#"{"id":"a","inheritsFrom":"1.20.1"}"#).unwrap();
        assert_eq!(inherited_from(&path).as_deref(), Some("1.20.1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
