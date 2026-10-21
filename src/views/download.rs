use egui::{Align, Color32, Layout, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::{App, CoreEvent};
use crate::core::content::modrinth::{self, ContentKind, Project, SearchHit, SearchResponse, Version};
use crate::core::minecraft::loaders::{self, LoaderKind, LoaderVersion};
use crate::core::minecraft::manifest::VersionEntry;
use crate::icons;
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

    pub content_kind: usize,
    pub content_query: String,
    pub content_hits: Vec<SearchHit>,
    pub content_total: u32,
    pub content_loading: bool,
    pub content_error: Option<String>,
    pub content_seq: u64,
    pub content_detail: Option<Box<Project>>,
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
            content_kind: 0,
            content_query: String::new(),
            content_hits: Vec::new(),
            content_total: 0,
            content_loading: false,
            content_error: None,
            content_seq: 0,
            content_detail: None,
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

    pub fn content_kind(&self) -> ContentKind {
        ContentKind::all()[self.content_kind.min(4)]
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
            app.download.tab = tab;
        }
    });
    ui.add_space(14.0);

    match app.download.tab {
        0 => game_tab(app, ui),
        1 => loader_tab(app, ui),
        _ => content_tab(app, ui),
    }
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
        install_game(app, entry);
    }
}

fn install_game(app: &mut App, entry: VersionEntry) {
    let game_dir = app.cfg.game_dir.clone();
    let id = entry.id.clone();
    app.spawn_download(format!("安装 {id}"), move |dl, progress, job| {
        progress.set_message("正在获取版本信息 ...");
        crate::core::minecraft::install::ensure_version_json(&dl, &game_dir, &entry)?;
        let report = crate::core::minecraft::install::install_version(&dl, &game_dir, &id, &progress)?;
        job.send(CoreEvent::Installed(id.clone()));
        Ok(format!("{id} {}", report.summary()))
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
                    let labels: Vec<String> = app
                        .download
                        .loader_versions
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
        let loader_version = app
            .download
            .loader_versions
            .get(app.download.loader_version_index)
            .map(|v| v.version.clone());
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
                if let Some(loader_version) = loader_version.clone() {
                    install_loader(app, kind, mc.clone(), loader_version);
                }
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

fn install_loader(app: &mut App, kind: LoaderKind, mc: String, loader_version: String) {
    let game_dir = app.cfg.game_dir.clone();
    let java = app.java_for(&mc);
    let Some(java) = java else {
        app.error("没有找到可用的 Java，安装器需要 Java 才能运行");
        return;
    };
    let manifest_entry = app
        .manifest
        .as_ref()
        .and_then(|m| m.versions.iter().find(|v| v.id == mc).cloned());
    let installed = app.cfg.is_installed(&mc);

    app.spawn_download(
        format!("安装 {} {} ({})", kind.label(), loader_version, mc),
        move |dl, progress, job| {
            if !installed {
                let entry = manifest_entry
                    .ok_or_else(|| anyhow::anyhow!("版本列表中找不到 {mc}"))?;
                progress.set_message(format!("正在安装原版 {mc} ..."));
                crate::core::minecraft::install::ensure_version_json(&dl, &game_dir, &entry)?;
                crate::core::minecraft::install::install_version(&dl, &game_dir, &mc, &progress)?;
            }
            let id = loaders::install(
                &dl,
                kind,
                &mc,
                &loader_version,
                &game_dir,
                &java,
                &progress,
            )?;
            job.send(CoreEvent::Installed(id.clone()));
            Ok(format!("{} 安装完成", id))
        },
    );
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
            &format!("搜索 {}（来自 Modrinth）", kind.label()),
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
            ui::empty_state(ui, &p, "loader", "正在搜索", "内容来自 Modrinth 开放平台");
        } else {
            ui::empty_state(
                ui,
                &p,
                kind.icon(),
                &format!("搜索{}", kind.label()),
                "输入关键词后回车，或直接点搜索浏览热门内容",
            );
        }
        return;
    }

    let hits = app.download.content_hits.clone();
    let width = ui.available_width();
    let columns = if width > 1180.0 { 3 } else { 2 };
    let card_width = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;

    let mut open_detail: Option<SearchHit> = None;
    for chunk in hits.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for hit in chunk {
                ui.vertical(|ui| {
                    ui.set_width(card_width);
                    let response = ui::card_padded(ui, &p, 14, |ui| {
                        ui.horizontal_top(|ui| {
                            let icon_rect = Rect::from_min_size(ui.cursor().min, Vec2::splat(44.0));
                            ui.allocate_space(Vec2::new(44.0, 44.0));
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
                                        icon_rect.shrink(12.0),
                                        p.text_faint,
                                    );
                                }
                            } else {
                                icons::paint(
                                    ui.ctx(),
                                    ui.painter(),
                                    kind.icon(),
                                    icon_rect.shrink(10.0),
                                    p.text_faint,
                                );
                            }
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(&hit.title)
                                        .font(ui::font(14.0))
                                        .color(p.text)
                                        .strong(),
                                );
                                ui.add_space(2.0);
                                ui.label(
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
                                );
                            });
                        });
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(&hit.description)
                                .font(ui::font(12.5))
                                .color(p.text_weak),
                        );
                        ui.add_space(8.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            for category in hit.categories.iter().take(3) {
                                widgets::badge(ui, &p, category, p.text_faint);
                            }
                        });
                    });
                    let clickable = response.interact(Sense::click());
                    if clickable.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if clickable.clicked() {
                        open_detail = Some(hit.clone());
                    }
                });
            }
        });
        ui.add_space(12.0);
    }

    if let Some(hit) = open_detail {
        open_project(app, &hit.project_id);
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
    log::info!("开始搜索 {}（关键词 {query:?}）", kind.label());

    let source = app.cfg.download.source;
    let timeout = app.cfg.download.timeout_secs;
    app.spawn(format!("搜索{}", kind.label()), false, move |job| {
        let result = (|| -> anyhow::Result<SearchResponse> {
            let dl = crate::core::download::Downloader::new(source, timeout)?;
            modrinth::search(
                &dl,
                kind,
                &query,
                mc.as_deref(),
                loader.as_deref(),
                0,
                24,
            )
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

fn open_project(app: &mut App, project_id: &str) {
    app.download.content_versions.clear();
    app.download.content_versions_loading = true;
    app.download.content_detail = None;

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

    app.spawn("获取项目详情", false, move |job| {
        let result = (|| -> anyhow::Result<(Project, Vec<Version>)> {
            let dl = crate::core::download::Downloader::new(source, timeout)?;
            let project = modrinth::project(&dl, &id)?;
            let versions =
                modrinth::versions(&dl, &id, mc.as_deref(), loader.as_deref()).unwrap_or_default();
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

    ui.horizontal(|ui| {
        if ui::button(ui, &p, ButtonOpts::ghost("返回列表").icon("arrow-left")).clicked() {
            app.download.content_detail = None;
            app.download.content_versions.clear();
        }
        if !project.source_url.is_none() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(url) = &project.source_url {
                    if ui::ghost(ui, &p, "查看源码").clicked() {
                        let _ = open::that(url);
                    }
                }
            });
        }
    });
    ui.add_space(10.0);

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
                }
            } else {
                icons::paint(ui.ctx(), ui.painter(), kind.icon(), icon_rect.shrink(16.0), p.text_faint);
            }
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(&project.title)
                        .font(ui::font(19.0))
                        .color(p.text)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(&project.description)
                        .font(ui::font(13.0))
                        .color(p.text_weak),
                );
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
                        widgets::badge(ui, &p, category, p.text_faint);
                    }
                });
            });
        });
    });
    ui.add_space(12.0);

    ui::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("选择版本")
                    .font(ui::font(14.5))
                    .color(p.text)
                    .strong(),
            );
            if app.download.content_versions_loading {
                ui::spinner(ui, &p, 14.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} 个可用版本", app.download.content_versions.len()))
                        .font(ui::font(12.0))
                        .color(p.text_faint),
                );
            });
        });
        ui.add_space(10.0);

        if app.download.content_versions.is_empty() {
            ui.label(
                egui::RichText::new("当前筛选条件下没有可用版本，可以关闭兼容性筛选后重试")
                    .font(ui::font(12.5))
                    .color(p.text_weak),
            );
            return;
        }

        let labels: Vec<String> = app
            .download
            .content_versions
            .iter()
            .map(|v| {
                let games = if v.game_versions.is_empty() {
                    "通用".to_string()
                } else {
                    v.game_versions.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
                };
                format!("{} · {} · {}", v.version_number, v.loader_label(), games)
            })
            .collect();

        let mut index = app.download.content_version_index.min(labels.len() - 1);
        let current = labels[index].clone();
        if let Some(selected) = ui::dropdown(ui, &p, "content_version", &current, &labels, 420.0) {
            index = selected;
            app.download.content_version_index = selected;
        }
        ui.add_space(12.0);

        let version = app.download.content_versions[index].clone();
        ui.horizontal(|ui| {
            let enabled = app.selected_version_id().is_some();
            if ui::button(
                ui,
                &p,
                ButtonOpts::primary("安装到游戏目录")
                    .icon("download-arrow")
                    .enabled(enabled),
            )
            .clicked()
            {
                install_content(app, kind, version.clone(), project.title.clone());
            }
            if !enabled {
                ui.label(
                    egui::RichText::new("请先在首页选择一个游戏版本")
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

fn install_content(app: &mut App, kind: ContentKind, version: Version, title: String) {
    let Some(version_id) = app.selected_version_id() else {
        app.error("请先选择一个游戏版本");
        return;
    };
    let game_dir = app.cfg.effective_game_dir(&version_id);
    let auto_deps = app.cfg.download.auto_install_dependencies;
    let mc = app
        .selected_version()
        .map(|v| v.mc_version.clone())
        .unwrap_or_default();
    let loader = app
        .selected_version()
        .and_then(|v| v.loader.as_ref().map(|l| l.to_lowercase()));

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

fn strip_markdown(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("![") || trimmed.starts_with("<img") || trimmed.starts_with("<iframe")
        {
            continue;
        }
        let cleaned = trimmed
            .trim_start_matches('#')
            .trim_start_matches(['-', '*', '>'])
            .trim();
        let cleaned = cleaned.replace("**", "").replace("__", "");
        if cleaned.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&cleaned);
            out.push('\n');
        }
        if out.chars().count() > 1200 {
            out.push_str("…");
            break;
        }
    }
    out.trim().to_string()
}

fn short_date(date: &str) -> String {
    let trimmed = date.trim();
    if trimmed.len() >= 10 {
        trimmed[..10].to_string()
    } else {
        trimmed.to_string()
    }
}
