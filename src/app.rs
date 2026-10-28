use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use egui::{Align, Color32, CornerRadius, Layout, Margin, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::core::auth::{Account, SkinModel};
use crate::core::config::Config;
use crate::core::content::modrinth::{SearchResponse, Version as ModVersion};
use crate::core::content::ContentSource;
use crate::core::download::{DownloadProgress, DownloadSource, Downloader};
use crate::core::java::JavaInfo;
use crate::core::minecraft::launch::{GameLog, GameLogLine, GameProcess, LogSource};
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

/// 右侧日志面板的宽度
pub const DOCK_WIDTH: f32 = 392.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Download,
    Versions,
    Accounts,
    Settings,
    About,
}

/// 游戏进程的状态
///
/// 「正在启动」「运行中」「已退出」这几个说法现在有了唯一的出处，
/// 状态栏、日志面板、提示条都读它，不会各说各话。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    /// 正在拉起，游戏窗口还没出来
    Starting,
    /// 已经跑起来了
    Running,
    /// 正常退出
    Exited,
    /// 异常退出
    Failed,
}

impl GamePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "正在启动",
            Self::Running => "运行中",
            Self::Exited => "已退出",
            Self::Failed => "异常退出",
        }
    }

    pub fn color(self, p: &Palette) -> Color32 {
        match self {
            Self::Starting => p.text_weak,
            Self::Running => p.success,
            Self::Exited => p.text_weak,
            Self::Failed => p.danger,
        }
    }
}

/// 游戏日志面板只在游戏跑着的时候出现
///
/// 游戏一关，面板、整页日志、右边的小标签全部收掉，
/// 屏幕上不留任何跟上次运行有关的东西。
pub fn clear_game_log_ui(app: &mut App) {
    app.game_log_open = false;
    app.game_log_docked = false;
    app.show_game_log_page = false;
    app.game_log_filter.clear();
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

    /// 页面序号，用来给入场动画区分不同的页面
    pub fn page_index(self) -> u8 {
        match self {
            Page::Home => 0,
            Page::Download => 1,
            Page::Versions => 2,
            Page::Accounts => 3,
            Page::Settings => 4,
            Page::About => 5,
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
pub const SHOT_SCENES: usize = 36;

/// 截图自检用的假日志
///
/// 真启动一次游戏太慢，界面好不好看用这几行就够了。
const SHOT_MOCK_LOG: [&str; 12] = [
    "[12:31:04] [main/INFO]: Environment: Environment[sessionHost=https://sessionserver.mojang.com, name=PCL]",
    "[12:31:04] [main/INFO]: Setting user: Steve",
    "[12:31:05] [main/INFO]: Loading 12 recipes",
    "[12:31:06] [Render thread/INFO]: Created: 1024x512 textures-atlas",
    "[12:31:06] [Render thread/INFO]: Created: 128x128 minecraft:item_texture",
    "[12:31:07] [Sound Library/INFO]: Sound engine started",
    "[12:31:07] [Render thread/INFO]: Narrator library for x64 successfully loaded",
    "[12:31:08] [Server thread/INFO]: Starting minecraft server version 1.20.1",
    "[12:31:08] [Server thread/INFO]: Preparing level \"world\"",
    "[12:31:09] [Server thread/INFO]: Loaded 1871 advancements",
    "[12:31:11] [Render thread/INFO]: Time elapsed: 7123 ms",
    "[12:31:12] [Server thread/WARN]: Can't keep up! Did the system get too busy?",
];

impl App {
    fn apply_shot_scene(&mut self, scene: usize) {
        // 每次切换场景先关掉所有弹窗，避免互相遮挡
        self.home_ui.config_open = false;
        self.versions_ui.loader_modal = None;
        self.versions_ui.shader_modal = None;
        self.versions_ui.confirm_delete = None;
        self.versions_ui.rename_target = None;
        self.download.install_target = None;
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
                // 版本详情：原版实例（左侧导航没有模组与光影）
                self.page = Page::Versions;
                if let Some(id) = self.shot_vanilla_instance_id() {
                    views::versions::open_detail(self, &id);
                }
            }
            19 => {
                // 版本详情 - 设置页
                self.page = Page::Versions;
                if let Some(id) = self.shot_vanilla_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "settings";
                }
            }
            20 => {
                // 版本详情 - 模组（带加载器的实例才有这一项）
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "mod";
                }
            }
            21 => {
                // 版本详情 - 光影
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "shader";
                }
            }
            22 => {
                // 版本详情 - 资源包
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "resourcepack";
                }
            }
            23 => {
                // 皮肤站登录：展示角色选择列表
                self.page = Page::Accounts;
                self.accounts_ui.tab = 2;
                self.accounts_ui.authlib_status =
                    "登录成功，该账号下有 3 个角色，请选择一个".into();
                self.accounts_ui.authlib_profiles = vec![
                    crate::core::auth::authlib::AuthlibProfile {
                        id: "8f6b1c2d-3e4f-5a6b-7c8d-9e0f1a2b3c4d".into(),
                        name: "林间晚风".into(),
                    },
                    crate::core::auth::authlib::AuthlibProfile {
                        id: "1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d".into(),
                        name: "Steve".into(),
                    },
                    crate::core::auth::authlib::AuthlibProfile {
                        id: "2b3c4d5e-6f7a-8b9c-0d1e-2f3a4b5c6d7e".into(),
                        name: "铁匠阿福".into(),
                    },
                ];
            }
            24 => {
                // 版本详情 - 模组自带的蓝图（机械动力）
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "create_schematic";
                }
            }
            25 => {
                // 版本详情 - 模组自带的投影蓝图
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "litematic";
                }
            }
            26 => {
                // 版本详情 - 存档
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "saves";
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
            27 => {
                // 整合包详情：安装成独立实例
                self.page = Page::Download;
                self.download.tab = 5;
                self.download.content_loaded_tab = Some(5);
                views::download::open_project(self, "compsmp", "CompSMP-OptionalModpack");
            }
            28 => {
                // 整合包实例详情：独立实例与重新安装入口
                self.page = Page::Versions;
                if let Some(id) = self.shot_modpack_instance_id() {
                    views::versions::open_detail(self, &id);
                }
            }
            29 => {
                // 加载器列表页
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    let mc = self
                        .instances
                        .iter()
                        .find(|i| i.instance_id == id)
                        .map(|i| i.real_mc_version())
                        .unwrap_or_else(|| id.clone());
                    views::versions::open_loaders_page(self, &mc);
                }
            }
            30 => {
                // 更换光影加载器弹窗
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.shader_modal = Some(id);
                }
            }
            31 => {
                // 版本详情 - 导出整合包
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "export";
                }
            }
            32 => {
                // 版本详情 - 概览里的个性化与快捷方式
                self.page = Page::Versions;
                if let Some(id) = self.shot_loader_instance_id() {
                    views::versions::open_detail(self, &id);
                    self.versions_ui.section = "info";
                }
            }
            33 => {
                // 下载页切到 CurseForge 平台
                self.page = Page::Download;
                self.download.tab = 2;
                self.download.content_source = ContentSource::CurseForge;
                self.cfg.download.content_source = ContentSource::CurseForge;
                self.download.content_query = "sodium".into();
                // 上一个场景可能停在详情页，这里退回列表
                self.download.content_detail = None;
                self.download.content_hits.clear();
                self.download.content_loaded_tab = None;
            }
            34 => {
                // 右侧停靠的游戏日志面板
                self.page = Page::Home;
                self.shot_mock_game_log();
            }
            35 => {
                // 启动过场：居中的卡片，底下还看得见启动器
                self.page = Page::Home;
                self.shot_mock_game_log();
                self.game_starting = true;
            }
            _ => {
                // 提示条位置与动效检查
                self.page = Page::Home;
                self.info("这是一条信息提示，用于检查提示条位置");
            }
        }
    }

    /// 造一份假的游戏日志，用来检查日志面板的样子
    fn shot_mock_game_log(&mut self) {
        let log = Arc::new(GameLog::default());
        for (i, text) in SHOT_MOCK_LOG.iter().enumerate() {
            let source = if i % 6 == 5 {
                LogSource::Stderr
            } else {
                LogSource::Game
            };
            log.push((*text).to_string(), source);
        }
        // 假进程没有真实的子进程，跑起来的时间倒着算，好让「已运行」有数值
        let started = std::time::Instant::now() - std::time::Duration::from_secs(372);
        self.game = Some(GameProcess {
            child: None,
            pid: 0,
            log,
            version_id: "1.20.1-Fabric".into(),
            started,
            game_dir: self.cfg.game_dir.clone(),
        });
        self.game_phase = GamePhase::Running;
        self.game_log_docked = true;
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
    /// 版本详情页「更换加载器」弹窗里的版本列表
    DetailLoaderVersions {
        kind: LoaderKind,
        mc: String,
        result: Result<Vec<LoaderVersion>, String>,
    },
    /// 装好加载器后切到新的变体
    LoaderSwitched {
        version_id: String,
        instance_id: String,
    },
    DeviceCode(Result<crate::core::auth::microsoft::DeviceCode, String>),
    MsPoll(Result<crate::core::auth::microsoft::PollOutcome, String>),
    AuthlibLogin(Result<crate::core::auth::authlib::AuthlibAuth, String>),
    /// 令牌过期后用记住的密码自动续期
    AuthlibRelogin {
        account_id: String,
        result: Result<crate::core::auth::authlib::AuthlibAuth, String>,
    },
    /// 皮肤站登录成功，等待用户挑选角色
    AuthlibSession(Result<crate::core::auth::authlib::AuthlibSession, String>),
    /// authlib-injector 准备完成（皮肤站账号启动游戏时需要挂载）
    AuthlibJar(Result<PathBuf, String>),
    SkinReady {
        account_id: String,
        path: PathBuf,
        model: SkinModel,
    },
    /// 本地皮肤文件在后台读完了
    ///
    /// 文件可能在网络盘上，放到后台读，界面才不会卡住。
    LocalSkinLoaded {
        account_id: String,
        image: Option<SkinImage>,
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
    /// CurseForge API Key 连通性测试结果
    CurseForgeKeyTest(Result<String, String>),
    /// 本地内容反查到的线上项目
    ContentLookup {
        kind: crate::core::content::ContentKind,
        source: crate::core::content::ContentSource,
        map: std::collections::HashMap<String, crate::core::content::RemoteProject>,
        /// 问过的文件名，没出现在结果里的就是没找到
        asked: Vec<String>,
    },
    /// 反查没成功，下次打开还要再试
    ContentLookupFailed(crate::core::content::ContentKind),
    /// 某个内容查完或更新完
    ContentUpdated {
        key: String,
        kind: crate::core::content::ContentKind,
        /// 换下来的旧文件名
        old_file: String,
        /// 新文件名，没换就是 None
        new_file: Option<String>,
        version: String,
        /// 有没有真的换文件
        updated: bool,
    },
    /// 某个内容查失败
    ContentUpdateFailed {
        key: String,
        error: String,
    },
    /// 内容装好了，记进本地缓存
    ContentInstalled {
        kind: crate::core::content::ContentKind,
        source: crate::core::content::ContentSource,
        file_name: String,
        /// 平台项目标识：Modrinth 是 slug，CurseForge 是数字编号
        project_id: String,
        version_id: String,
        slug: String,
        icon_url: Option<String>,
    },
    /// 各实例里装了哪些模组自带的内容
    ModFeatures(
        std::collections::HashMap<String, Vec<crate::core::content::modfeature::ModFeature>>,
    ),
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
    /// 第二层专用的镂空纹理（半透明像素已被抹掉）
    pub cutout: egui::TextureHandle,
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
    /// 按 Minecraft 版本聚合出的游戏实例（原版与各加载器版本合成一条）
    pub instances: Vec<crate::core::minecraft::GameInstance>,
    /// 各实例装了哪些已适配的模组（按 Minecraft 版本索引）
    ///
    /// 装了机械动力、投影这类模组后，实例里才会多出它们自己的内容入口。
    pub mod_features: HashMap<String, Vec<crate::core::content::modfeature::ModFeature>>,
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
    /// 本地内容的线上资料缓存（模组 / 光影 / 资源包）
    ///
    /// 查过一次就记在这里，下次打开直接读本地，不再联网反查。
    pub content_cache: crate::core::content::cache::ContentCache,
    pub accounts_ui: views::accounts::AccountsState,
    pub settings_ui: views::settings::SettingsState,
    pub home_ui: views::home::HomeState,
    pub versions_ui: views::versions::VersionsState,

    pub images: HashMap<String, egui::TextureHandle>,
    /// 本地模组 jar 自带图标的纹理
    pub mod_icons: HashMap<String, egui::TextureHandle>,
    /// 解码失败的本地图标，记下来免得每帧重试
    mod_icon_failed: HashSet<String>,
    images_pending: HashSet<String>,
    pub skin_cache: HashMap<String, SkinTexture>,
    pub skin_pending: HashSet<String>,
    /// 已经查过本地皮肤文件的账号
    ///
    /// 网络盘上的文件不能每帧都去戳，界面会卡住。
    pub skin_local_checked: HashSet<String>,
    /// 皮肤预览的朝向
    ///
    /// 默认停在斜侧的角度（像角色侧着身看屏幕），拖动才会转。
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
    /// 这次启动里已经尝试过自动续期，避免续期失败时反复重试
    pub authlib_relogin_attempted: bool,
    /// 游戏运行中，启动器被隐藏了
    pub launcher_hidden: bool,
    /// 游戏退出后重新打开启动器
    pub launcher_reopen: bool,
    /// 游戏退出后一起关掉启动器
    pub launcher_close_on_exit: bool,
    /// 正在启动游戏，这段时间播放启动动画
    pub game_starting: bool,
    /// 启动器窗口的原生句柄，用来把小窗挪到屏幕右下角
    pub window_handle: Option<isize>,
    /// 游戏当前的状态
    pub game_phase: GamePhase,
    /// 游戏日志面板是否停靠在右侧
    pub game_log_docked: bool,
    /// 日志面板的过滤词
    pub game_log_filter: String,
    /// 「启动完成」这句话报过没有
    pub game_ready_announced: bool,
    /// 在启动器主界面里显示游戏日志
    pub show_game_log_page: bool,
    /// authlib-injector 是否正在下载
    pub authlib_jar_loading: bool,
    /// authlib-injector 上一次准备失败的原因
    pub authlib_jar_error: Option<String>,
    /// CurseForge API Key 连通性测试是否在进行
    pub cf_key_testing: bool,
    /// CurseForge API Key 连通性测试的结果
    pub cf_key_test: Option<Result<String, String>>,
    pub dwm_applied: bool,
    pub shot: Option<ShotState>,
    pub download_panel_open: bool,
    pub download_panel_auto: bool,
    /// 模组内容扫描是否正在进行，避免重复起线程
    mod_features_scanning: bool,
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
        ui::set_animations_enabled(cfg.ui.animations);
        icons::warmup(&cc.egui_ctx, palette.text_weak, 16.0);

        let (tx, rx) = channel();
        let (image_tx, image_rx) = std::sync::mpsc::channel::<String>();
        let image_rx = Arc::new(std::sync::Mutex::new(image_rx));
        let log_dock = cfg.ui.log_dock;

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
            instances: Vec::new(),
            mod_features: HashMap::new(),
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
            content_cache: crate::core::content::cache::ContentCache::load(),
            accounts_ui: Default::default(),
            settings_ui: Default::default(),
            home_ui: Default::default(),
            versions_ui: Default::default(),
            images: HashMap::new(),
            mod_icons: HashMap::new(),
            mod_icon_failed: HashSet::new(),
            images_pending: HashSet::new(),
            skin_cache: HashMap::new(),
            skin_pending: HashSet::new(),
            skin_local_checked: HashSet::new(),
            preview_yaw: -0.55,
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
            authlib_relogin_attempted: false,
            launcher_hidden: false,
            launcher_reopen: false,
            launcher_close_on_exit: false,
            game_starting: false,
            window_handle: None,
            game_phase: GamePhase::Starting,
            game_log_docked: log_dock,
            game_log_filter: String::new(),
            game_ready_announced: false,
            show_game_log_page: false,
            authlib_jar_loading: false,
            authlib_jar_error: None,
            cf_key_testing: false,
            cf_key_test: None,
            dwm_applied: false,
            download_panel_open: false,
            download_panel_auto: false,
            mod_features_scanning: false,
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
        app.purge_old_logs();
        app.load_manifest();
        app.load_news();
        app.refresh_mcmod_db();
        if app.has_authlib_account() {
            // 有皮肤站账号就把 authlib-injector 先备好，避免启动时卡住
            app.refresh_authlib_jar();
        }
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

    /// 准备 authlib-injector
    ///
    /// 皮肤站账号启动游戏时需要把它挂成 javaagent，
    /// 少了它皮肤与披风都不会生效。这里提前下载好，启动时就不用等。
    pub fn refresh_authlib_jar(&mut self) {
        if self.authlib_jar.is_some() || self.authlib_jar_loading {
            return;
        }
        // 已经下载过就直接用
        if let Some(path) = crate::core::minecraft::launch::cached_authlib_injector() {
            self.authlib_jar = Some(path);
            return;
        }
        self.authlib_jar_loading = true;
        self.authlib_jar_error = None;
        let source = self.cfg.download.source;
        let timeout = self.cfg.download.timeout_secs;
        self.spawn("准备皮肤站组件", false, move |job| {
            let result = (|| -> anyhow::Result<PathBuf> {
                let dl = Downloader::new(source, timeout.max(30))?;
                crate::core::minecraft::launch::ensure_authlib_injector(&dl)
            })();
            job.send(CoreEvent::AuthlibJar(result.map_err(|e| format!("{e}"))));
        });
    }

    /// 是否有皮肤站账号（这类账号启动时需要 authlib-injector）
    pub fn has_authlib_account(&self) -> bool {
        self.accounts
            .accounts
            .iter()
            .any(|a| a.authlib_url().is_some())
    }

    pub fn downloader(&self) -> anyhow::Result<Downloader> {
        Downloader::new(self.cfg.download.source, self.cfg.download.timeout_secs)
            .map(|dl| dl.with_curseforge_key(self.cfg.download.curseforge_key()))
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
        let curseforge_key = self.cfg.download.curseforge_key();

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
                let dl = Downloader::new(source, timeout)?
                    .with_curseforge_key(curseforge_key);
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

    /// 游戏启动后按设置处理启动器自身的可见性
    ///
    /// keep = 一直显示；hide = 隐藏（游戏退出也不回来）；
    /// hide_and_reopen = 隐藏并在游戏退出后重新打开；close = 直接退出启动器。
    pub fn apply_launcher_visibility(&mut self, visibility: &str) {
        match visibility {
            "hide" => {
                self.ctx_send_visible(false);
                self.launcher_hidden = true;
                self.launcher_reopen = false;
            }
            "hide_and_reopen" => {
                self.ctx_send_visible(false);
                self.launcher_hidden = true;
                self.launcher_reopen = true;
            }
            "close" => {
                self.egui_ctx
                    .send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
    }

    fn ctx_send_visible(&self, visible: bool) {
        self.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(visible));
    }

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

    /// 清理 30 天前的启动器日志
    pub fn purge_old_logs(&mut self) {
        if !self.cfg.ui.auto_purge_logs {
            return;
        }
        let dir = paths::logs_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(30 * 24 * 3600));
        let mut removed = 0usize;
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let Ok(modified) = meta.modified() else { continue };
            if let Some(cutoff) = cutoff {
                if modified < cutoff && std::fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
            }
        }
        if removed > 0 {
            log::info!("已清理 {removed} 个过期日志");
        }
    }

    pub fn reload_background(&mut self) {
        self.background_frames.clear();
        self.background_delays.clear();
        self.background_index = 0;
        self.background_next = 0.0;
        self.background_path = None;

        let Some(path) = theme::resolve_background(&self.theme, &self.cfg.ui.background, self.cfg.ui.random_background) else {
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

    /// 界面自检要打开的实例：优先当前正在用的那个
    fn shot_instance_id(&self) -> Option<String> {
        let selected = self.cfg.selected_version.as_deref();
        self.instances
            .iter()
            .find(|i| Some(i.active.as_str()) == selected)
            .or_else(|| self.instances.first())
            .map(|i| i.instance_id.clone())
    }

    /// 界面自检要打开的原版实例（没有加载器的）
    fn shot_vanilla_instance_id(&self) -> Option<String> {
        self.instances
            .iter()
            .find(|i| i.variants.iter().all(|v| v.loader.is_none()))
            .map(|i| i.instance_id.clone())
            .or_else(|| self.shot_instance_id())
    }

    /// 界面自检要打开的带加载器实例
    ///
    /// 优先挑本地内容最多的那个：模组、光影、资源包这些页才有东西可截，
    /// 全是空目录的话截图看不出界面到底正不正常。
    fn shot_loader_instance_id(&self) -> Option<String> {
        let mut best: Option<(usize, String)> = None;
        for inst in &self.instances {
            if !inst.variants.iter().any(|v| v.loader.is_some()) {
                continue;
            }
            let dir = self.cfg.effective_game_dir(&inst.instance_id);
            let mut count = 0usize;
            for sub in ["mods", "shaderpacks", "resourcepacks"] {
                if let Ok(entries) = std::fs::read_dir(dir.join(sub)) {
                    count += entries.flatten().count();
                }
            }
            let id = inst.instance_id.clone();
            match &best {
                Some((max, _)) if *max >= count => {}
                _ => best = Some((count, id)),
            }
        }
        best.map(|(_, id)| id)
            .or_else(|| self.shot_instance_id())
    }

    /// 界面自检要打开的整合包实例
    fn shot_modpack_instance_id(&self) -> Option<String> {
        self.instances
            .iter()
            .find(|i| i.is_modpack)
            .map(|i| i.instance_id.clone())
    }

    // ------------------------------------------------------------ 数据刷新

    pub fn refresh_local_versions(&mut self) {
        self.local_versions = crate::core::minecraft::scan_local(&self.cfg.game_dir);
        self.migrate_instance_dirs();
        self.migrate_version_overrides();
        self.instances = crate::core::minecraft::group_instances(
            &self.local_versions,
            self.cfg.selected_version.as_deref(),
        );
        // 目录可能被动过，内容列表的缓存要重新算
        self.versions_ui.content_cache.clear();
        self.refresh_mod_features();
    }

    /// 扫描各实例装了什么模组，决定左侧导航要多出哪些内容入口
    ///
    /// 扫描要开 jar 读元数据，整合包上百个模组时在主线程做会明显卡一下，
    /// 所以放到后台线程去，结果回来再更新。
    fn refresh_mod_features(&mut self) {
        // 已经在扫了就别再起一个
        if self.mod_features_scanning {
            return;
        }
        self.mod_features_scanning = true;
        let dirs: Vec<(String, std::path::PathBuf)> = self
            .instances
            .iter()
            .map(|inst| {
                (
                    inst.instance_id.clone(),
                    self.cfg.effective_game_dir(&inst.instance_id),
                )
            })
            .collect();
        if dirs.is_empty() {
            self.mod_features.clear();
            return;
        }
        self.spawn("扫描模组内容", false, move |job| {
            let mut found = std::collections::HashMap::new();
            for (id, dir) in dirs {
                let features = crate::core::content::modfeature::detect(&dir);
                if !features.is_empty() {
                    found.insert(id, features);
                }
            }
            job.send(CoreEvent::ModFeatures(found));
            // 这里不发 TaskDone：任务完成会触发一次版本刷新，
            // 刷新又会再起一个扫描任务，就成死循环了
        });
    }

    /// 旧布局把实例目录放在版本目录里，现在统一按 Minecraft 版本存放
    ///
    /// 老的布局是 `versions/<版本目录 id>/mods`，新的布局是
    /// `versions/<Minecraft 版本>/mods`。加载器变体目录里的实例内容
    /// 搬到对应的实例目录下，这样原版与加载器版本共用同一份模组与存档。
    fn migrate_instance_dirs(&mut self) {
        const INSTANCE_DIRS: [&str; 6] = [
            "mods",
            "shaderpacks",
            "resourcepacks",
            "saves",
            "config",
            "texturepacks",
        ];
        let versions_dir = crate::core::paths::versions_dir(&self.cfg.game_dir);
        for v in self.local_versions.clone() {
            // 自己一个目录的实例，内容本来就该留在自己这儿，别往默认实例那边搬
            if v.instance.is_some() {
                continue;
            }
            if v.instance_id == v.id {
                continue;
            }
            let old_dir = versions_dir.join(&v.id);
            let new_dir = versions_dir.join(&v.instance_id);
            for name in INSTANCE_DIRS {
                let from = old_dir.join(name);
                let to = new_dir.join(name);
                if !from.is_dir() || to.exists() {
                    continue;
                }
                if std::fs::rename(&from, &to).is_ok() {
                    log::info!("已迁移实例内容 {} -> {}", from.display(), to.display());
                }
            }
        }
    }

    /// 版本覆盖设置改为按 Minecraft 版本存放，把旧的版本目录 id 键迁移过来
    fn migrate_version_overrides(&mut self) {
        let mut migrated = false;
        for v in self.local_versions.clone() {
            if let Some(ov) = self.cfg.version_overrides.remove(&v.id) {
                self.cfg
                    .version_overrides
                    .entry(v.instance_id.clone())
                    .or_insert(ov);
                migrated = true;
            }
        }
        if migrated {
            self.save_config();
        }
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

    /// 实例的显示名
    ///
    /// 用户在「概览 → 个性化」里改过名字就用他改的，
    /// 没改过就还是 Minecraft 版本号或整合包名字。
    pub fn instance_display_name(&self, inst: &crate::core::minecraft::GameInstance) -> String {
        let meta = self.cfg.instance_meta(&inst.instance_id);
        if !meta.name.trim().is_empty() {
            meta.name.clone()
        } else {
            inst.display_name.clone()
        }
    }

    /// 当前实例对应的 Minecraft 版本
    ///
    /// 整合包实例的 id 是它自己的名字，直接拿去做版本筛选会搜不到东西，
    /// 这里返回它真正的 Minecraft 版本。
    pub fn current_mc_version(&self) -> Option<String> {
        let version = self.selected_version()?;
        Some(
            version
                .modpack
                .as_ref()
                .map(|m| m.mc_version.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| version.mc_version.clone()),
        )
    }

    pub fn selected_account(&self) -> Option<&Account> {
        self.accounts.selected_account()
    }

    pub fn java_for(&self, mc_version: &str) -> Option<JavaInfo> {
        crate::core::java::pick_for_version(
            crate::core::java::required_major(mc_version),
            self.cfg.java.path.as_deref(),
            &self.javas,
        )
    }

    /// 当前选中的实例
    ///
    /// 配置里存的是版本目录 id，整合包实例的 id 是它自己的名字，
    /// 所以要靠变体列表反查回是哪一条实例。
    pub fn selected_instance(&self) -> Option<&crate::core::minecraft::GameInstance> {
        let id = self.cfg.selected_version.as_deref()?;
        self.instances
            .iter()
            .find(|i| i.variants.iter().any(|v| v.id == id))
    }

    /// 当前实例启动时实际会用到的 Java
    ///
    /// 和启动流程同一套逻辑：实例自己指定的优先，
    /// 没指定就按游戏版本自动挑一个够用的。
    pub fn java_for_current(&self) -> Option<JavaInfo> {
        let inst = self.selected_instance()?;
        let required = crate::core::java::required_major(&inst.real_mc_version());
        let manual = self.cfg.effective_java(&inst.instance_id);
        crate::core::java::pick_for_version(required, manual.as_deref(), &self.javas)
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
        //
        // 皮肤文件可能放在网络盘上，主线程去戳一下能把界面卡住几十秒，
        // 所以检查与读取都放到后台线程去做，而且只做一次。
        if let Some(path) = account.skin_path().map(|p| p.to_path_buf()) {
            if self.skin_local_checked.contains(account_id) {
                // 已经查过本地文件了，走在线或生成
            } else {
                self.skin_local_checked.insert(account_id.to_string());
                self.skin_pending.insert(account_id.to_string());
                let id = account_id.to_string();
                self.spawn("读取本地皮肤", false, move |job| {
                    let image = if path.is_file() {
                        match SkinImage::load(&path) {
                            Ok(image) => Some(image),
                            Err(err) => {
                                log::warn!("读取皮肤失败: {err}");
                                None
                            }
                        }
                    } else {
                        None
                    };
                    job.send(CoreEvent::LocalSkinLoaded {
                        account_id: id,
                        image,
                    });
                    job.done(true, "");
                });
                return;
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
        // 第二层用镂空版本，避免半透明像素和第一层混色
        let cutout = self.egui_ctx.load_texture(
            format!("skin_cutout:{account_id}"),
            image.to_cutout_color_image(),
            egui::TextureOptions::NEAREST,
        );
        self.skin_cache.insert(
            account_id.to_string(),
            SkinTexture {
                texture,
                head,
                cutout,
                model,
                source,
                has_overlay: image.height >= 64,
            },
        );
    }

    pub fn image(&self, url: &str) -> Option<&egui::TextureHandle> {
        self.images.get(url)
    }

    /// 本地模组图标（jar 自带）的纹理
    ///
    /// 解码一次就缓存住，之后同一个模组直接拿纹理，不再重复解压图片。
    pub fn mod_icon_texture(
        &mut self,
        key: &str,
        bytes: &[u8],
    ) -> Option<egui::TextureHandle> {
        if let Some(texture) = self.mod_icons.get(key) {
            return Some(texture.clone());
        }
        let decoded = image::load_from_memory(bytes).ok()?;
        let rgba = decoded.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        if w == 0 || h == 0 {
            return None;
        }
        let texture = self.egui_ctx.load_texture(
            format!("modicon:{key}"),
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        self.mod_icons.insert(key.to_string(), texture.clone());
        Some(texture)
    }

    /// 本地文件图标的纹理（版本目录里的自定义图标等）
    ///
    /// 与模组图标共用一份缓存，同一个文件只解码一次。
    /// 解码失败的也要记下来，不然每帧都会重读一遍，界面直接卡住。
    pub fn file_icon_texture(&mut self, path: &std::path::Path) -> Option<egui::TextureHandle> {
        let key = path.to_string_lossy().to_string();
        if let Some(texture) = self.mod_icons.get(&key) {
            return Some(texture.clone());
        }
        if self.mod_icon_failed.contains(&key) {
            return None;
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.mod_icon_failed.insert(key);
                return None;
            }
        };
        match self.mod_icon_texture(&key, &bytes) {
            Some(texture) => Some(texture),
            None => {
                self.mod_icon_failed.insert(key);
                None
            }
        }
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
                CoreEvent::DetailLoaderVersions { kind, mc, result } => {
                    self.versions_ui.on_loader_versions(kind, &mc, result);
                }
                CoreEvent::LoaderSwitched {
                    version_id,
                    instance_id,
                } => {
                    // 换完加载器直接切过去，省得用户还要自己再选一次
                    self.cfg.selected_version = Some(version_id.clone());
                    self.save_config();
                    self.refresh_local_versions();
                    self.versions_ui.detail = Some(instance_id.clone());
                    self.success(format!("已切换到 {version_id}"));
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
                        // 新增了皮肤站账号，把 authlib-injector 备好
                        self.refresh_authlib_jar();
                    }
                }
                CoreEvent::AuthlibSession(result) => {
                    self.accounts_ui.on_authlib_session(result);
                }
                CoreEvent::AuthlibRelogin { account_id, result } => {
                    match result {
                        Ok(auth) => {
                            if let Some(crate::core::auth::Account::Authlib(a)) =
                                self.accounts.get_mut(&account_id)
                            {
                                a.access_token = auth.access_token;
                                a.client_token = auth.client_token;
                                a.expires_at = auth.expires_at;
                                a.skin_url = auth.skin_url.clone();
                                if auth.skin_model == "slim" {
                                    a.skin_model = crate::core::auth::SkinModel::Slim;
                                } else {
                                    a.skin_model = crate::core::auth::SkinModel::Classic;
                                }
                            }
                            self.save_accounts();
                            self.success("皮肤站登录已自动续期");
                            // 令牌换好了，继续刚才的启动流程
                            crate::views::launch_game(self);
                        }
                        Err(err) => {
                            self.error(format!("皮肤站自动续期失败：{err}"));
                            self.accounts_ui.tab = 2;
                            self.page = crate::app::Page::Accounts;
                        }
                    }
                }
                CoreEvent::AuthlibJar(result) => {
                    self.authlib_jar_loading = false;
                    match result {
                        Ok(path) => {
                            log::info!("皮肤站组件已就绪: {}", path.display());
                            self.authlib_jar = Some(path);
                            self.authlib_jar_error = None;
                        }
                        Err(err) => {
                            log::warn!("皮肤站组件准备失败: {err}");
                            self.authlib_jar_error = Some(err);
                            self.error("皮肤站组件下载失败，皮肤与披风将无法加载");
                        }
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
                CoreEvent::LocalSkinLoaded { account_id, image } => {
                    self.skin_pending.remove(&account_id);
                    if let Some(image) = image {
                        let model = match self.accounts.get(&account_id) {
                            Some(a) if a.skin_model() == SkinModel::Classic && image.detect_slim() => {
                                SkinModel::Slim
                            }
                            Some(a) => a.skin_model(),
                            None => SkinModel::Classic,
                        };
                        self.upload_skin(&account_id, &image, model, SkinSource::Local);
                        return;
                    }
                    // 本地没有这个文件，接着走在线下载或自动生成
                    self.ensure_skin(&account_id);
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
                CoreEvent::CurseForgeKeyTest(result) => {
                    self.cf_key_testing = false;
                    self.cf_key_test = Some(result);
                }
                CoreEvent::ContentLookup {
                    kind,
                    source,
                    map,
                    asked,
                } => {
                    for (name, project) in map {
                        if source == crate::core::content::ContentSource::CurseForge {
                            self.versions_ui.content_cf.insert(name.clone(), project.clone());
                        } else {
                            self.versions_ui.content_slugs.insert(name.clone(), project.clone());
                        }
                        // 查到的结果存进本地缓存，下次打开这一页不用再联网
                        self.content_cache.set(
                            crate::core::content::cache::Entry::from_remote(
                                kind,
                                source,
                                &name,
                                &project,
                            ),
                        );
                    }
                    // 问过却没找到的也记下来，
                    // 下次打开不会又去猜一遍文件名
                    for name in asked {
                        let already = self
                            .content_cache
                            .get(source, kind, &name)
                            .map(|entry| !entry.is_miss())
                            .unwrap_or(false);
                        if already {
                            continue;
                        }
                        self.content_cache.set(
                            crate::core::content::cache::Entry::miss(kind, source, &name),
                        );
                    }
                    self.content_cache.save();
                }
                CoreEvent::ContentLookupFailed(kind) => {
                    // 这次没查成，去掉标记，下次打开还能再试
                    let prefix = format!("{}|", kind.facet());
                    self.versions_ui
                        .content_looked_up
                        .retain(|key| !key.starts_with(&prefix));
                    self.versions_ui
                        .content_cf_looked_up
                        .retain(|key| !key.starts_with(&prefix));
                }
                CoreEvent::ContentUpdated {
                    key,
                    kind,
                    old_file,
                    new_file,
                    version,
                    updated,
                } => {
                    let state = if updated {
                        views::versions::ContentUpdateState::Done(version.clone())
                    } else {
                        views::versions::ContentUpdateState::UpToDate
                    };
                    self.versions_ui.content_updates.insert(key, state);
                    if updated {
                        if let Some(new_file) = new_file {
                            if new_file != old_file {
                                // 文件名变了，缓存里的记录跟着挪
                                self.content_cache.rename_entry(kind, &old_file, &new_file);
                                // 内存里的项目表也跟着挪
                                if let Some(project) =
                                    self.versions_ui.content_slugs.remove(&old_file)
                                {
                                    self.versions_ui.content_slugs.insert(new_file.clone(), project);
                                }
                                if let Some(project) = self.versions_ui.content_cf.remove(&old_file)
                                {
                                    self.versions_ui.content_cf.insert(new_file.clone(), project);
                                }
                                // 更新状态也跟着挪到新名字上
                                let old_key = format!("{}|{}", kind.facet(), old_file);
                                let new_key = format!("{}|{}", kind.facet(), new_file);
                                if let Some(state) = self.versions_ui.content_updates.remove(&old_key)
                                {
                                    self.versions_ui.content_updates.insert(new_key, state);
                                }
                            }
                            // 目录内容变了，列表缓存要作废
                            self.versions_ui.content_cache.clear();
                        }
                        self.content_cache.save();
                    }
                }
                CoreEvent::ContentUpdateFailed { key, error } => {
                    self.versions_ui
                        .content_updates
                        .insert(key, views::versions::ContentUpdateState::Failed(error));
                }
                CoreEvent::ContentInstalled {
                    kind,
                    source,
                    file_name,
                    project_id,
                    version_id,
                    slug,
                    icon_url,
                } => {
                    // 装好的内容记进缓存，下次打开这一页直接认得它
                    let mut entry = crate::core::content::cache::Entry::from_remote(
                        kind,
                        source,
                        &file_name,
                        &crate::core::content::RemoteProject {
                            slug: slug.clone(),
                            icon_url: icon_url.clone(),
                            project_id: Some(project_id.clone()),
                        },
                    );
                    entry.installed_version = Some(version_id);
                    self.content_cache.set(entry);
                    self.content_cache.save();
                    // 内存里的项目表也补上，图标与链接立刻就有
                    let project = crate::core::content::RemoteProject {
                        slug,
                        icon_url,
                        project_id: Some(project_id),
                    };
                    match source {
                        crate::core::content::ContentSource::CurseForge => {
                            self.versions_ui
                                .content_cf
                                .insert(file_name.clone(), project);
                        }
                        crate::core::content::ContentSource::Modrinth => {
                            self.versions_ui
                                .content_slugs
                                .insert(file_name.clone(), project);
                        }
                    }
                }
                CoreEvent::ModFeatures(map) => {
                    self.mod_features_scanning = false;
                    self.mod_features = map;
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
                    // 整合包实例一定要有自己的目录，
                    // 不然它的模组会跟原版实例混在一起
                    let inst_dir =
                        crate::core::minecraft::version::version_dir(&self.cfg.game_dir, &version);
                    if crate::core::minecraft::modpack::is_instance(&inst_dir) {
                        let ov = self.cfg.version_overrides.entry(version.clone()).or_default();
                        ov.isolation = Some(true);
                        self.save_config();
                    }
                    self.refresh_local_versions();
                    let label = self
                        .instances
                        .iter()
                        .find(|i| i.is_modpack && i.variants.iter().any(|v| v.id == version))
                        .map(|i| i.display_name.clone())
                        .unwrap_or_else(|| version.clone());
                    if self.cfg.download.auto_select_after_install {
                        self.cfg.selected_version = Some(version.clone());
                        self.save_config();
                    }
                    self.success(format!("{label} 已安装完成"));
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
        // 客户端窗口起来没有：它自己的日志里会先冒出来那些标记，
        // 看到就说明已经进游戏了，这时候说「启动完成」才是准的。
        let ready = !self.game_ready_announced
            && (game.log.client_ready() || game.started.elapsed().as_secs() >= 25);
        let version = game.version_id.clone();
        let mut exit: Option<(i32, u64, PathBuf, Arc<GameLog>)> = None;
        let mut broken = false;
        match game.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code().unwrap_or(-1);
                    let ran_secs = game.started.elapsed().as_secs();
                    let game_dir = game.game_dir.clone();
                    let log = game.log.clone();
                    log.set_exit(code);
                    exit = Some((code, ran_secs, game_dir, log));
                }
                Ok(None) => {}
                Err(_) => broken = true,
            },
            None => {}
        }
        // 到这里对 game 的借用结束了，下面可以放心改 App 的状态
        if ready {
            self.game_ready_announced = true;
            self.game_phase = GamePhase::Running;
            self.game_starting = false;
            self.success(format!("{version} 启动完成"));
            if self.cfg.launch.sound {
                crate::core::sound::play_done();
            }
        }
        if let Some((code, ran_secs, game_dir, _log)) = exit {
            self.game = None;
            self.game_starting = false;
            self.game_ready_announced = false;
            self.game_phase = if code == 0 {
                GamePhase::Exited
            } else {
                GamePhase::Failed
            };
            // 崩了的话把崩溃报告的名字给出来，用户不用自己去翻目录
            let crash = if code != 0 {
                crate::core::minecraft::launch::latest_crash_report(&game_dir)
            } else {
                None
            };
            // 游戏没了，日志相关的界面也一起收掉，屏幕上不留上次运行的东西
            clear_game_log_ui(self);
            self.handle_events();
            if code == 0 {
                self.info(format!("{version} 已退出"));
            } else if let Some(path) = &crash {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.error(format!("{version} 异常退出（代码 {code}），崩溃报告 {name}"));
            } else {
                self.error(format!("{version} 异常退出（代码 {code}）"));
            }
            let _ = ran_secs;
            self.refresh_local_versions();
            // 游戏结束了，按设置把启动器恢复回来
            self.restore_launcher_after_game();
        } else if broken {
            self.game = None;
            self.game_starting = false;
            self.game_phase = GamePhase::Failed;
            clear_game_log_ui(self);
        } else {
            self.egui_ctx
                .request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    /// 游戏退出后处理启动器的隐藏与关闭
    fn restore_launcher_after_game(&mut self) {
        if self.launcher_hidden && self.launcher_reopen {
            self.ctx_send_visible(true);
        }
        self.launcher_hidden = false;
        self.launcher_reopen = false;
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
        // 直接借用 painter：克隆会把这一帧已绘制的图形全拷一遍
        let painter = ui.painter();
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
                    // 最下方：品牌标识，跟 HMCL 一样把名字放在左下角
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("HTMCL")
                            .font(ui::font(11.5))
                            .color(self.palette.text_faint)
                            .strong(),
                    );
                    ui.add_space(6.0);

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
                ui::progress(ui, &palette, &format!("task-{}", task.id), task.fraction, 6.0);
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
            // 手动结束也算游戏关了，日志相关的界面一起收掉
            clear_game_log_ui(self);
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

    /// 启动器内的游戏日志页
    ///
    /// 右边放不下的时候可以整页打开：日志占满内容区，方便从头翻。
    fn game_log_page(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let Some((version, phase, _, log, game_dir)) = self.game_log_info() else {
            self.show_game_log_page = false;
            return;
        };
        let max_lines = self.cfg.ui.max_log_lines.clamp(500, 20000);
        let filter = self.game_log_filter.trim().to_lowercase();
        let all = log.snapshot(max_lines);
        let shown: Vec<GameLogLine> = if filter.is_empty() {
            all.clone()
        } else {
            all.iter()
                .filter(|l| l.text.to_lowercase().contains(&filter))
                .cloned()
                .collect()
        };

        let mut clear = false;
        let mut copy = false;
        let mut open_game_logs = false;
        let mut open_launcher_logs = false;

        ui.horizontal(|ui| {
            if ui::ghost(ui, &palette, "返回").clicked() {
                self.show_game_log_page = false;
            }
            ui.label(
                egui::RichText::new("游戏日志")
                    .font(ui::font(16.0))
                    .color(palette.text)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!("{} · {}", version, phase.label()))
                    .font(ui::font(12.0))
                    .color(palette.text_weak),
            );
            ui.label(
                egui::RichText::new(format!("共 {} 行", all.len()))
                    .font(ui::font(12.0))
                    .color(palette.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::ghost(ui, &palette, "打开启动器日志").clicked() {
                    open_launcher_logs = true;
                }
                if ui::ghost(ui, &palette, "打开游戏日志").clicked() {
                    open_game_logs = true;
                }
                if ui::ghost(ui, &palette, "清空显示").clicked() {
                    clear = true;
                }
                if ui::ghost(ui, &palette, "复制全部").clicked() {
                    copy = true;
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("过滤")
                    .font(ui::font(12.0))
                    .color(palette.text_weak),
            );
            ui::text_field(ui, &palette, &mut self.game_log_filter, "按关键字过滤", 240.0, false);
            if !self.game_log_filter.is_empty() {
                if ui::ghost(ui, &palette, "清除").clicked() {
                    self.game_log_filter.clear();
                }
            }
            ui.label(
                egui::RichText::new(format!("显示 {} 行", shown.len()))
                    .font(ui::font(12.0))
                    .color(palette.text_faint),
            );
        });
        ui.add_space(8.0);
        egui::Frame::NONE
            .fill(palette.surface_alt)
            .corner_radius(palette.corner_sm())
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(520.0)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        draw_log_lines(ui, &palette, &shown);
                    });
            });

        if clear {
            log.clear();
        }
        if copy {
            let text = shown
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            ui.ctx().copy_text(text);
        }
        if open_game_logs {
            paths::reveal(&game_dir.join("logs"));
        }
        if open_launcher_logs {
            paths::reveal(&paths::logs_dir());
        }
    }

    /// 现在还有没有日志可以看
    ///
    /// 只认正在跑的游戏：游戏一关，日志相关的界面全部撤掉。
    pub fn has_game_log(&self) -> bool {
        self.game.is_some()
    }

    /// 当前这份日志的出处：版本、状态、跑了多久、日志本体、游戏目录
    fn game_log_info(&self) -> Option<(String, GamePhase, u64, Arc<GameLog>, PathBuf)> {
        let game = self.game.as_ref()?;
        Some((
            game.version_id.clone(),
            self.game_phase,
            game.started.elapsed().as_secs(),
            game.log.clone(),
            game.game_dir.clone(),
        ))
    }

    /// 右侧停靠的游戏日志面板
    ///
    /// 游戏跑起来以后日志就摆在右手边：状态、耗时、实时日志、常用按钮
    /// 都在这一块里，不用再去角落里找那个圆球。
    fn game_log_dock(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let Some((version, phase, elapsed, log, game_dir)) = self.game_log_info() else {
            return;
        };
        let max_lines = self.cfg.ui.max_log_lines.clamp(500, 20000);
        let filter = self.game_log_filter.trim().to_lowercase();
        let all = log.snapshot(max_lines);
        let shown: Vec<GameLogLine> = if filter.is_empty() {
            all.clone()
        } else {
            all.iter()
                .filter(|l| l.text.to_lowercase().contains(&filter))
                .cloned()
                .collect()
        };

        let mut collapse = false;
        let mut full_page = false;
        let mut clear = false;
        let mut copy = false;
        let mut open_logs = false;
        let mut stop = false;

        egui::Frame::NONE
            .fill(palette.surface)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(palette.corner_sm())
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::paint(
                        ui.ctx(),
                        ui.painter(),
                        "terminal",
                        Rect::from_center_size(
                            Pos2::new(ui.cursor().left() + 9.0, ui.cursor().center().y),
                            Vec2::splat(17.0),
                        ),
                        palette.accent,
                    );
                    ui.add_space(18.0);
                    ui.label(
                        egui::RichText::new(&version)
                            .font(ui::font(14.5))
                            .color(palette.text)
                            .strong(),
                    );
                    status_badge(ui, &palette, phase.label(), phase.color(&palette));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui::ghost(ui, &palette, "收起").clicked() {
                            collapse = true;
                        }
                        if ui::ghost(ui, &palette, "整页").clicked() {
                            full_page = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("已运行 {:02}:{:02}", elapsed / 60, elapsed % 60))
                            .font(ui::font(12.0))
                            .color(palette.text_weak),
                    );
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui::text_field(ui, &palette, &mut self.game_log_filter, "过滤日志", 120.0, false);
                    if ui::ghost(ui, &palette, "复制").clicked() {
                        copy = true;
                    }
                    if ui::ghost(ui, &palette, "清空").clicked() {
                        clear = true;
                    }
                    if ui::ghost(ui, &palette, "目录").clicked() {
                        open_logs = true;
                    }
                });
                ui.add_space(8.0);
                // 状态与操作放在日志上面，日志区就正好占满剩下的地方，
                // 底部那一行不会被日志顶出去。
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("共 {} 行", all.len()))
                            .font(ui::font(11.5))
                            .color(palette.text_faint),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.game.is_some() {
                            if ui::button(ui, &palette, ui::ButtonOpts::danger("结束游戏")).clicked() {
                                stop = true;
                            }
                        }
                    });
                });
                ui.add_space(6.0);
                let log_height = ui.available_height().max(80.0);
                egui::Frame::NONE
                    .fill(palette.surface_alt)
                    .corner_radius(palette.corner_sm())
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(log_height)
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                draw_log_lines(ui, &palette, &shown);
                            });
                    });
            });

        if collapse {
            self.game_log_docked = false;
            self.cfg.ui.log_dock = false;
            self.save_config();
        }
        if full_page {
            self.show_game_log_page = true;
        }
        if clear {
            log.clear();
        }
        if copy {
            let text = shown
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            ui.ctx().copy_text(text);
        }
        if open_logs {
            paths::reveal(&game_dir.join("logs"));
        }
        if stop {
            if let Some(game) = self.game.as_mut() {
                game.kill();
            }
            self.game = None;
            self.game_starting = false;
            self.game_phase = GamePhase::Failed;
            self.info("已结束游戏进程");
        }
    }

    /// 收起状态下的日志入口：贴在右边边缘的一小块
    ///
    /// 不再是右下角那个圆球：那个位置跟下载中心和提示条挤在一起，
    /// 挪到右边缘中间，就不挡别的东西了。
    fn draw_log_tab(&mut self, root: &mut Ui) {
        let ctx = root.ctx().clone();
        let palette = self.palette;
        let full = root.max_rect();
        let size = Vec2::new(30.0, 76.0);
        let rect = Rect::from_center_size(
            Pos2::new(full.right() - size.x / 2.0 - 6.0, full.center().y),
            size,
        );
        let response = root.interact(rect, egui::Id::new("htmcl_log_tab"), Sense::click());
        let hovered = response.hovered();
        root.painter().rect_filled(
            rect,
            palette.corner_sm(),
            if hovered {
                palette.accent_weak
            } else {
                palette.surface
            },
        );
        root.painter().rect_stroke(
            rect,
            palette.corner_sm(),
            Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
        icons::paint(
            &ctx,
            root.painter(),
            "terminal",
            Rect::from_center_size(
                Pos2::new(rect.center().x, rect.center().y - 12.0),
                Vec2::splat(18.0),
            ),
            if hovered {
                palette.accent
            } else {
                palette.text_weak
            },
        );
        let count = self
            .game_log_info()
            .map(|(_, _, _, log, _)| log.len())
            .unwrap_or(0);
        root.painter().text(
            Pos2::new(rect.center().x, rect.center().y + 18.0),
            egui::Align2::CENTER_CENTER,
            format!("{count}"),
            ui::font(10.5),
            palette.text_faint,
        );
        if response.clicked() {
            self.game_log_docked = true;
            self.cfg.ui.log_dock = true;
            self.save_config();
        }
        if response.hovered() {
            response.on_hover_ui(|ui| {
                ui.label(
                    egui::RichText::new("展开游戏日志")
                        .font(ui::font(11.5))
                        .color(palette.text_weak),
                );
            });
        }
    }

    /// 启动过场：居中的一张小卡片
    ///
    /// 不再把整个窗口涂成一片空白：底下照样看得见启动器，
    /// 卡片里写着版本、耗时和刚冒出来的几行日志。
    ///
    /// 这张卡片是自己画出来的，没有用 egui 的弹窗容器：
    /// 那种容器第一次出现时位置会飘一下，卡片会歪到一边去。
    fn draw_starting_overlay(&mut self, root: &mut Ui) {
        let ctx = root.ctx().clone();
        let now = ctx.input(|i| i.time);
        let full = root.max_rect();
        let palette = self.palette;

        let (version, elapsed, recent) = match self.game_log_info() {
            Some((version, _, elapsed, log, _)) => (version, elapsed, log.snapshot(4)),
            None => (String::new(), 0, Vec::new()),
        };

        // 半透明遮罩：底下的界面还认得出来
        root.painter()
            .rect_filled(full, CornerRadius::ZERO, Color32::from_black_alpha(110));

        const CARD_W: f32 = 440.0;
        const PAD: f32 = 20.0;
        let log_rows = recent.len().max(1) as f32;
        let log_h = log_rows * 18.0 + 12.0;
        // 顶行 26 + 版本 32 + 转圈 26 + 日志框 + 提示 24 + 上下留白
        let card_h = PAD * 2.0 + 26.0 + 32.0 + 26.0 + log_h + 24.0;
        let card = Rect::from_center_size(full.center(), Vec2::new(CARD_W, card_h));
        let radius = palette.corner(2);
        root.painter().rect_filled(card, radius, palette.surface);
        root.painter().rect_stroke(
            card,
            radius,
            Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );

        let left = card.left() + PAD;
        let mut y = card.top() + PAD;

        // 顶行：正在启动 + 不用等了
        root.painter().text(
            Pos2::new(left, y + 10.0),
            egui::Align2::LEFT_CENTER,
            "正在启动",
            ui::font(12.5),
            palette.text_weak,
        );
        let dismiss_rect = Rect::from_min_size(
            Pos2::new(card.right() - PAD - 72.0, y),
            Vec2::new(72.0, 22.0),
        );
        let dismiss = root.interact(
            dismiss_rect,
            egui::Id::new("htmcl_starting_dismiss"),
            Sense::click(),
        );
        if dismiss.hovered() {
            root.painter().rect_filled(dismiss_rect, palette.corner_sm(), palette.surface_alt);
        }
        root.painter().text(
            dismiss_rect.center(),
            egui::Align2::CENTER_CENTER,
            "不用等了",
            ui::font(12.0),
            if dismiss.hovered() {
                palette.text
            } else {
                palette.text_weak
            },
        );
        if dismiss.clicked() {
            self.game_starting = false;
        }
        y += 26.0;

        // 版本号
        root.painter().text(
            Pos2::new(left, y + 10.0),
            egui::Align2::LEFT_CENTER,
            version,
            ui::font(19.0),
            palette.text,
        );
        y += 32.0;

        // 转圈 + 已用时
        draw_spinner(
            root.painter(),
            Pos2::new(left + 9.0, y + 8.0),
            9.0,
            now,
            palette.accent,
        );
        root.painter().text(
            Pos2::new(left + 26.0, y + 8.0),
            egui::Align2::LEFT_CENTER,
            format!("已用时 {elapsed} 秒"),
            ui::font(13.0),
            palette.text_weak,
        );
        y += 26.0;

        // 最近几行日志
        let log_rect = Rect::from_min_max(
            Pos2::new(left, y),
            Pos2::new(card.right() - PAD, y + log_rows * 18.0 + 12.0),
        );
        root.painter()
            .rect_filled(log_rect, palette.corner_sm(), palette.surface_alt);
        if recent.is_empty() {
            root.painter().text(
                Pos2::new(log_rect.left() + 9.0, log_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "正在等游戏的第一行日志 ...",
                ui::mono(11.0),
                palette.text_faint,
            );
        } else {
            for (i, line) in recent.iter().enumerate() {
                let text = widgets::ellipsize(&line.text, log_rect.width() - 18.0, 11.0);
                root.painter().text(
                    Pos2::new(log_rect.left() + 9.0, log_rect.top() + 15.0 + i as f32 * 18.0),
                    egui::Align2::LEFT_CENTER,
                    text,
                    ui::mono(11.0),
                    palette.text_weak,
                );
            }
        }
        y = log_rect.bottom() + 10.0;

        root.painter().text(
            Pos2::new(left, y + 6.0),
            egui::Align2::LEFT_CENTER,
            "游戏在后台运行，日志一直记着，右侧面板里能看",
            ui::font(11.5),
            palette.text_faint,
        );
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    fn game_log_modal(&mut self, ctx: &egui::Context) {
        if !self.game_log_open {
            return;
        }
        let palette = self.palette;
        let lines = self
            .game
            .as_ref()
            .map(|g| g.log.snapshot(self.cfg.ui.max_log_lines.clamp(500, 20000)))
            .unwrap_or_default();
        let mut open = true;
        let mut clear = false;
        let mut collapse = false;
        widgets::modal(ctx, &palette, &mut open, "游戏日志", 720.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("共 {} 行", lines.len()))
                        .font(ui::font(12.0))
                        .color(palette.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::ghost(ui, &palette, "收起").clicked() {
                        collapse = true;
                    }
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
                            draw_log_lines(ui, &palette, &lines);
                        });
                });
        });
        if clear {
            if let Some(game) = self.game.as_ref() {
                game.log.clear();
            }
        }
        if collapse {
            open = false;
        }
        self.game_log_open = open && self.game.is_some();
        // 弹窗收起来了，右边的日志面板接着显示
        if !self.game_log_open {
            self.game_log_docked = true;
        }
    }
}

/// 画一段日志
///
/// 标准错误用红色标出来，其余按正文颜色显示。
fn draw_log_lines(ui: &mut Ui, p: &Palette, lines: &[GameLogLine]) {
    for line in lines {
        let color = match line.source {
            LogSource::Stderr => p.danger,
            LogSource::Stdout | LogSource::Game => p.text,
        };
        // 日志要能选中复制，所以这里单独打开可选
        ui.add(
            egui::Label::new(
                egui::RichText::new(&line.text)
                    .font(ui::mono(11.0))
                    .color(color),
            )
            .selectable(true),
        );
    }
}

/// 状态小标签
fn status_badge(ui: &mut Ui, p: &Palette, text: &str, color: Color32) {
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.18))
        .corner_radius(p.corner_sm())
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .font(ui::font(11.0))
                    .color(color)
                    .strong(),
            );
        });
}

/// 转圈的进度点
///
/// 一串小方块绕着圈跑，尾巴越跑越淡。
/// 用方块不用圆点：方块跟 Minecraft 的像素调子也搭。
fn draw_spinner(painter: &egui::Painter, center: Pos2, radius: f32, now: f64, color: Color32) {
    let count = 10;
    let spin = (now * 1.9) as f32;
    for i in 0..count {
        let a = spin + i as f32 * std::f32::consts::TAU / count as f32;
        let tail = i as f32 / count as f32;
        let pos = center + Vec2::new(a.cos(), a.sin()) * radius;
        let size = 2.5 + 3.5 * tail;
        painter.rect_filled(
            Rect::from_center_size(pos, Vec2::splat(size)),
            CornerRadius::ZERO,
            color.gamma_multiply(0.22 + 0.78 * tail),
        );
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
        // 记一下这一帧花了多久，卡住时日志里能看出是哪一页
        let frame_start = std::time::Instant::now();
        let page_before = self.page.page_index();

        if self.first_frame {
            self.first_frame = false;
            if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(frame) {
                if let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw() {
                    apply_win11_style(win.hwnd.get());
                    self.window_handle = Some(win.hwnd.get() as isize);
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

        // 正在启动：盖一层过场卡片，底下的界面还看得见
        // （这层画在最后，别挡着日志面板）

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
        // 侧栏收放是滑动的，不是啪地一下变窄
        let target_width = if collapsed { 64.0 } else { 186.0 };
        let sidebar_width = if ui::animations_enabled() {
            ctx.animate_value_with_time(egui::Id::new("sidebar_width"), target_width, 0.2)
        } else {
            target_width
        };
        let sidebar_rect = Rect::from_min_size(
            Pos2::new(body.left() + 8.0, body.top()),
            Vec2::new(sidebar_width, body.height()),
        );

        // 页面切换：内容淡入的同时略微上移，像是推进来
        let page_enter = ui::anim::enter(&ctx, egui::Id::new(("page", self.page.page_index())), 0.0);
        if page_enter.opacity < 1.0 {
            ctx.request_repaint();
        }
        let page_offset = page_enter.offset;

        // 右侧停靠的游戏日志面板
        //
        // 游戏跑起来以后日志就摆在右手边，内容区相应让出一块，
        // 面板是滑进来的，不是啪地一下出现。
        let dock_target = if self.game_log_docked && self.has_game_log() && !self.show_game_log_page {
            1.0
        } else {
            0.0
        };
        let dock_anim = if ui::animations_enabled() {
            ctx.animate_value_with_time(egui::Id::new("game_log_dock"), dock_target, 0.18)
        } else {
            dock_target
        };
        let dock_width = DOCK_WIDTH * dock_anim;

        let content_rect = Rect::from_min_max(
            Pos2::new(sidebar_rect.right() + 8.0, body.top() + page_offset),
            Pos2::new(body.right() - 18.0 - dock_width, body.bottom()),
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

        root.scope_builder(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_width(content_rect.width());
                ui.set_opacity(page_enter.opacity);
                ui.add_space(12.0);
                // 状态栏只在日志面板收起时出现，两处都摆一遍就重复了
                if !(self.game_log_docked && self.has_game_log() && !self.show_game_log_page) {
                    self.game_bar(ui);
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.set_max_width(content_rect.width() - 14.0);
                        ui.set_width(content_rect.width() - 14.0);
                        if self.show_game_log_page && self.has_game_log() {
                            self.game_log_page(ui);
                        } else {
                            match self.page {
                                Page::Home => views::home::ui(self, ui),
                                Page::Download => views::download::ui(self, ui),
                                Page::Versions => views::versions::ui(self, ui),
                                Page::Accounts => views::accounts::ui(self, ui),
                                Page::Settings => views::settings::ui(self, ui),
                                Page::About => views::about::ui(self, ui),
                            }
                        }
                        ui.add_space(24.0);
                    });
            },
        );

        // 右侧停靠的游戏日志面板
        if dock_anim > 0.001 {
            let dock_rect = Rect::from_min_max(
                Pos2::new(body.right() - 18.0 - dock_width, body.top() + page_offset),
                Pos2::new(body.right() - 18.0, body.bottom()),
            );
            root.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(dock_rect)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    ui.set_width(dock_rect.width());
                    ui.set_opacity(dock_anim);
                    self.game_log_dock(ui);
                },
            );
        } else if self.has_game_log() && !self.show_game_log_page {
            // 收起来了，右边缘留一个小标签
            self.draw_log_tab(root);
        }

        views::home_config::game_config_modal(self, &ctx);
        self.game_log_modal(&ctx);
        self.download_panel(&ctx);
        // 提示条摆在窗口下方中间：右下角让给下载中心，右边让给日志面板
        widgets::toast_layer(&ctx, &self.palette, &mut self.toasts, now, 0.0);

        // 启动过场盖在最上面，底下的界面还看得见
        if self.game_starting {
            self.draw_starting_overlay(root);
        }

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

        // 这一帧花了多久：卡住时日志里能看出是哪一页
        let elapsed = frame_start.elapsed();
        if elapsed.as_millis() > 300 {
            log::warn!(
                "界面渲染耗时 {} 毫秒（当前页面 {}）",
                elapsed.as_millis(),
                page_before
            );
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
        20 => 5.0,
        21 => 7.0,
        22 => 7.0,
        23 => 2.0,
        27 => 5.0,
        28 => 2.0,
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
        19 => "版本管理-启动选项",
        20 => "版本管理-模组",
        21 => "版本管理-光影",
        22 => "版本管理-资源包",
        24 => "版本管理-蓝图",
        25 => "版本管理-投影蓝图",
        26 => "版本管理-存档",
        27 => "下载-整合包详情",
        28 => "版本管理-整合包",
        29 => "版本管理-加载器",
        30 => "版本管理-光影加载器",
        31 => "版本管理-导出",
        32 => "版本管理-概览",
        33 => "下载-CurseForge",
        34 => "游戏日志面板",
        35 => "启动过场",
        _ => "账号-皮肤站",
    }
}

#[cfg(windows)]
/// 把窗口挪到屏幕右下角
fn move_window_to_bottom_right(handle: Option<isize>) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SetWindowPos, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN, SWP_NOSIZE,
        SWP_NOACTIVATE, SWP_SHOWWINDOW,
    };
    // SAFETY: 句柄来自 eframe 的原生窗口，由系统管理
    unsafe {
        let hwnd = match handle {
            Some(handle) => handle as HWND,
            None => return,
        };
        let screen_w = GetSystemMetrics(SM_CXSCREEN) as i32;
        let screen_h = GetSystemMetrics(SM_CYSCREEN) as i32;
        let size = 64;
        let x = screen_w - size - 12;
        let y = screen_h - size - 12;
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

#[cfg(windows)]
/// 当前启动器窗口的原生句柄
fn current_window_handle() -> Option<isize> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            None
        } else {
            Some(hwnd as isize)
        }
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
