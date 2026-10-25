//! 读取本地模组 jar 里的元数据
//!
//! 不同加载器的元数据格式不同，这里统一读出「显示名」与「模组 id」：
//! - Fabric：`fabric.mod.json`
//! - Quilt：`quilt.mod.json`
//! - Forge / NeoForge：`META-INF/mods.toml`、`META-INF/neoforge.mods.toml`
//! - 旧版 Forge：`mcmod.info`
//!
//! 拿到 mod id 后可以再去 MCMod 词库里查中文名，
//! 查不到时退回元数据自带的名称。

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;

/// 从 jar 元数据里读出的模组信息
#[derive(Debug, Clone, Default)]
pub struct ModMeta {
    /// 模组 id，例如 `sodium`
    pub mod_id: String,
    /// 元数据里的显示名（可能是英文）
    pub name: String,
    /// 简介
    pub description: String,
    /// 版本号
    pub version: String,
    /// 加载器：fabric / quilt / forge / unknown
    pub loader: String,
    /// jar 内图标的路径（元数据里写的相对路径）
    pub icon_path: Option<String>,
    /// jar 内自带图标的原始字节
    ///
    /// 模组大多会在 jar 里带一张图，拿到它就不用联网去找头像了。
    pub icon: Option<Arc<Vec<u8>>>,
}

/// 缓存的键：路径 + 修改时间 + 大小
///
/// jar 装好后内容不会再变，换了文件这三个值就会变，缓存自然失效。
type CacheKey = (PathBuf, u64, u64);

/// 元数据缓存
///
/// 整合包动辄上百个模组，列表每帧都要过一遍，
/// 每次都重新开 zip 读文件会把界面卡住。
/// 这里按「路径 + 修改时间 + 大小」记住结果，重复扫描几乎不花钱。
fn cache() -> &'static Mutex<HashMap<CacheKey, Option<Arc<ModMeta>>>> {
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, Option<Arc<ModMeta>>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缓存最多留多少条，超了整体清掉重来
const CACHE_LIMIT: usize = 1024;

/// 取一个 jar 的身份标识
fn stamp(jar: &Path) -> Option<CacheKey> {
    let meta = std::fs::metadata(jar).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Some((jar.to_path_buf(), mtime, meta.len()))
}

/// 读取一个 jar 的模组元数据
///
/// 不是模组、或元数据无法解析时返回 None。
/// 结果带缓存，重复读取同一个 jar 不会再去碰磁盘。
pub fn read_meta(jar: &Path) -> Option<Arc<ModMeta>> {
    let key = stamp(jar)?;
    if let Ok(c) = cache().lock() {
        if let Some(hit) = c.get(&key) {
            return hit.clone();
        }
    }
    let meta = read_meta_uncached(jar).map(Arc::new);
    if let Ok(mut c) = cache().lock() {
        if c.len() >= CACHE_LIMIT {
            c.clear();
        }
        c.insert(key, meta.clone());
    }
    meta
}

/// 真正去 jar 里翻元数据
fn read_meta_uncached(jar: &Path) -> Option<ModMeta> {
    let file = std::fs::File::open(jar).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    // 先找出元数据和 jar 内图标的路径
    //
    // Forge 系的优先：有些模组在 jar 里会塞一份占位的 fabric.mod.json
    // （id 后面带 -wrongloader），读它会拿到错误的模组 id。
    let mut meta = {
        let mut found = None;
        for name in ["META-INF/mods.toml", "META-INF/neoforge.mods.toml"] {
            if let Some(text) = read_text_entry(&mut archive, name) {
                if let Some(meta) = parse_mods_toml(&text) {
                    found = Some(meta);
                    break;
                }
            }
        }
        match found {
            Some(meta) => Some(meta),
            None => {
                if let Some(value) = read_json_entry(&mut archive, "fabric.mod.json") {
                    fabric_meta(value, "fabric")
                } else if let Some(value) = read_json_entry(&mut archive, "quilt.mod.json") {
                    fabric_meta(value, "quilt")
                } else {
                    // 旧版 Forge
                    read_text_entry(&mut archive, "mcmod.info")
                        .and_then(|text| parse_mcmod_info(&text))
                }
            }
        }
    }?;

    // 图标就在 jar 里，顺手读出来
    if let Some(inner) = meta.icon_path.clone() {
        meta.icon = read_binary_entry(&mut archive, &inner).map(Arc::new);
    }
    Some(meta)
}

/// 读取 jar 内的文本条目
fn read_text_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

/// 读取 jar 内的 JSON 条目
fn read_json_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<serde_json::Value> {
    let text = read_text_entry(archive, name)?;
    serde_json::from_str(&text).ok()
}

/// 读取 jar 内的二进制条目
fn read_binary_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;
    Some(data)
}

/// fabric.mod.json / quilt.mod.json 的字段
#[derive(Debug, Deserialize, Default)]
struct FabricJson {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    /// jar 内图标的相对路径，例如 `assets/sodium/icon.png`
    #[serde(default)]
    icon: String,
}

fn fabric_meta(value: serde_json::Value, loader: &str) -> Option<ModMeta> {
    let json: FabricJson = serde_json::from_value(value).ok()?;
    if json.id.is_empty() && json.name.is_empty() {
        return None;
    }
    // 模组在别的加载器上塞的占位包，id 会带这个后缀
    if json.id.ends_with("-wrongloader") {
        return None;
    }
    Some(ModMeta {
        mod_id: json.id,
        name: json.name,
        description: json.description,
        version: json.version,
        loader: loader.to_string(),
        icon_path: if json.icon.is_empty() {
            None
        } else {
            Some(json.icon)
        },
        icon: None,
    })
}

/// 旧版 mcmod.info 的字段
#[derive(Debug, Deserialize, Default)]
struct McmodInfo {
    #[serde(default)]
    #[serde(rename = "modList")]
    mod_list: Vec<McmodEntry>,
}

#[derive(Debug, Deserialize, Default)]
struct McmodEntry {
    #[serde(default)]
    modid: String,
    #[serde(default)]
    name: String,
    /// 少数变体用驼峰的 displayName
    #[serde(default, rename = "displayName")]
    displayname: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
}

fn parse_mcmod_info(text: &str) -> Option<ModMeta> {
    let info: McmodInfo = serde_json::from_str(text).ok()?;
    let entry = info.mod_list.into_iter().next()?;
    if entry.modid.is_empty() && entry.name.is_empty() {
        return None;
    }
    let name = if entry.displayname.is_empty() {
        entry.name
    } else {
        entry.displayname
    };
    Some(ModMeta {
        mod_id: entry.modid,
        name,
        description: entry.description,
        version: entry.version,
        loader: "forge".to_string(),
        ..Default::default()
    })
}

/// 解析 `META-INF/mods.toml`
///
/// 只需要 `[[mods]]` 段里的 modId / name / version / description，
/// 不做完整 TOML 解析，遇到多行字符串也能处理。
fn parse_mods_toml(text: &str) -> Option<ModMeta> {
    let mut meta = ModMeta {
        loader: "forge".to_string(),
        ..Default::default()
    };
    let mut in_mods = false;
    let mut found = false;
    let mut desc_lines: Vec<String> = Vec::new();
    let mut in_desc = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // 段标题
        if line.starts_with('[') {
            // 段标题后面也可能跟着注释，例如 `[[mods]] #mandatory`
            in_mods = strip_comment(line) == "[[mods]]";
            in_desc = false;
            continue;
        }
        if !in_mods {
            continue;
        }
        // 多行字符串结束
        if in_desc {
            if line == "'''" || line == "\"\"\"" {
                in_desc = false;
                meta.description = desc_lines.join("\n");
                desc_lines.clear();
                continue;
            }
            desc_lines.push(line.to_string());
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        // 值后面可能跟着注释，例如 `modId="jei" #mandatory`
        let value = strip_comment(value.trim());
        match key {
            "modId" => {
                meta.mod_id = unquote(value);
                found = true;
            }
            // Forge 的 mods.toml 用 displayName 作为显示名，少数模组直接写 name
            "displayName" | "name" => {
                if meta.name.is_empty() {
                    meta.name = unquote(value);
                    found = true;
                }
            }
            "version" => {
                meta.version = unquote(value);
            }
            // jar 内图标的相对路径，例如 `logo.png`
            // Forge 写 logoFile，NeoForge 改成 iconFile，两个都认
            "logoFile" | "iconFile" => {
                if meta.icon_path.is_none() {
                    meta.icon_path = Some(unquote(value));
                }
            }
            "description" => {
                if value.starts_with("'''") || value.starts_with("\"\"\"") {
                    let body = value.trim_start_matches('\'').trim_start_matches('"');
                    if body.is_empty() {
                        // 多行字符串，内容在后面几行
                        in_desc = true;
                    } else {
                        let body = body.trim_end_matches('\'').trim_end_matches('"');
                        meta.description = body.to_string();
                    }
                } else {
                    meta.description = unquote(value);
                }
            }
            _ => {}
        }
    }

    if !desc_lines.is_empty() {
        meta.description = desc_lines.join("\n");
    }
    if found {
        Some(meta)
    } else {
        None
    }
}

/// 去掉 TOML 行尾的注释
///
/// 例如 `modId="jei" #mandatory` 里的 `#mandatory` 不是值的一部分。
fn strip_comment(value: &str) -> &str {
    // 引号里的 # 不是注释
    let mut in_quote: Option<char> = None;
    for (i, ch) in value.char_indices() {
        match in_quote {
            Some(q) => {
                if ch == q {
                    in_quote = None;
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    in_quote = Some(ch);
                } else if ch == '#' {
                    return value[..i].trim_end();
                }
            }
        }
    }
    value
}

/// 去掉 TOML 字符串两端的引号
fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    let stripped = trimmed
        .strip_prefix("'''")
        .and_then(|s| s.strip_suffix("'''"))
        .or_else(|| {
            trimmed
                .strip_prefix("\"\"\"")
                .and_then(|s| s.strip_suffix("\"\"\""))
        })
        .or_else(|| {
            trimmed
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
        })
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        });
    match stripped {
        Some(text) => text.to_string(),
        None => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forge_mods_toml() {
        let toml = r#"
modLoader="javafml"
loaderVersion="[47,)"
license="MIT"

[[mods]]
modId="sodium"
version="0.5.13"
displayName="Sodium"
description='''
A powerful rendering engine
'''

[[dependencies.sodium]]
    modId="minecraft"
"#;
        let meta = parse_mods_toml(toml).expect("应能解析");
        assert_eq!(meta.mod_id, "sodium");
        assert_eq!(meta.version, "0.5.13");
        // Forge 的 mods.toml 用 displayName
        assert_eq!(meta.name, "Sodium");
        assert_eq!(meta.description, "A powerful rendering engine");
        assert_eq!(meta.loader, "forge");
    }

    #[test]
    fn parses_mods_toml_with_plain_name() {
        let toml = r#"
[[mods]]
modId="jei"
name="Just Enough Items"
version="15.2.0"
"#;
        let meta = parse_mods_toml(toml).expect("应能解析");
        assert_eq!(meta.mod_id, "jei");
        assert_eq!(meta.name, "Just Enough Items");
        assert_eq!(meta.version, "15.2.0");
    }

    /// 官方元数据里字段后面常跟着注释，值不能把注释一起带进来
    #[test]
    fn ignores_trailing_comments_in_mods_toml() {
        let toml = r#"
[[mods]] #mandatory
modId="jei" #mandatory
version="31.14.0.72" #mandatory
displayName="Just Enough Items" #mandatory
iconFile="jei-icon.png"
"#;
        let meta = parse_mods_toml(toml).expect("应能解析");
        assert_eq!(meta.mod_id, "jei");
        assert_eq!(meta.name, "Just Enough Items");
        assert_eq!(meta.version, "31.14.0.72");
        // NeoForge 用 iconFile，Forge 用 logoFile，两个都认
        assert_eq!(meta.icon_path.as_deref(), Some("jei-icon.png"));
    }

    /// 模组在别的加载器上塞的占位包，id 会带 -wrongloader
    #[test]
    fn rejects_placeholder_fabric_meta() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"id":"journeymap-wrongloader","version":"not a fabric mod","name":"Journeymap-Forge"}"#,
        )
        .unwrap();
        assert!(fabric_meta(value, "fabric").is_none());
    }

    #[test]
    fn parses_mods_toml_without_description() {
        let toml = r#"
[[mods]]
modId="jei"
name="Just Enough Items"
version="15.2.0"
"#;
        let meta = parse_mods_toml(toml).expect("应能解析");
        assert_eq!(meta.mod_id, "jei");
        assert_eq!(meta.name, "Just Enough Items");
        assert_eq!(meta.version, "15.2.0");
    }

    #[test]
    fn parses_mcmod_info() {
        // 标准格式没有 displayName，用 name
        let json = r#"{"modList":[{"modid":"jei","name":"JEI物品管理器","version":"4.0"}]}"#;
        let meta = parse_mcmod_info(json).expect("应能解析");
        assert_eq!(meta.mod_id, "jei");
        assert_eq!(meta.name, "JEI物品管理器");
        assert_eq!(meta.version, "4.0");

        // 带驼峰 displayName 的变体优先用它
        let json2 = r#"{"modList":[{"modid":"jei","name":"JEI物品管理器","displayName":"物品管理器"}]}"#;
        let meta2 = parse_mcmod_info(json2).expect("应能解析");
        assert_eq!(meta2.name, "物品管理器");
    }

    #[test]
    fn unquote_strips_quotes() {
        assert_eq!(unquote("\"abc\""), "abc");
        assert_eq!(unquote("'abc'"), "abc");
        assert_eq!(unquote("abc"), "abc");
    }
}
