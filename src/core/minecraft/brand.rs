//! 游戏内主菜单左下角显示的启动器名字
//!
//! 主菜单左下角那行文字是 `Minecraft ` 加上版本名，
//! 版本名取自 classpath 里的 `/version.json`
//! （`DetectedVersion.tryDetectVersion()` 从 classpath 读这个资源），
//! 与 `versions` 目录下那份版本 JSON 无关。
//!
//! 客户端 jar 里那份 `version.json` 是签过名的，改了会触发
//! `SHA-384 digest error` 导致游戏起不来，所以分两种办法：
//!
//! - 原版（以及 Forge、OptiFine 这类按 classpath 顺序找资源的加载器）：
//!   做一个只含 `version.json` 的目录，放在 classpath 最前面。
//!   客户端 jar 保持官方原样，签名完好，不会多出「（已修改）」标记。
//! - Fabric、Quilt：它们的 Knot 优先读自己 classpath 里的客户端 jar，
//!   目录排在后面没用，得换一份品牌版的 jar 顶上。
//!   反正这类实例本来就带「（已修改）」标记。

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// 游戏内显示的名字
pub const BRAND_NAME: &str = "HTMCL";

/// jar 内版本信息文件
const VERSION_ENTRY: &str = "version.json";

/// 品牌目录名，放在版本目录里
const BRAND_DIR: &str = "htmcl-brand";

/// 品牌 jar 名，放在版本目录里
const BRAND_JAR: &str = "htmcl-client.jar";

/// 读出客户端 jar 里的 version.json
fn read_version_json(jar_path: &Path) -> Result<serde_json::Value> {
    let file = File::open(jar_path)
        .with_context(|| format!("打开客户端 jar 失败: {}", jar_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("读取客户端 jar 失败: {}", jar_path.display()))?;
    let mut entry = archive
        .by_name(VERSION_ENTRY)
        .with_context(|| format!("客户端 jar 里没有 {VERSION_ENTRY}"))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    serde_json::from_str(&text).context("解析 jar 内 version.json 失败")
}

/// 官方版本名后面加一个品牌名
///
/// 官方原本的版本名要留着（`26.2` 就是 `26.2`），
/// 品牌名只是额外加上去的一段：`26.2 (HTMCL)`。
fn brand_name(original: &str) -> String {
    let original = original.trim();
    if original.is_empty() {
        return BRAND_NAME.to_string();
    }
    if original.contains(BRAND_NAME) {
        return original.to_string();
    }
    format!("{original} ({BRAND_NAME})")
}

/// 客户端 jar 里那份 version.json，给版本名加上品牌之后的内容
///
/// 1.13.2 及更早的版本 jar 里没有这个文件，版本名写死在代码里，
/// 这种版本返回 `None`，改不了也不必改。
fn brand_version_json(jar_path: &Path) -> Result<Option<serde_json::Value>> {
    let mut archive = match ZipArchive::new(
        File::open(jar_path)
            .with_context(|| format!("打开客户端 jar 失败: {}", jar_path.display()))?,
    ) {
        Ok(archive) => archive,
        Err(err) => return Err(anyhow::Error::new(err).context("读取客户端 jar 失败")),
    };
    let mut entry = match archive.by_name(VERSION_ENTRY) {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    let mut json: serde_json::Value =
        serde_json::from_str(&text).context("解析 jar 内 version.json 失败")?;
    let original = json
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    if original.trim().is_empty() {
        return Ok(None);
    }
    json["name"] = serde_json::Value::String(brand_name(&original));
    Ok(Some(json))
}

/// 版本名是不是已经是想要的样子
fn name_matches(jar_path: &Path, expected: &serde_json::Value) -> bool {
    let Ok(current) = read_version_json(jar_path) else {
        return false;
    };
    current.get("name") == expected.get("name")
}

/// 目标比源新，说明还是现成的
fn is_fresh(target: &Path, source: &Path) -> bool {
    let (Ok(t), Ok(s)) = (target.metadata(), source.metadata()) else {
        return false;
    };
    let (Ok(t), Ok(s)) = (t.modified(), s.modified()) else {
        return false;
    };
    t >= s
}

/// 品牌目录：只放一份 version.json
///
/// 放在 classpath 最前面，游戏读到的版本名就是品牌名，
/// 客户端 jar 本身不动，签名完好。
pub fn prepare_brand_dir(jar_path: &Path) -> Result<Option<PathBuf>> {
    let Some(json) = brand_version_json(jar_path)? else {
        return Ok(None);
    };
    let content = serde_json::to_string(&json).context("生成品牌版本信息失败")?;
    let dir = jar_path.with_file_name(BRAND_DIR);
    let file = dir.join(VERSION_ENTRY);
    let same = std::fs::read_to_string(&file)
        .map(|old| old == content)
        .unwrap_or(false);
    if !same {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("创建品牌目录失败: {}", dir.display()))?;
        std::fs::write(&file, &content)
            .with_context(|| format!("写入品牌版本信息失败: {}", file.display()))?;
    }
    Ok(Some(dir))
}

/// 品牌 jar：复制一份客户端 jar，改掉版本名并去掉签名
///
/// 签名去掉是必须的：jar 里的 `version.json` 一改，原来的签名就对不上了，
/// JVM 读到它会直接抛 `SHA-384 digest error`。
pub fn prepare_brand_jar(jar_path: &Path) -> Result<Option<PathBuf>> {
    let Some(json) = brand_version_json(jar_path)? else {
        return Ok(None);
    };
    let patched = serde_json::to_string(&json).context("生成品牌版本信息失败")?;
    let out = jar_path.with_file_name(BRAND_JAR);
    // 现成的品牌 jar 内容对得上就不用重建
    if is_fresh(&out, jar_path) && name_matches(&out, &json) {
        return Ok(Some(out));
    }

    let tmp = out.with_extension("jar.tmp");
    {
        let src = File::open(jar_path)?;
        let mut archive = ZipArchive::new(src)?;
        let target = File::create(&tmp)
            .with_context(|| format!("创建临时文件失败: {}", tmp.display()))?;
        let mut writer = ZipWriter::new(target);
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let name = entry.name()?.to_string();
            if is_signature_entry(&name) {
                // 签名文件留着也没用，还会害得 JVM 去校验
                continue;
            }
            if name == VERSION_ENTRY {
                writer.start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )?;
                writer.write_all(patched.as_bytes())?;
            } else if entry.is_dir() {
                writer.add_directory(name, SimpleFileOptions::default())?;
            } else {
                // 其它条目直接搬压缩后的数据，不用重新压缩一遍
                writer.raw_copy_file(entry)?;
            }
        }
        writer.finish()?;
    }

    // 新 jar 能正常打开再替换，免得留下坏文件
    {
        let file = File::open(&tmp)?;
        ZipArchive::new(file).context("生成的品牌 jar 打不开")?;
    }
    let _ = std::fs::remove_file(&out);
    std::fs::rename(&tmp, &out)
        .with_context(|| format!("写入品牌 jar 失败: {}", out.display()))?;
    Ok(Some(out))
}

/// 是不是签名文件（.SF / .RSA / .DSA / .EC）
fn is_signature_entry(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.starts_with("META-INF/")
        && (upper.ends_with(".SF")
            || upper.ends_with(".RSA")
            || upper.ends_with(".DSA")
            || upper.ends_with(".EC"))
}

/// 客户端 jar 的版本名里有没有带上品牌
pub fn has_brand(jar_path: &Path) -> bool {
    read_version_json(jar_path)
        .ok()
        .and_then(|json| {
            json.get("name")
                .and_then(|v| v.as_str())
                .map(|name| name.contains(BRAND_NAME))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_jar(name: &str) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buf);
            writer
                .start_file(
                    VERSION_ENTRY,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .unwrap();
            writer
                .write_all(format!("{{\"id\":\"1.0\",\"name\":\"{name}\"}}").as_bytes())
                .unwrap();
            writer
                .start_file(
                    "META-INF/MOJANGCS.SF",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(b"signature").unwrap();
            writer
                .start_file(
                    "a/b.txt",
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(b"hello").unwrap();
            writer.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn brand_dir_holds_patched_version_json() {
        let dir = std::env::temp_dir().join(format!("htmcl-brand-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let jar = dir.join("client.jar");
        std::fs::write(&jar, make_jar("1.0")).unwrap();

        let brand_dir = prepare_brand_dir(&jar).unwrap().unwrap();
        let text = std::fs::read_to_string(brand_dir.join(VERSION_ENTRY)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        // 官方版本名保留，只在后面加品牌
        assert_eq!(json.get("name").unwrap().as_str().unwrap(), "1.0 (HTMCL)");
        assert_eq!(json.get("id").unwrap().as_str().unwrap(), "1.0");

        // 原 jar 不该被动过
        let original = read_version_json(&jar).unwrap();
        assert_eq!(original.get("name").unwrap().as_str().unwrap(), "1.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn brand_jar_patches_name_and_drops_signature() {
        let dir = std::env::temp_dir().join(format!("htmcl-brand-jar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let jar = dir.join("client.jar");
        std::fs::write(&jar, make_jar("1.0")).unwrap();

        let brand_jar = prepare_brand_jar(&jar).unwrap().unwrap();
        assert!(has_brand(&brand_jar));
        // 版本名保留，品牌加在后面
        let json = read_version_json(&brand_jar).unwrap();
        assert_eq!(json.get("name").unwrap().as_str().unwrap(), "1.0 (HTMCL)");
        // 签名文件去掉了，JVM 不会再校验
        let file = File::open(&brand_jar).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert!(archive.by_name("META-INF/MOJANGCS.SF").is_err());
        // 其它条目还在
        let mut text = String::new();
        archive.by_name("a/b.txt").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "hello");
        // 原 jar 不该被动过
        assert!(!has_brand(&jar));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
