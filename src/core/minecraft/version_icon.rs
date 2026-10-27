//! 版本分类与图标
//!
//! 主流启动器都会按版本类型给不同的图标：正式版是草方块、
//! 快照是命令方块、旧版是圆石、愚人节版本另给一个。
//! 整合包和自定义版本还可以在版本目录里放自己的图标，
//! 这里把三家认的位置都找一遍。

use std::path::{Path, PathBuf};

/// 版本分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VersionCategory {
    /// 正式版
    #[default]
    Release,
    /// 快照
    Snapshot,
    /// 旧版 Beta
    OldBeta,
    /// 旧版 Alpha
    OldAlpha,
    /// 愚人节版本
    AprilFools,
}

impl VersionCategory {
    /// 显示名
    pub fn label(self) -> &'static str {
        match self {
            Self::Release => "正式版",
            Self::Snapshot => "快照",
            Self::OldBeta => "旧版 Beta",
            Self::OldAlpha => "旧版 Alpha",
            Self::AprilFools => "愚人节",
        }
    }

    /// 对应的图标名（assets/icons 里的文件名）
    ///
    /// 这几张都是方块像素画，跟 Minecraft 里的方块一个路子：
    /// 正式版草方块、快照命令方块、旧版 Beta 圆石、旧版 Alpha 石头、
    /// 愚人节苦力怕脸。图标自带颜色，不跟着主题色走。
    pub fn icon(self) -> &'static str {
        match self {
            Self::Release => "grass",
            Self::Snapshot => "command",
            Self::OldBeta => "cobble",
            Self::OldAlpha => "stone",
            Self::AprilFools => "april",
        }
    }
}

/// 已知的愚人节版本号
///
/// 这些版本号的发布时间不一定正好在 4 月 1 日，
/// 只按日期判断会漏掉，所以单列一份。
const APRIL_FOOLS_IDS: [&str; 12] = [
    "20w14infinite",
    "20w14∞",
    "3d shareware v1.34",
    "1.rv-pre1",
    "15w14a",
    "22w13oneblockatatime",
    "23w13a_or_b",
    "24w14potato",
    "25w14craftmine",
    "26w14a",
    "13w12~",
    "2point0",
];

/// 判断是不是愚人节版本
///
/// 两条都算：一是已知的愚人节版本号，
/// 二是发布时间落在 4 月 1 日、版本号又不是常规正式版本的那种。
pub fn is_april_fools(id: &str, release_time: Option<&str>) -> bool {
    let lower = id.trim().to_lowercase();
    if APRIL_FOOLS_IDS.iter().any(|known| *known == lower) {
        return true;
    }
    if lower.starts_with("2.0") {
        return true;
    }
    let Some(time) = release_time else {
        return false;
    };
    // 时间形如 2023-04-01T12:00:00+00:00，中间那五位就是月日
    if time.get(5..10) != Some("04-01") {
        return false;
    }
    !looks_like_release(&lower)
}

/// 版本号是不是常规的正式版本号，例如 1.20.1、26.2
fn looks_like_release(id: &str) -> bool {
    let mut parts = id.split('.');
    let major = parts.next().unwrap_or("");
    let minor = parts.next().unwrap_or("");
    major.parse::<u32>().is_ok() && minor.parse::<u32>().is_ok()
}

/// 按版本类型与版本号判断分类
pub fn classify(version_type: &str, id: &str, release_time: Option<&str>) -> VersionCategory {
    if is_april_fools(id, release_time) {
        return VersionCategory::AprilFools;
    }
    match version_type.trim().to_lowercase().as_str() {
        "release" => VersionCategory::Release,
        "old_beta" => VersionCategory::OldBeta,
        "old_alpha" => VersionCategory::OldAlpha,
        _ => VersionCategory::Snapshot,
    }
}

/// 版本目录里自定义图标的候选扩展名
const ICON_EXTS: [&str; 7] = ["png", "jpg", "jpeg", "bmp", "gif", "webp", "apng"];

/// 找版本目录里的自定义图标
///
/// 三家启动器的约定不一样，这里都认：
/// - HMCL：`versions/<id>/icon.png`（扩展名可以换）
/// - PCL：`versions/<id>/PCL/Logo.png`
/// - SJMCL：`versions/<id>/icon`（没有扩展名）
pub fn custom_icon(version_dir: &Path) -> Option<PathBuf> {
    for ext in ICON_EXTS {
        let path = version_dir.join(format!("icon.{ext}"));
        if path.is_file() {
            return Some(path);
        }
    }
    let pcl = version_dir.join("PCL").join("Logo.png");
    if pcl.is_file() {
        return Some(pcl);
    }
    let sjmcl = version_dir.join("icon");
    if sjmcl.is_file() {
        return Some(sjmcl);
    }
    None
}

/// 给版本目录换一个自定义图标
///
/// 图片复制进版本目录，命名成 `icon.<扩展名>`，
/// 与 HMCL 的约定一致，其它启动器也认。
pub fn set_custom_icon(version_dir: &Path, source: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(source.is_file(), "选择的图标文件不存在");
    crate::core::paths::ensure_dir(version_dir)?;
    // 先把旧的清掉，免得两个图标同时存在
    clear_custom_icon(version_dir)?;
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let ext = if ICON_EXTS.contains(&ext.as_str()) {
        ext
    } else {
        "png".to_string()
    };
    let dest = version_dir.join(format!("icon.{ext}"));
    std::fs::copy(source, &dest)?;
    Ok(())
}

/// 清掉版本目录里的自定义图标，恢复成默认样子
pub fn clear_custom_icon(version_dir: &Path) -> anyhow::Result<()> {
    for ext in ICON_EXTS {
        let path = version_dir.join(format!("icon.{ext}"));
        if path.is_file() {
            let _ = std::fs::remove_file(&path);
        }
    }
    let pcl = version_dir.join("PCL").join("Logo.png");
    if pcl.is_file() {
        let _ = std::fs::remove_file(&pcl);
    }
    let sjmcl = version_dir.join("icon");
    if sjmcl.is_file() {
        let _ = std::fs::remove_file(&sjmcl);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_version_type() {
        assert_eq!(classify("release", "1.20.1", None), VersionCategory::Release);
        assert_eq!(
            classify("snapshot", "15w14a", None),
            VersionCategory::AprilFools
        );
        assert_eq!(
            classify("snapshot", "23w31a", None),
            VersionCategory::Snapshot
        );
        assert_eq!(
            classify("old_beta", "b1.7.3", None),
            VersionCategory::OldBeta
        );
        assert_eq!(
            classify("old_alpha", "a1.2.6", None),
            VersionCategory::OldAlpha
        );
    }

    /// 发布时间在 4 月 1 日、版本号又不是正式版的，多半是愚人节
    #[test]
    fn april_fools_by_release_date() {
        assert!(is_april_fools(
            "24w09a",
            Some("2024-04-01T12:00:00+00:00")
        ));
        // 正式版本就算在 4 月 1 日发布也不算
        assert!(!is_april_fools(
            "1.21.5",
            Some("2025-04-01T12:00:00+00:00")
        ));
    }

    #[test]
    fn categories_have_icons() {
        for category in [
            VersionCategory::Release,
            VersionCategory::Snapshot,
            VersionCategory::OldBeta,
            VersionCategory::OldAlpha,
            VersionCategory::AprilFools,
        ] {
            assert!(!category.icon().is_empty());
            assert!(!category.label().is_empty());
        }
    }
}
