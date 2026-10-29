//! 独立游戏实例
//!
//! 同一个 Minecraft 版本可以装多个实例：每个实例有自己的目录，
//! 模组、存档、配置、截图各一份，互不干扰。
//!
//! 实例目录放在 `versions/<实例 id>/`，里面除了实例内容，
//! 还放一个标记文件 `htmcl-instance.json`，记着这个实例基于哪个 Minecraft 版本。
//! 扫描器看到标记就知道这是一个独立实例，不会把它并进同版本的其他实例里。
//!
//! 没有标记的目录是老布局：一个 Minecraft 版本一个目录，
//! 原版与各加载器变体共用它。这种目录一律当作默认实例，行为跟以前完全一样。

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::core::minecraft::version::VersionJson;

/// 实例标记文件名
pub const MARKER_FILE: &str = "htmcl-instance.json";

/// 实例标记里的信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstanceInfo {
    /// 这个实例基于哪个 Minecraft 版本
    pub mc_version: String,
    /// 实例的名字，留空用 Minecraft 版本号
    #[serde(default)]
    pub name: String,
    /// 创建时间（秒）
    #[serde(default)]
    pub created: i64,
}

/// 标记文件在实例目录下的位置
pub fn marker_path(version_dir: &Path) -> PathBuf {
    version_dir.join(MARKER_FILE)
}

/// 读实例标记，不是独立实例时返回 None
pub fn read_marker(version_dir: &Path) -> Option<InstanceInfo> {
    let text = std::fs::read_to_string(marker_path(version_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 写实例标记
pub fn write_marker(version_dir: &Path, info: &InstanceInfo) -> Result<()> {
    crate::core::paths::ensure_dir(version_dir)?;
    let text = serde_json::to_string_pretty(info)?;
    std::fs::write(marker_path(version_dir), text)?;
    Ok(())
}

/// 这个版本目录是不是一个独立实例
pub fn is_instance_dir(version_dir: &Path) -> bool {
    marker_path(version_dir).is_file()
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

/// 新建实例时要用到的源信息
pub struct InstanceSource {
    /// 装着客户端 jar 的那个目录：原版目录，或者整合包自己的目录
    pub base_dir: PathBuf,
    /// 客户端 jar 的名字，也就是 jar 文件名去掉 .jar
    pub base_id: String,
    /// 实例当前使用的变体目录：装着版本描述 json 与 natives 的那个
    pub variant_dir: PathBuf,
    /// 变体的版本描述
    pub json: VersionJson,
}

/// 复制出一个新的独立实例
///
/// 新实例有自己的目录、自己的版本描述与客户端 jar，
/// 模组、存档、配置这些内容不复制过去，新实例是干净的。
/// 依赖库与素材本来就在公共目录里，不用动。
pub fn create_instance(
    game_dir: &Path,
    source: &InstanceSource,
    new_id: &str,
    mc_version: &str,
    name: &str,
) -> Result<PathBuf> {
    let new_dir = crate::core::paths::versions_dir(game_dir).join(new_id);
    if new_dir.exists() {
        anyhow::bail!("实例目录已存在: {}", new_dir.display());
    }
    crate::core::paths::ensure_dir(&new_dir)?;

    // 版本描述：只改 id 与名字，继承链、依赖库、启动参数跟源实例一致
    let mut json = source.json.clone();
    json.id = new_id.to_string();
    json.name = name.to_string();
    json.local_path = None;
    // 指明自己的 jar，新实例才不会去动原版目录里那一份
    json.jar = Some(new_id.to_string());
    let json_text = serde_json::to_string_pretty(&json)?;
    std::fs::write(new_dir.join(format!("{new_id}.json")), json_text)?;

    // 客户端 jar：有自己的 jar 就不依赖原版目录了
    let src_jar = source.base_dir.join(format!("{}.jar", source.base_id));
    if src_jar.is_file() {
        std::fs::copy(&src_jar, new_dir.join(format!("{new_id}.jar")))?;
    }

    // natives：启动参数里按实例目录找，缺一个就起不来。
    // natives 跟着变体走，加载器版本的在自己的变体目录里
    let src_natives = source.variant_dir.join("natives");
    if src_natives.is_dir() {
        copy_dir(&src_natives, &new_dir.join("natives"))?;
    }

    write_marker(
        &new_dir,
        &InstanceInfo {
            mc_version: mc_version.to_string(),
            name: name.to_string(),
            created: chrono::Utc::now().timestamp(),
        },
    )?;

    Ok(new_dir)
}

/// 复制一个目录
fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    crate::core::paths::ensure_dir(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("htmcl-instance-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn marker_round_trip() {
        let dir = temp_dir("marker");
        let info = InstanceInfo {
            mc_version: "1.20.1".into(),
            name: "我的实例".into(),
            created: 1_700_000_000,
        };
        write_marker(&dir, &info).unwrap();
        let read = read_marker(&dir).unwrap();
        assert_eq!(read.mc_version, "1.20.1");
        assert_eq!(read.name, "我的实例");
        assert!(is_instance_dir(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn id_avoids_taken_names() {
        let taken = vec!["1.20.1".to_string(), "1.20.1-2".to_string()];
        assert_eq!(unique_id("1.20.1", &taken), "1.20.1-3");
        assert_eq!(unique_id("1.21", &taken), "1.21");
    }

    /// 新实例要有自己的目录、json、jar 与 natives
    #[test]
    fn create_instance_copies_game_files() {
        let game_dir = temp_dir("create");
        let base_dir = crate::core::paths::versions_dir(&game_dir).join("1.20.1");
        crate::core::paths::ensure_dir(&base_dir).unwrap();
        std::fs::write(base_dir.join("1.20.1.jar"), b"jar").unwrap();
        crate::core::paths::ensure_dir(&base_dir.join("natives")).unwrap();
        std::fs::write(base_dir.join("natives").join("lwjgl.dll"), b"dll").unwrap();

        let source = InstanceSource {
            base_dir: base_dir.clone(),
            base_id: "1.20.1".into(),
            variant_dir: base_dir.clone(),
            json: VersionJson {
                id: "1.20.1".into(),
                inherits_from: Some("1.20.1".into()),
                ..Default::default()
            },
        };
        let new_dir = create_instance(&game_dir, &source, "1.20.1-2", "1.20.1", "第二个实例").unwrap();

        assert!(new_dir.join("1.20.1-2.json").is_file());
        assert!(new_dir.join("1.20.1-2.jar").is_file());
        assert!(new_dir.join("natives").join("lwjgl.dll").is_file());
        let info = read_marker(&new_dir).unwrap();
        assert_eq!(info.mc_version, "1.20.1");
        assert_eq!(info.name, "第二个实例");
        // 版本描述里的 id 要换成新实例的
        let json: VersionJson =
            serde_json::from_str(&std::fs::read_to_string(new_dir.join("1.20.1-2.json")).unwrap())
                .unwrap();
        assert_eq!(json.id, "1.20.1-2");
        // 并且要用自己的 jar，不能再去动原版目录里那一份
        assert_eq!(json.jar.as_deref(), Some("1.20.1-2"));
        let _ = std::fs::remove_dir_all(&game_dir);
    }

    /// natives 在加载器变体自己的目录里，新实例要从那儿拿
    #[test]
    fn create_instance_takes_natives_from_variant_dir() {
        let game_dir = temp_dir("natives");
        let base_dir = crate::core::paths::versions_dir(&game_dir).join("1.20.1");
        crate::core::paths::ensure_dir(&base_dir).unwrap();
        std::fs::write(base_dir.join("1.20.1.jar"), b"jar").unwrap();
        let variant_dir = crate::core::paths::versions_dir(&game_dir).join("fabric-loader-0.16.0-1.20.1");
        crate::core::paths::ensure_dir(&variant_dir).unwrap();
        crate::core::paths::ensure_dir(&variant_dir.join("natives")).unwrap();
        std::fs::write(variant_dir.join("natives").join("lwjgl.dll"), b"dll").unwrap();

        let source = InstanceSource {
            base_dir: base_dir.clone(),
            base_id: "1.20.1".into(),
            variant_dir: variant_dir.clone(),
            json: VersionJson {
                id: "fabric-loader-0.16.0-1.20.1".into(),
                inherits_from: Some("1.20.1".into()),
                ..Default::default()
            },
        };
        let new_dir = create_instance(&game_dir, &source, "1.20.1-2", "1.20.1", "第二个实例").unwrap();

        assert!(new_dir.join("1.20.1-2.jar").is_file());
        assert!(new_dir.join("natives").join("lwjgl.dll").is_file());
        let _ = std::fs::remove_dir_all(&game_dir);
    }
}
