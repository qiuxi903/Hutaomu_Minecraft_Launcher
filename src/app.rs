use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use egui::{Align, Color32, CornerRadius, Layout, Margin, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::core::auth::{Account, SkinModel};
use crate::core::config::Config;
use crate::core::content::modrinth::{SearchResponse, Version as ModVersion};
use crate::core::download::{DownloadProgress, DownloadSource, Downloader};
use crate::core::java::JavaInfo;
use crate::core::minecraft::launch::GameProcess;
use crate::core::minecraft::loaders::{LoaderKind, LoaderVersion};
use crate::core::minecraft::manifest::VersionManifest;
use crate::core::minecraft::LocalVersion;
use crate::core::news::NewsItem;
use crate::core::paths;
use crate::core::skin::SkinImage;
use crate::icons;
use crate::theme::{self, Palette, ThemeDef};
use crate::ui::{self, widgets, Toast};
use crate::views;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Download,
    Versions,
    Accounts,
    Settings,
    About,
}

impl Page {
    pub fn all() -> [Page; 6] {
        [
            Page::Home,
            Page::Download,
            Page::Versions,
            Page::Accounts,
            Page::Settings,
            Page::About,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Page::Home => "首页",
            Page::Download => "下载",
            Page::Versions => "版本管理",
            Page::Accounts => "账号",
            Page::Settings => "设置",
            Page::About => "关于",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Page::Home => "home",
            Page::Download => "download",
            Page::Versions => "layers",
            Page::Accounts => "user",
            Page::Settings => "settings",
            Page::About => "info",
        }
    }
}

/// 自检截图模式：设置环境变量 HTMCL_SHOT=<目录> 后，
/// 启动器会依次切换页面、截图保存，然后自动退出。
pub struct ShotState {
    pub dir: PathBuf,
    pub page_index: usize,
    pub phase: u8,
    pub next_at: f64,
}

/// 自检截图覆盖的场景（页面 + 关键子标签）
pub const SHOT_SCENES: usize = 23;

impl App {
    fn apply_shot_scene(&mut self, scene: usize) {
        // 每次切换场景先关掉所有弹窗，避免互相遮挡
        self.home_ui.config_open = false;
        match scene {
            0 => self.page = Page::Home,
            1 => {
                self.page = Page::Download;
                self.download.tab = 0;
            }
            2 => {
                self.page = Page::Download;
                self.download.tab = 1;
                if self.download.loader_mc.is_empty() {
                    if let Some(manifest) = &self.manifest {
                        self.download.loader_mc = manifest.latest.release.clone();
                    }
                }
                views::download::load_loader_versions(self);
            }
            3 => {
                // Mod 标签：靠「切标签自动加载」触发，不手动搜索
                self.page = Page::Download;
                self.download.tab = 2;
                self.download.content_query = "sodium".into();
            }
            4 => self.page = Page::Versions,
            5 => self.page = Page::Accounts,
            6 => {
                self.page = Page::Settings;
                self.settings_ui.tab = 2;
            }
            7 => {
                self.page = Page::Settings;
                self.settings_ui.tab = 0;
            }
            8 => {
                self.page = Page::Settings;
                self.settings_ui.tab = 4;
            }
            9 => self.page = Page::About,
            10 => {
                self.page = Page::Home;
                self.home_ui.config_open = true;
            }
            11 => {
                // 整合包标签：同样靠自动加载
                self.page = Page::Download;
                self.download.tab = 5;
                self.download.content_query = "complementary".into();
            }
            16 => {
                // 光影标签：验证内容类型随标签切换
                self.page = Page::Download;
                self.download.tab = 3;
                self.download.content_query = "光影".into();
            }
            17 => {
                // 项目详情：验证 MCMod 链接与中文名
                self.page = Page::Download;
                self.download.tab = 2;
                self.download.content_loaded_tab = Some(2);
                views::download::open_project(self, "sodium", "jellysquid3");
            }
            18 => {
                // 版本详情：验证二级页面的导航与设置
                self.page = Page::Versions;
                if let Some(id) = self.local_versions.first().map(|v| v.id.clone()) {
                    views::versions::open_detail(self, &id);
                }
            }
            19 => {
                // 版本详情 - 设置页
                self.page = Page::Versions;
                if let Some(id) = self.local_versions.first().map(|v| v.id.clone()) {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = 1;
                }
            }
            20 => {
                // 版本详情 - 模组
                self.page = Page::Versions;
                if let Some(id) = self.local_versions.first().map(|v| v.id.clone()) {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = 4;
                }
            }
            21 => {
                // 版本详情 - 光影
                self.page = Page::Versions;
                if let Some(id) = self.local_versions.first().map(|v| v.id.clone()) {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = 5;
                }
            }
            22 => {
                // 版本详情 - 资源包
                self.page = Page::Versions;
                if let Some(id) = self.local_versions.first().map(|v| v.id.clone()) {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = 6;
                }
            }
            12 => {
                // 触发一次真实下载，用于检查下载中心面板
                self.page = Page::Home;
                if let Some(manifest) = &self.manifest {
                    if let Some(entry) = manifest
                        .versions
                        .iter()
                        .find(|v| v.id == "1.7.10")
                        .cloned()
                    {
                        let game_dir = self.cfg.game_dir.clone();
                        let id = entry.id.clone();
                        let opts = self.cfg.download.install_options();
                        self.spawn_download(format!("安装 {id}"), move |dl, progress, _job| {
                            crate::core::minecraft::install::ensure_version_json(
                                &dl, &game_dir, &entry,
                            )?;
                            crate::core::minecraft::install::install_version(
                                &dl, &game_dir, &id, &progress, opts,
                            )?;
                            Ok(format!("{id} 安装完成"))
                        });
                    }
                }
            }
            14 => {
                // 安装弹窗：选择加载器与版本
                self.page = Page::Download;
                self.download.tab = 0;
                self.download.install_target = None;
                if let Some(manifest) = &self.manifest {
                    if let Some(entry) = manifest.versions.iter().find(|v| v.id == "1.20.1").cloned()
                    {
                        views::download::open_install_dialog(self, entry);
                        // 预选 Fabric 并展示手动版本列表
                        self.download.install_loader_index = 1;
                        self.download.install_loader_auto = false;
                        views::download::load_install_loader_versions(
                            self,
                            "1.20.1",
                            crate::core::minecraft::loaders::LoaderKind::Fabric,
                        );
                    }
                }
            }
            15 => {
                // 设置页新增的加载器选项
                self.page = Page::Settings;
                self.settings_ui.tab = 1;
            }
            _ => {
                // 提示条位置与动效检查
                self.page = Page::Home;
                self.info("这是一条信息提示，用于检查提示条位置");
            }
        }
    }
}

pub struct TaskItem {
    pub id: u64,
    pub title: String,
    pub fraction: f32,
    pub message: String,
    pub done: bool,
    pub ok: bool,
    pub cancellable: bool,
    pub progress: Option<Arc<DownloadProgress>>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// 实时速度（字节/秒）
    pub speed: f32,
    pub started: std::time::Instant,
    last_bytes: u64,
    last_time: std::time::Instant,
}

#[derive(Clone)]
pub struct Job {
    pub tx: Sender<CoreEvent>,
    pub egui: egui::Context,
    pub id: u64,
}

impl Job {
    pub fn send(&self, event: CoreEvent) {
        let _ = self.tx.send(event);
        self.egui.request_repaint();
    }

    pub fn progress(&self, fraction: f32, message: impl Into<String>) {
        self.send(CoreEvent::Progress {
            id: self.id,
            fraction,
            done: 0,
            total: 0,
            message: message.into(),
        });
    }

    pub fn progress_bytes(
        &self,
        fraction: f32,
        done: u64,
        total: u64,
        message: impl Into<String>,
    ) {
        self.send(CoreEvent::Progress {
            id: self.id,
            fraction,
            done,
            total,
            message: message.into(),
        });
    }

    pub fn toast(&self, toast: Toast) {
        self.send(CoreEvent::Toast(toast));
    }

    pub fn info(&self, text: impl Into<String>) {
        self.toast(Toast::info(text));
    }

    pub fn success(&self, text: impl Into<String>) {
        self.toast(Toast::success(text));
    }

    pub fn error(&self, text: impl Into<String>) {
        self.toast(Toast::error(text));
    }

    pub fn done(&self, ok: bool, message: impl Into<String>) {
        self.send(CoreEvent::TaskDone {
            id: self.id,
            ok,
            message: message.into(),
        });
    }
}

pub enum CoreEvent {
    Toast(Toast),
    Progress {
        id: u64,
        fraction: f32,
        done: u64,
        total: u64,
        message: String,
    },
    TaskDone {
        id: u64,
        ok: bool,
        message: String,
    },
    Manifest(Result<Box<VersionManifest>, String>),
    JavaProgress(String),
    JavaDone(Result<Vec<JavaInfo>, String>),
    News(Result<Vec<NewsItem>, String>),
    Search {
        seq: u64,
        result: Result<Box<SearchResponse>, String>,
    },
    ModVersions {
        project_id: String,
        result: Result<Vec<ModVersion>, String>,
    },
    LoaderVersions {
        kind: LoaderKind,
        mc: String,
        result: Result<Vec<LoaderVersion>, String>,
    },
    /// 安装弹窗内加载器版本列表
    InstallLoaderVersions {
        kind: LoaderKind,
        mc: String,
        result: Result<Vec<LoaderVersion>, String>,
    },
    DeviceCode(Result<crate::core::auth::microsoft::DeviceCode, String>),
    MsPoll(Result<crate::core::auth::microsoft::PollOutcome, String>),
    AuthlibLogin(Result<crate::core::auth::authlib::AuthlibAuth, String>),
    SkinReady {
        account_id: String,
        path: PathBuf,
        model: SkinModel,
    },
    ProjectDetail {
        project: Box<crate::core::content::modrinth::Project>,
        versions: Vec<ModVersion>,
    },
    /// 项目的中文简介（翻译服务不可用时不会发送）
    ContentTranslated {
        project_id: String,
        text: String,
    },
    /// MCMod 元数据更新完成
    McmodDbReady(crate::core::content::mcmod::Database),
    /// 本地内容反查到的 Modrinth 项目：文件名 → slug
    ContentSlugs(std::collections::HashMap<String, String>),
    ImageReady {
        url: String,
        path: PathBuf,
    },
    Installed(String),
    GameExited {
        version: String,
        code: i32,
    },
}

pub struct SkinTexture {
    pub texture: egui::TextureHandle,
    /// 8x8 头部（含帽子叠加层）放大后的头像纹理
    pub head: egui::TextureHandle,
    pub model: SkinModel,
    pub source: SkinSource,
    /// 是否为 64x64 皮肤（有第二层）
    pub has_overlay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinSource {
    Local,
    Remote,
    Generated,
}

pub struct App {
    pub cfg: Config,
    pub accounts: crate::core::auth::AccountStore,
    pub palette: Palette,
    pub theme: ThemeDef,
    pub themes: Vec<ThemeDef>,
    pub page: Page,
    pub toasts: Vec<Toast>,
    pub tasks: Vec<TaskItem>,
    next_task_id: u64,
    tx: Sender<CoreEvent>,
    rx: Receiver<CoreEvent>,
    pub egui_ctx: egui::Context,

    pub manifest: Option<VersionManifest>,
    pub manifest_loading: bool,
    pub manifest_error: Option<String>,
    pub local_versions: Vec<LocalVersion>,
    pub javas: Vec<JavaInfo>,
    pub java_searching: bool,
    pub java_search_message: String,
    pub java_detailed: bool,
    pub java_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub news: Vec<NewsItem>,
    pub news_loading: bool,
    pub news_error: Option<String>,
    pub news_fetched_at: Option<std::time::Instant>,

    pub download: views::download::DownloadState,
    /// MCMod 元数据库（Mod 中文名、缩写与 MCMod 链接）
    pub mcmod_db: crate::core::content::mcmod::Database,
    /// MCMod 元数据是否正在更新
    pub mcmod_db_loading: bool,
    pub accounts_ui: views::accounts::AccountsState,
    pub settings_ui: views::settings::SettingsState,
    pub home_ui: views::home::HomeState,
    pub versions_ui: views::versions::VersionsState,

    pub images: HashMap<String, egui::TextureHandle>,
    images_pending: HashSet<String>,
    pub skin_cache: HashMap<String, SkinTexture>,
    pub skin_pending: HashSet<String>,
    pub preview_yaw: f32,

    pub background_frames: Vec<egui::TextureHandle>,
    pub background_delays: Vec<f32>,
    pub background_index: usize,
    pub background_next: f64,
    pub background_path: Option<PathBuf>,

    pub game: Option<GameProcess>,
    pub game_log_open: bool,
    pub maximized: bool,
    pub first_frame: bool,
    pub authlib_jar: Option<PathBuf>,
    pub dwm_applied: bool,
    pub shot: Option<ShotState>,
    pub download_panel_open: bool,
    download_panel_auto: bool,
    /// 页面切换动画：上次切换时间与上次页面
    page_changed_at: f64,
    last_page: Page,
    pub last_window_size: egui::Vec2,
    image_tx: std::sync::mpsc::Sender<String>,
    image_rx: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<String>>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::core::paths::migrate_legacy_layout();
        crate::core::log::init();
        let cfg = Config::load();
        crate::core::download::set_use_system_proxy(cfg.download.use_system_proxy);
        let accounts = crate::core::auth::AccountStore::load();
        let themes = theme::all_themes();
        let theme = themes
            .iter()
            .find(|t| t.id == cfg.ui.theme)
            .cloned()
            .unwrap_or_default();
        let accent = cfg.ui.accent.as_deref().map(theme::hex);
        let palette = Palette::from_def(&theme, accent, cfg.ui.radius);

        ui::install_fonts(&cc.egui_ctx);
        ui::install_style(&cc.egui_ctx, &palette, cfg.ui.font_scale);
        icons::warmup(&cc.egui_ctx, palette.text_weak, 16.0);

        let (tx, rx) = channel();
        let (image_tx, image_rx) = std::sync::mpsc::channel::<String>();
        let image_rx = Arc::new(std::sync::Mutex::new(image_rx));

        let mut app = Self {
            cfg,
            accounts,
            palette,
            theme,
            themes,
            page: Page::Home,
            toasts: Vec::new(),
            tasks: Vec::new(),
            next_task_id: 1,
            tx,
            rx,
            egui_ctx: cc.egui_ctx.clone(),
            manifest: None,
            manifest_loading: false,
            manifest_error: None,
            local_versions: Vec::new(),
            javas: Vec::new(),
            java_searching: false,
            java_search_message: String::new(),
            java_detailed: false,
            java_cancel: None,
            news: Vec::new(),
            news_loading: false,
            news_error: None,
            news_fetched_at: None,
            download: Default::default(),
            mcmod_db: crate::core::content::mcmod::Database::load_cached().unwrap_or_default(),
            mcmod_db_loading: false,
            accounts_ui: Default::default(),
            settings_ui: Default::default(),
            home_ui: Default::default(),
            versions_ui: Default::default(),
            images: HashMap::new(),
            images_pending: HashSet::new(),
            skin_cache: HashMap::new(),
            skin_pending: HashSet::new(),
            preview_yaw: 0.6,
            background_frames: Vec::new(),
            background_delays: Vec::new(),
            background_index: 0,
            background_next: 0.0,
            background_path: None,
            game: None,
            game_log_open: false,
            maximized: false,
            first_frame: true,
            authlib_jar: None,
            dwm_applied: false,
            download_panel_open: false,
            download_panel_auto: false,
            page_changed_at: 0.0,
            last_page: Page::Home,
            last_window_size: egui::vec2(1120.0, 720.0),
            image_tx,
            image_rx: image_rx.clone(),
            shot: std::env::var("HTMCL_SHOT").ok().map(|dir| {
                let dir = PathBuf::from(dir);
                let _ = paths::ensure_dir(&dir);
                ShotState {
                    dir,
                    page_index: 0,
                    phase: 0,
                    next_at: 8.0,
                }
            }),
        };

        app.start_image_workers(&cc.egui_ctx);
        app.javas = app.cfg.java_cache.clone();
        app.refresh_local_versions();
        app.reload_background();
        app.load_manifest();
        app.load_news();
        app.refresh_mcmod_db();
        if app.javas.is_empty() {
            app.start_java_search(false);
        }
        app
    }

    /// 后台更新 MCMod 元数据（Mod 中文名与 MCMod 链接）
    ///
    /// 缓存 7 天有效；下载失败时沿用旧数据，不影响使用。
    pub fn refresh_mcmod_db(&mut self) {
        if self.mcmod_db_loading {
            return;
        }
        if let Some(db) = crate::core::content::mcmod::Database::load_cached() {
            self.mcmod_db = db;
            return;
        }
        self.mcmod_db_loading = true;
        let source = self.cfg.download.source;
        let timeout = self.cfg.download.timeout_secs;
        self.spawn("更新 MCMod 元数据", false, move |job| {
            let result = (|| -> anyhow::Result<crate::core::content::mcmod::Database> {
                let dl = Downloader::new(source, timeout.max(30))?;
                crate::core::content::mcmod::refresh(&dl)
                    .ok_or_else(|| anyhow::anyhow!("MCMod 元数据下载失败"))
            })();
            if let Ok(db) = result {
                job.send(CoreEvent::McmodDbReady(db));
            }
            job.done(true, "");
        });
    }

    // ------------------------------------------------------------ 基础访问

    pub fn downloader(&self) -> anyhow::Result<Downloader> {
        Downloader::new(self.cfg.download.source, self.cfg.download.timeout_secs)
    }

    /// 轻量后台任务：不进入下载中心，用于拉取清单、搜索等短请求
    pub fn spawn<F>(&mut self, title: impl Into<String>, cancellable: bool, f: F) -> u64
    where
        F: FnOnce(Job) + Send + 'static,
    {
        let _ = (title, cancellable);
        let id = self.next_task_id;
        self.next_task_id += 1;
        let job = Job {
            tx: self.tx.clone(),
            egui: self.egui_ctx.clone(),
            id,
        };
        std::thread::spawn(move || f(job));
        id
    }

    /// 带下载进度的任务
    pub fn spawn_download<F>(&mut self, title: impl Into<String>, f: F) -> u64
    where
        F: FnOnce(Downloader, Arc<DownloadProgress>, Job) -> anyhow::Result<String> + Send + 'static,
    {
        let id = self.next_task_id;
        self.next_task_id += 1;
        let progress = DownloadProgress::new();
        self.tasks.push(TaskItem {
            id,
            title: title.into(),
            fraction: 0.0,
            message: "准备中 ...".into(),
            done: false,
            ok: true,
            cancellable: true,
            progress: Some(progress.clone()),
            bytes_done: 0,
            bytes_total: 0,
            speed: 0.0,
            started: std::time::Instant::now(),
            last_bytes: 0,
            last_time: std::time::Instant::now(),
        });
        self.download_panel_open = true;
        // 任务全部结束后自动收起面板；若用户手动打开过则保持打开
        self.download_panel_auto = true;

        let job = Job {
            tx: self.tx.clone(),
            egui: self.egui_ctx.clone(),
            id,
        };
        let source = self.cfg.download.source;
        let timeout = self.cfg.download.timeout_secs;

        {
            let job = job.clone();
            let progress = progress.clone();
            std::thread::spawn(move || {
                loop {
                    if progress.is_cancelled() || progress.finished.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        break;
                    }
                    job.progress_bytes(
                        progress.fraction(),
                        progress.done.load(std::sync::atomic::Ordering::Relaxed),
                        progress.total.load(std::sync::atomic::Ordering::Relaxed),
                        progress.message(),
                    );
                    std::thread::sleep(std::time::Duration::from_millis(180));
                }
            });
        }

        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<String> {
                let dl = Downloader::new(source, timeout)?;
                f(dl, progress.clone(), job.clone())
            })();
            progress
                .finished
                .store(true, std::sync::atomic::Ordering::Relaxed);
            match result {
                Ok(message) => job.done(true, message),
                Err(err) => {
                    log::error!("任务失败: {err:#}");
                    job.done(false, format!("{err}"))
                }
            }
        });
        id
    }

    pub fn toast(&mut self, toast: Toast) {
        let mut toast = toast;
        toast.created = self.egui_ctx.input(|i| i.time);
        self.toasts.push(toast);
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.toast(Toast::info(text));
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.toast(Toast::success(text));
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.toast(Toast::error(text));
    }

    pub fn save_config(&mut self) {
        if let Err(err) = self.cfg.save() {
            log::error!("保存配置失败: {err}");
            self.error(format!("保存配置失败: {err}"));
        }
    }

    pub fn save_accounts(&mut self) {
        if let Err(err) = self.accounts.save() {
            log::error!("保存账号失败: {err}");
            self.error(format!("保存账号失败: {err}"));
        }
    }

    // ------------------------------------------------------------ 主题

    pub fn apply_theme(&mut self) {
        let accent = self.cfg.ui.accent.as_deref().map(theme::hex);
        self.palette = Palette::from_def(&self.theme, accent, self.cfg.ui.radius);
        ui::install_style(&self.egui_ctx, &self.palette, self.cfg.ui.font_scale);
        icons::warmup(&self.egui_ctx, self.palette.text_weak, 16.0);
    }

    pub fn set_theme(&mut self, id: &str) {
        self.cfg.ui.theme = id.to_string();
        if let Some(found) = self.themes.iter().find(|t| t.id == id).cloned() {
            self.theme = found;
        }
        self.apply_theme();
        self.reload_background();
        self.save_config();
    }

    pub fn reload_background(&mut self) {
        self.background_frames.clear();
        self.background_delays.clear();
        self.background_index = 0;
        self.background_next = 0.0;
        self.background_path = None;

        let Some(path) = theme::resolve_background(&self.theme, &self.cfg.ui.background) else {
            return;
        };
        match theme::load_background_frames(&path) {
            Ok(frames) => {
                log::info!("已加载背景: {} ({} 帧)", path.display(), frames.len());
                for (image, delay) in frames {
                    let handle = self.egui_ctx.load_texture(
                        format!("bg:{}", path.display()),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.background_frames.push(handle);
                    self.background_delays.push(delay);
                }
                self.background_path = Some(path);
            }
            Err(err) => {
                log::warn!("加载背景失败: {err}");
                self.error(format!("背景加载失败: {err}"));
            }
        }
    }

    pub fn refresh_themes(&mut self) {
        self.themes = theme::all_themes();
    }

    // ------------------------------------------------------------ 数据刷新

    pub fn refresh_local_versions(&mut self) {
        self.local_versions = crate::core::minecraft::scan_local(&self.cfg.game_dir);
    }

    pub fn load_manifest(&mut self) {
        if self.manifest_loading {
            return;
        }
        self.manifest_loading = true;
        self.manifest_error = None;
        let source = self.cfg.download.source;
        self.spawn("获取版本列表", false, move |job| {
            match crate::core::minecraft::manifest::fetch_manifest(source) {
                Ok(manifest) => job.send(CoreEvent::Manifest(Ok(Box::new(manifest)))),
                Err(err) => job.send(CoreEvent::Manifest(Err(format!("{err}")))),
            }
            job.done(true, "");
        });
    }

    pub fn load_news(&mut self) {
        if !self.cfg.news.enabled {
            self.news = crate::core::news::builtin_tips();
            return;
        }
        self.news_loading = true;
        let source = self.cfg.news.source.clone();
        let url = self.cfg.news.url.clone();
        let timeout = self.cfg.download.timeout_secs;
        self.spawn("获取资讯", false, move |job| {
            let started = std::time::Instant::now();
            let result = (|| -> anyhow::Result<Vec<NewsItem>> {
                let dl = Downloader::new(DownloadSource::Official, timeout)?;
                match crate::core::news::fetch(&dl, &source, &url) {
                    Ok(items) => Ok(items),
                    Err(first) => {
                        // 资讯源偶发失败，重试一次
                        std::thread::sleep(std::time::Duration::from_millis(600));
                        crate::core::news::fetch(&dl, &source, &url).map_err(|second| {
                            log::warn!("资讯首次失败: {first}");
                            second
                        })
                    }
                }
            })();
            match &result {
                Ok(items) => log::info!(
                    "资讯已更新：{} 条，耗时 {:.2}s",
                    items.len(),
                    started.elapsed().as_secs_f32()
                ),
                Err(err) => log::warn!("资讯获取失败（{:.2}s）: {err}", started.elapsed().as_secs_f32()),
            }
            job.send(CoreEvent::News(result.map_err(|e| format!("{e}"))));
            job.done(true, "");
        });
    }

    pub fn start_java_search(&mut self, detailed: bool) {
        if self.java_searching {
            return;
        }
        self.java_searching = true;
        self.java_detailed = detailed;
        self.java_search_message = if detailed {
            "正在全盘搜索 Java ...".into()
        } else {
            "正在搜索 Java ...".into()
        };
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.java_cancel = Some(cancel.clone());
        self.spawn(
            if detailed { "详细搜索 Java" } else { "简要搜索 Java" },
            true,
            move |job| {
                let result = if detailed {
                    let cancel_for_progress = cancel.clone();
                    let job_progress = job.clone();
                    let _ = cancel_for_progress;
                    let list = crate::core::java::full_search(&cancel, |message| {
                        job_progress.send(CoreEvent::JavaProgress(message));
                    });
                    Ok(list)
                } else {
                    Ok(crate::core::java::brief_search())
                };
                job.send(CoreEvent::JavaDone(result.map_err(|e: String| e)));
            },
        );
    }

    pub fn cancel_java_search(&mut self) {
        if let Some(cancel) = &self.java_cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub fn selected_version(&self) -> Option<&LocalVersion> {
        let id = self.cfg.selected_version.as_deref()?;
        self.local_versions.iter().find(|v| v.id == id)
    }

    pub fn selected_version_id(&self) -> Option<String> {
        self.cfg
            .selected_version
            .clone()
            .filter(|id| self.local_versions.iter().any(|v| &v.id == id))
            .or_else(|| self.local_versions.first().map(|v| v.id.clone()))
    }

    pub fn selected_account(&self) -> Option<&Account> {
        self.accounts.selected_account()
    }

    pub fn java_for(&self, mc_version: &str) -> Option<JavaInfo> {
        if let Some(path) = self.cfg.java.path.clone() {
            if let Some(found) = self.javas.iter().find(|j| j.path == path) {
                return Some(found.clone());
            }
            if let Some(probed) = crate::core::java::probe(&path) {
                return Some(probed);
            }
        }
        crate::core::java::pick_for(mc_version, &self.javas)
    }

    // ------------------------------------------------------------ 皮肤

    pub fn ensure_skin(&mut self, account_id: &str) {
        if self.skin_cache.contains_key(account_id) || self.skin_pending.contains(account_id) {
            return;
        }
        let Some(account) = self.accounts.get(account_id).cloned() else {
            return;
        };

        // 本地皮肤文件优先
        if let Some(path) = account.skin_path() {
            if path.is_file() {
                match SkinImage::load(path) {
                    Ok(image) => {
                        let model = if account.skin_model() == SkinModel::Classic && image.detect_slim()
                        {
                            SkinModel::Slim
                        } else {
                            account.skin_model()
                        };
                        self.upload_skin(account_id, &image, model, SkinSource::Local);
                        return;
                    }
                    Err(err) => log::warn!("读取皮肤失败: {err}"),
                }
            }
        }

        // 离线账号直接生成
        if account.is_offline() {
            let image = crate::core::skin::generate_default_skin(account.name());
            self.upload_skin(account_id, &image, SkinModel::Classic, SkinSource::Generated);
            return;
        }

        // 在线账号：下载皮肤
        if let Some(url) = account.skin_url().map(|s| s.to_string()) {
            self.skin_pending.insert(account_id.to_string());
            let id = account_id.to_string();
            let timeout = self.cfg.download.timeout_secs;
            self.spawn("获取皮肤", false, move |job| {
                let result = (|| -> anyhow::Result<PathBuf> {
                    let dl = Downloader::new(DownloadSource::Official, timeout)?;
                    crate::core::skin::download_skin(&dl, &url, &id)
                })();
                match result {
                    Ok(path) => job.send(CoreEvent::SkinReady {
                        account_id: id.clone(),
                        path,
                        model: SkinModel::Classic,
                    }),
                    Err(err) => {
                        log::warn!("下载皮肤失败: {err}");
                        job.send(CoreEvent::SkinReady {
                            account_id: id.clone(),
                            path: PathBuf::new(),
                            model: SkinModel::Classic,
                        });
                    }
                }
                job.done(true, "");
            });
            return;
        }

        let image = crate::core::skin::generate_default_skin(account.name());
        self.upload_skin(account_id, &image, SkinModel::Classic, SkinSource::Generated);
    }

    pub fn upload_skin(
        &mut self,
        account_id: &str,
        image: &SkinImage,
        model: SkinModel,
        source: SkinSource,
    ) {
        let texture = self.egui_ctx.load_texture(
            format!("skin:{account_id}"),
            image.to_color_image(),
            egui::TextureOptions::NEAREST,
        );
        let head = self.egui_ctx.load_texture(
            format!("head:{account_id}"),
            image.head_icon(64),
            egui::TextureOptions::NEAREST,
        );
        self.skin_cache.insert(
            account_id.to_string(),
            SkinTexture {
                texture,
                head,
                model,
                source,
                has_overlay: image.height >= 64,
            },
        );
    }

    pub fn image(&self, url: &str) -> Option<&egui::TextureHandle> {
        self.images.get(url)
    }

    /// 按需下载并缓存远程图片（项目图标等）。请求进入共享队列，
    /// 由固定的工作线程下载，避免为每张图新建连接。
    pub fn ensure_image(&mut self, url: &str) {
        if self.images.contains_key(url) || self.images_pending.contains(url) {
            return;
        }
        if self.images_pending.len() > 120 {
            return;
        }
        self.images_pending.insert(url.to_string());
        if self.image_tx.send(url.to_string()).is_err() {
            log::warn!("图片队列已关闭");
        }
    }

    fn start_image_workers(&mut self, ctx: &egui::Context) {
        let timeout = self.cfg.download.timeout_secs;
        let source = self.cfg.download.source;
        for _ in 0..3 {
            let rx = self.image_rx.clone();
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let dl = match Downloader::new(source, timeout) {
                    Ok(dl) => dl,
                    Err(err) => {
                        log::error!("图片下载线程启动失败: {err}");
                        return;
                    }
                };
                loop {
                    let url = {
                        let guard = match rx.lock() {
                            Ok(guard) => guard,
                            Err(_) => return,
                        };
                        match guard.recv() {
                            Ok(url) => url,
                            Err(_) => return,
                        }
                    };
                    let result = (|| -> anyhow::Result<PathBuf> {
                        let dir = paths::cache_dir().join("images");
                        paths::ensure_dir(&dir)?;
                        let name = crate::core::download::sha1_hex(url.as_bytes());
                        let dest = dir.join(format!("{name}.png"));
                        if dest.is_file() {
                            return Ok(dest);
                        }
                        let bytes = dl.get_bytes_direct(&url)?;
                        std::fs::write(&dest, bytes)?;
                        Ok(dest)
                    })();
                    match result {
                        Ok(path) => {
                            let _ = tx.send(CoreEvent::ImageReady { url, path });
                        }
                        Err(err) => {
                            log::debug!("图片加载失败: {err}");
                            let _ = tx.send(CoreEvent::ImageReady {
                                url,
                                path: PathBuf::new(),
                            });
                        }
                    }
                    ctx.request_repaint();
                }
            });
        }
    }

    pub fn skin_of(&self, account_id: &str) -> Option<&SkinTexture> {
        self.skin_cache.get(account_id)
    }

    // ------------------------------------------------------------ 事件处理

    fn handle_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                CoreEvent::Toast(toast) => self.toast(toast),
                CoreEvent::Progress {
                    id,
                    fraction,
                    done,
                    total,
                    message,
                } => {
                    if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                        task.fraction = fraction;
                        if done > 0 || total > 0 {
                            task.bytes_done = done;
                            task.bytes_total = total;
                        }
                        if !message.is_empty() {
                            task.message = message;
                        }
                        let now = std::time::Instant::now();
                        let elapsed = now.duration_since(task.last_time).as_secs_f32();
                        if elapsed >= 0.4 && done >= task.last_bytes {
                            let instant =
                                (done - task.last_bytes) as f32 / elapsed;
                            task.speed = if task.speed <= 0.0 {
                                instant
                            } else {
                                task.speed * 0.6 + instant * 0.4
                            };
                            task.last_bytes = done;
                            task.last_time = now;
                        }
                    }
                }
                CoreEvent::TaskDone { id, ok, message } => {
                    self.tasks.retain(|t| t.id != id);
                    if self.tasks.is_empty() && self.download_panel_auto {
                        self.download_panel_open = false;
                        self.download_panel_auto = false;
                    }
                    if ok {
                        if !message.is_empty() {
                            self.success(message);
                        }
                    } else {
                        self.error(format!("操作失败：{message}"));
                    }
                    self.refresh_local_versions();
                }
                CoreEvent::Manifest(result) => {
                    self.manifest_loading = false;
                    match result {
                        Ok(manifest) => {
                            log::info!("版本清单已更新，共 {} 个版本", manifest.versions.len());
                            self.manifest = Some(*manifest);
                        }
                        Err(err) => {
                            self.manifest_error = Some(err.clone());
                            log::error!("获取版本清单失败: {err}");
                        }
                    }
                }
                CoreEvent::JavaProgress(message) => {
                    self.java_search_message = message;
                }
                CoreEvent::JavaDone(result) => {
                    self.java_searching = false;
                    self.java_cancel = None;
                    match result {
                        Ok(list) => {
                            let count = list.len();
                            self.javas = list;
                            self.cfg.java_cache = self.javas.clone();
                            self.save_config();
                            if count == 0 {
                                self.error("没有找到可用的 Java，请手动指定 java.exe");
                            } else {
                                self.success(format!("找到 {count} 个 Java"));
                            }
                        }
                        Err(err) => self.error(err),
                    }
                }
                CoreEvent::News(result) => {
                    self.news_loading = false;
                    self.news_fetched_at = Some(std::time::Instant::now());
                    match result {
                        Ok(items) => {
                            self.news = items;
                            self.news_error = None;
                        }
                        Err(err) => {
                            self.news_error = Some(err);
                            self.news = crate::core::news::builtin_tips();
                        }
                    }
                }
                CoreEvent::Search { seq, result } => {
                    log::info!(
                        "收到搜索结果 seq={seq} current={}",
                        self.download.content_seq
                    );
                    self.download.on_search_result(seq, result);
                }
                CoreEvent::ModVersions {
                    project_id,
                    result,
                } => {
                    self.download.on_versions_result(&project_id, result);
                }
                CoreEvent::LoaderVersions { kind, mc, result } => {
                    self.download.on_loader_versions(kind, &mc, result);
                }
                CoreEvent::InstallLoaderVersions { kind, mc, result } => {
                    self.download.on_install_loader_versions(kind, &mc, result);
                }
                CoreEvent::DeviceCode(result) => {
                    self.accounts_ui.on_device_code(result);
                }
                CoreEvent::MsPoll(result) => {
                    self.accounts_ui.on_ms_poll(result, &mut self.accounts);
                    if self.accounts_ui.just_logged_in {
                        self.accounts_ui.just_logged_in = false;
                        self.save_accounts();
                        self.success("微软账号登录成功");
                    }
                }
                CoreEvent::AuthlibLogin(result) => {
                    self.accounts_ui.on_authlib_login(result, &mut self.accounts);
                    if self.accounts_ui.just_logged_in {
                        self.accounts_ui.just_logged_in = false;
                        self.save_accounts();
                        self.success("皮肤站账号登录成功");
                    }
                }
                CoreEvent::SkinReady {
                    account_id,
                    path,
                    model,
                } => {
                    self.skin_pending.remove(&account_id);
                    if path.as_os_str().is_empty() {
                        let name = self
                            .accounts
                            .get(&account_id)
                            .map(|a| a.name().to_string())
                            .unwrap_or_default();
                        let image = crate::core::skin::generate_default_skin(&name);
                        self.upload_skin(&account_id, &image, model, SkinSource::Generated);
                    } else {
                        match SkinImage::load(&path) {
                            Ok(image) => {
                                let detected = if image.detect_slim() {
                                    SkinModel::Slim
                                } else {
                                    SkinModel::Classic
                                };
                                if let Some(account) = self.accounts.get_mut(&account_id) {
                                    account.set_skin_path(Some(path.clone()));
                                    account.set_skin_model(detected);
                                }
                                self.save_accounts();
                                self.upload_skin(&account_id, &image, detected, SkinSource::Remote);
                            }
                            Err(err) => log::warn!("皮肤解析失败: {err}"),
                        }
                    }
                }
                CoreEvent::ProjectDetail { project, versions } => {
                    self.download.content_versions_loading = false;
                    self.download.content_versions = versions;
                    self.download.content_version_index = 0;
                    self.download.content_detail = Some(project);
                }
                CoreEvent::ContentTranslated { project_id, text } => {
                    self.download.content_translation_loading = false;
                    // 只在详情页对应的项目上生效，避免旧请求串台
                    if self
                        .download
                        .content_detail
                        .as_ref()
                        .map(|p| p.id == project_id)
                        .unwrap_or(false)
                    {
                        self.download.content_translation = Some(text);
                    }
                }
                CoreEvent::McmodDbReady(db) => {
                    self.mcmod_db_loading = false;
                    self.mcmod_db = db;
                }
                CoreEvent::ContentSlugs(map) => {
                    for (name, slug) in map {
                        self.versions_ui.content_slugs.insert(name, slug);
                    }
                }
                CoreEvent::ImageReady { url, path } => {
                    self.images_pending.remove(&url);
                    if path.as_os_str().is_empty() {
                        continue;
                    }
                    let decoded = std::fs::read(&path)
                        .ok()
                        .and_then(|bytes| image::load_from_memory(&bytes).ok());
                    match decoded {
                        Some(img) => {
                            let rgba = img.to_rgba8();
                            let (w, h) = (rgba.width(), rgba.height());
                            let texture = self.egui_ctx.load_texture(
                                format!("remote:{url}"),
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [w as usize, h as usize],
                                    rgba.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            );
                            self.images.insert(url, texture);
                        }
                        None => log::warn!("图片解析失败: {}", path.display()),
                    }
                }
                CoreEvent::Installed(version) => {
                    self.refresh_local_versions();
                    if self.cfg.download.auto_select_after_install {
                        self.cfg.selected_version = Some(version.clone());
                        self.save_config();
                    }
                    self.success(format!("{version} 已安装完成"));
                }
                CoreEvent::GameExited { version, code } => {
                    if code == 0 {
                        self.info(format!("{version} 已退出"));
                    } else {
                        self.error(format!("{version} 异常退出（代码 {code}），可查看游戏日志"));
                    }
                }
            }
        }
    }

    fn poll_game(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match game.child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                let version = game.version_id.clone();
                game.log.set_exit(code);
                self.game = None;
                self.handle_events();
                self.toast(if code == 0 {
                    Toast::info(format!("{version} 已退出"))
                } else {
                    Toast::error(format!("{version} 异常退出（代码 {code}）"))
                });
                self.refresh_local_versions();
            }
            Ok(None) => {
                self.egui_ctx
                    .request_repaint_after(std::time::Duration::from_millis(500));
            }
            Err(_) => {
                self.game = None;
            }
        }
    }

    // ------------------------------------------------------------ 绘制

    fn draw_background(&mut self, ui: &Ui, rect: Rect) {
        if self.background_frames.is_empty() {
            return;
        }
        let now = ui.input(|i| i.time);
        if self.background_frames.len() > 1 {
            if now >= self.background_next {
                self.background_index = (self.background_index + 1) % self.background_frames.len();
                let delay = self.background_delays[self.background_index] as f64;
                self.background_next = now + delay.max(0.05);
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(
                    delay.max(0.05),
                ));
            }
        }
        let Some(texture) = self.background_frames.get(self.background_index) else {
            return;
        };
        let image_size = texture.size_vec2();
        if image_size.x <= 0.0 || image_size.y <= 0.0 {
            return;
        }
        let rect_ratio = rect.width() / rect.height().max(1.0);
        let image_ratio = image_size.x / image_size.y;
        let (uv_w, uv_h) = if image_ratio > rect_ratio {
            (rect_ratio / image_ratio, 1.0)
        } else {
            (1.0, image_ratio / rect_ratio)
        };
        let uv = Rect::from_center_size(Pos2::new(0.5, 0.5), Vec2::new(uv_w, uv_h));
        let opacity = self.cfg.ui.background.opacity.clamp(0.05, 1.0);
        ui.painter().image(
            texture.id(),
            rect,
            uv,
            Color32::WHITE.gamma_multiply(opacity),
        );
        let dim = self.cfg.ui.background.dim.clamp(0.0, 1.0);
        if dim > 0.0 {
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, self.palette.overlay(dim));
        }
    }

    fn title_bar(&mut self, ui: &mut Ui) {
        let height = 40.0;
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter().clone();
        painter.rect_filled(rect, CornerRadius::ZERO, self.palette.surface);
        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, self.palette.border),
        );

        let palette = self.palette;
        let ctx = ui.ctx().clone();
        let mut close_requested = false;
        let mut minimize_requested = false;
        let mut maximize_requested = false;

        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(4.0, 0.0)))
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                if window_button(ui, &palette, "win-close", true).clicked() {
                    close_requested = true;
                }
                let max_icon = if self.maximized {
                    "win-restore"
                } else {
                    "win-maximize"
                };
                if window_button(ui, &palette, max_icon, false).clicked() {
                    maximize_requested = true;
                }
                if window_button(ui, &palette, "win-minimize", false).clicked() {
                    minimize_requested = true;
                }

                let remaining = ui.available_width();
                let (drag_rect, drag) =
                    ui.allocate_exact_size(Vec2::new(remaining, height), Sense::click_and_drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    maximize_requested = true;
                }
                let p = ui.painter();
                let icon_rect = Rect::from_center_size(
                    Pos2::new(drag_rect.left() + 20.0, drag_rect.center().y),
                    Vec2::splat(20.0),
                );
                icons::paint(&ctx, p, "cube", icon_rect, palette.accent);
                p.text(
                    Pos2::new(drag_rect.left() + 38.0, drag_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "胡桃木启动器",
                    ui::font(13.5),
                    palette.text,
                );
                p.text(
                    Pos2::new(drag_rect.left() + 130.0, drag_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    format!("v{}", env!("CARGO_PKG_VERSION")),
                    ui::font(11.5),
                    palette.text_faint,
                );
            },
        );

        if close_requested {
            self.save_config();
            self.save_accounts();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if minimize_requested {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        if maximize_requested {
            self.maximized = !self.maximized;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximized));
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        let collapsed = self.cfg.ui.sidebar_collapsed;
        let width = if collapsed { 64.0 } else { 186.0 };
        ui.allocate_ui_with_layout(
            Vec2::new(width, ui.available_height()),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(width);
                ui.add_space(10.0);

                let mut clicked: Option<Page> = None;
                for page in Page::all() {
                    let selected = self.page == page;
                    if collapsed {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(width - 12.0, 42.0), Sense::click());
                        if ui.is_rect_visible(rect) {
                            if selected {
                                ui.painter().rect_filled(
                                    rect,
                                    self.palette.corner(-2),
                                    self.palette.accent_weak,
                                );
                            } else if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    self.palette.corner(-2),
                                    self.palette.surface_alt,
                                );
                            }
                            icons::paint(
                                ui.ctx(),
                                ui.painter(),
                                page.icon(),
                                Rect::from_center_size(rect.center(), Vec2::splat(19.0)),
                                if selected {
                                    self.palette.accent
                                } else {
                                    self.palette.text_weak
                                },
                            );
                        }
                        if response.clicked() {
                            clicked = Some(page);
                        }
                        ui.add_space(2.0);
                    } else if ui::nav_item(ui, &self.palette, page.icon(), page.label(), selected)
                        .clicked()
                    {
                        clicked = Some(page);
                    }
                }
                if let Some(page) = clicked {
                    self.page = page;
                }

                // 底部：账号与折叠按钮
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.add_space(10.0);
                    if ui::icon_button(
                        ui,
                        &self.palette,
                        if collapsed {
                            "chevron-right"
                        } else {
                            "chevron-left"
                        },
                        if collapsed { "展开侧栏" } else { "收起侧栏" },
                    )
                    .clicked()
                    {
                        self.cfg.ui.sidebar_collapsed = !collapsed;
                        self.save_config();
                    }

                    // 下载中心入口
                    ui.add_space(4.0);
                    let active = self.tasks.iter().filter(|t| !t.done).count();
                    let label = if active > 0 {
                        format!("下载中心 · {active}")
                    } else {
                        "下载中心".to_string()
                    };
                    let opened = if collapsed {
                        ui::icon_button(ui, &self.palette, "download", &label).clicked()
                    } else {
                        ui::button(
                            ui,
                            &self.palette,
                            ui::ButtonOpts::ghost(&label).icon("download").width(width - 12.0),
                        )
                        .clicked()
                    };
                    if opened {
                        self.download_panel_open = true;
                        self.download_panel_auto = false;
                    }

                    if !collapsed {
                        ui.add_space(6.0);
                        let account_name = self
                            .selected_account()
                            .map(|a| a.name().to_string())
                            .unwrap_or_else(|| "添加账号".into());
                        let kind = self
                            .selected_account()
                            .map(|a| a.kind_label())
                            .unwrap_or("未登录");
                        let account_id = self.selected_account().map(|a| a.id().to_string());
                        let head = account_id
                            .as_ref()
                            .and_then(|id| self.skin_cache.get(id))
                            .map(|s| s.head.clone());
                        let palette = self.palette;
                        let clicked = ui
                            .scope(|ui| {
                                ui.set_width(width - 12.0);
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::new(width - 12.0, 46.0),
                                    Sense::click(),
                                );
                                if ui.is_rect_visible(rect) {
                                    let t = ui.ctx().animate_bool_with_time(
                                        response.id.with("h"),
                                        response.hovered(),
                                        0.12,
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        palette.corner_sm(),
                                        widgets::lerp_color(palette.surface_alt, palette.surface, t),
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        palette.corner_sm(),
                                        egui::Stroke::new(1.0, palette.border),
                                        egui::StrokeKind::Inside,
                                    );
                                    let avatar_rect = Rect::from_min_size(
                                        Pos2::new(rect.left() + 8.0, rect.center().y - 15.0),
                                        Vec2::splat(30.0),
                                    );
                                    match &head {
                                        Some(head) => {
                                            ui.painter().image(
                                                head.id(),
                                                avatar_rect,
                                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                                Color32::WHITE,
                                            );
                                        }
                                        None => {
                                            icons::paint(
                                                ui.ctx(),
                                                ui.painter(),
                                                "user",
                                                avatar_rect.shrink(6.0),
                                                palette.text_faint,
                                            );
                                        }
                                    }
                                    ui.painter().text(
                                        Pos2::new(avatar_rect.right() + 8.0, rect.center().y - 8.0),
                                        egui::Align2::LEFT_CENTER,
                                        &account_name,
                                        ui::font(12.5),
                                        palette.text,
                                    );
                                    ui.painter().text(
                                        Pos2::new(avatar_rect.right() + 8.0, rect.center().y + 8.0),
                                        egui::Align2::LEFT_CENTER,
                                        kind,
                                        ui::font(10.5),
                                        palette.text_faint,
                                    );
                                }
                                response
                            })
                            .inner;
                        if clicked.clicked() {
                            self.page = Page::Accounts;
                        }
                    }
                });
            },
        );
    }

    fn task_bar(&mut self, ui: &mut Ui) {
        let active: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.done)
            .map(|(i, _)| i)
            .collect();
        if active.is_empty() {
            return;
        }
        let palette = self.palette;
        let mut cancel: Option<u64> = None;
        ui.add_space(6.0);
        ui::card_padded(ui, &palette, 12, |ui| {
            for index in active {
                let task = &self.tasks[index];
                ui.horizontal(|ui| {
                    ui::spinner(ui, &palette, 15.0);
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(&task.title)
                            .font(ui::font(13.0))
                            .color(palette.text)
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if task.cancellable {
                            if ui::ghost(ui, &palette, "取消").clicked() {
                                cancel = Some(task.id);
                            }
                        }
                        let percent = (task.fraction * 100.0).round() as i32;
                        ui.label(
                            egui::RichText::new(format!("{percent}%"))
                                .font(ui::font(12.0))
                                .color(palette.text_weak),
                        );
                    });
                });
                ui.add_space(4.0);
                ui::progress(ui, &palette, task.fraction, 6.0);
                if !task.message.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(&task.message)
                            .font(ui::font(11.5))
                            .color(palette.text_faint),
                    );
                }
                ui.add_space(6.0);
            }
        });
        if let Some(id) = cancel {
            self.cancel_task(id);
        }
    }

    pub fn cancel_task(&mut self, id: u64) {
        if let Some(task) = self.tasks.iter().find(|t| t.id == id) {
            if let Some(progress) = &task.progress {
                progress.cancel();
            }
        }
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.done = true;
            task.ok = false;
            task.message = "已取消".into();
        }
        self.info("任务已取消");
    }

    fn game_bar(&mut self, ui: &mut Ui) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let palette = self.palette;
        let version = game.version_id.clone();
        let elapsed = game.started.elapsed().as_secs();
        let minutes = elapsed / 60;
        let seconds = elapsed % 60;
        let mut stop = false;
        let mut open_log = false;

        ui.add_space(6.0);
        ui::card_padded(ui, &palette, 12, |ui| {
            ui.horizontal(|ui| {
                icons::paint(
                    ui.ctx(),
                    ui.painter(),
                    "play",
                    Rect::from_center_size(
                        Pos2::new(ui.cursor().left() + 9.0, ui.cursor().center().y),
                        Vec2::splat(16.0),
                    ),
                    palette.success,
                );
                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new(format!("{version} 正在运行"))
                        .font(ui::font(13.0))
                        .color(palette.text)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!("已运行 {minutes:02}:{seconds:02}"))
                        .font(ui::font(12.0))
                        .color(palette.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::button(ui, &palette, ui::ButtonOpts::danger("结束游戏")).clicked() {
                        stop = true;
                    }
                    if ui::ghost(ui, &palette, "游戏日志").clicked() {
                        open_log = true;
                    }
                });
            });
        });

        if stop {
            if let Some(game) = self.game.as_mut() {
                game.kill();
            }
            self.game = None;
            self.info("已结束游戏进程");
        }
        if open_log {
            self.game_log_open = true;
        }
    }

    fn run_screenshots(&mut self, ctx: &egui::Context) {
        if self.shot.is_none() {
            return;
        }
        ctx.request_repaint();
        let now = ctx.input(|i| i.time);

        let images: Vec<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect()
        });

        for image in images {
            let (dir, index) = {
                let shot = self.shot.as_ref().unwrap();
                (shot.dir.clone(), shot.page_index)
            };
            let label = shot_scene_label(index);
            let name = format!("{:02}-{label}.png", index + 1);
            let path = dir.join(&name);
            let [width, height] = image.size;
            let mut bytes = Vec::with_capacity(width * height * 4);
            for pixel in &image.pixels {
                bytes.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
            }
            if let Some(buffer) = image::RgbaImage::from_raw(width as u32, height as u32, bytes) {
                match buffer.save(&path) {
                    Ok(()) => log::info!("已保存截图 {}", path.display()),
                    Err(err) => log::warn!("保存截图失败: {err}"),
                }
            }

            let next = index + 1;
            if next >= SHOT_SCENES {
                log::info!("截图自检完成，退出启动器");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            self.apply_shot_scene(next);
            if let Some(shot) = self.shot.as_mut() {
                shot.page_index = next;
                shot.phase = 1;
                shot.next_at = now + shot_scene_wait(next);
            }
        }

        let (phase, next_at) = {
            let shot = self.shot.as_ref().unwrap();
            (shot.phase, shot.next_at)
        };
        match phase {
            0 => {
                if now >= next_at {
                    let index = self.shot.as_ref().unwrap().page_index;
                    self.apply_shot_scene(index);
                    if let Some(shot) = self.shot.as_mut() {
                        shot.phase = 1;
                        shot.next_at = now + shot_scene_wait(index);
                    }
                }
            }
            1 => {
                if now >= next_at {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                    if let Some(shot) = self.shot.as_mut() {
                        shot.phase = 2;
                    }
                }
            }
            _ => {}
        }
    }

    fn game_log_modal(&mut self, ctx: &egui::Context) {
        if !self.game_log_open {
            return;
        }
        let palette = self.palette;
        let lines = self
            .game
            .as_ref()
            .map(|g| g.log.snapshot(400))
            .unwrap_or_default();
        let mut open = true;
        let mut clear = false;
        widgets::modal(ctx, &palette, &mut open, "游戏日志", 720.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("共 {} 行", lines.len()))
                        .font(ui::font(12.0))
                        .color(palette.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::ghost(ui, &palette, "清空显示").clicked() {
                        clear = true;
                    }
                    if ui::ghost(ui, &palette, "打开日志文件夹").clicked() {
                        paths::reveal(&paths::logs_dir());
                    }
                });
            });
            ui.add_space(8.0);
            egui::Frame::NONE
                .fill(palette.surface_alt)
                .corner_radius(palette.corner_sm())
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(420.0)
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for line in &lines {
                                ui.label(
                                    egui::RichText::new(line)
                                        .font(ui::mono(11.5))
                                        .color(palette.text),
                                );
                            }
                        });
                });
        });
        if clear {
            if let Some(game) = self.game.as_ref() {
                let _ = game.log.snapshot(0);
            }
        }
        self.game_log_open = open && self.game.is_some();
    }
}

fn window_button(ui: &mut Ui, p: &Palette, icon: &str, is_close: bool) -> egui::Response {
    let size = Vec2::new(42.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            let bg = if is_close {
                Color32::from_rgb(0xE0, 0x4B, 0x4B)
            } else {
                p.surface_alt
            };
            ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
        }
        let color = if hovered && is_close {
            Color32::WHITE
        } else if hovered {
            p.text
        } else {
            p.text_weak
        };
        icons::paint(
            ui.ctx(),
            ui.painter(),
            icon,
            Rect::from_center_size(rect.center(), Vec2::splat(15.0)),
            color,
        );
    }
    response
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let size = self.last_window_size;
        if size.x > 400.0 && size.y > 300.0 {
            self.cfg.ui.window_size = [size.x, size.y];
        }
        self.save_config();
        self.save_accounts();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let c = self.palette.bg;
        [
            c.r() as f32 / 255.0,
            c.g() as f32 / 255.0,
            c.b() as f32 / 255.0,
            1.0,
        ]
    }

    fn ui(&mut self, root: &mut Ui, frame: &mut eframe::Frame) {
        if self.first_frame {
            self.first_frame = false;
            if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(frame) {
                if let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw() {
                    apply_win11_style(win.hwnd.get());
                }
            }
        }

        self.handle_events();
        self.poll_game();

        let ctx = root.ctx().clone();
        let now = ctx.input(|i| i.time);
        let window_size = ctx.content_rect().size();
        if !self.maximized {
            self.last_window_size = window_size;
        }

        let full = root.max_rect();
        root.painter()
            .rect_filled(full, CornerRadius::ZERO, self.palette.bg);
        self.draw_background(root, full);

        let title_height = 40.0;
        let title_rect = Rect::from_min_size(full.min, Vec2::new(full.width(), title_height));
        root.scope_builder(
            egui::UiBuilder::new()
                .max_rect(title_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| self.title_bar(ui),
        );

        let body = Rect::from_min_max(
            Pos2::new(full.left(), full.top() + title_height),
            full.max,
        );
        let collapsed = self.cfg.ui.sidebar_collapsed;
        let sidebar_width = if collapsed { 64.0 } else { 186.0 };
        let sidebar_rect = Rect::from_min_size(
            Pos2::new(body.left() + 8.0, body.top()),
            Vec2::new(sidebar_width, body.height()),
        );
        let content_rect = Rect::from_min_max(
            Pos2::new(sidebar_rect.right() + 8.0, body.top()),
            Pos2::new(body.right() - 18.0, body.bottom()),
        );

        root.scope_builder(
            egui::UiBuilder::new()
                .max_rect(sidebar_rect)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_width(sidebar_width);
                self.sidebar(ui);
            },
        );

        // 页面切换淡入
        if self.last_page != self.page {
            self.last_page = self.page;
            self.page_changed_at = now;
        }
        let page_t = ui::ease_out(((now - self.page_changed_at) / 0.18) as f32);
        if page_t < 1.0 {
            ctx.request_repaint();
        }

        root.scope_builder(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_width(content_rect.width());
                ui.set_opacity(0.25 + 0.75 * page_t);
                ui.add_space(12.0);
                self.game_bar(ui);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.set_max_width(content_rect.width() - 14.0);
                        ui.set_width(content_rect.width() - 14.0);
                        match self.page {
                            Page::Home => views::home::ui(self, ui),
                            Page::Download => views::download::ui(self, ui),
                            Page::Versions => views::versions::ui(self, ui),
                            Page::Accounts => views::accounts::ui(self, ui),
                            Page::Settings => views::settings::ui(self, ui),
                            Page::About => views::about::ui(self, ui),
                        }
                        ui.add_space(24.0);
                    });
            },
        );

        views::home_config::game_config_modal(self, &ctx);
        self.game_log_modal(&ctx);
        self.download_panel(&ctx);
        // 提示条固定在右下角，下载中心展开时自动上移避让
        let active_tasks = self.tasks.iter().filter(|t| !t.done).count();
        let panel_offset = if self.download_panel_open && active_tasks > 0 {
            (80.0 + active_tasks as f32 * 126.0).min(500.0)
        } else {
            0.0
        };
        widgets::toast_layer(
            &ctx,
            &self.palette,
            &mut self.toasts,
            now,
            panel_offset,
        );

        self.run_screenshots(&ctx);

        // 资讯定时刷新
        if self.cfg.news.enabled {
            let interval = (self.cfg.news.refresh_minutes.max(5) as u64) * 60;
            if let Some(at) = self.news_fetched_at {
                if at.elapsed().as_secs() > interval && !self.news_loading {
                    self.load_news();
                }
            }
        }
    }
}

/// 每个场景在截图前等待的时间（秒），给异步请求留出时间
fn shot_scene_wait(scene: usize) -> f64 {
    match scene {
        2 => 4.0,
        3 => 9.0,
        4 => 2.6,
        6 => 2.4,
        8 => 2.4,
        10 => 2.2,
        11 => 9.0,
        12 => 6.0,
        13 => 1.2,
        14 => 4.0,
        16 => 4.0,
        17 => 4.0,
        18 => 2.0,
        19 => 2.0,
        20 => 2.5,
        21 => 2.5,
        22 => 2.5,
        _ => 1.8,
    }
}

fn shot_scene_label(scene: usize) -> &'static str {
    match scene {
        0 => "首页",
        1 => "下载-游戏",
        2 => "下载-加载器",
        3 => "下载-Mod",
        4 => "版本管理",
        5 => "账号",
        6 => "设置-游戏",
        7 => "设置-外观",
        8 => "设置-资讯",
        9 => "关于",
        10 => "首页-游戏配置",
        11 => "下载-整合包",
        12 => "下载中心",
        13 => "提示条",
        14 => "下载-安装弹窗",
        15 => "设置-下载",
        16 => "下载-光影",
        17 => "下载-项目详情",
        18 => "版本管理-详情",
        19 => "版本管理-设置",
        20 => "版本管理-模组",
        21 => "版本管理-光影",
        _ => "版本管理-资源包",
    }
}

#[cfg(windows)]
fn apply_win11_style(hwnd: isize) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    let preference: i32 = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd as _,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

#[cfg(not(windows))]
fn apply_win11_style(_hwnd: isize) {}
