//! 模组自带的内容目录
//!
//! 有些模组把蓝图、投影之类的文件放在自己的目录下，
//! 装了这些模组，版本实例的左侧导航里才需要多出对应的入口。
//!
//! 目录位置取自各模组自己的实现：
//! - 机械动力（Create）的 `CreatePaths.SCHEMATICS_DIR` 指向游戏目录下的 `schematics`
//! - 投影（Litematica）同样把蓝图放在游戏目录的 `schematics` 下，后缀是 `.litematic`

use std::path::Path;

use super::modmeta;

/// 一个模组带来的内容入口
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModFeature {
    /// 模组 id（fabric.mod.json 的 id、mods.toml 的 modId）
    pub mod_id: &'static str,
    /// 导航项 id
    pub section: &'static str,
    /// 导航项显示名
    pub label: &'static str,
    /// 导航项图标
    pub icon: &'static str,
    /// 相对实例目录的目录，按顺序查找
    ///
    /// 第一个是习惯用的位置，后面是模组自己实际读的位置，
    /// 两个都列出来，文件放在哪边都能看到。
    pub dirs: &'static [&'static str],
    /// 关心的文件扩展名（小写，不含点）
    pub exts: &'static [&'static str],
}

/// 已适配的模组内容
pub const MOD_FEATURES: &[ModFeature] = &[
    ModFeature {
        mod_id: "create",
        section: "create_schematic",
        label: "蓝图",
        icon: "mod",
        dirs: &["blueprints", "schematics"],
        exts: &["nbt", "schematic"],
    },
    ModFeature {
        mod_id: "litematica",
        section: "litematic",
        label: "投影蓝图",
        icon: "cube",
        dirs: &["blueprints", "schematics"],
        exts: &["litematic"],
    },
];

/// 扫描实例的模组目录，找出装了哪些已适配的模组
pub fn detect(game_dir: &Path) -> Vec<ModFeature> {
    let mods_dir = game_dir.join("mods");
    let Ok(entries) = std::fs::read_dir(&mods_dir) else {
        return Vec::new();
    };
    let mut found: Vec<ModFeature> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_jar = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("jar"))
            .unwrap_or(false);
        if !is_jar {
            continue;
        }
        let Some(meta) = modmeta::read_meta(&path) else {
            continue;
        };
        let Some(feature) = MOD_FEATURES.iter().find(|f| f.mod_id == meta.mod_id) else {
            continue;
        };
        if !found.iter().any(|f| f.section == feature.section) {
            log::info!("检测到模组内容入口: {} ({})", feature.label, feature.mod_id);
            found.push(*feature);
        }
    }
    found
}

/// 按 id 找一个模组内容入口
pub fn find_by_section(section: &str) -> Option<&'static ModFeature> {
    MOD_FEATURES.iter().find(|f| f.section == section)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_feature_by_section() {
        assert_eq!(
            find_by_section("create_schematic").map(|f| f.mod_id),
            Some("create")
        );
        assert_eq!(find_by_section("litematic").map(|f| f.mod_id), Some("litematica"));
        assert!(find_by_section("not_exist").is_none());
    }

    /// 目录位置不能写错，Create 与 Litematica 的蓝图都在游戏目录下
    #[test]
    fn features_use_expected_dirs() {
        let create = find_by_section("create_schematic").unwrap();
        assert!(create.dirs.contains(&"blueprints"));
        assert!(create.dirs.contains(&"schematics"));
        assert!(create.exts.contains(&"nbt"));

        let litematica = find_by_section("litematic").unwrap();
        assert!(litematica.dirs.contains(&"blueprints"));
        assert!(litematica.exts.contains(&"litematic"));
    }

    /// 没装模组时不该有入口
    #[test]
    fn detect_returns_empty_without_mods_dir() {
        let dir = std::env::temp_dir().join("htmcl-modfeature-no-such-dir");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(detect(&dir).is_empty());
    }
}
