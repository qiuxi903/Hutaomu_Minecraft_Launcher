use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::content::ContentSource;
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
    /// 每个版本的启动统计（启动次数、最后启动时间）
    pub version_stats: HashMap<String, VersionStat>,
    /// 每个实例的自定义信息（名字、描述、分类、收藏）
    pub instance_meta: HashMap<String, InstanceMeta>,
}

/// 实例的自定义信息
///
/// 键是实例 id，跟版本列表里显示的那个一致。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InstanceMeta {
    /// 自定义名字，留空用默认显示名
    pub name: String,
    /// 实例描述
    pub description: String,
    /// 分类标签
    pub category: String,
    /// 是否收藏
    pub favorite: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionStat {
    pub launches: u32,
    /// 最后启动的 Unix 时间戳
    pub last_launch: i64,
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
    /// 自动分配内存：按物理内存推算，不再手动填
    pub auto_memory: bool,
    /// 初始内存 -Xms，0 表示不设置
    pub initial_memory_mb: u32,
    /// 游戏窗口标题，留空用游戏默认
    pub custom_title: String,
    /// 进程优先级：normal / above / high / below / low
    pub process_priority: String,
    /// 垃圾回收器：auto / g1 / zgc / shenandoah / parallel / serial
    pub garbage_collector: String,
    /// 启动器可见性：keep / hide / hide_and_reopen / close
    pub launcher_visibility: String,
    /// 用 java.exe 而不是 javaw.exe 启动（会带控制台窗口）
    pub no_javaw: bool,
    /// 不检查 Java 与游戏版本的兼容性
    pub no_jvm_check: bool,
    /// 不注入默认的 JVM 参数
    pub no_default_jvm_args: bool,
    /// 启动与游戏启动完成时播放提示音
    pub sound: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadSettings {
    pub source: DownloadSource,
    pub threads: usize,
    pub timeout_secs: u64,
    pub auto_install_dependencies: bool,
    pub use_system_proxy: bool,
    /// 安装游戏时默认预选的加载器：none / fabric / quilt / forge / neoforge
    pub default_loader: String,
    /// 加载器版本策略：stable（推荐稳定版）/ latest（最新）/ ask（每次询问）
    pub loader_policy: String,
    /// 加载器安装时机：parallel（与游戏同时）/ after（下载完成后）/ manual（不自动安装）
    pub loader_timing: String,
    /// 安装完成后自动把该版本设为当前版本
    pub auto_select_after_install: bool,
    /// 安装时同时下载资源文件（关闭则只装核心文件，速度更快）
    pub install_assets: bool,
    /// 并发数由网络状况自动决定
    pub auto_threads: bool,
    /// 下载缓存目录，留空用默认
    pub cache_dir: Option<PathBuf>,
    /// 下载缓存上限（MB），0 表示不限制
    pub cache_limit_mb: u64,
    /// 内容平台：Modrinth 或 CurseForge
    pub content_source: ContentSource,
    /// CurseForge 的 API Key
    ///
    /// 留空时依次找环境变量 `CURSEFORGE_API_KEY` 与编译期注入的内置 key。
    pub curseforge_key: String,
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
    /// 每次打开启动器随机换一张背景
    pub random_background: bool,
    /// 动画帧率上限，0 表示不限制
    pub max_fps: u32,
    /// 游戏日志窗口保留的最大行数
    pub max_log_lines: usize,
    /// 游戏跑起来时，日志面板默认就停靠在右边
    pub log_dock: bool,
    /// 下载中心悬浮面板被拖动后偏离右下角多少（x, y）
    pub download_panel_offset: [f32; 2],
    /// 自动清理启动器日志（30 天前）
    pub auto_purge_logs: bool,
    /// 调试模式：日志里输出更详细的信息
    pub debug_mode: bool,
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
    /// 内置资讯源 id，或 "custom" 表示使用 url
    pub source: String,
    pub url: String,
    pub refresh_minutes: u32,
    pub last_fetch: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionOverride {
    pub memory_mb: Option<u32>,
    /// 内存分配方式：global（跟随全局）/ auto（自动）/ custom（自定义）
    pub memory_mode: Option<String>,
    /// 初始内存 -Xms，0 表示不设置
    pub initial_memory_mb: Option<u32>,
    pub java_path: Option<PathBuf>,
    pub jvm_args: Option<String>,
    /// 额外的游戏启动参数
    pub extra_args: Option<String>,
    pub game_dir: Option<PathBuf>,
    /// 该版本是否启用版本隔离（独立游戏目录），None 表示跟随全局设置
    pub isolation: Option<bool>,
    /// 游戏窗口标题，留空用全局设置
    pub custom_title: Option<String>,
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
            version_stats: HashMap::new(),
            instance_meta: HashMap::new(),
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
            auto_memory: false,
            initial_memory_mb: 0,
            custom_title: String::new(),
            process_priority: "normal".into(),
            garbage_collector: "auto".into(),
            launcher_visibility: "keep".into(),
            no_javaw: false,
            no_jvm_check: false,
            no_default_jvm_args: false,
            sound: true,
        }
    }
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            // 默认使用国内镜像源，直连即可
            source: DownloadSource::Bmclapi,
            threads: default_threads(),
            timeout_secs: 30,
            auto_install_dependencies: true,
            use_system_proxy: true,
            default_loader: "none".into(),
            loader_policy: "stable".into(),
            loader_timing: "parallel".into(),
            auto_select_after_install: true,
            install_assets: true,
            auto_threads: false,
            cache_dir: None,
            cache_limit_mb: 0,
            content_source: ContentSource::default(),
            curseforge_key: String::new(),
        }
    }
}

impl DownloadSettings {
    /// 默认使用的加载器，None 表示只装原版
    pub fn default_loader_kind(&self) -> Option<crate::core::minecraft::loaders::LoaderKind> {
        crate::core::minecraft::loaders::LoaderKind::from_id(&self.default_loader)
    }

    /// 加载器版本选择策略
    pub fn loader_policy(&self) -> crate::core::minecraft::loaders::LoaderPolicy {
        crate::core::minecraft::loaders::LoaderPolicy::from_id(&self.loader_policy)
    }

    /// 加载器安装时机
    pub fn loader_timing(&self) -> crate::core::minecraft::bundle::LoaderTiming {
        crate::core::minecraft::bundle::LoaderTiming::from_id(&self.loader_timing)
    }

    /// 下载选项（并发数与资源文件开关）
    pub fn install_options(&self) -> crate::core::minecraft::install::InstallOptions {
        crate::core::minecraft::install::InstallOptions {
            threads: if self.auto_threads {
                default_threads().clamp(1, 32)
            } else {
                self.threads.clamp(1, 32)
            },
            install_assets: self.install_assets,
        }
    }

    /// 实际使用的 CurseForge API Key
    ///
    /// 优先级：设置里填的 → 环境变量 `CURSEFORGE_API_KEY` → 编译期注入的内置 key。
    pub fn curseforge_key(&self) -> Option<String> {
        let from_config = self.curseforge_key.trim();
        if !from_config.is_empty() {
            return Some(from_config.to_string());
        }
        if let Ok(value) = std::env::var("CURSEFORGE_API_KEY") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
        let builtin = crate::core::content::curseforge::BUILTIN_API_KEY.trim();
        if builtin.is_empty() {
            None
        } else {
            Some(builtin.to_string())
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
            random_background: false,
            max_fps: 0,
            max_log_lines: 4000,
            log_dock: true,
            download_panel_offset: [0.0, 0.0],
            auto_purge_logs: false,
            debug_mode: false,
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
            source: "official_zh".into(),
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

    /// 导出一份配置备份
    ///
    /// 写到启动器目录下的 `htmcl-backup-<日期>.json`。
    pub fn export_backup(&self) -> anyhow::Result<PathBuf> {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = paths::exe_dir().join(format!("htmcl-backup-{stamp}.json"));
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        Ok(path)
    }

    /// 从备份文件导入设置
    pub fn import_backup(&mut self, path: &Path) -> anyhow::Result<()> {
        let text = std::fs::read_to_string(path)?;
        let mut incoming: Config = serde_json::from_str(&text)?;
        incoming.schema = SCHEMA;
        // 路径类设置跟着当前机器走，导入备份时保留本机的目录
        incoming.game_dir = self.game_dir.clone();
        incoming.java_cache = self.java_cache.clone();
        *self = incoming;
        Ok(())
    }

    pub fn version_override(&self, mc_version: &str) -> VersionOverride {
        self.version_overrides
            .get(mc_version)
            .cloned()
            .unwrap_or_default()
    }

    pub fn effective_memory(&self, mc_version: &str) -> u32 {
        let ov = self.version_override(mc_version);
        match ov.memory_mode.as_deref() {
            // 自动：按物理内存推算
            Some("auto") => self.recommended_memory(),
            // 自定义：用实例自己填的，没填就退回全局的值
            Some("custom") => ov.memory_mb.unwrap_or(self.java.memory_mb),
            // 跟随全局
            _ => match ov.memory_mb {
                Some(mb) => mb,
                None if self.launch.auto_memory => self.recommended_memory(),
                None => self.java.memory_mb,
            },
        }
    }

    /// 该实例的初始内存 -Xms
    pub fn effective_initial_memory(&self, mc_version: &str) -> u32 {
        let ov = self.version_override(mc_version);
        ov.initial_memory_mb
            .unwrap_or(self.launch.initial_memory_mb)
    }

    /// 该实例额外的游戏启动参数
    pub fn effective_extra_args(&self, mc_version: &str) -> String {
        let ov = self.version_override(mc_version);
        match ov.extra_args {
            Some(args) if !args.trim().is_empty() => args,
            _ => self.launch.extra_args.clone(),
        }
    }

    /// 该实例的游戏窗口标题
    pub fn effective_custom_title(&self, mc_version: &str) -> String {
        let ov = self.version_override(mc_version);
        match &ov.custom_title {
            Some(title) if !title.trim().is_empty() => title.clone(),
            _ => self.launch.custom_title.clone(),
        }
    }

    /// 按物理内存推荐的上限
    ///
    /// 留三分之一给系统与启动器，最多 16G，最少 1G。
    pub fn recommended_memory(&self) -> u32 {
        let total = crate::core::java::total_memory_mb();
        if total == 0 {
            return 4096;
        }
        ((total as f64 * 2.0 / 3.0) as u32).clamp(1024, 16384)
    }

    pub fn effective_java(&self, mc_version: &str) -> Option<PathBuf> {
        let ov = self.version_override(mc_version);
        ov.java_path.or_else(|| self.java.path.clone())
    }

    pub fn effective_jvm_args(&self, mc_version: &str) -> String {
        let ov = self.version_override(mc_version);
        match ov.jvm_args {
            Some(args) if !args.trim().is_empty() => args,
            _ => self.java.jvm_args.clone(),
        }
    }

    /// 该实例实际使用的游戏目录
    ///
    /// 优先顺序：该实例单独指定的目录 → 该实例的隔离开关 → 全局隔离开关。
    /// 版本隔离开着时用 `versions/<MC 版本>`，同一个 Minecraft 版本下的
    /// 原版与各加载器版本共用这一个目录，模组、存档、配置只有一份。
    pub fn effective_game_dir(&self, mc_version: &str) -> PathBuf {
        let ov = self.version_override(mc_version);
        if let Some(dir) = ov.game_dir {
            return dir;
        }
        if self.isolation_for(mc_version) {
            paths::versions_dir(&self.game_dir).join(mc_version)
        } else {
            self.game_dir.clone()
        }
    }

    /// 该实例是否启用版本隔离
    pub fn isolation_for(&self, mc_version: &str) -> bool {
        let ov = self.version_override(mc_version);
        ov.isolation.unwrap_or(self.launch.isolation)
    }

    pub fn record_launch(&mut self, version: &str) {
        let entry = self.version_stats.entry(version.to_string()).or_default();
        entry.launches += 1;
        entry.last_launch = chrono::Utc::now().timestamp();
    }

    pub fn stat_of(&self, version: &str) -> VersionStat {
        self.version_stats.get(version).cloned().unwrap_or_default()
    }

    /// 某个实例的自定义信息
    pub fn instance_meta(&self, id: &str) -> InstanceMeta {
        self.instance_meta.get(id).cloned().unwrap_or_default()
    }

    /// 改某个实例的自定义信息
    pub fn set_instance_meta(&mut self, id: &str, meta: InstanceMeta) {
        self.instance_meta.insert(id.to_string(), meta);
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
