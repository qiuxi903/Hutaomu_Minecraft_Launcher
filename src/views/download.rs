use egui::{Align, Color32, Layout, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::{App, CoreEvent};
use crate::core::content::mcmod;
use crate::core::content::modrinth::{self, ContentKind, Project, SearchHit, SearchResponse, Version};
use crate::core::content::translate;
use crate::core::minecraft::bundle;
use crate::core::minecraft::loaders::{self, LoaderKind, LoaderPolicy, LoaderVersion};
use crate::core::minecraft::manifest::VersionEntry;
use crate::icons;
use crate::theme::Palette;
use crate::ui::{self, widgets, ButtonOpts};
use crate::views;

const TABS: [&str; 6] = ["游戏", "加载器", "Mod", "光影", "资源包", "整合包"];

pub struct DownloadState {
    pub tab: usize,
    pub game_filter: usize,
    pub game_search: String,
    pub game_limit: usize,

    pub loader_kind: usize,
    pub loader_mc: String,
    pub loader_versions: Vec<LoaderVersion>,
    pub loader_version_index: usize,
    pub loader_loading: bool,
    pub loader_error: Option<String>,

    /// 安装弹窗的目标版本
    pub install_target: Option<VersionEntry>,
    /// 弹窗中选择的加载器下标：0 = 不安装加载器
    pub install_loader_index: usize,
    /// 弹窗中加载器版本是否交给启动器自动选择
    pub install_loader_auto: bool,
    /// 弹窗中可手动选择的加载器版本
    pub install_loader_versions: Vec<LoaderVersion>,
    pub install_loader_version_index: usize,
    pub install_loader_loading: bool,
    pub install_loader_error: Option<String>,
    /// 加载器版本列表对应的游戏版本
    pub install_loader_mc: String,
    /// 弹窗中当前选择的加载器类型，用于校验版本列表是否匹配
    pub install_loader_kind: Option<LoaderKind>,

    /// 已自动加载过的内容标签页，用于切回时避免重复搜索
    pub content_loaded_tab: Option<usize>,

    /// 详情页选择的安装实例（本地版本 id），默认当前实例
    pub content_install_instance: Option<String>,
    /// 详情页的加载器筛选：空 = 跟随实例
    pub content_loader_filter: String,
    /// 详情页的游戏版本筛选：空 = 跟随实例，"*" = 全部
    pub content_game_filter: String,
    /// 详情页的项目作者（从列表带入）
    pub content_author: String,

    pub content_query: String,
    pub content_hits: Vec<SearchHit>,
    pub content_total: u32,
    pub content_loading: bool,
    pub content_error: Option<String>,
    pub content_seq: u64,
    pub content_detail: Option<Box<Project>>,
    /// 当前详情页的中文简介（翻译服务不可用时为空）
    pub content_translation: Option<String>,
    pub content_translation_loading: bool,
    pub content_versions: Vec<Version>,
    pub content_versions_loading: bool,
    pub content_version_index: usize,
    pub use_version_filter: bool,
}

impl Default for DownloadState {
    fn default() -> Self {
        Self {
            tab: 0,
            game_filter: 0,
            game_search: String::new(),
            game_limit: 60,
            loader_kind: 0,
            loader_mc: String::new(),
            loader_versions: Vec::new(),
            loader_version_index: 0,
            loader_loading: false,
            loader_error: None,
            install_target: None,
            install_loader_index: 0,
            install_loader_auto: true,
            install_loader_versions: Vec::new(),
            install_loader_version_index: 0,
            install_loader_loading: false,
            install_loader_error: None,
            install_loader_mc: String::new(),
            install_loader_kind: None,
            content_loaded_tab: None,
            content_install_instance: None,
            content_loader_filter: String::new(),
            content_game_filter: String::new(),
            content_author: String::new(),
            content_query: String::new(),
            content_hits: Vec::new(),
            content_total: 0,
            content_loading: false,
            content_error: None,
            content_seq: 0,
            content_detail: None,
            content_translation: None,
            content_translation_loading: false,
            content_versions: Vec::new(),
            content_versions_loading: false,
            content_version_index: 0,
            use_version_filter: true,
        }
    }
}

impl DownloadState {
    pub fn on_search_result(&mut self, seq: u64, result: Result<Box<SearchResponse>, String>) {
        if seq != self.content_seq {
            return;
        }
        self.content_loading = false;
        match result {
            Ok(response) => {
                log::info!("搜索完成：{} 条结果", response.hits.len());
                self.content_total = response.total_hits;
                self.content_hits = response.hits;
                self.content_error = None;
            }
            Err(err) => {
                self.content_error = Some(err);
                self.content_hits.clear();
            }
        }
    }

    pub fn on_versions_result(&mut self, project_id: &str, result: Result<Vec<Version>, String>) {
        if self
            .content_detail
            .as_ref()
            .map(|p| p.id != project_id)
            .unwrap_or(true)
        {
            return;
        }
        self.content_versions_loading = false;
        match result {
            Ok(list) => {
                self.content_versions = list;
                self.content_version_index = 0;
            }
            Err(err) => {
                self.content_error = Some(err);
                self.content_versions.clear();
            }
        }
    }

    pub fn on_loader_versions(
        &mut self,
        kind: LoaderKind,
        mc: &str,
        result: Result<Vec<LoaderVersion>, String>,
    ) {
        if LoaderKind::all()[self.loader_kind] != kind || self.loader_mc != mc {
            return;
        }
        self.loader_loading = false;
        match result {
            Ok(list) => {
                self.loader_versions = list;
                self.loader_version_index = 0;
                self.loader_error = None;
            }
            Err(err) => {
                self.loader_versions.clear();
                self.loader_error = Some(err);
            }
        }
    }

    /// 安装弹窗中加载器版本列表的返回结果
    pub fn on_install_loader_versions(
        &mut self,
        kind: LoaderKind,
        mc: &str,
        result: Result<Vec<LoaderVersion>, String>,
    ) {
        // 游戏版本或加载器类型变了就丢弃过期结果
        if self.install_loader_mc != mc || self.install_loader_kind != Some(kind) {
            return;
        }
        self.install_loader_loading = false;
        match result {
            Ok(list) => {
                self.install_loader_versions = list;
                self.install_loader_version_index = 0;
                self.install_loader_error = None;
            }
            Err(err) => {
                self.install_loader_versions.clear();
                self.install_loader_error = Some(err);
            }
        }
    }

    /// 内容标签页对应的内容类型
    ///
    /// Mod / 光影 / 资源包 / 整合包 四个标签共用一个内容页，
    /// 类型直接由标签页推导，避免切换标签后内容类型没跟着变。
    pub fn content_kind(&self) -> ContentKind {
        match self.tab {
            3 => ContentKind::Shader,
            4 => ContentKind::ResourcePack,
            5 => ContentKind::Modpack,
            _ => ContentKind::Mod,
        }
    }
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(
        ui,
        &p,
        "下载",
        "安装游戏本体、加载器，以及 Mod、光影、资源包与整合包",
    );

    ui.horizontal(|ui| {
        let mut tab = app.download.tab;
        if ui::segmented(ui, &p, &mut tab, &TABS).clicked() {
            if tab != app.download.tab {
                // 换标签时收起详情与安装弹窗，避免内容与新标签对不上
                app.download.content_detail = None;
                app.download.install_target = None;
            }
            app.download.tab = tab;
        }
    });
    ui.add_space(14.0);

    // 切到内容标签时自动加载，不用再点搜索按钮
    let tab = app.download.tab;
    if (2..=5).contains(&tab) && app.download.content_loaded_tab != Some(tab) {
        app.download.content_loaded_tab = Some(tab);
        run_search(app);
    }

    match app.download.tab {
        0 => game_tab(app, ui),
        1 => loader_tab(app, ui),
        _ => content_tab(app, ui),
    }

    install_modal(app, ui);
}

// ------------------------------------------------------------------ 游戏本体

fn game_tab(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui.horizontal(|ui| {
        let mut search = app.download.game_search.clone();
        if ui::search_field(ui, &p, &mut search, "搜索版本号，例如 1.20.1", 260.0).changed() {
            app.download.game_search = search;
            app.download.game_limit = 60;
        }
        ui.add_space(6.0);
        let filters = ["正式版", "快照", "远古", "全部"];
        let mut filter = app.download.game_filter;
        if ui::segmented(ui, &p, &mut filter, &filters).clicked() {
            app.download.game_filter = filter;
            app.download.game_limit = 60;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::icon_button(ui, &p, "refresh", "重新获取版本列表").clicked() {
                app.load_manifest();
            }
            if app.manifest_loading {
                ui::spinner(ui, &p, 15.0);
            }
            if let Some(manifest) = &app.manifest {
                ui.label(
                    egui::RichText::new(format!("共 {} 个版本", manifest.versions.len()))
                        .font(ui::font(12.0))
                        .color(p.text_faint),
                );
            }
        });
    });
    ui.add_space(12.0);

    if let Some(err) = app.manifest_error.clone() {
        ui::inset(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui::icon(ui, "alert", 15.0, p.warning);
                ui.label(
                    egui::RichText::new(format!("版本列表获取失败：{err}"))
                        .font(ui::font(12.5))
                        .color(p.text_weak),
                );
                if ui::ghost(ui, &p, "重试").clicked() {
                    app.load_manifest();
                }
            });
        });
        ui.add_space(10.0);
    }

    let Some(manifest) = app.manifest.clone() else {
        if app.manifest_loading {
            ui::empty_state(ui, &p, "loader", "正在获取版本列表", "首次获取需要几秒钟");
        } else {
            ui::empty_state(ui, &p, "alert", "暂无版本列表", "点击右上角刷新重试");
        }
        return;
    };

    let keyword = app.download.game_search.trim().to_lowercase();
    let filter = app.download.game_filter;
    let entries: Vec<VersionEntry> = manifest
        .versions
        .iter()
        .filter(|v| match filter {
            0 => v.is_release(),
            1 => v.is_snapshot(),
            2 => v.is_old(),
            _ => true,
        })
        .filter(|v| keyword.is_empty() || v.id.to_lowercase().contains(&keyword))
        .take(app.download.game_limit)
        .cloned()
        .collect();

    if entries.is_empty() {
        ui::empty_state(ui, &p, "search", "没有匹配的版本", "换个关键词试试");
        return;
    }

    let mut install: Option<VersionEntry> = None;
    for entry in &entries {
        let installed = app.cfg.is_installed(&entry.id);
        let response = ui::card_padded(ui, &p, 12, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&entry.id)
                        .font(ui::font(14.5))
                        .color(p.text)
                        .strong(),
                );
                widgets::badge(
                    ui,
                    &p,
                    entry.type_label(),
                    views::version_type_color(&entry.version_type),
                );
                if installed {
                    widgets::badge(ui, &p, "已安装", p.success);
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(short_date(&entry.release_time))
                        .font(ui::font(11.5))
                        .color(p.text_faint),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if installed {
                        if ui::button(ui, &p, ButtonOpts::secondary("打开目录")).clicked() {
                            views::open_version_dir(app, &entry.id);
                        }
                    } else if ui::button(ui, &p, ButtonOpts::primary("安装").icon("download-arrow")).clicked()
                    {
                        install = Some(entry.clone());
                    }
                });
            });
        });
        let _ = response;
        ui.add_space(6.0);
    }

    if entries.len() >= app.download.game_limit {
        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("加载更多")).clicked() {
                app.download.game_limit += 60;
            }
        });
    }

    if let Some(entry) = install {
        open_install_dialog(app, entry);
    }
}

/// 打开安装弹窗，并按设置预选加载器
pub(crate) fn open_install_dialog(app: &mut App, entry: VersionEntry) {
    app.download.install_target = Some(entry);
    app.download.install_loader_index = default_loader_index(app);
    app.download.install_loader_auto = app.cfg.download.loader_policy() != LoaderPolicy::Ask;
    app.download.install_loader_versions.clear();
    app.download.install_loader_version_index = 0;
    app.download.install_loader_error = None;
    app.download.install_loader_mc.clear();
    app.download.install_loader_kind = None;
}

/// 设置里配置的默认加载器对应的下标（0 = 不安装）
fn default_loader_index(app: &App) -> usize {
    match app.cfg.download.default_loader_kind() {
        None => 0,
        Some(LoaderKind::Fabric) => 1,
        Some(LoaderKind::Quilt) => 2,
        Some(LoaderKind::Forge) => 3,
        Some(LoaderKind::NeoForge) => 4,
    }
}

/// 弹窗中下标对应的加载器
fn install_loader_kind(index: usize) -> Option<LoaderKind> {
    match index {
        1 => Some(LoaderKind::Fabric),
        2 => Some(LoaderKind::Quilt),
        3 => Some(LoaderKind::Forge),
        4 => Some(LoaderKind::NeoForge),
        _ => None,
    }
}

// ------------------------------------------------------------------ 安装弹窗

/// 安装弹窗：选好加载器与版本后一次把游戏和加载器都装好
fn install_modal(app: &mut App, ui: &mut Ui) {
    // 安装弹窗只在「游戏」标签下有意义
    if app.download.tab != 0 {
        app.download.install_target = None;
        return;
    }
    let Some(entry) = app.download.install_target.clone() else {
        return;
    };
    let p = app.palette;
    let mut open = true;
    let mut start = false;
    let mut cancel = false;
    let ctx = ui.ctx().clone();
    let mc = entry.id.clone();
    let title = format!("安装 Minecraft {mc}");

    let loader_labels = ["不安装", "Fabric", "Quilt", "Forge", "NeoForge"];

    widgets::modal(&ctx, &p, &mut open, &title, 500.0, |ui| {
        ui.label(
            egui::RichText::new("启动器会把游戏和加载器一起装好，不需要再单独安装加载器。")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(16.0);

        // ---------- 加载器 ----------
        ui.label(
            egui::RichText::new("加载器")
                .font(ui::font(13.0))
                .color(p.text_weak),
        );
        ui.add_space(6.0);
        let mut kind_index = app.download.install_loader_index.min(4);
        if ui::segmented(ui, &p, &mut kind_index, &loader_labels).clicked() {
            app.download.install_loader_index = kind_index;
            app.download.install_loader_versions.clear();
            app.download.install_loader_version_index = 0;
            app.download.install_loader_error = None;
            app.download.install_loader_mc.clear();
            app.download.install_loader_kind = None;
        }
        ui.add_space(10.0);

        let kind = install_loader_kind(app.download.install_loader_index);

        if let Some(kind) = kind {
            ui.label(
                egui::RichText::new(kind.description())
                    .font(ui::font(12.0))
                    .color(p.text_faint),
            );
            ui.add_space(10.0);

            let supported = loaders::supports(kind, &mc);
            if !supported {
                ui.label(
                    egui::RichText::new(format!("{} 不支持 Minecraft {mc}", kind.label()))
                        .font(ui::font(12.0))
                        .color(p.warning),
                );
                ui.add_space(8.0);
            }

            let policy = app.cfg.download.loader_policy();
            let force_manual = policy == LoaderPolicy::Ask;

            if force_manual {
                // 策略为「每次询问」时只能手动选版本
                ui.label(
                    egui::RichText::new("加载器版本")
                        .font(ui::font(13.0))
                        .color(p.text_weak),
                );
                ui.add_space(6.0);
                loader_version_picker(app, ui, &mc, kind);
            } else {
                ui.label(
                    egui::RichText::new("加载器版本")
                        .font(ui::font(13.0))
                        .color(p.text_weak),
                );
                ui.add_space(6.0);
                let modes = ["自动选择", "手动选择"];
                let mut mode = if app.download.install_loader_auto { 0 } else { 1 };
                if ui::segmented(ui, &p, &mut mode, &modes).clicked() {
                    app.download.install_loader_auto = mode == 0;
                    if mode == 1 {
                        load_install_loader_versions(app, &mc, kind);
                    } else {
                        app.download.install_loader_versions.clear();
                        app.download.install_loader_error = None;
                    }
                }
                ui.add_space(8.0);
                if app.download.install_loader_auto {
                    let hint = match policy {
                        LoaderPolicy::Latest => "自动选择最新版本",
                        _ => "自动选择推荐的稳定版本",
                    };
                    ui.label(
                        egui::RichText::new(hint)
                            .font(ui::font(12.0))
                            .color(p.text_faint),
                    );
                } else {
                    loader_version_picker(app, ui, &mc, kind);
                }
            }
        }

        ui.add_space(14.0);
        let timing = app.cfg.download.loader_timing();
        let timing_hint = match timing {
            bundle::LoaderTiming::Parallel => "加载器会在游戏下载过程中一并安装。",
            bundle::LoaderTiming::After => "游戏下载完成后再安装加载器。",
            bundle::LoaderTiming::Manual => "当前设置不自动安装加载器，手动选择后仍会安装。",
        };
        ui.label(
            egui::RichText::new(timing_hint)
                .font(ui::font(12.0))
                .color(p.text_faint),
        );

        ui.add_space(18.0);
        ui.horizontal(|ui| {
            let enabled = kind
                .map(|k| loaders::supports(k, &mc))
                .unwrap_or(true);
            if ui::button(
                ui,
                &p,
                ButtonOpts::primary("开始安装")
                    .icon("download-arrow")
                    .enabled(enabled),
            )
            .clicked()
            {
                start = true;
            }
            if ui::button(ui, &p, ButtonOpts::secondary("取消")).clicked() {
                cancel = true;
            }
        });
    });

    if cancel {
        open = false;
    }
    if !open {
        app.download.install_target = None;
    }
    if start {
        let kind = install_loader_kind(app.download.install_loader_index);
        // 自动选择时交给启动器按策略挑选，手动时用用户选定的版本
        let version = if app.download.install_loader_auto {
            None
        } else {
            app.download
                .install_loader_versions
                .get(app.download.install_loader_version_index)
                .map(|v| v.version.clone())
        };
        app.download.install_target = None;
        start_install(app, entry, kind, version);
    }
}

/// 加载器版本下拉框
fn loader_version_picker(
    app: &mut App,
    ui: &mut Ui,
    mc: &str,
    kind: LoaderKind,
) -> Option<String> {
    if app.download.install_loader_loading {
        ui.horizontal(|ui| {
            ui::spinner(ui, &app.palette, 14.0);
            ui.label(
                egui::RichText::new("正在获取加载器版本 ...")
                    .font(ui::font(12.0))
                    .color(app.palette.text_faint),
            );
        });
        return None;
    }

    if let Some(err) = app.download.install_loader_error.clone() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(err)
                    .font(ui::font(12.0))
                    .color(app.palette.danger),
            );
            if ui::ghost(ui, &app.palette, "重试").clicked() {
                load_install_loader_versions(app, mc, kind);
            }
        });
        return None;
    }

    if app.download.install_loader_versions.is_empty() {
        load_install_loader_versions(app, mc, kind);
        ui.label(
            egui::RichText::new("正在获取加载器版本 ...")
                .font(ui::font(12.0))
                .color(app.palette.text_faint),
        );
        return None;
    }

    let labels: Vec<String> = app
        .download
        .install_loader_versions
        .iter()
        .map(|v| {
            if v.stable {
                format!("{}（推荐）", v.version)
            } else {
                v.version.clone()
            }
        })
        .collect();
    let current = labels
        .get(app.download.install_loader_version_index)
        .cloned()
        .unwrap_or_default();
    if let Some(index) = ui::dropdown(
        ui,
        &app.palette,
        "install_loader_version",
        &current,
        &labels,
        320.0,
    ) {
        app.download.install_loader_version_index = index;
    }
    app.download
        .install_loader_versions
        .get(app.download.install_loader_version_index)
        .map(|v| v.version.clone())
}

/// 后台获取加载器版本列表
pub(crate) fn load_install_loader_versions(app: &mut App, mc: &str, kind: LoaderKind) {
    if app.download.install_loader_loading {
        return;
    }
    if app.download.install_loader_mc == mc
        && !app.download.install_loader_versions.is_empty()
    {
        return;
    }
    app.download.install_loader_loading = true;
    app.download.install_loader_error = None;
    app.download.install_loader_mc = mc.to_string();
    app.download.install_loader_kind = Some(kind);
    let source = app.cfg.download.source;
    let timeout = app.cfg.download.timeout_secs;
    let mc = mc.to_string();
    app.spawn(format!("获取 {} 版本列表", kind.label()), false, move |job| {
        let result = (|| -> anyhow::Result<Vec<LoaderVersion>> {
            let dl = crate::core::download::Downloader::new(source, timeout)?;
            loaders::list_versions(&dl, kind, &mc)
        })();
        job.send(CoreEvent::InstallLoaderVersions {
            kind,
            mc: mc.clone(),
            result: result.map_err(|e| format!("{e}")),
        });
        job.done(true, "");
    });
}

/// 启动一次「游戏 + 加载器」的完整安装
fn start_install(
    app: &mut App,
    entry: VersionEntry,
    kind: Option<LoaderKind>,
    loader_version: Option<String>,
) {
    let game_dir = app.cfg.game_dir.clone();
    let java = app.java_for(&entry.id);
    let opts = app.cfg.download.install_options();
    let policy = app.cfg.download.loader_policy();
    let timing = app.cfg.download.loader_timing();
    let mc = entry.id.clone();

    app.spawn_download(format!("安装 {mc}"), move |dl, progress, job| {
        let req = bundle::InstallRequest {
            mc_version: mc.clone(),
            entry: entry.clone(),
            loader: kind,
            loader_version: loader_version.clone(),
            loader_policy: policy,
            timing,
            game_dir: game_dir.clone(),
            java: java.clone(),
            opts,
        };
        let outcome = bundle::install_bundle(&dl, &req, &progress)?;
        let target = outcome
            .loader_id
            .clone()
            .unwrap_or_else(|| outcome.game_id.clone());
        job.send(CoreEvent::Installed(target));
        Ok(match &outcome.loader_id {
            Some(loader) => format!("{mc} 与 {loader} 安装完成：{}", outcome.report.summary()),
            None => format!("{mc} 安装完成：{}", outcome.report.summary()),
        })
    });
}

// ------------------------------------------------------------------ 加载器

fn loader_tab(app: &mut App, ui: &mut Ui) {
    let p = app.palette;

    ui::card(ui, &p, |ui| {
        ui.label(
            egui::RichText::new("安装加载器")
                .font(ui::font(14.5))
                .color(p.text)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("加载器用于支持 Mod。安装时会自动补齐对应的原版版本。")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(14.0);

        // 加载器类型
        let kinds: Vec<&str> = LoaderKind::all().iter().map(|k| k.label()).collect();
        let mut kind_index = app.download.loader_kind;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("加载器")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            if ui::segmented(ui, &p, &mut kind_index, &kinds).clicked() {
                app.download.loader_kind = kind_index;
                app.download.loader_versions.clear();
                app.download.loader_version_index = 0;
                load_loader_versions(app);
            }
        });
        ui.add_space(6.0);
        let kind = LoaderKind::all()[app.download.loader_kind];
        ui.label(
            egui::RichText::new(kind.description())
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(12.0);

        // MC 版本
        let releases: Vec<String> = app
            .manifest
            .as_ref()
            .map(|m| {
                m.versions
                    .iter()
                    .filter(|v| v.is_release())
                    .map(|v| v.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("游戏版本")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if releases.is_empty() {
                    ui.label(
                        egui::RichText::new("版本列表尚未就绪")
                            .font(ui::font(12.5))
                            .color(p.text_faint),
                    );
                } else {
                    let current = if app.download.loader_mc.is_empty() {
                        releases.first().cloned().unwrap_or_default()
                    } else {
                        app.download.loader_mc.clone()
                    };
                    if let Some(index) =
                        ui::dropdown(ui, &p, "loader_mc", &current, &releases, 240.0)
                    {
                        app.download.loader_mc = releases[index].clone();
                        app.download.loader_versions.clear();
                        load_loader_versions(app);
                    }
                }
            });
        });
        ui.add_space(6.0);

        // 加载器版本
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("加载器版本")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if app.download.loader_loading {
                    ui::spinner(ui, &p, 15.0);
                }
                if let Some(err) = app.download.loader_error.clone() {
                    ui.label(
                        egui::RichText::new(err)
                            .font(ui::font(12.0))
                            .color(p.danger),
                    );
                } else if app.download.loader_versions.is_empty() {
                    ui.label(
                        egui::RichText::new("选择游戏版本后自动加载")
                            .font(ui::font(12.5))
                            .color(p.text_faint),
                    );
                } else {
                    // 第 0 项为「自动选择」，交由启动器按策略挑选
                    let mut labels: Vec<String> = vec!["自动选择（推荐）".to_string()];
                    labels.extend(app.download.loader_versions.iter().map(|v| {
                        if v.stable {
                            format!("{}（推荐）", v.version)
                        } else {
                            v.version.clone()
                        }
                    }));
                    let current = labels
                        .get(app.download.loader_version_index)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(index) =
                        ui::dropdown(ui, &p, "loader_ver", &current, &labels, 240.0)
                    {
                        app.download.loader_version_index = index;
                    }
                }
            });
        });

        ui.add_space(16.0);
        let ready = !app.download.loader_versions.is_empty();
        let kind = LoaderKind::all()[app.download.loader_kind];
        let mc = app.download.loader_mc.clone();
        // 下标 0 表示自动选择，交给启动器按策略挑选
        let loader_version = if app.download.loader_version_index == 0 {
            None
        } else {
            app.download
                .loader_versions
                .get(app.download.loader_version_index - 1)
                .map(|v| v.version.clone())
        };
        let supported = loaders::supports(kind, &mc);

        ui.horizontal(|ui| {
            let enabled = ready && supported && !mc.is_empty();
            let install_label = format!("安装 {} 加载器", kind.label());
            if ui::button(
                ui,
                &p,
                ButtonOpts::primary(&install_label)
                    .icon("download-arrow")
                    .enabled(enabled),
            )
            .clicked()
            {
                install_loader(app, kind, mc.clone(), loader_version.clone());
            }
            if !supported && !mc.is_empty() {
                ui.label(
                    egui::RichText::new(format!("{} 不支持 Minecraft {}", kind.label(), mc))
                        .font(ui::font(12.0))
                        .color(p.warning),
                );
            }
        });
    });
}

pub(crate) fn load_loader_versions(app: &mut App) {
    let kind = LoaderKind::all()[app.download.loader_kind];
    let mc = app.download.loader_mc.clone();
    if mc.is_empty() {
        return;
    }
    app.download.loader_loading = true;
    app.download.loader_error = None;
    let source = app.cfg.download.source;
    let timeout = app.cfg.download.timeout_secs;
    app.spawn(format!("获取 {} 版本列表", kind.label()), false, move |job| {
        let result = (|| -> anyhow::Result<Vec<LoaderVersion>> {
            let dl = crate::core::download::Downloader::new(source, timeout)?;
            loaders::list_versions(&dl, kind, &mc)
        })();
        job.send(CoreEvent::LoaderVersions {
            kind,
            mc: mc.clone(),
            result: result.map_err(|e| format!("{e}")),
        });
        job.done(true, "");
    });
}

/// 加载器页的安装入口：走与游戏安装同一套管线
fn install_loader(app: &mut App, kind: LoaderKind, mc: String, loader_version: Option<String>) {
    let Some(entry) = app
        .manifest
        .as_ref()
        .and_then(|m| m.versions.iter().find(|v| v.id == mc).cloned())
    else {
        app.error(format!("版本列表中找不到 {mc}，请先刷新版本列表"));
        return;
    };
    start_install(app, entry, Some(kind), loader_version);
}

// ------------------------------------------------------------------ 内容（Mod / 光影 / 资源包 / 整合包）

fn content_tab(app: &mut App, ui: &mut Ui) {
    let kind = app.download.content_kind();
    if app.download.content_detail.is_some() {
        content_detail(app, ui, kind);
        return;
    }

    let p = app.palette;
    ui.horizontal(|ui| {
        let mut query = app.download.content_query.clone();
        let response = ui::search_field(
            ui,
            &p,
            &mut query,
            &format!("搜索{}（支持中文）", kind.label()),
            300.0,
        );
        if response.changed() {
            app.download.content_query = query;
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            run_search(app);
        }
        if ui::button(ui, &p, ButtonOpts::primary("搜索").icon("search")).clicked() {
            run_search(app);
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if app.download.content_loading {
                ui::spinner(ui, &p, 15.0);
            }
            if app.download.content_total > 0 {
                ui.label(
                    egui::RichText::new(format!("共 {} 个结果", app.download.content_total))
                        .font(ui::font(12.0))
                        .color(p.text_faint),
                );
            }
        });
    });
    ui.add_space(10.0);

    // 过滤条件
    ui.horizontal(|ui| {
        let mut use_filter = app.download.use_version_filter;
        if ui::toggle(ui, &p, &mut use_filter).changed() {
            app.download.use_version_filter = use_filter;
        }
        ui.label(
            egui::RichText::new("只显示与当前版本兼容的内容")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        if let Some(version) = app.selected_version() {
            widgets::badge(ui, &p, &version.mc_version, p.accent);
            if let Some(loader) = &version.loader {
                widgets::badge(ui, &p, loader, views::loader_color(Some(loader)));
            }
        } else {
            ui.label(
                egui::RichText::new("（未选择游戏版本）")
                    .font(ui::font(12.0))
                    .color(p.text_faint),
            );
        }
    });
    ui.add_space(12.0);

    if let Some(err) = app.download.content_error.clone() {
        ui::inset(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui::icon(ui, "alert", 15.0, p.warning);
                ui.label(
                    egui::RichText::new(err)
                        .font(ui::font(12.5))
                        .color(p.text_weak),
                );
            });
        });
        ui.add_space(10.0);
    }

    if app.download.content_hits.is_empty() {
        if app.download.content_loading {
            ui::empty_state(ui, &p, "loader", "正在加载", "内容来自 Modrinth 开放平台");
        } else {
            ui::empty_state(
                ui,
                &p,
                kind.icon(),
                &format!("没有找到{}", kind.label()),
                "换个关键词试试，支持中文搜索",
            );
        }
        return;
    }

    let hits = app.download.content_hits.clone();
    let width = ui.available_width();
    let columns = if width > 1180.0 {
        3
    } else if width > 720.0 {
        2
    } else {
        1
    };
    let card_width = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;

    // 卡片高度固定，保证同一排的上下边缘完全对齐
    const CARD_HEIGHT: f32 = 150.0;
    const CARD_PADDING: f32 = 12.0;
    const ICON_SIZE: f32 = 40.0;

    let mut open_detail: Option<SearchHit> = None;
    for chunk in hits.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for hit in chunk {
                let desc_width = card_width - CARD_PADDING * 2.0;
                // MCMod 元数据：有中文名时用中文名，缩写按 PCL2 的写法放在前面
                let mcmod_entry = app.mcmod_db.lookup_modrinth(&hit.slug).cloned();
                let display_title = match &mcmod_entry {
                    Some(entry) if !entry.name.is_empty() => {
                        if entry.abbr.is_empty() {
                            entry.name.clone()
                        } else {
                            format!("[{}] {}", entry.abbr, entry.name)
                        }
                    }
                    _ => hit.title.clone(),
                };
                let mut mcmod_link: Option<String> = None;
                let response = widgets::fixed_card(
                    ui,
                    &p,
                    Vec2::new(card_width, CARD_HEIGHT),
                    CARD_PADDING,
                    |ui, inner| {
                        // 顶部：图标 + 标题 + 作者
                        ui.horizontal_top(|ui| {
                            let icon_rect =
                                Rect::from_min_size(ui.cursor().min, Vec2::splat(ICON_SIZE));
                            ui.allocate_space(Vec2::splat(ICON_SIZE));
                            if let Some(url) = &hit.icon_url {
                                app.ensure_image(url);
                                if let Some(texture) = app.image(url) {
                                    ui.painter().image(
                                        texture.id(),
                                        icon_rect,
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                } else {
                                    ui.painter().rect_filled(
                                        icon_rect,
                                        p.corner_sm(),
                                        p.surface_alt,
                                    );
                                    icons::paint(
                                        ui.ctx(),
                                        ui.painter(),
                                        kind.icon(),
                                        icon_rect.shrink(10.0),
                                        p.text_faint,
                                    );
                                }
                            } else {
                                icons::paint(
                                    ui.ctx(),
                                    ui.painter(),
                                    kind.icon(),
                                    icon_rect.shrink(9.0),
                                    p.text_faint,
                                );
                            }
                            ui.vertical(|ui| {
                                // 标题固定单行，超长由 egui 省略
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&display_title)
                                            .font(ui::font(14.0))
                                            .color(p.text)
                                            .strong(),
                                    )
                                    .truncate(),
                                );
                                ui.add_space(2.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "{} · 下载 {}",
                                            if hit.author.is_empty() {
                                                "未知作者"
                                            } else {
                                                &hit.author
                                            },
                                            modrinth::format_downloads(hit.downloads)
                                        ))
                                        .font(ui::font(11.5))
                                        .color(p.text_faint),
                                    )
                                    .truncate(),
                                );
                            });
                        });
                        ui.add_space(8.0);
                        // 描述：按两行容量截断原文，再交给 egui 排版，超出部分裁掉
                        let budget = ((desc_width / (12.5 * 0.62)) as usize).max(12) * 2;
                        let mut desc: String = hit.description.chars().take(budget).collect();
                        if hit.description.chars().count() > budget {
                            desc.push('…');
                        }
                        let desc_rect = Rect::from_min_size(
                            ui.cursor().min,
                            Vec2::new(desc_width, 34.0),
                        );
                        ui.allocate_space(Vec2::new(desc_width, 34.0));
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(desc_rect)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                            |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(desc)
                                            .font(ui::font(12.5))
                                            .color(p.text_weak),
                                    )
                                    .wrap(),
                                );
                            },
                        );
                        ui.add_space(8.0);
                        // 分类标签
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            for category in hit.categories.iter().take(3) {
                                widgets::badge(
                                    ui,
                                    &p,
                                    &modrinth::category_label(category),
                                    p.text_faint,
                                );
                            }
                            // MCMod 链接，可直接打开 MC 百科页面
                            if let Some(entry) = &mcmod_entry {
                                let badge = widgets::badge(ui, &p, "MCMod", p.accent);
                                if badge.clicked() {
                                    mcmod_link = Some(entry.url());
                                }
                                if badge.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                            }
                        });
                        let _ = inner;
                    },
                );
                let clickable = response.interact(Sense::click());
                if clickable.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if clickable.clicked() {
                    open_detail = Some(hit.clone());
                }
                // MCMod 链接优先于打开详情
                if let Some(url) = mcmod_link {
                    let _ = open::that(url);
                    open_detail = None;
                }
            }
        });
        ui.add_space(12.0);
    }

    if let Some(hit) = open_detail {
        open_project(app, &hit.project_id, &hit.author);
    }
}

pub(crate) fn run_search(app: &mut App) {
    let kind = app.download.content_kind();
    let query = app.download.content_query.clone();
    let (mc, loader) = if app.download.use_version_filter {
        match app.selected_version() {
            Some(v) => (Some(v.mc_version.clone()), v.loader.clone()),
            None => (None, None),
        }
    } else {
        (None, None)
    };
    let loader = loader.map(|l| l.to_lowercase());

    app.download.content_seq += 1;
    let seq = app.download.content_seq;
    app.download.content_loading = true;
    app.download.content_error = None;
    log::info!(
        "开始搜索 {}（标签 {}，关键词 {query:?}）",
        kind.label(),
        app.download.tab
    );

    let source = app.cfg.download.source;
    let timeout = app.cfg.download.timeout_secs;
    // 中文关键词：先用 MCMod 中文名库反查英文名，再补一次英文搜索
    let supplement = if mcmod::has_chinese(&query) {
        app.mcmod_db.english_hint(&query)
    } else {
        None
    };
    app.spawn(format!("搜索{}", kind.label()), false, move |job| {
        let result = (|| -> anyhow::Result<SearchResponse> {
            let dl = crate::core::download::Downloader::new(source, timeout)?;
            let mut response = modrinth::search(
                &dl,
                kind,
                &query,
                mc.as_deref(),
                loader.as_deref(),
                0,
                24,
            )?;
            // MCMod 反查出的英文名搜索结果排在前面，两边结果按项目去重合并
            if let Some(keyword) = &supplement {
                if !keyword.eq_ignore_ascii_case(&query) {
                    if let Ok(extra) = modrinth::search(
                        &dl,
                        kind,
                        keyword,
                        mc.as_deref(),
                        loader.as_deref(),
                        0,
                        12,
                    ) {
                        let mut merged: Vec<SearchHit> = Vec::new();
                        let mut seen: std::collections::HashSet<String> =
                            std::collections::HashSet::new();
                        for hit in extra.hits.into_iter().chain(response.hits) {
                            if seen.insert(hit.project_id.clone()) {
                                merged.push(hit);
                            }
                        }
                        let total = merged.len() as u32;
                        response.hits = merged;
                        response.total_hits = response.total_hits.max(total);
                    }
                }
            }
            Ok(response)
        })();
        if let Err(err) = &result {
            log::warn!("搜索失败: {err}");
        }
        job.send(CoreEvent::Search {
            seq,
            result: result.map(Box::new).map_err(|e| format!("{e}")),
        });
        job.done(true, "");
    });
}

pub(crate) fn open_project(app: &mut App, project_id: &str, author: &str) {
    app.download.content_versions.clear();
    app.download.content_versions_loading = true;
    app.download.content_detail = None;
    app.download.content_translation = None;
    app.download.content_translation_loading = true;
    app.download.content_author = author.to_string();
    // 安装目标默认当前实例
    app.download.content_install_instance = app.selected_version_id();
    app.download.content_loader_filter.clear();
    app.download.content_game_filter.clear();
    app.download.content_version_index = 0;

    let id = project_id.to_string();
    let source = app.cfg.download.source;
    let timeout = app.cfg.download.timeout_secs;
    let (mc, loader) = if app.download.use_version_filter {
        match app.selected_version() {
            Some(v) => (Some(v.mc_version.clone()), v.loader.as_ref().map(|l| l.to_lowercase())),
            None => (None, None),
        }
    } else {
        (None, None)
    };
    let _ = (mc, loader);

    // 顺带请求中文简介：翻译服务不可用时静默回退到原文
    let id_for_translate = id.clone();
    let translate_source = source;
    let translate_timeout = timeout;
    app.spawn("获取中文简介", false, move |job| {
        let result = (|| -> anyhow::Result<String> {
            let dl = crate::core::download::Downloader::new(translate_source, translate_timeout)?;
            let mut cache = translate::Cache::load();
            let text = translate::translate_cached(&dl, &id_for_translate, &mut cache)
                .ok_or_else(|| anyhow::anyhow!("没有可用的中文翻译"))?;
            cache.save();
            Ok(text)
        })();
        if let Ok(text) = result {
            job.send(CoreEvent::ContentTranslated {
                project_id: id_for_translate,
                text,
            });
        } else {
            log::debug!("中文翻译不可用，使用原文");
        }
        job.done(true, "");
    });

    app.spawn("获取项目详情", false, move |job| {
        let result = (|| -> anyhow::Result<(Project, Vec<Version>)> {
            let dl = crate::core::download::Downloader::new(source, timeout)?;
            let project = modrinth::project(&dl, &id)?;
            // 拉全部版本，由界面按所选实例过滤
            let versions = modrinth::versions(&dl, &id, None, None).unwrap_or_default();
            Ok((project, versions))
        })();
        match result {
            Ok((project, versions)) => {
                job.send(CoreEvent::ProjectDetail {
                    project: Box::new(project),
                    versions,
                });
            }
            Err(err) => job.error(format!("获取项目详情失败：{err}")),
        }
        job.done(true, "");
    });
}

fn content_detail(app: &mut App, ui: &mut Ui, kind: ContentKind) {
    let p = app.palette;
    let Some(project) = app.download.content_detail.clone() else {
        return;
    };
    // MCMod（MC 百科）元数据：有中文名与 MCMod 页面链接
    let mcmod_entry = app.mcmod_db.lookup_modrinth(&project.slug).cloned();

    // ---------- 顶部：返回 ----------
    ui.horizontal(|ui| {
        if ui::button(ui, &p, ButtonOpts::ghost("返回列表").icon("arrow-left")).clicked() {
            app.download.content_detail = None;
            app.download.content_versions.clear();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                egui::RichText::new(match kind {
                    ContentKind::Mod => "模组",
                    ContentKind::Shader => "光影",
                    ContentKind::ResourcePack => "资源包",
                    ContentKind::Modpack => "整合包",
                    ContentKind::World => "世界",
                })
                .font(ui::font(12.0))
                .color(p.text_faint),
            );
        });
    });
    ui.add_space(10.0);

    // ---------- 项目信息 ----------
    ui::card(ui, &p, |ui| {
        ui.horizontal_top(|ui| {
            let icon_rect = Rect::from_min_size(ui.cursor().min, Vec2::splat(72.0));
            ui.allocate_space(Vec2::new(72.0, 72.0));
            if let Some(url) = &project.icon_url {
                app.ensure_image(url);
                if let Some(texture) = app.image(url) {
                    ui.painter().image(
                        texture.id(),
                        icon_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    ui.painter()
                        .rect_filled(icon_rect, p.corner_sm(), p.surface_alt);
                }
            } else {
                icons::paint(
                    ui.ctx(),
                    ui.painter(),
                    kind.icon(),
                    icon_rect.shrink(16.0),
                    p.text_faint,
                );
            }
            ui.add_space(10.0);
            ui.vertical(|ui| {
                // 标题优先用 MCMod 上的中文名
                let display_title = match &mcmod_entry {
                    Some(entry) if !entry.name.is_empty() => {
                        if entry.abbr.is_empty() {
                            entry.name.clone()
                        } else {
                            format!("[{}] {}", entry.abbr, entry.name)
                        }
                    }
                    _ => project.title.clone(),
                };
                ui.label(
                    egui::RichText::new(&display_title)
                        .font(ui::font(19.0))
                        .color(p.text)
                        .strong(),
                );

                // 副标题：英文名 · 作者 · 更新时间
                let mut subtitle: Vec<String> = Vec::new();
                if let Some(entry) = &mcmod_entry {
                    if !entry.subname.is_empty() && entry.subname != display_title {
                        subtitle.push(entry.subname.clone());
                    }
                } else if !project.title.is_empty() && project.title != display_title {
                    subtitle.push(project.title.clone());
                }
                let author = app.download.content_author.clone();
                if !author.is_empty() {
                    subtitle.push(format!("作者 {author}"));
                }
                if !project.updated.is_empty() {
                    subtitle.push(format!("更新于 {}", short_date(&project.updated)));
                }
                if !subtitle.is_empty() {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(subtitle.join(" · "))
                            .font(ui::font(12.5))
                            .color(p.text_faint),
                    );
                }

                ui.add_space(6.0);
                // 简介：中文翻译优先
                let translated = app.download.content_translation.clone();
                if let Some(text) = &translated {
                    ui.horizontal_top(|ui| {
                        ui.label(
                            egui::RichText::new(text)
                                .font(ui::font(13.0))
                                .color(p.text_weak),
                        );
                        ui.label(
                            egui::RichText::new("中文")
                                .font(ui::font(10.5))
                                .color(p.accent),
                        );
                    });
                } else {
                    ui.label(
                        egui::RichText::new(&project.description)
                            .font(ui::font(13.0))
                            .color(p.text_weak),
                    );
                    if app.download.content_translation_loading {
                        ui.horizontal(|ui| {
                            ui::spinner(ui, &p, 12.0);
                            ui.label(
                                egui::RichText::new("正在获取中文简介 ...")
                                    .font(ui::font(11.5))
                                    .color(p.text_faint),
                            );
                        });
                    }
                }

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    widgets::badge(
                        ui,
                        &p,
                        &format!("下载 {}", modrinth::format_downloads(project.downloads)),
                        p.accent,
                    );
                    widgets::badge(
                        ui,
                        &p,
                        &format!("关注 {}", modrinth::format_downloads(project.followers)),
                        p.text_weak,
                    );
                    if !project.license.name.is_empty() {
                        widgets::badge(ui, &p, &project.license.name, p.text_faint);
                    }
                    for category in project.categories.iter().take(4) {
                        widgets::badge(
                            ui,
                            &p,
                            &modrinth::category_label(category),
                            p.text_faint,
                        );
                    }
                });

                // ---------- 平台链接 ----------
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let modrinth_url = format!(
                        "https://modrinth.com/{}/{}",
                        project.project_type, project.slug
                    );
                    if ui::button(ui, &p, ButtonOpts::secondary("Modrinth 页面").icon("external-link"))
                        .clicked()
                    {
                        let _ = open::that(&modrinth_url);
                    }
                    if let Some(entry) = &mcmod_entry {
                        if ui::button(ui, &p, ButtonOpts::secondary("MCMod 页面").icon("external-link"))
                            .clicked()
                        {
                            let _ = open::that(entry.url());
                        }
                    }
                    for (label, url) in [
                        ("源码", &project.source_url),
                        ("问题反馈", &project.issues_url),
                        ("Wiki", &project.wiki_url),
                    ] {
                        if let Some(url) = url {
                            if ui::button(ui, &p, ButtonOpts::secondary(label).icon("external-link"))
                                .clicked()
                            {
                                let _ = open::that(url);
                            }
                        }
                    }
                });
            });
        });
    });
    ui.add_space(12.0);

    // ---------- 安装 ----------
    install_card(app, ui, &p, &project, kind);

    if !project.body.is_empty() {
        ui.add_space(12.0);
        ui::card(ui, &p, |ui| {
            ui.label(
                egui::RichText::new("项目介绍")
                    .font(ui::font(14.5))
                    .color(p.text)
                    .strong(),
            );
            ui.add_space(8.0);
            let text = strip_markdown(&project.body);
            ui.label(
                egui::RichText::new(text)
                    .font(ui::font(12.5))
                    .color(p.text_weak),
            );
        });
    }
}

/// 安装区：选择实例、筛选兼容版本并安装
fn install_card(app: &mut App, ui: &mut Ui, p: &Palette, project: &Project, kind: ContentKind) {
    // ---- 实例列表 ----
    let instances: Vec<(String, Option<String>, String)> = app
        .local_versions
        .iter()
        .map(|v| {
            let loader = v.loader.clone().unwrap_or_else(|| "原版".to_string());
            (v.id.clone(), v.loader.clone(), loader)
        })
        .collect();

    // 当前选中的实例
    let mut instance_id = app
        .download
        .content_install_instance
        .clone()
        .or_else(|| app.selected_version_id());
    if let Some(id) = &instance_id {
        if !instances.iter().any(|(vid, _, _)| vid == id) {
            instance_id = None;
        }
    }
    let instance = instance_id.as_ref().and_then(|id| {
        app.local_versions
            .iter()
            .find(|v| &v.id == id)
            .map(|v| (v.id.clone(), v.mc_version.clone(), v.loader.clone()))
    });

    // 实例自带的加载器与 MC 版本，作为筛选的默认值
    let instance_loader = instance
        .as_ref()
        .and_then(|(_, _, l)| l.as_ref())
        .map(|l| l.to_lowercase());
    let instance_mc = instance.as_ref().map(|(_, mc, _)| mc.clone());

    // ---- 筛选条件 ----
    let mut loader_filter = app.download.content_loader_filter.clone();
    let mut game_filter = app.download.content_game_filter.clone();

    // 实际生效的筛选值
    let effective_loader = if loader_filter.is_empty() {
        instance_loader.clone()
    } else if loader_filter == "*" {
        None
    } else {
        Some(loader_filter.clone())
    };
    let effective_game = if game_filter.is_empty() {
        instance_mc.clone()
    } else if game_filter == "*" {
        None
    } else {
        Some(game_filter.clone())
    };

    // 按筛选条件过滤版本
    let all_versions = app.download.content_versions.clone();
    let filtered: Vec<Version> = all_versions
        .iter()
        .filter(|v| {
            if let Some(loader) = &effective_loader {
                if !v.loaders.is_empty()
                    && !v.loaders.iter().any(|l| l.eq_ignore_ascii_case(loader))
                {
                    return false;
                }
            }
            if let Some(mc) = &effective_game {
                if !v.game_versions.is_empty()
                    && !v.game_versions.iter().any(|g| g == mc)
                {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    // 可选的游戏版本：项目版本列表里出现过的
    let mut game_options: Vec<String> = Vec::new();
    for v in &all_versions {
        for g in &v.game_versions {
            if !game_options.contains(g) {
                game_options.push(g.clone());
            }
        }
    }
    game_options.sort_by(|a, b| version_key(b).cmp(&version_key(a)));

    // ---- 卡片 ----
    ui::card(ui, p, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("安装")
                    .font(ui::font(14.5))
                    .color(p.text)
                    .strong(),
            );
            if app.download.content_versions_loading {
                ui::spinner(ui, p, 14.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} 个可用版本", filtered.len()))
                        .font(ui::font(12.0))
                        .color(p.text_faint),
                );
            });
        });
        ui.add_space(12.0);

        // ---- 安装到哪个实例 ----
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("安装到实例")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if instances.is_empty() {
                    ui.label(
                        egui::RichText::new("还没有已安装的版本")
                            .font(ui::font(12.5))
                            .color(p.warning),
                    );
                } else {
                    let labels: Vec<String> = instances
                        .iter()
                        .map(|(id, _, loader)| format!("{id} · {loader}"))
                        .collect();
                    let current = instance_id
                        .as_ref()
                        .and_then(|id| {
                            instances
                                .iter()
                                .position(|(vid, _, _)| vid == id)
                        })
                        .map(|i| labels[i].clone())
                        .unwrap_or_else(|| "选择实例".to_string());
                    if let Some(index) =
                        ui::dropdown(ui, p, "install_instance", &current, &labels, 300.0)
                    {
                        app.download.content_install_instance = Some(instances[index].0.clone());
                        // 换实例后筛选回到「跟随实例」
                        app.download.content_loader_filter.clear();
                        app.download.content_game_filter.clear();
                        app.download.content_version_index = 0;
                    }
                }
            });
        });
        ui.add_space(6.0);

        // ---- 安装位置 ----
        if let Some(id) = &instance_id {
            let dir = app.cfg.effective_game_dir(id);
            let target = match kind {
                ContentKind::Mod => dir.join("mods"),
                ContentKind::Shader => dir.join("shaderpacks"),
                ContentKind::ResourcePack => dir.join("resourcepacks"),
                _ => dir,
            };
            let path_text = target.display().to_string();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("安装位置")
                        .font(ui::font(13.0))
                        .color(p.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // 路径可能很长，按可用宽度省略
                    let width = ((ui.available_width() - 60.0).max(120.0) / 7.0) as usize;
                    let short = ellipsize_path(&path_text, width);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(short)
                                .font(ui::font(12.0))
                                .color(p.text_faint),
                        )
                        .truncate(),
                    );
                    // 完整路径放到悬浮提示里
                    if ui
                        .add(egui::Label::new(
                            egui::RichText::new("完整路径")
                                .font(ui::font(12.0))
                                .color(p.text_faint),
                        ))
                        .on_hover_text(&path_text)
                        .clicked()
                    {
                        let _ = crate::core::paths::reveal(&target);
                    }
                });
            });
            ui.add_space(10.0);
        }

        // ---- 兼容筛选 ----
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("兼容筛选")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // 游戏版本
                let mut game_labels: Vec<String> = vec![match instance_mc.clone() {
                    Some(mc) => format!("跟随实例（{mc}）"),
                    None => "跟随实例".to_string(),
                }];
                game_labels.push("全部".to_string());
                for g in game_options.iter().take(18) {
                    game_labels.push(g.clone());
                }
                let game_current = if game_filter.is_empty() {
                    game_labels[0].clone()
                } else if game_filter == "*" {
                    "全部".to_string()
                } else {
                    game_filter.clone()
                };
                if let Some(index) = ui::dropdown(
                    ui,
                    p,
                    "install_game_filter",
                    &game_current,
                    &game_labels,
                    190.0,
                ) {
                    game_filter = if index == 0 {
                        String::new()
                    } else if index == 1 {
                        "*".to_string()
                    } else {
                        game_labels[index].clone()
                    };
                    app.download.content_game_filter = game_filter.clone();
                    app.download.content_version_index = 0;
                }

                // 加载器
                let mut loader_labels: Vec<String> = vec![match instance_loader.clone() {
                    Some(l) => format!("跟随实例（{l}）"),
                    None => "跟随实例".to_string(),
                }];
                loader_labels.push("全部".to_string());
                for l in ["fabric", "forge", "quilt", "neoforge", "rift", "liteloader"] {
                    loader_labels.push(l.to_string());
                }
                let loader_current = if loader_filter.is_empty() {
                    loader_labels[0].clone()
                } else if loader_filter == "*" {
                    "全部".to_string()
                } else {
                    loader_filter.clone()
                };
                if let Some(index) = ui::dropdown(
                    ui,
                    p,
                    "install_loader_filter",
                    &loader_current,
                    &loader_labels,
                    190.0,
                ) {
                    loader_filter = if index == 0 {
                        String::new()
                    } else if index == 1 {
                        "*".to_string()
                    } else {
                        loader_labels[index].clone()
                    };
                    app.download.content_loader_filter = loader_filter.clone();
                    app.download.content_version_index = 0;
                }
            });
        });
        ui.add_space(12.0);

        if app.download.content_versions_loading {
            ui.label(
                egui::RichText::new("正在获取版本列表 ...")
                    .font(ui::font(12.5))
                    .color(p.text_faint),
            );
            return;
        }

        if filtered.is_empty() {
            ui.label(
                egui::RichText::new(if all_versions.is_empty() {
                    "该项目没有可安装的版本"
                } else {
                    "当前筛选条件下没有可用版本，可以把兼容筛选调成「全部」"
                })
                .font(ui::font(12.5))
                .color(p.text_weak),
            );
            return;
        }

        // ---- 版本选择 ----
        let labels: Vec<String> = filtered
            .iter()
            .map(|v| {
                let games = if v.game_versions.is_empty() {
                    "通用".to_string()
                } else {
                    v.game_versions
                        .iter()
                        .take(3)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let date = short_date(&v.date_published);
                if date.is_empty() {
                    format!("{} · {} · {}", v.version_number, v.loader_label(), games)
                } else {
                    format!(
                        "{} · {} · {} · {}",
                        v.version_number,
                        v.loader_label(),
                        games,
                        date
                    )
                }
            })
            .collect();

        let mut index = app.download.content_version_index.min(labels.len() - 1);
        let current = labels[index].clone();
        if let Some(selected) = ui::dropdown(ui, p, "content_version", &current, &labels, 520.0) {
            index = selected;
            app.download.content_version_index = selected;
        }
        ui.add_space(12.0);

        // ---- 安装按钮 ----
        let version = filtered[index].clone();
        ui.horizontal(|ui| {
            let enabled = instance_id.is_some();
            let label = match &instance_id {
                Some(id) => format!("下载并安装到 {id}"),
                None => "下载并安装".to_string(),
            };
            if ui::button(
                ui,
                p,
                ButtonOpts::primary(&label).icon("download-arrow").enabled(enabled),
            )
            .clicked()
            {
                if let Some(id) = instance_id.clone() {
                    install_content(app, kind, version.clone(), project.title.clone(), id);
                }
            }
            if !enabled {
                ui.label(
                    egui::RichText::new("请先选择一个已安装的实例")
                        .font(ui::font(12.0))
                        .color(p.warning),
                );
            }
            if let Some(file) = version.primary_file() {
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {}",
                        file.filename,
                        crate::core::paths::format_size(file.size)
                    ))
                    .font(ui::font(11.5))
                    .color(p.text_faint),
                );
            }
        });
    });
}

/// 版本号排序键：让 1.20.1 排在 1.9 前面
fn version_key(version: &str) -> Vec<u32> {
    version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap_or(0))
        .collect()
}

/// 路径过长时保留开头与结尾，中间用省略号
fn ellipsize_path(path: &str, max_chars: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= max_chars {
        return path.to_string();
    }
    let keep = max_chars.saturating_sub(3) / 2;
    if keep == 0 {
        return "…".to_string();
    }
    let head: String = chars[..keep].iter().collect();
    let tail: String = chars[chars.len() - keep..].iter().collect();
    format!("{head}…{tail}")
}

/// 安装内容到指定实例
fn install_content(
    app: &mut App,
    kind: ContentKind,
    version: Version,
    title: String,
    instance_id: String,
) {
    let game_dir = app.cfg.effective_game_dir(&instance_id);
    let auto_deps = app.cfg.download.auto_install_dependencies;
    // 依赖筛选按目标实例的 MC 版本与加载器来算
    let (mc, loader) = app
        .local_versions
        .iter()
        .find(|v| v.id == instance_id)
        .map(|v| (v.mc_version.clone(), v.loader.as_ref().map(|l| l.to_lowercase())))
        .unwrap_or_default();

    app.spawn_download(format!("安装 {title}"), move |dl, progress, _job| {
        crate::core::paths::ensure_dir(&game_dir)?;
        if kind == ContentKind::Modpack {
            modrinth::install_modpack(&dl, &version, &game_dir, &progress)?;
            return Ok(format!("{title} 整合包安装完成"));
        }
        let path = modrinth::install(&dl, kind, &version, &game_dir, &progress)?;
        if auto_deps && kind == ContentKind::Mod {
            let deps = modrinth::required_dependencies(
                &dl,
                &version,
                Some(mc.as_str()),
                loader.as_deref(),
            );
            for dep in deps {
                progress.set_message(format!("正在安装前置依赖 {} ...", dep.version_number));
                let _ = modrinth::install(&dl, kind, &dep, &game_dir, &progress);
            }
        }
        Ok(format!("已安装到 {}", path.display()))
    });
}

/// 把 Markdown 简介转成可读纯文本
///
/// 处理链接、图片、粗体、斜体、行内代码、标题、引用与 HTML 标签，
/// 避免把原始语法直接暴露给用户。
fn strip_markdown(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let mut line = line.trim().to_string();
        // 去掉 HTML 标签（<center>、<br>、<img ...> 等）
        line = remove_html_tags(&line);
        // 图片：![说明](地址) → 说明
        line = replace_markdown(&line, "![");
        // 链接：[文字](地址) → 文字
        line = replace_markdown(&line, "[");
        // 粗体 / 斜体
        line = line.replace("***", "").replace("**", "");
        line = line.replace("__", "").replace('*', "");
        // 行内代码
        line = line.replace('`', "");
        // 标题、引用、列表符号
        let cleaned = line
            .trim_start_matches('#')
            .trim_start_matches('>')
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .trim_start_matches("+ ")
            .trim();
        // 分割线
        if cleaned.is_empty() || cleaned.chars().all(|c| c == '-' || c == '=') {
            out.push('\n');
            continue;
        }
        out.push_str(cleaned);
        out.push('\n');
        if out.chars().count() > 2000 {
            out.push_str("…");
            break;
        }
    }
    out.trim().to_string()
}

/// 去掉 HTML 标签
fn remove_html_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for ch in text.chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    out
}

/// 处理 `[文字](地址)` / `![说明](地址)`，只保留文字部分
fn replace_markdown(text: &str, open: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len()..];
        let Some(close) = after.find(']') else {
            break;
        };
        // 跳过 ]( 之间的地址
        let tail = &after[close + 1..];
        let label = &after[..close];
        out.push_str(&rest[..start]);
        out.push_str(label);
        if let Some(paren) = tail.find(')') {
            rest = &tail[paren + 1..];
        } else {
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

fn short_date(date: &str) -> String {
    let trimmed = date.trim();
    if trimmed.len() >= 10 {
        trimmed[..10].to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_links_and_images_become_plain_text() {
        let text = strip_markdown("[捐赠](https://example.com/donate)");
        assert_eq!(text, "捐赠");
        assert!(!text.contains("https://"));
    }

    #[test]
    fn markdown_html_tags_are_removed() {
        let text = strip_markdown("<center>hello</center>");
        assert_eq!(text, "hello");
        assert!(!text.contains('<'));
    }

    #[test]
    fn markdown_emphasis_is_stripped() {
        let text = strip_markdown("**粗体** 与 *斜体*");
        assert_eq!(text, "粗体 与 斜体");
    }

    #[test]
    fn markdown_headings_and_lists_are_cleaned() {
        let text = strip_markdown("## 标题\n- 条目一\n- 条目二");
        assert!(text.contains("标题"));
        assert!(text.contains("条目一"));
        assert!(!text.contains('#'));
    }

    #[test]
    fn version_key_orders_numerically() {
        // 升序：1.9 < 1.10 < 1.20.1
        let mut list = vec!["1.9", "1.20.1", "1.10"];
        list.sort_by_key(|v| version_key(v));
        assert_eq!(list, vec!["1.9", "1.10", "1.20.1"]);

        // 界面里用的是降序，最新的排在最前
        let mut desc = vec!["1.9", "1.20.1", "1.10"];
        desc.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
        assert_eq!(desc, vec!["1.20.1", "1.10", "1.9"]);
    }

    #[test]
    fn long_path_is_shortened() {
        let path = "C:\\Users\\someone\\a-very-long-path\\.minecraft\\versions\\fabric\\mods";
        let short = ellipsize_path(path, 20);
        assert!(short.len() < path.len());
        assert!(short.starts_with("C:\\"));
        assert!(short.ends_with("mods"));
        // 短路径原样返回
        assert_eq!(ellipsize_path("C:\\a", 20), "C:\\a");
    }
}
