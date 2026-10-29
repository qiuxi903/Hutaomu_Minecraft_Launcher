//! 光影加载器：负责光影包与渲染增强的那一个模组
//!
//! 现在社区里不再靠装 OptiFine 来用光影，而是装对应的模组：
//! - Iris：Fabric / Quilt / NeoForge
//! - Oculus：Forge / NeoForge
//! - Sodium：Fabric / Quilt / NeoForge（渲染优化，也能带光影）
//! - Embeddium：Fabric / Forge / NeoForge（渲染优化）
//!
//! 这里只管「认出装的是哪一个」与「装 / 卸」，
//! 真正的下载仍走 Modrinth 那条路。

use std::path::Path;

use anyhow::{anyhow, Result};

use crate::core::content::modrinth::{self, ContentKind};
use crate::core::download::{DownloadProgress, Downloader};
use crate::core::minecraft::loaders::LoaderKind;

/// 一个光影加载器
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderLoader {
    Iris,
    Oculus,
    Sodium,
    Embeddium,
}

impl ShaderLoader {
    /// 显示名
    pub fn label(self) -> &'static str {
        match self {
            Self::Iris => "Iris",
            Self::Oculus => "Oculus",
            Self::Sodium => "Sodium",
            Self::Embeddium => "Embeddium",
        }
    }

    /// 一句话说清它是干什么的
    pub fn description(self) -> &'static str {
        match self {
            Self::Iris => "现代光影加载器，兼容绝大多数 OptiFine 光影包",
            Self::Oculus => "Iris 的 Forge / NeoForge 分支，光影效果一样",
            Self::Sodium => "重写渲染器提升帧率，也能加载光影",
            Self::Embeddium => "Sodium 的 Forge 分支，兼顾性能与模组兼容",
        }
    }

    /// Modrinth 上的项目标识
    pub fn project_id(self) -> &'static str {
        match self {
            Self::Iris => "iris",
            Self::Oculus => "oculus",
            Self::Sodium => "sodium",
            Self::Embeddium => "embeddium",
        }
    }

    /// 支持的加载器
    pub fn loaders(self) -> &'static [LoaderKind] {
        match self {
            Self::Iris => &[LoaderKind::Fabric, LoaderKind::Quilt, LoaderKind::NeoForge],
            Self::Oculus => &[LoaderKind::Forge, LoaderKind::NeoForge],
            Self::Sodium => &[LoaderKind::Fabric, LoaderKind::Quilt, LoaderKind::NeoForge],
            Self::Embeddium => &[LoaderKind::Fabric, LoaderKind::Forge, LoaderKind::NeoForge],
        }
    }

    /// 这个光影加载器能不能配给指定的加载器
    pub fn works_with(self, loader: Option<LoaderKind>) -> bool {
        match loader {
            Some(kind) => self.loaders().contains(&kind),
            None => false,
        }
    }

    /// 全部可选的光影加载器
    pub fn all() -> [ShaderLoader; 4] {
        [
            Self::Iris,
            Self::Oculus,
            Self::Sodium,
            Self::Embeddium,
        ]
    }

    /// 模组文件名前缀，用来在本地认出来
    fn keyword(self) -> &'static str {
        match self {
            Self::Iris => "iris",
            Self::Oculus => "oculus",
            Self::Sodium => "sodium",
            Self::Embeddium => "embeddium",
        }
    }
}

/// 实例里当前装着的光影加载器
#[derive(Debug, Clone)]
pub struct Installed {
    pub kind: ShaderLoader,
    /// 从文件名里认出来的版本号，认不出就空着
    pub version: String,
    /// 模组文件所在位置
    pub file: std::path::PathBuf,
}

/// 在实例的模组目录里找出当前装的光影加载器
///
/// 一个实例里只会有一个，认到先算数的那个。
pub fn detect(game_dir: &Path) -> Option<Installed> {
    let mods = game_dir.join("mods");
    let entries = std::fs::read_dir(&mods).ok()?;
    let mut found: Option<Installed> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_string())
        else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(".jar") {
            continue;
        }
        let stem = lower.trim_end_matches(".jar");
        for kind in ShaderLoader::all() {
            if !matches_keyword(stem, kind.keyword()) {
                continue;
            }
            if found.is_none() {
                found = Some(Installed {
                    kind,
                    version: parse_version(stem, kind.keyword()),
                    file: path,
                });
                break;
            }
        }
    }
    found
}

/// 判断文件名是不是这个模组
///
/// 模组文件名一般是「名字-版本.jar」，
/// 用前缀匹配比包含匹配准，免得别的模组名字里带了关键词被认错。
fn matches_keyword(stem: &str, keyword: &str) -> bool {
    let lower = stem.to_ascii_lowercase();
    if !lower.starts_with(keyword) {
        return false;
    }
    // 后面必须跟版本号之类的分隔符，避免把 iridium 认成 iris
    match lower[keyword.len()..].chars().next() {
        Some(c) => c == '-' || c == '_' || c == '+' || c.is_ascii_digit(),
        None => true,
    }
}

/// 从文件名里抠出版本号
fn parse_version(stem: &str, keyword: &str) -> String {
    let rest = &stem[keyword.len()..];
    let rest = rest.trim_start_matches(['-', '_']);
    let end = rest
        .find(|c: char| c == '+' || c == '-' || c == ' ')
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

/// 把加载器名转成 Modrinth 用的写法
pub fn modrinth_loader(kind: Option<LoaderKind>) -> Option<&'static str> {
    match kind {
        Some(LoaderKind::Fabric) => Some("fabric"),
        Some(LoaderKind::Quilt) => Some("quilt"),
        Some(LoaderKind::Forge) => Some("forge"),
        Some(LoaderKind::NeoForge) => Some("neoforge"),
        None => None,
    }
}

/// 装一个光影加载器
///
/// 光影加载器本身就是个模组，装到实例的 mods 目录里。
/// Iris 这类还需要 Sodium 才能跑，前置依赖一并装上。
///
/// 返回装好的模组文件路径与额外装上的依赖数量。
pub fn install(
    dl: &Downloader,
    kind: ShaderLoader,
    mc_version: &str,
    loader: LoaderKind,
    game_dir: &Path,
    progress: &std::sync::Arc<DownloadProgress>,
) -> Result<(std::path::PathBuf, usize)> {
    let Some(tag) = modrinth_loader(Some(loader)) else {
        return Err(anyhow!("原版不带光影支持，先装一个加载器"));
    };
    let versions = modrinth::versions(dl, kind.project_id(), Some(mc_version), Some(tag))?;
    let Some(version) = versions.first() else {
        return Err(anyhow!(
            "没有找到适用于 Minecraft {} 的 {}",
            mc_version,
            kind.label()
        ));
    };
    let path = modrinth::install(dl, ContentKind::Mod, version, game_dir, progress)?;

    // 前置依赖一起装上，Iris 没有 Sodium 是跑不起来的
    let deps = modrinth::required_dependencies(dl, version, Some(mc_version), Some(tag));
    let mut extra = 0;
    for dep in deps {
        progress.set_message(format!("正在安装前置依赖 {} ...", dep.version_number));
        if modrinth::install(dl, ContentKind::Mod, &dep, game_dir, progress).is_ok() {
            extra += 1;
        }
    }
    Ok((path, extra))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 认得出模组文件名() {
        assert!(matches_keyword("iris-1.7.6+mc1.20.1", "iris"));
        assert!(matches_keyword("sodium-0.5.8+mc1.20.1", "sodium"));
        assert!(matches_keyword("oculus-1.8.3", "oculus"));
        assert!(matches_keyword("embeddium-0.3.5+mc1.20.1", "embeddium"));
    }

    #[test]
    fn 不会把名字相近的模组认错() {
        assert!(!matches_keyword("iridium-1.0.0", "iris"));
        assert!(!matches_keyword("sodiumish-1.0.0", "sodium"));
        assert!(!matches_keyword("better-iris-1.0.0", "iris"));
    }

    #[test]
    fn 从文件名里取出版本号() {
        assert_eq!(parse_version("iris-1.7.6+mc1.20.1", "iris"), "1.7.6");
        assert_eq!(parse_version("oculus-1.8.3", "oculus"), "1.8.3");
        assert_eq!(parse_version("sodium-0.5.8+mc1.20.1", "sodium"), "0.5.8");
    }

    #[test]
    fn 光影加载器与加载器的搭配() {
        assert!(ShaderLoader::Iris.works_with(Some(LoaderKind::Fabric)));
        assert!(!ShaderLoader::Iris.works_with(Some(LoaderKind::Forge)));
        assert!(ShaderLoader::Oculus.works_with(Some(LoaderKind::Forge)));
        assert!(!ShaderLoader::Iris.works_with(None));
    }
}
