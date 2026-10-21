use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::download::DownloadSource;
use crate::core::java::JavaInfo;
use crate::core::paths;

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema: u32,
    pub game_dir: PathBuf,
    pub selected_version: Option<String>,
    pub selected_account: Option<String>,
    pub java: JavaSettings,
    pub launch: LaunchSettings,
    pub download: DownloadSettings,
    pub ui: UiSettings,
    pub news: NewsSettings,
    pub java_cache: Vec<JavaInfo>,
    pub version_overrides: HashMap<String, VersionOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JavaSettings {
    /// true = 自动从搜索到的 Java 中挑选合适的版本
    pub auto: bool,
    pub path: Option<PathBuf>,
    pub memory_mb: u32,
    pub jvm_args: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchSettings {
    pub fullscreen: bool,
    pub width: u32,
    pub height: u32,
    pub extra_args: String,
    /// 版本隔离：每个版本使用独立的存档/模组目录
    pub isolation: bool,
    pub show_game_log: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadSettings {
    pub source: DownloadSource,
    pub threads: usize,
    pub timeout_secs: u64,
    pub auto_install_dependencies: bool,
    pub use_system_proxy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub theme: String,
    pub accent: Option<String>,
    pub radius: u8,
    pub font_scale: f32,
    pub background: Background,
    pub sidebar_collapsed: bool,
    pub window_size: [f32; 2],
    pub animations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Background {
    pub kind: BackgroundKind,
    pub path: Option<PathBuf>,
    pub opacity: f32,
    pub dim: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundKind {
    None,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NewsSettings {
    pub enabled: bool,
    pub url: String,
    pub refresh_minutes: u32,
    pub last_fetch: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionOverride {
    pub memory_mb: Option<u32>,
    pub java_path: Option<PathBuf>,
    pub jvm_args: Option<String>,
    pub game_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            game_dir: paths::default_game_dir(),
            selected_version: None,
            selected_account: None,
            java: JavaSettings::default(),
            launch: LaunchSettings::default(),
            download: DownloadSettings::default(),
            ui: UiSettings::default(),
            news: NewsSettings::default(),
            java_cache: Vec::new(),
            version_overrides: HashMap::new(),
        }
    }
}

impl Default for JavaSettings {
    fn default() -> Self {
        Self {
            auto: true,
            path: None,
            memory_mb: default_memory(),
            jvm_args: String::new(),
        }
    }
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            width: 1280,
            height: 720,
            extra_args: String::new(),
            isolation: true,
            show_game_log: true,
        }
    }
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            source: DownloadSource::Official,
            threads: default_threads(),
            timeout_secs: 30,
            auto_install_dependencies: true,
            use_system_proxy: true,
        }
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            accent: None,
            radius: 10,
            font_scale: 1.0,
            background: Background::default(),
            sidebar_collapsed: false,
            window_size: [1120.0, 720.0],
            animations: true,
        }
    }
}

impl Default for Background {
    fn default() -> Self {
        Self {
            kind: BackgroundKind::None,
            path: None,
            opacity: 0.35,
            dim: 0.55,
        }
    }
}

impl Default for NewsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            url: String::new(),
            refresh_minutes: 60,
            last_fetch: None,
        }
    }
}

pub fn default_threads() -> usize {
    num_cpus::get().clamp(4, 32)
}

/// 默认内存：物理内存的 1/4，限制在 2G-8G
pub fn default_memory() -> u32 {
    let total_mb = crate::core::java::total_memory_mb();
    if total_mb == 0 {
        return 4096;
    }
    ((total_mb / 4) as u32).clamp(2048, 8192)
}

impl Config {
    pub fn load() -> Self {
        let path = paths::config_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Config>(&text) {
                Ok(mut cfg) => {
                    cfg.schema = SCHEMA;
                    cfg.download.threads = cfg.download.threads.clamp(1, 64);
                    cfg.ui.radius = cfg.ui.radius.clamp(0, 24);
                    cfg.ui.font_scale = cfg.ui.font_scale.clamp(0.8, 1.6);
                    cfg
                }
                Err(err) => {
                    log::warn!("配置文件解析失败，已备份并使用默认配置: {err}");
                    let _ = std::fs::rename(&path, paths::exe_dir().join("htmcl.json.bak"));
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = paths::config_file();
        paths::ensure_parent(&path)?;
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn version_override(&self, version: &str) -> VersionOverride {
        self.version_overrides.get(version).cloned().unwrap_or_default()
    }

    pub fn effective_memory(&self, version: &str) -> u32 {
        let ov = self.version_override(version);
        ov.memory_mb.unwrap_or(self.java.memory_mb)
    }

    pub fn effective_java(&self, version: &str) -> Option<PathBuf> {
        let ov = self.version_override(version);
        ov.java_path.or_else(|| self.java.path.clone())
    }

    pub fn effective_jvm_args(&self, version: &str) -> String {
        let ov = self.version_override(version);
        match ov.jvm_args {
            Some(args) if !args.trim().is_empty() => args,
            _ => self.java.jvm_args.clone(),
        }
    }

    /// 该版本实际使用的游戏目录（开启版本隔离时返回 versions/<id>）
    pub fn effective_game_dir(&self, version: &str) -> PathBuf {
        let ov = self.version_override(version);
        if let Some(dir) = ov.game_dir {
            return dir;
        }
        if self.launch.isolation {
            paths::versions_dir(&self.game_dir).join(version)
        } else {
            self.game_dir.clone()
        }
    }

    pub fn set_game_dir(&mut self, dir: impl Into<PathBuf>) {
        self.game_dir = dir.into();
    }

    pub fn game_dir_exists(&self) -> bool {
        self.game_dir.is_dir()
    }

    pub fn theme_dir(&self) -> PathBuf {
        paths::themes_dir()
    }

    pub fn assets_index_dir(&self) -> PathBuf {
        paths::assets_dir(&self.game_dir).join("indexes")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        paths::libraries_dir(&self.game_dir)
    }

    pub fn versions_dir(&self) -> PathBuf {
        paths::versions_dir(&self.game_dir)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.versions_dir().join(id).join(format!("{id}.json")).is_file()
    }

    pub fn open_game_dir(&self) {
        let _ = paths::ensure_dir(&self.game_dir);
        paths::reveal(&self.game_dir);
    }

    pub fn game_dir_display(&self) -> String {
        self.game_dir.display().to_string()
    }

    pub fn path_is_inside_game_dir(&self, path: &Path) -> bool {
        path.starts_with(&self.game_dir)
    }
}
