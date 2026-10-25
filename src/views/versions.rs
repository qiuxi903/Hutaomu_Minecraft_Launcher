//! 版本管理：一级是所有版本列表，点进去进入该版本的二级页面。
//!
//! 二级页面左侧是导航（概览 / 设置 / 存档 / 截图 / 模组），右侧是内容。
//! 二级页面里的设置只对当前版本生效，会覆盖全局设置。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Align, Color32, Layout, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::{App, CoreEvent, Page};
use crate::core::minecraft::version;
use crate::core::paths;
use crate::icons;
use crate::theme::Palette;
use crate::ui::{self, widgets, ButtonOpts};
use crate::views;

pub struct VersionsState {
    pub search: String,
    /// 0 全部 / 1 正式版 / 2 快照 / 3 加载器 / 4 其他
    pub filter: usize,
    /// 排序：0 最近修改 / 1 名称 / 2 占用空间
    pub sort: usize,
    /// 当前打开的版本详情，None 表示停留在一级列表
    pub detail: Option<String>,
    /// 详情页左侧导航选中项，对应 sections_for 里的 id
    pub section: &'static str,
    /// 本地文件名 → Modrinth 项目 slug（后台反查得到，用于跳转官网）
    pub content_slugs: HashMap<String, String>,
    /// 已发起过反查的内容分区（模组 / 光影 / 资源包各反查一次）
    pub content_lookup_section: Option<&'static str>,
    /// 内容列表每行放几个（1 或 2）
    pub content_columns: usize,
    pub detail_memory: f32,
    pub detail_java: usize,
    pub detail_jvm: String,
    pub detail_dir: String,
    /// 该版本是否启用版本隔离
    pub detail_isolation: bool,
    pub confirm_delete: Option<String>,
    pub rename_target: Option<String>,
    pub rename_value: String,
    pub screenshots: HashMap<PathBuf, egui::TextureHandle>,
    pub loaded_for: Option<String>,
}

impl Default for VersionsState {
    fn default() -> Self {
        Self {
            search: String::new(),
            filter: 0,
            sort: 0,
            detail: None,
            section: "info",
            content_slugs: HashMap::new(),
            content_lookup_section: None,
            content_columns: 2,
            detail_memory: 4096.0,
            detail_java: 0,
            detail_jvm: String::new(),
            detail_dir: String::new(),
            detail_isolation: true,
            confirm_delete: None,
            rename_target: None,
            rename_value: String::new(),
            screenshots: HashMap::new(),
            loaded_for: None,
        }
    }
}

const FILTERS: [&str; 5] = ["全部", "正式版", "快照", "加载器", "其他"];
const SORTS: [&str; 3] = ["最近修改", "名称", "占用空间"];

/// 详情页左侧导航
///
/// 原版没有加载器，既装不了模组也用不上光影，这两项就不列出来。
/// 装了机械动力、投影这类模组的实例，还会多出模组自己的内容入口。
fn sections_for(
    inst: &crate::core::minecraft::GameInstance,
    features: &[crate::core::content::modfeature::ModFeature],
) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut list: Vec<(&'static str, &'static str, &'static str)> = vec![
        ("info", "概览", "info"),
        ("settings", "设置", "sliders"),
        ("saves", "存档", "world"),
        ("screenshots", "截图", "image"),
    ];
    let has_loader = inst.variants.iter().any(|v| v.loader.is_some());
    if has_loader {
        list.push(("mod", "模组", "mod"));
        list.push(("shader", "光影", "shader"));
    }
    list.push(("resourcepack", "资源包", "resourcepack"));
    // 模组自带的内容放在最后，视觉上像是补充项
    for feature in features {
        list.push((feature.section, feature.label, feature.icon));
    }
    list
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    if app.versions_ui.detail.is_some() {
        detail_page(app, ui);
    } else {
        list_page(app, ui);
    }

    delete_modal(app, ui);
    rename_modal(app, ui);
}

// ------------------------------------------------------------------ 一级：版本列表

fn list_page(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(
        ui,
        &p,
        "版本管理",
        "查看已安装的实例，点进去可以切换加载器、配置启动参数、管理存档与模组",
    );

    if app.instances.is_empty() {
        ui::empty_state(
            ui,
            &p,
            "layers",
            "还没有安装任何版本",
            "到「下载」页面安装游戏本体或加载器",
        );
        return;
    }

    toolbar(app, ui);
    ui.add_space(14.0);
    version_grid(app, ui);
}

/// 一级页面的工具栏
fn toolbar(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let total_size: u64 = app.instances.iter().map(|i| i.size()).sum();

    ui.horizontal(|ui| {
        let mut search = app.versions_ui.search.clone();
        if ui::search_field(ui, &p, &mut search, "搜索版本", 240.0).changed() {
            app.versions_ui.search = search;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::icon_button(ui, &p, "refresh", "刷新列表").clicked() {
                app.refresh_local_versions();
            }
            if ui::button(ui, &p, ButtonOpts::secondary("打开游戏目录").icon("folder")).clicked() {
                app.cfg.open_game_dir();
            }
            if ui::button(ui, &p, ButtonOpts::primary("下载新版本").icon("download")).clicked() {
                app.page = Page::Download;
            }
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let mut filter = app.versions_ui.filter;
        if ui::segmented(ui, &p, "ver_filter", &mut filter, &FILTERS).clicked() {
            app.versions_ui.filter = filter;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "共 {} 个实例 · 占用 {}",
                    app.instances.len(),
                    paths::format_size(total_size)
                ))
                .font(ui::font(11.5))
                .color(p.text_faint),
            );
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("排序")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        let mut sort = app.versions_ui.sort;
        if ui::segmented(ui, &p, "ver_sort", &mut sort, &SORTS).clicked() {
            app.versions_ui.sort = sort;
        }
    });
}

/// 按筛选与排序整理出的实例列表
fn filtered_instances(app: &App) -> Vec<crate::core::minecraft::GameInstance> {
    let keyword = app.versions_ui.search.trim().to_lowercase();
    let filter = app.versions_ui.filter;
    let mut instances: Vec<crate::core::minecraft::GameInstance> = app
        .instances
        .iter()
        .filter(|inst| {
            // 版本类型看原版变体，实例本身就是按 Minecraft 版本聚合的
            let version_type = inst
                .vanilla()
                .map(|v| v.version_type.as_str())
                .unwrap_or("");
            match filter {
                1 => version_type == "release",
                2 => version_type == "snapshot",
                3 => inst.variants.iter().any(|v| v.loader.is_some()),
                4 => !version_type.is_empty()
                    && version_type != "release"
                    && version_type != "snapshot",
                _ => true,
            }
        })
        .filter(|inst| {
            keyword.is_empty()
                || inst.mc_version.to_lowercase().contains(&keyword)
                || inst.display_name.to_lowercase().contains(&keyword)
                || inst
                    .variants
                    .iter()
                    .any(|v| v.id.to_lowercase().contains(&keyword))
        })
        .cloned()
        .collect();

    match app.versions_ui.sort {
        1 => instances.sort_by(|a, b| a.mc_version.to_lowercase().cmp(&b.mc_version.to_lowercase())),
        2 => instances.sort_by(|a, b| b.size().cmp(&a.size())),
        _ => instances.sort_by(|a, b| b.last_modified().cmp(&a.last_modified())),
    }
    instances
}

/// 版本卡片网格
fn version_grid(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let instances = filtered_instances(app);

    if instances.is_empty() {
        ui::empty_state(ui, &p, "search", "没有匹配的版本", "换个关键词或筛选条件试试");
        return;
    }

    let width = ui.available_width();
    let columns = if width > 1080.0 {
        4
    } else if width > 800.0 {
        3
    } else if width > 520.0 {
        2
    } else {
        1
    };
    let card_width = (width - 14.0 * (columns as f32 - 1.0)) / columns as f32;

    let mut open: Option<String> = None;
    let mut index = 0usize;
    for chunk in instances.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            for inst in chunk {
                // 卡片逐个淡入，网格不会整片糊上来
                let delay = ui::anim::stagger(index, 0.035);
                let appear = ui::anim::fade_in(ui.ctx(), ui.id().with(("inst", index)), delay);
                ui.set_opacity(appear);
                index += 1;
                let is_current = app
                    .instances
                    .iter()
                    .find(|i| i.mc_version == inst.mc_version)
                    .and_then(|i| i.active_version())
                    .map(|v| app.cfg.selected_version.as_deref() == Some(v.id.as_str()))
                    .unwrap_or(false);
                // 版本目录里有自定义图标就用它，没有再按版本分类给一个
                let icon_texture = inst
                    .custom_icon
                    .as_ref()
                    .and_then(|path| app.file_icon_texture(path));
                let response = instance_card(ui, &p, inst, card_width, is_current, icon_texture);
                let clickable = response.interact(Sense::click());
                if clickable.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if clickable.clicked() {
                    open = Some(inst.mc_version.clone());
                }
                ui.set_opacity(1.0);
            }
        });
        ui.add_space(14.0);
    }

    if let Some(id) = open {
        open_detail(app, &id);
    }
}

/// 单个实例卡片
///
/// 整张卡片用 painter 绘制：egui 的 Label 即使设了 truncate，测量宽度仍是原文，
/// 版本号一长就会把整行布局撑开，所以这里自己截断文本。
fn instance_card(
    ui: &mut Ui,
    p: &crate::theme::Palette,
    inst: &crate::core::minecraft::GameInstance,
    card_width: f32,
    is_current: bool,
    icon_texture: Option<egui::TextureHandle>,
) -> egui::Response {
    const CARD_HEIGHT: f32 = 88.0;
    const PAD: f32 = 12.0;
    const ICON_SIZE: f32 = 26.0;

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(card_width, CARD_HEIGHT), Sense::click());
    // 悬停时轻微上浮并把边框染上主色，让人知道这张卡能点
    let hovered = response.hovered();
    let lift = if hovered && ui::animations_enabled() { 1.5 } else { 0.0 };
    let rect = rect.translate(Vec2::new(0.0, -lift));
    let border = if is_current {
        p.accent
    } else if hovered {
        widgets::lerp_color(p.border, p.accent, 0.55)
    } else {
        p.border
    };
    ui.painter().rect_filled(
        rect,
        p.corner_sm(),
        if hovered {
            widgets::lerp_color(p.surface, p.surface_alt, 0.6)
        } else {
            p.surface
        },
    );
    ui.painter().rect_stroke(
        rect,
        p.corner_sm(),
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );

    // 图标
    //
    // 版本目录里放了自定义图标就用它（整合包多是这样），
    // 没有的话按版本分类给一个：正式版草方块、快照命令方块、
    // 旧版圆石、愚人节另有一个。
    let icon_rect = Rect::from_min_size(rect.min + Vec2::new(PAD, PAD), Vec2::splat(ICON_SIZE));
    ui.painter()
        .rect_filled(icon_rect, p.corner_sm(), p.surface_alt);
    match icon_texture {
        Some(texture) => {
            ui.painter().image(
                texture.id(),
                icon_rect.shrink(4.0),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            icons::paint(
                ui.ctx(),
                ui.painter(),
                inst.category.icon(),
                icon_rect.shrink(6.0),
                p.text_weak,
            );
        }
    }

    // 标题：自行截断，保证不超出卡片
    let text_x = icon_rect.right() + 8.0;
    let avail = (rect.right() - PAD - text_x).max(40.0);
    let title = widgets::ellipsize(&inst.display_name, avail, 13.0);
    ui.painter().text(
        Pos2::new(text_x, icon_rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        ui::font(13.0),
        p.text,
    );

    // 副信息：加载器 · 变体数 · 当前状态
    let mut meta: Vec<String> = vec![inst.loader_label()];
    // 版本分类，正式版就不用重复写了
    if inst.category != crate::core::minecraft::VersionCategory::Release {
        meta.push(inst.category.label().to_string());
    }
    if inst.variant_count() > 1 {
        meta.push(format!("{} 个变体", inst.variant_count()));
    }
    if is_current {
        meta.push("当前使用".to_string());
    }
    let meta_y = icon_rect.bottom() + 15.0;
    ui.painter().text(
        Pos2::new(text_x, meta_y),
        egui::Align2::LEFT_CENTER,
        widgets::ellipsize(&meta.join(" · "), avail, 11.0),
        ui::font(11.0),
        p.text_weak,
    );

    // 底部：占用与时间（坐标相对卡片，不能用屏幕坐标）
    let bottom = format!(
        "{} · {}",
        paths::format_size(inst.size()),
        inst.last_modified()
            .map(|t| {
                let dt: chrono::DateTime<chrono::Local> = t.into();
                dt.format("%m-%d %H:%M").to_string()
            })
            .unwrap_or_else(|| "未知时间".into())
    );
    ui.painter().text(
        Pos2::new(rect.left() + PAD, rect.bottom() - PAD),
        egui::Align2::LEFT_BOTTOM,
        bottom,
        ui::font(10.5),
        p.text_faint,
    );

    response
}

// ------------------------------------------------------------------ 二级：版本详情

/// 打开某个实例的详情页
pub(crate) fn open_detail(app: &mut App, mc_version: &str) {
    app.versions_ui.detail = Some(mc_version.to_string());
    app.versions_ui.section = "info";
    load_detail(app, mc_version);
}

fn detail_page(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let Some(mc) = app.versions_ui.detail.clone() else {
        return;
    };
    let Some(inst) = app.instances.iter().find(|i| i.mc_version == mc).cloned() else {
        // 版本已被删除，回到列表
        app.versions_ui.detail = None;
        return;
    };
    if app.versions_ui.loaded_for.as_deref() != Some(mc.as_str()) {
        load_detail(app, &mc);
    }
    let Some(active) = inst.active_version().cloned() else {
        app.versions_ui.detail = None;
        return;
    };
    let is_current = app.cfg.selected_version.as_deref() == Some(active.id.as_str());

    // 左侧导航：原版没有加载器，模组与光影不列出来；
    // 装了机械动力、投影这类模组的实例会多出模组自己的内容入口
    let features = app
        .mod_features
        .get(&inst.mc_version)
        .cloned()
        .unwrap_or_default();
    let sections = sections_for(&inst, &features);
    // 切换加载器后当前分区可能已经不在列表里，退回概览
    if !sections.iter().any(|(id, _, _)| *id == app.versions_ui.section) {
        app.versions_ui.section = "info";
    }

    // ---------- 顶部：返回 + 版本信息 + 操作 ----------
    ui.horizontal(|ui| {
        if ui::button(ui, &p, ButtonOpts::ghost("返回版本列表").icon("arrow-left")).clicked() {
            app.versions_ui.detail = None;
            return;
        }
    });
    ui.add_space(8.0);

    // 版本目录里有自定义图标就用它，没有再按版本分类给一个
    let icon_texture = inst
        .custom_icon
        .as_ref()
        .and_then(|path| app.file_icon_texture(path));


    ui::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
            ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
            match icon_texture.as_ref() {
                Some(texture) => {
                    ui.painter().image(
                        texture.id(),
                        rect.shrink(6.0),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    icons::paint(
                        ui.ctx(),
                        ui.painter(),
                        inst.category.icon(),
                        rect.shrink(10.0),
                        p.text_weak,
                    );
                }
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&inst.display_name)
                            .font(ui::font(18.0))
                            .color(p.text)
                            .strong(),
                    );
                    if let Some(loader) = &active.loader {
                        widgets::badge(ui, &p, loader, views::loader_color(Some(loader)));
                    }
                    widgets::badge(
                        ui,
                        &p,
                        version_type_label(&active.version_type),
                        views::version_type_color(&active.version_type),
                    );
                    if is_current {
                        widgets::badge(ui, &p, "当前使用", p.accent);
                    }
                });
                ui.add_space(3.0);
                let game_dir = app.cfg.effective_game_dir(&inst.mc_version);
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {} 个变体 · 游戏目录 {}",
                        paths::format_size(inst.size()),
                        inst.variant_count(),
                        game_dir.display()
                    ))
                    .font(ui::font(11.5))
                    .color(p.text_faint),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::primary("启动").icon("play")).clicked() {
                    app.cfg.selected_version = Some(active.id.clone());
                    app.save_config();
                    views::launch_game(app);
                }
                if !is_current {
                    if ui::button(ui, &p, ButtonOpts::secondary("设为当前")).clicked() {
                        app.cfg.selected_version = Some(active.id.clone());
                        app.save_config();
                        app.refresh_local_versions();
                    }
                }
                if ui::button(ui, &p, ButtonOpts::secondary("打开目录").icon("folder")).clicked() {
                    views::open_version_dir(app, &inst.mc_version);
                }
            });
        });
        ui.add_space(12.0);

        // ---------- 加载器变体切换 ----------
        if inst.variant_count() > 1 {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("加载器版本")
                        .font(ui::font(13.0))
                        .color(p.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let labels: Vec<String> =
                        inst.variants.iter().map(|v| v.variant_label()).collect();
                    let current = labels
                        .iter()
                        .position(|l| *l == active.variant_label())
                        .unwrap_or(0);
                    let current_label = labels[current].clone();
                    if let Some(index) =
                        ui::dropdown(ui, &p, "variant", &current_label, &labels, 260.0)
                    {
                        let picked = inst.variants[index].id.clone();
                        app.cfg.selected_version = Some(picked);
                        app.save_config();
                        app.refresh_local_versions();
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("切换加载器版本会同时改用该版本启动，模组、光影与存档仍共用这一份。")
                    .font(ui::font(11.5))
                    .color(p.text_faint),
            );
        }
    });
    ui.add_space(12.0);

    // ---------- 左侧导航 + 右侧内容 ----------
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.vertical(|ui| {
            ui.set_width(150.0);
            // 导航项会随安装的模组变多，装不下时滚动，别把后面的项挤出去
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    ui::card_padded(ui, &p, 8, |ui| {
                        ui.set_width(134.0);
                        for (id, label, icon) in sections.iter() {
                            let selected = app.versions_ui.section == *id;
                            if ui::nav_item(ui, &p, icon, label, selected).clicked() {
                                app.versions_ui.section = id;
                            }
                            ui.add_space(2.0);
                        }
                    });
                });
        });
        ui.vertical(|ui| {
            let width = ui.available_width();
            ui.set_width(width);
            // 换子标签时内容淡入，不是一下子跳过去
            let section = app.versions_ui.section.clone();
            let appear = ui::anim::fade_in(ui.ctx(), ui.id().with(("section", &section)), 0.0);
            ui.set_opacity(appear);
            match app.versions_ui.section {
                "info" => overview_tab(app, ui, &inst, &active),
                "settings" => settings_tab(app, ui, &inst),
                "saves" => saves_tab(app, ui, &inst),
                "screenshots" => screenshots_tab(app, ui, &inst),
                "mod" => content_tab(app, ui, &inst, ContentKind::Mod, "mod"),
                "shader" => content_tab(app, ui, &inst, ContentKind::Shader, "shader"),
                "resourcepack" => {
                    content_tab(app, ui, &inst, ContentKind::ResourcePack, "resourcepack")
                }
                section => match features.iter().find(|f| f.section == section) {
                    Some(feature) => mod_feature_tab(app, ui, &inst, feature),
                    None => overview_tab(app, ui, &inst, &active),
                },
            }
            ui.set_opacity(1.0);
        });
    });
}

fn version_type_label(version_type: &str) -> &'static str {
    match version_type {
        "release" => "正式版",
        "snapshot" => "快照",
        "old_beta" => "远古 Beta",
        "old_alpha" => "远古 Alpha",
        _ => "自定义",
    }
}

fn count_files(dir: &Path) -> usize {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count()
}

fn overview_tab(
    app: &mut App,
    ui: &mut Ui,
    inst: &crate::core::minecraft::GameInstance,
    active: &crate::core::minecraft::LocalVersion,
) {
    let p = app.palette;
    let dir = version::version_dir(&app.cfg.game_dir, &active.id);
    let json_path = version::version_json_path(&app.cfg.game_dir, &active.id);
    let inherits = version::read_local_json(&app.cfg.game_dir, &active.id)
        .ok()
        .and_then(|json| json.inherits_from)
        .unwrap_or_default();
    let stat = app.cfg.stat_of(&active.id);
    let game_dir = app.cfg.effective_game_dir(&inst.mc_version);

    // 整合包实例的 id 是自己的名字，Minecraft 版本要看标记文件里记的
    let mc_version = active
        .modpack
        .as_ref()
        .map(|m| m.mc_version.clone())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| inst.mc_version.clone());
    widgets::key_value(ui, &p, "Minecraft 版本", &mc_version);
    ui.add_space(6.0);
    if let Some(info) = &active.modpack {
        widgets::key_value(ui, &p, "整合包", &info.name);
        ui.add_space(6.0);
    }
    widgets::key_value(
        ui,
        &p,
        "加载器",
        active.loader.as_deref().unwrap_or("原版"),
    );
    ui.add_space(6.0);
    if let Some(version) = &active.loader_version {
        widgets::key_value(ui, &p, "加载器版本", version);
        ui.add_space(6.0);
    }
    widgets::key_value(ui, &p, "当前变体", &active.id);
    ui.add_space(6.0);
    if !inherits.is_empty() {
        widgets::key_value(ui, &p, "继承自", &inherits);
        ui.add_space(6.0);
    }
    widgets::key_value(ui, &p, "版本类型", version_type_label(&active.version_type));
    ui.add_space(6.0);
    widgets::key_value(ui, &p, "启动次数", &format!("{} 次", stat.launches));
    ui.add_space(6.0);
    if stat.last_launch > 0 {
        let dt = chrono::DateTime::from_timestamp(stat.last_launch, 0)
            .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        widgets::key_value(ui, &p, "最后启动", &dt);
        ui.add_space(6.0);
    }
    widgets::key_value(ui, &p, "版本目录", &dir.display().to_string());
    ui.add_space(6.0);
    widgets::key_value(ui, &p, "游戏目录", &game_dir.display().to_string());
    ui.add_space(6.0);
    widgets::key_value(ui, &p, "占用空间", &paths::format_size(inst.size()));
    ui.add_space(14.0);

    ui.horizontal_wrapped(|ui| {
        if ui::button(ui, &p, ButtonOpts::secondary("打开版本目录").icon("folder")).clicked() {
            paths::reveal(&dir);
        }
        if ui::button(ui, &p, ButtonOpts::secondary("打开游戏目录").icon("folder-open")).clicked() {
            let _ = paths::ensure_dir(&game_dir);
            paths::reveal(&game_dir);
        }
        if ui::button(ui, &p, ButtonOpts::secondary("版本 JSON").icon("code")).clicked() {
            paths::reveal(&json_path);
        }
        if ui::button(ui, &p, ButtonOpts::secondary("重命名").icon("edit")).clicked() {
            app.versions_ui.rename_target = Some(active.id.clone());
            app.versions_ui.rename_value = active.id.clone();
        }
        if ui::button(ui, &p, ButtonOpts::secondary("复制版本").icon("copy")).clicked() {
            copy_version(app, &active.id);
        }
        if ui::button(ui, &p, ButtonOpts::danger("删除版本").icon("trash")).clicked() {
            app.versions_ui.confirm_delete = Some(active.id.clone());
        }
    });
}

fn load_detail(app: &mut App, mc_version: &str) {
    let ov = app.cfg.version_override(mc_version);
    app.versions_ui.detail_memory = ov.memory_mb.unwrap_or(app.cfg.java.memory_mb) as f32;
    app.versions_ui.detail_jvm = ov
        .jvm_args
        .clone()
        .unwrap_or_else(|| app.cfg.java.jvm_args.clone());
    app.versions_ui.detail_dir = ov
        .game_dir
        .as_ref()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    app.versions_ui.detail_java = ov
        .java_path
        .as_ref()
        .and_then(|path| app.javas.iter().position(|j| &j.path == path))
        .unwrap_or(0);
    // 隔离开关默认跟随全局（全局默认开启）
    app.versions_ui.detail_isolation = app.cfg.isolation_for(mc_version);
    app.versions_ui.loaded_for = Some(mc_version.to_string());
}

fn settings_tab(app: &mut App, ui: &mut Ui, inst: &crate::core::minecraft::GameInstance) {
    let p = app.palette;

    ui.label(
        egui::RichText::new("这里的设置只对当前实例生效，会覆盖全局设置。")
            .font(ui::font(12.0))
            .color(p.text_weak),
    );
    ui.add_space(14.0);

    // ---- 版本隔离 ----
    let mut isolation = app.versions_ui.detail_isolation;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("版本隔离")
                .font(ui::font(13.0))
                .color(p.text_weak),
        );
        ui.label(
            egui::RichText::new("每个实例使用独立的模组、存档与配置目录")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::toggle(ui, &p, &mut isolation).changed() {
                app.versions_ui.detail_isolation = isolation;
            }
        });
    });
    ui.add_space(4.0);
    let isolated_dir = version::version_dir(&app.cfg.game_dir, &inst.mc_version);
    ui.label(
        egui::RichText::new(if isolation {
            format!("该实例的游戏目录：{}", isolated_dir.display())
        } else {
            "该实例与其它实例共用游戏目录".to_string()
        })
        .font(ui::font(11.5))
        .color(p.text_faint),
    );
    ui.add_space(16.0);

    let mut memory = app.versions_ui.detail_memory;
    if ui::slider(ui, &p, "最大内存", "分配给游戏进程的堆内存上限", &mut memory, 512.0, 32768.0, 256.0, " MB").changed() {
        app.versions_ui.detail_memory = memory;
    }
    ui.add_space(6.0);

    let java_labels: Vec<String> = app
        .javas
        .iter()
        .map(|j| format!("Java {} · {} · {}", j.version, j.bits, j.vendor))
        .collect();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("使用 Java")
                .font(ui::font(13.0))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if java_labels.is_empty() {
                ui.label(
                    egui::RichText::new("尚未搜索到 Java")
                        .font(ui::font(12.0))
                        .color(p.text_faint),
                );
            } else {
                let index = app.versions_ui.detail_java.min(java_labels.len() - 1);
                let current = java_labels[index].clone();
                if let Some(selected) =
                    ui::dropdown(ui, &p, "detail_java", &current, &java_labels, 320.0)
                {
                    app.versions_ui.detail_java = selected;
                }
            }
        });
    });
    ui.add_space(10.0);

    ui.label(
        egui::RichText::new("额外 JVM 参数")
            .font(ui::font(12.5))
            .color(p.text_weak),
    );
    ui.add_space(4.0);
    ui::text_field(
        ui,
        &p,
        &mut app.versions_ui.detail_jvm,
        "例如 -XX:+UseG1GC",
        480.0,
        false,
    );
    ui.add_space(10.0);

    ui.label(
        egui::RichText::new("独立游戏目录（留空表示跟随全局 / 版本隔离）")
            .font(ui::font(12.5))
            .color(p.text_weak),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui::text_field(
            ui,
            &p,
            &mut app.versions_ui.detail_dir,
            "留空使用默认目录",
            400.0,
            false,
        );
        if ui::button(ui, &p, ButtonOpts::secondary("选择")).clicked() {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("选择该版本的游戏目录")
                .pick_folder()
            {
                app.versions_ui.detail_dir = dir.display().to_string();
            }
        }
    });
    ui.add_space(14.0);

    ui.horizontal(|ui| {
        if ui::button(ui, &p, ButtonOpts::primary("保存该实例设置")).clicked() {
            let java_path = app
                .javas
                .get(app.versions_ui.detail_java)
                .map(|j| j.path.clone());
            let dir_value = app.versions_ui.detail_dir.trim().to_string();
            let override_new = crate::core::config::VersionOverride {
                memory_mb: Some(app.versions_ui.detail_memory as u32),
                java_path,
                jvm_args: Some(app.versions_ui.detail_jvm.clone()),
                game_dir: if dir_value.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(dir_value))
                },
                isolation: Some(app.versions_ui.detail_isolation),
            };
            app.cfg
                .version_overrides
                .insert(inst.mc_version.clone(), override_new);
            app.save_config();
            app.success(format!("{} 的配置已保存", inst.mc_version));
        }
        if ui::button(ui, &p, ButtonOpts::secondary("恢复全局设置")).clicked() {
            app.cfg.version_overrides.remove(&inst.mc_version);
            app.save_config();
            load_detail(app, &inst.mc_version);
            app.info("已恢复为全局设置");
        }
    });
}

fn saves_tab(app: &mut App, ui: &mut Ui, inst: &crate::core::minecraft::GameInstance) {
    let p = app.palette;
    let game_dir = app.cfg.effective_game_dir(&inst.mc_version);
    let saves_dir = game_dir.join("saves");
    let mut worlds: Vec<(String, u64, Option<std::time::SystemTime>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&saves_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || !path.join("level.dat").is_file() {
                continue;
            }
            let size = paths::dir_size(&path);
            let modified = std::fs::metadata(path.join("level.dat"))
                .and_then(|m| m.modified())
                .ok();
            worlds.push((entry.file_name().to_string_lossy().to_string(), size, modified));
        }
    }
    worlds.sort_by(|a, b| b.2.cmp(&a.2));

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} 个存档", worlds.len()))
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("打开存档目录").icon("folder")).clicked() {
                let _ = paths::ensure_dir(&saves_dir);
                paths::reveal(&saves_dir);
            }
        });
    });
    ui.add_space(10.0);

    if worlds.is_empty() {
        ui::empty_state(ui, &p, "world", "还没有存档", "进入游戏创建世界后会显示在这里");
        return;
    }

    for (name, size, modified) in worlds.iter().take(30) {
        let response = ui::card_flat(ui, &p, |ui| {
            ui.horizontal(|ui| {
                icons::paint(
                    ui.ctx(),
                    ui.painter(),
                    "world",
                    Rect::from_center_size(
                        Pos2::new(ui.cursor().left() + 10.0, ui.cursor().center().y),
                        Vec2::splat(18.0),
                    ),
                    p.text_weak,
                );
                ui.add_space(28.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(name)
                            .font(ui::font(13.0))
                            .color(p.text)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {}",
                            paths::format_size(*size),
                            modified
                                .map(|t| {
                                    let dt: chrono::DateTime<chrono::Local> = t.into();
                                    dt.format("%Y-%m-%d %H:%M").to_string()
                                })
                                .unwrap_or_else(|| "未知".into())
                        ))
                        .font(ui::font(11.0))
                        .color(p.text_faint),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::ghost(ui, &p, "打开目录").clicked() {
                        paths::reveal(&saves_dir.join(name));
                    }
                });
            });
        });
        let _ = response;
        ui.add_space(6.0);
    }
}

fn screenshots_tab(app: &mut App, ui: &mut Ui, inst: &crate::core::minecraft::GameInstance) {
    let p = app.palette;
    let game_dir = app.cfg.effective_game_dir(&inst.mc_version);
    let shots_dir = game_dir.join("screenshots");
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&shots_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files.reverse();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} 张截图", files.len()))
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("打开截图目录").icon("folder")).clicked() {
                let _ = paths::ensure_dir(&shots_dir);
                paths::reveal(&shots_dir);
            }
        });
    });
    ui.add_space(10.0);

    if files.is_empty() {
        ui::empty_state(ui, &p, "image", "还没有截图", "在游戏里按 F2 截图后会显示在这里");
        return;
    }

    let columns = 3;
    let cell_width = (ui.available_width() - 12.0 * (columns as f32 - 1.0)) / columns as f32;
    for chunk in files.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for path in chunk {
                ui.vertical(|ui| {
                    ui.set_width(cell_width);
                    let cached = app.versions_ui.screenshots.get(path).cloned();
                    let texture = match cached {
                        Some(texture) => Some(texture),
                        None => load_thumbnail(app, path),
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(cell_width, cell_width * 0.56), Sense::click());
                    if let Some(texture) = &texture {
                        ui.painter().image(
                            texture.id(),
                            rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    } else {
                        ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
                    }
                    ui.painter().rect_stroke(
                        rect,
                        p.corner_sm(),
                        egui::Stroke::new(1.0, p.border),
                        egui::StrokeKind::Inside,
                    );
                    if response.clicked() {
                        let _ = open::that(path);
                    }
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default(),
                        )
                        .font(ui::font(10.5))
                        .color(p.text_faint),
                    );
                });
            }
        });
        ui.add_space(12.0);
    }
}

fn load_thumbnail(app: &mut App, path: &Path) -> Option<egui::TextureHandle> {
    let image = image::open(path).ok()?;
    let width = 260u32;
    let height = (image.height() as f32 * (width as f32 / image.width() as f32)).max(1.0) as u32;
    let resized = image.resize(width, height, image::imageops::FilterType::Triangle);
    let rgba = resized.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let texture = app.egui_ctx.load_texture(
        format!("shot:{}", path.display()),
        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw()),
        egui::TextureOptions::LINEAR,
    );
    app.versions_ui
        .screenshots
        .insert(path.to_path_buf(), texture.clone());
    Some(texture)
}

// ------------------------------------------------------------------ 内容列表

/// 本地内容类型：模组 / 光影 / 资源包
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Mod,
    Shader,
    ResourcePack,
}

impl ContentKind {
    fn label(self) -> &'static str {
        match self {
            Self::Mod => "模组",
            Self::Shader => "光影",
            Self::ResourcePack => "资源包",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
        }
    }

    /// Modrinth 的项目类型（决定主页链接的路径）
    fn modrinth_type(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
        }
    }

    /// 相对游戏目录的位置
    fn sub_dir(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::Shader => "shaderpacks",
            Self::ResourcePack => "resourcepacks",
        }
    }

    fn all() -> [ContentKind; 3] {
        [Self::Mod, Self::Shader, Self::ResourcePack]
    }

    /// 对应「下载」页里的标签页下标
    fn download_tab(self) -> usize {
        match self {
            Self::Mod => 2,
            Self::Shader => 3,
            Self::ResourcePack => 4,
        }
    }
}

/// 本地内容的一个条目
struct LocalContent {
    file_name: String,
    path: PathBuf,
    size: u64,
    modified: Option<std::time::SystemTime>,
    /// 展示名（优先中文名）
    display: String,
    /// 副标题：版本 / 加载器
    subtitle: String,
    /// 对应的 MCMod 条目（有则可跳转到 MC 百科页面）
    mcmod: Option<crate::core::content::mcmod::Entry>,
    /// 对应的 Modrinth slug（有则可跳转到 Modrinth 页面）
    modrinth_slug: Option<String>,
    /// jar 自带的图标
    icon: Option<std::sync::Arc<Vec<u8>>>,
    /// 排序用的稳定键
    ///
    /// 词库是异步加载的，显示名会后到后变。
    /// 排序按这个稳定的键走，名字补上时列表就不会整排跳位。
    sort_key: String,
}

/// 列出某个实例下的内容
fn list_contents(app: &App, kind: ContentKind, game_dir: &Path) -> Vec<LocalContent> {
    let dir = game_dir.join(kind.sub_dir());
    let mut items: Vec<LocalContent> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return items;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "jar" && ext != "zip" {
            continue;
        }
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();

        // 模组解析 jar 元数据取名字；光影与资源包一般没有元数据
        let meta = if kind == ContentKind::Mod {
            crate::core::content::modmeta::read_meta(&path)
        } else {
            None
        };

        // 依次尝试：MCMod 中文名 → 元数据名 → 文件名
        let mut display = String::new();
        let mut subtitle = String::new();
        let mut mcmod = None;
        let mut modrinth_slug = None;
        let mut sort_key = file_name.clone();
        if let Some(meta) = &meta {
            if !meta.mod_id.is_empty() {
                sort_key = meta.mod_id.clone();
            }
            // mod id 常常与 Modrinth slug 一致，先按 slug 查
            let entry = app
                .mcmod_db
                .lookup_modrinth(&meta.mod_id)
                .or_else(|| app.mcmod_db.lookup_english(&meta.name))
                .cloned();
            if let Some(entry) = &entry {
                display = entry.display_with_abbr();
                if !entry.subname.is_empty() && entry.subname != display {
                    subtitle = entry.subname.clone();
                }
                if !entry.modrinth_slug.is_empty() {
                    modrinth_slug = Some(entry.modrinth_slug.clone());
                }
            } else if !meta.name.is_empty() {
                display = meta.name.clone();
            }
            if !meta.version.is_empty() {
                if subtitle.is_empty() {
                    subtitle = format!("{} · {}", meta.loader, meta.version);
                } else {
                    subtitle = format!("{} · {}", subtitle, meta.version);
                }
            }
            mcmod = entry;
        }
        if display.is_empty() {
            // 没有元数据时，去掉版本号后缀当显示名
            display = strip_version_suffix(&file_name);
        }

        items.push(LocalContent {
            file_name,
            path,
            size,
            modified,
            display,
            subtitle,
            mcmod,
            icon: meta.and_then(|m| m.icon.clone()),
            modrinth_slug,
            sort_key,
        });
    }
    // 按稳定键排序：词库后到时只改名字，不改顺序
    items.sort_by(|a, b| a.sort_key.to_lowercase().cmp(&b.sort_key.to_lowercase()));
    items
}

/// 去掉文件名末尾的版本号后缀，得到一个好认的名字
fn strip_version_suffix(file_name: &str) -> String {
    let stem = file_name
        .strip_suffix(".jar")
        .or_else(|| file_name.strip_suffix(".zip"))
        .unwrap_or(file_name);
    // 形如 `sodium-fabric-0.5.13` → `sodium-fabric`
    let mut parts: Vec<&str> = stem.rsplit('-').collect();
    while parts.len() > 1 {
        let last = parts[0];
        let looks_like_version = last.is_empty()
            || last
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false);
        if !looks_like_version {
            break;
        }
        parts.remove(0);
    }
    parts.reverse();
    parts.join("-")
}

/// 文件名里属于加载器名、应当剔除的段
const LOADER_TOKENS: [&str; 7] = [
    "fabric",
    "forge",
    "neoforge",
    "quilt",
    "liteloader",
    "rift",
    "optifine",
];

/// 文件名里属于项目名一部分、不能剔除的段（如资源包分辨率 32x）
const KEEP_TOKENS: [&str; 6] = ["16x", "32x", "64x", "128x", "256x", "512x"];

/// 从文件名里猜出项目名（去掉版本号、加载器后缀等噪声）
///
/// `sodium-fabric-0.5.13+mc1.20.1.jar` → `sodium`
/// `VanillaTweaks-neoforge-1.20.2-1.5.72.jar` → `VanillaTweaks`
/// `Faithful 32x - 26.3.zip` → `Faithful 32x`
fn guess_project_name(file_name: &str) -> String {
    let stem = file_name
        .strip_suffix(".jar")
        .or_else(|| file_name.strip_suffix(".zip"))
        .unwrap_or(file_name);
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in stem.chars() {
        if ch == '-' || ch == '_' || ch == ' ' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    // 从右往左剔除版本号与加载器段
    while words.len() > 1 {
        let last = words.last().unwrap().to_lowercase();
        // 分辨率属于项目名的一部分，保留
        if KEEP_TOKENS.contains(&last.as_str()) {
            break;
        }
        let starts_with_digit = last
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false);
        let is_loader = LOADER_TOKENS.contains(&last.as_str());
        let is_plus = last.starts_with('+') || last.contains("+mc");
        if starts_with_digit || is_loader || is_plus {
            words.pop();
        } else {
            break;
        }
    }
    words.join(" ")
}

/// 由项目名猜出可能的 Modrinth slug
///
/// 例如 `VanillaTweaks` → `vanillatweaks` 与 `vanilla-tweaks` 两个候选。
fn slug_candidates(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let plain = name.trim().to_lowercase().replace(' ', "-");
    if !plain.is_empty() {
        out.push(plain.clone());
    }
    // 驼峰拆分成连字符：VanillaTweaks → vanilla-tweaks
    let mut kebab = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch.is_ascii_uppercase() && prev_lower {
            kebab.push('-');
        }
        kebab.push(ch.to_ascii_lowercase());
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    let kebab = kebab.replace(' ', "-");
    if !kebab.is_empty() && !out.contains(&kebab) {
        out.push(kebab);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 原版实例不列模组与光影，带加载器的实例才列
    #[test]
    fn vanilla_instance_hides_mod_and_shader() {
        let make = |id: &str, loader: Option<&str>| crate::core::minecraft::LocalVersion {
            id: id.to_string(),
            version_type: "release".into(),
            loader: loader.map(|l| l.to_string()),
            loader_version: None,
            mc_version: "26.2".into(),
            size: 1,
            last_modified: None,
            has_jar: true,
            icon_data_url: None,
            ..Default::default()
        };

        let vanilla = crate::core::minecraft::GameInstance {
            mc_version: "26.2".into(),
            variants: vec![make("26.2", None)],
            active: "26.2".into(),
            ..Default::default()
        };
        let ids: Vec<&str> = sections_for(&vanilla, &[])
            .iter()
            .map(|(id, _, _)| *id)
            .collect();
        assert!(!ids.contains(&"mod"));
        assert!(!ids.contains(&"shader"));
        assert!(ids.contains(&"resourcepack"));
        assert!(ids.contains(&"info"));

        let fabric = crate::core::minecraft::GameInstance {
            mc_version: "26.2".into(),
            variants: vec![make("26.2", None), make("fabric-loader-0.20.1-26.2", Some("Fabric"))],
            active: "fabric-loader-0.20.1-26.2".into(),
            ..Default::default()
        };
        let ids: Vec<&str> = sections_for(&fabric, &[])
            .iter()
            .map(|(id, _, _)| *id)
            .collect();
        assert!(ids.contains(&"mod"));
        assert!(ids.contains(&"shader"));
    }

    /// 装了机械动力、投影这类模组后，导航里多出模组自己的内容入口
    #[test]
    fn mod_features_add_their_own_sections() {        let inst = crate::core::minecraft::GameInstance {
            mc_version: "26.2".into(),
            variants: vec![],
            active: String::new(),
            ..Default::default()
        };
        let features = vec![
            crate::core::content::modfeature::MOD_FEATURES[0],
            crate::core::content::modfeature::MOD_FEATURES[1],
        ];
        let ids: Vec<&str> = sections_for(&inst, &features)
            .iter()
            .map(|(id, _, _)| *id)
            .collect();
        assert!(ids.contains(&"create_schematic"));
        assert!(ids.contains(&"litematic"));
    }

    /// 蓝图目录里的子文件夹也要列出来，方便按分类管理
    #[test]
    fn feature_files_include_sub_folders() {
        let root = std::env::temp_dir().join("htmcl-feature-files-test");
        let _ = std::fs::remove_dir_all(&root);
        let sub = root.join("分类A");
        std::fs::create_dir_all(&sub).expect("应能建目录");
        std::fs::write(root.join("顶层.nbt"), b"x").expect("应能写文件");
        std::fs::write(sub.join("建筑.nbt"), b"x").expect("应能写文件");
        std::fs::write(sub.join("无关.txt"), b"x").expect("应能写文件");

        let mut files = Vec::new();
        list_feature_files(&root, &["nbt"], &mut files);
        let names: Vec<String> = files.iter().map(|f| f.rel.clone()).collect();
        assert!(names.contains(&"顶层.nbt".to_string()));
        assert!(names.contains(&"分类A/建筑.nbt".to_string()));
        // 后缀不匹配的不要
        assert!(!names.iter().any(|n| n.ends_with(".txt")));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn strips_version_suffix_from_file_name() {
        assert_eq!(strip_version_suffix("sodium-fabric-0.5.13.jar"), "sodium-fabric");
        assert_eq!(strip_version_suffix("iris-neoforge-1.11.7.jar"), "iris-neoforge");
        assert_eq!(strip_version_suffix("Faithful 32x - 26.3.zip"), "Faithful 32x - 26.3");
        // 没有版本号时原样返回
        assert_eq!(strip_version_suffix("beautiful-world.jar"), "beautiful-world");
    }

    #[test]
    fn guesses_project_name_from_file_name() {
        // 加载器与版本号都要剔除
        assert_eq!(guess_project_name("sodium-fabric-0.5.13+mc1.20.1.jar"), "sodium");
        assert_eq!(
            guess_project_name("VanillaTweaks-neoforge-1.20.2-1.5.72.jar"),
            "VanillaTweaks"
        );
        assert_eq!(guess_project_name("iris-neoforge-1.11.7+mc26.3.jar"), "iris");
        // 资源包分辨率属于项目名，不能被当成版本号弹掉
        assert_eq!(guess_project_name("Faithful 32x - 26.3.zip"), "Faithful 32x");
    }

    #[test]
    fn derives_slug_candidates() {
        assert_eq!(slug_candidates("sodium"), vec!["sodium"]);
        // 驼峰要能拆成连字符
        let candidates = slug_candidates("VanillaTweaks");
        assert!(candidates.contains(&"vanillatweaks".to_string()));
        assert!(candidates.contains(&"vanilla-tweaks".to_string()));
        // 空格转连字符
        assert!(slug_candidates("Faithful 32x").contains(&"faithful-32x".to_string()));
    }
}

/// 站点链接：小字，悬停变色可点
fn site_link(ui: &mut Ui, p: &Palette, text: &str) -> egui::Response {
    let response = ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(ui::font(10.5))
                .color(p.accent)
                .strong(),
        )
        .truncate(),
    );
    if response.clicked() || response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// 卡片下方的堆叠纸边
///
/// 像卡片压在一叠纸上：只露一条边，不画描边，
/// 免得在浅色主题下看成几条错位的线。
fn paint_stacked_edges(ui: &Ui, p: &Palette, rect: &Rect) {
    let radius = p.corner(0);
    // 两层纸，每层往下挪一点、左右也窄一点
    for layer in 1..=2u8 {
        let offset = layer as f32 * 3.0;
        let edge = Rect::from_min_max(
            Pos2::new(rect.left() + offset, rect.bottom() + offset - 3.0),
            Pos2::new(rect.right() - offset, rect.bottom() + offset),
        );
        ui.painter()
            .rect_filled(edge, radius, widgets::alpha(p.text, if p.dark { 10 } else { 7 }));
    }
}

/// 内容区：模组 / 光影 / 资源包
fn content_tab(
    app: &mut App,
    ui: &mut Ui,
    inst: &crate::core::minecraft::GameInstance,
    kind: ContentKind,
    section: &'static str,
) {
    let p = app.palette;
    let game_dir = app.cfg.effective_game_dir(&inst.mc_version);

    let dir = game_dir.join(kind.sub_dir());
    let items = list_contents(app, kind, &game_dir);

    // 光影与资源包没有内置词条，用文件名反查 Modrinth 项目
    if kind != ContentKind::Mod && app.versions_ui.content_lookup_section != Some(section) {
        app.versions_ui.content_lookup_section = Some(section);
        let names: Vec<String> = items.iter().map(|i| i.file_name.clone()).collect();
        lookup_content_slugs(app, names);
    }

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} 个{}", items.len(), kind.label()))
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("打开目录").icon("folder")).clicked() {
                let _ = paths::ensure_dir(&dir);
                paths::reveal(&dir);
            }
            // 每行放一个还是两个（图标按钮，网格/列表）
            let columns = app.versions_ui.content_columns.clamp(1, 2);
            let (icon_name, tip) = if columns == 1 {
                ("list", "当前一行一个，点击改成一行两个")
            } else {
                ("grid", "当前一行两个，点击改成一行一个")
            };
            if ui::icon_button(ui, &p, icon_name, tip).clicked() {
                app.versions_ui.content_columns = if columns == 1 { 2 } else { 1 };
            }
            let download_label = format!("去下载{}", kind.label());
            if ui::button(ui, &p, ButtonOpts::secondary(&download_label).icon("download")).clicked() {
                // 跳到下载页并定位到对应标签
                app.download.tab = kind.download_tab();
                // 下载页装到当前实例，别让用户再选一遍
                app.download.content_install_instance = Some(inst.mc_version.clone());
                app.download.content_loaded_tab = None;
                app.page = Page::Download;
            }
        });
    });
    ui.add_space(10.0);

    if items.is_empty() {
        let tip = match kind {
            ContentKind::Mod => "到「下载」页搜索模组，默认会装到当前实例",
            ContentKind::Shader => "把光影包放进这个目录即可生效",
            ContentKind::ResourcePack => "把资源包放进这个目录即可生效",
        };
        ui::empty_state(ui, &p, kind.icon(), &format!("还没有{}", kind.label()), tip);
        return;
    }

    // 网格排布：每张卡片限宽，不让一行拉满整个内容区
    let width = ui.available_width();
    let columns = match app.versions_ui.content_columns {
        1 => 1,
        _ => {
            if width > 980.0 {
                2
            } else if width > 640.0 {
                2
            } else {
                1
            }
        }
    };
    let card_width = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;

    let mut index = 0usize;
    for chunk in items.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for item in chunk {
                let current = index;
                index += 1;
                // 逐项淡入，一长串卡片像依次落位，而不是整片糊上来
                let delay = ui::anim::stagger(current, 0.022);
                let appear = ui::anim::fade_in(ui.ctx(), ui.id().with(("content", current)), delay);
                ui.set_opacity(appear);
                // 在线页面：模组用 MCMod 词库里的 slug，光影与资源包用后台反查的结果
                let modrinth_slug = item
                    .modrinth_slug
                    .clone()
                    .or_else(|| app.versions_ui.content_slugs.get(&item.file_name).cloned());
                ui.allocate_ui_with_layout(
                    Vec2::new(card_width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                let response = ui::card_flat(ui, &p, |ui| {
                    // 卡片总宽 = 内容宽 + 卡片内边距（左右各 16）
                    ui.set_width(card_width - 32.0);
                    ui.horizontal(|ui| {
                        // 模组 jar 里大多自带一张图，用它当头像，没有才退回分类图标
                        let icon_key = format!(
                            "{}|{}",
                            item.path.display(),
                            item.modified
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs())
                                .unwrap_or(0)
                        );
                        let texture = item
                            .icon
                            .as_ref()
                            .and_then(|bytes| app.mod_icon_texture(&icon_key, bytes));
                        match texture {
                            Some(tex) => {
                                let size = 30.0;
                                let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
                                if ui.is_rect_visible(rect) {
                                    ui.painter().rect_filled(
                                        rect,
                                        p.corner_sm(),
                                        p.surface_alt,
                                    );
                                    ui.painter().image(
                                        tex.id(),
                                        rect.shrink(2.0),
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                        egui::Color32::WHITE,
                                    );
                                }
                            }
                            None => {
                                icons::paint(
                                    ui.ctx(),
                                    ui.painter(),
                                    kind.icon(),
                                    Rect::from_center_size(
                                        Pos2::new(ui.cursor().left() + 10.0, ui.cursor().center().y),
                                        Vec2::splat(18.0),
                                    ),
                                    p.text_weak,
                                );
                                ui.add_space(26.0);
                            }
                        }
                        ui.vertical(|ui| {
                            // 标题、版本信息、文件名分三层排开，不再挤在一起
                            // 文字过长时截断，免得把卡片撑出格子
                            ui.set_width((card_width - 96.0).max(120.0));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&item.display)
                                        .font(ui::font(13.5))
                                        .color(p.text)
                                        .strong(),
                                )
                                .truncate(),
                            );
                            ui.add_space(3.0);
                            if !item.subtitle.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&item.subtitle)
                                            .font(ui::font(11.5))
                                            .color(p.text_weak),
                                    )
                                    .truncate(),
                                );
                                ui.add_space(2.0);
                            }
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "{} · {}",
                                        item.file_name,
                                        paths::format_size(item.size)
                                    ))
                                    .font(ui::font(10.5))
                                    .color(p.text_faint),
                                )
                                .truncate(),
                            );
                        });
                        // 右上角的角标：打开所在目录、用外部浏览器看项目主页
                        let mcmod_url = item.mcmod.as_ref().map(|e| e.url());
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            // 打开所在目录
                            let folder = ui::round_icon_button(ui, &p, "folder-open", 22.0);
                            if folder.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                folder.clone().on_hover_ui(|ui| {
                                    ui.label(
                                        egui::RichText::new("打开所在目录")
                                            .font(ui::font(11.0)),
                                    );
                                });
                            }
                            if folder.clicked() {
                                paths::reveal(&item.path);
                            }
                            if modrinth_slug.is_some() || mcmod_url.is_some() {
                                let badge = ui::round_icon_button(ui, &p, "external-link", 22.0);
                                if badge.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    if let Some(slug) = &modrinth_slug {
                                        badge.clone().on_hover_ui(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "在浏览器里打开 Modrinth：{}",
                                                    slug
                                                ))
                                                .font(ui::font(11.0)),
                                            );
                                        });
                                    } else if mcmod_url.is_some() {
                                        badge.clone().on_hover_ui(|ui| {
                                            ui.label(
                                                egui::RichText::new("在浏览器里打开 MC 百科")
                                                    .font(ui::font(11.0)),
                                            );
                                        });
                                    }
                                }
                                if badge.clicked() {
                                    if let Some(slug) = &modrinth_slug {
                                        let _ = open::that(crate::core::content::modrinth::project_url(
                                            kind.modrinth_type(),
                                            slug,
                                        ));
                                    } else if let Some(url) = &mcmod_url {
                                        let _ = open::that(url);
                                    }
                                }
                            }
                        });
                    });
                    ui.add_space(5.0);
                    // 底行：左边是文件位置，右边是相关网站与外链角标
                    ui.horizontal(|ui| {
                        let path_width = (card_width - 170.0).max(60.0);
                        let path_label = ui.add_sized(
                            [path_width, 16.0],
                            egui::Label::new(
                                egui::RichText::new(item.path.display().to_string())
                                    .font(ui::font(10.0))
                                    .color(p.text_faint),
                            )
                            .truncate(),
                        );
                        if path_label.clicked() {
                            paths::reveal(&item.path);
                        }
                        if path_label.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            path_label.clone().on_hover_ui(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "点击打开所在目录：{}",
                                        item.path.display()
                                    ))
                                    .font(ui::font(11.0)),
                                );
                            });
                        }
                        // 相关网站：各自可点，点谁开谁
                        let mcmod_url = item.mcmod.as_ref().map(|e| e.url());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if let Some(url) = &mcmod_url {
                                if site_link(ui, &p, "MC 百科").clicked() {
                                    let _ = open::that(url);
                                }
                                ui.label(
                                    egui::RichText::new("·")
                                        .font(ui::font(10.5))
                                        .color(p.text_faint),
                                );
                            }
                            if let Some(slug) = &modrinth_slug {
                                if site_link(ui, &p, "Modrinth").clicked() {
                                    let _ = open::that(
                                        crate::core::content::modrinth::project_url(
                                            kind.modrinth_type(),
                                            slug,
                                        ),
                                    );
                                }
                            }
                        });
                    });
                });
                // 堆叠：卡片下面露出几层纸边
                paint_stacked_edges(ui, &p, &response.rect);
                });
                ui.set_opacity(1.0);
            }
        });
        ui.add_space(12.0);
    }
}

/// 模组自带的内容区：列出该模组目录下的文件
///
/// 机械动力的蓝图、投影的蓝图都放在游戏目录的 `blueprints` 下，
/// 模组自己实际读的是 `schematics`，两个位置都列出来。
/// 目录里可以再分子文件夹分类，这里一并列出来。
fn mod_feature_tab(
    app: &mut App,
    ui: &mut Ui,
    inst: &crate::core::minecraft::GameInstance,
    feature: &crate::core::content::modfeature::ModFeature,
) {
    let p = app.palette;
    let game_dir = app.cfg.effective_game_dir(&inst.mc_version);

    let mut files: Vec<FeatureFile> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for sub in feature.dirs {
        let dir = game_dir.join(sub);
        list_feature_files(&dir, feature.exts, &mut files);
        dirs.push(dir);
    }
    files.sort_by(|a, b| a.rel.to_lowercase().cmp(&b.rel.to_lowercase()));

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} 个{}", files.len(), feature.label))
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("打开目录").icon("folder")).clicked() {
                // 优先打开已经存在的目录，没有就建第一个
                let target = dirs
                    .iter()
                    .find(|d| d.is_dir())
                    .cloned()
                    .unwrap_or_else(|| dirs[0].clone());
                let _ = paths::ensure_dir(&target);
                paths::reveal(&target);
            }
        });
    });
    ui.add_space(10.0);

    if files.is_empty() {
        let tip = format!(
            "把文件放进 {} 即可生效，目录里可以再分子文件夹分类",
            dirs.first()
                .map(|d| d.display().to_string())
                .unwrap_or_default()
        );
        ui::empty_state(ui, &p, feature.icon, &format!("还没有{}", feature.label), &tip);
        return;
    }

    for file in &files {
        ui::card_flat(ui, &p, |ui| {
            ui.horizontal(|ui| {
                icons::paint(
                    ui.ctx(),
                    ui.painter(),
                    feature.icon,
                    Rect::from_center_size(
                        Pos2::new(ui.cursor().left() + 10.0, ui.cursor().center().y),
                        Vec2::splat(18.0),
                    ),
                    p.text_weak,
                );
                ui.add_space(28.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(&file.name)
                            .font(ui::font(13.0))
                            .color(p.text)
                            .strong(),
                    );
                    // 副标题带上子文件夹，看得出分类
                    let sub = if file.rel.contains('/') {
                        format!("{} · ", file.rel)
                    } else {
                        String::new()
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "{}{} · {}",
                            sub,
                            file.ext,
                            paths::format_size(file.size)
                        ))
                        .font(ui::font(11.0))
                        .color(p.text_faint),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::ghost(ui, &p, "打开位置").clicked() {
                        paths::reveal(&file.path);
                    }
                });
            });
        });
        ui.add_space(6.0);
    }
}

/// 模组内容目录里的一个文件
struct FeatureFile {
    name: String,
    /// 相对目录的路径，含子文件夹
    rel: String,
    path: PathBuf,
    size: u64,
    ext: String,
}

/// 递归列出目录里指定后缀的文件
///
/// 子文件夹也要进去，这样用户可以自己按分类整理蓝图。
fn list_feature_files(root: &Path, exts: &[&str], out: &mut Vec<FeatureFile>) {
    collect_feature_files(root, root, exts, 0, out);
}

/// 递归收集一层，最多往下走 4 层
fn collect_feature_files(
    root: &Path,
    dir: &Path,
    exts: &[&str],
    depth: usize,
    out: &mut Vec<FeatureFile>,
) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_feature_files(root, &path, exts, depth + 1, out);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !exts.contains(&ext.as_str()) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        out.push(FeatureFile {
            name,
            rel,
            path,
            size,
            ext,
        });
    }
}

/// 后台用文件名反查 Modrinth 项目，结果回来后再显示官网链接
fn lookup_content_slugs(app: &mut App, names: Vec<String>) {
    if names.is_empty() {
        return;
    }
    let source = app.cfg.download.source;
    let timeout = app.cfg.download.timeout_secs;
    app.spawn("查找项目页面", false, move |job| {
        let result = (|| -> anyhow::Result<std::collections::HashMap<String, String>> {
            let dl = crate::core::download::Downloader::new(source, timeout.max(20))?;
            let mut found = std::collections::HashMap::new();
            for name in &names {
                let guess = guess_project_name(name);
                for candidate in slug_candidates(&guess) {
                    if let Ok(Some(project)) =
                        crate::core::content::modrinth::find_by_slug(&dl, &candidate)
                    {
                        found.insert(name.clone(), project.slug.clone());
                        break;
                    }
                }
            }
            Ok(found)
        })();
        if let Ok(map) = result {
            job.send(CoreEvent::ContentSlugs(map));
        }
        job.done(true, "");
    });
}

// ------------------------------------------------------------------ 重命名 / 复制 / 删除

fn rename_modal(app: &mut App, ui: &mut Ui) {
    let Some(old_id) = app.versions_ui.rename_target.clone() else {
        return;
    };
    let p = app.palette;
    let mut open = true;
    let mut confirm = false;
    let ctx = ui.ctx().clone();
    let title = format!("重命名 {old_id}");

    widgets::modal(&ctx, &p, &mut open, &title, 420.0, |ui| {
        ui.label(
            egui::RichText::new("新名称只影响版本目录与显示，不会改变游戏内容。")
                .font(ui::font(12.0))
                .color(p.text_weak),
        );
        ui.add_space(10.0);
        let mut name = app.versions_ui.rename_value.clone();
        if ui::text_field(ui, &p, &mut name, "版本名称", 360.0, false).changed() {
            app.versions_ui.rename_value = name;
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::primary("重命名")).clicked() {
                    confirm = true;
                }
                if ui::button(ui, &p, ButtonOpts::secondary("取消")).clicked() {
                    app.versions_ui.rename_target = None;
                }
            });
        });
    });

    if confirm {
        let new_id = app.versions_ui.rename_value.trim().to_string();
        match rename_version(app, &old_id, &new_id) {
            Ok(()) => {
                let new_mc_version = app
                    .local_versions
                    .iter()
                    .find(|v| v.id == new_id)
                    .map(|v| v.mc_version.clone())
                    .unwrap_or_else(|| new_id.clone());
                app.versions_ui.detail = Some(new_mc_version);
                app.success(format!("已重命名为 {new_id}"));
            }
            Err(err) => app.error(format!("重命名失败：{err}")),
        }
        app.versions_ui.rename_target = None;
    }
    if !open {
        app.versions_ui.rename_target = None;
    }
}

fn rename_version(app: &mut App, old_id: &str, new_id: &str) -> anyhow::Result<()> {
    use anyhow::anyhow;
    if new_id.is_empty() {
        return Err(anyhow!("名称不能为空"));
    }
    if new_id.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(anyhow!("名称不能包含路径分隔符或特殊字符"));
    }
    let old_dir = version::version_dir(&app.cfg.game_dir, old_id);
    let new_dir = version::version_dir(&app.cfg.game_dir, new_id);
    if new_dir.exists() {
        return Err(anyhow!("同名版本已存在"));
    }
    if !old_dir.is_dir() {
        return Err(anyhow!("版本目录不存在"));
    }

    std::fs::rename(&old_dir, &new_dir)?;
    let old_json = new_dir.join(format!("{old_id}.json"));
    let new_json = new_dir.join(format!("{new_id}.json"));
    if old_json.is_file() {
        let mut json: crate::core::minecraft::VersionJson =
            serde_json::from_str(&std::fs::read_to_string(&old_json)?)?;
        json.id = new_id.to_string();
        std::fs::write(&new_json, serde_json::to_string_pretty(&json)?)?;
        let _ = std::fs::remove_file(&old_json);
    }
    let old_jar = new_dir.join(format!("{old_id}.jar"));
    if old_jar.is_file() {
        let _ = std::fs::rename(&old_jar, new_dir.join(format!("{new_id}.jar")));
    }

    if app.cfg.selected_version.as_deref() == Some(old_id) {
        app.cfg.selected_version = Some(new_id.to_string());
    }

    if let Some(stat) = app.cfg.version_stats.remove(old_id) {
        app.cfg.version_stats.insert(new_id.to_string(), stat);
    }
    app.save_config();
    app.refresh_local_versions();
    Ok(())
}

fn copy_version(app: &mut App, id: &str) {
    let src = version::version_dir(&app.cfg.game_dir, id);
    let mut index = 1;
    let new_id = loop {
        let candidate = format!("{id}-copy{index}");
        if !version::version_dir(&app.cfg.game_dir, &candidate).exists() {
            break candidate;
        }
        index += 1;
    };
    let dst = version::version_dir(&app.cfg.game_dir, &new_id);

    let result = (|| -> anyhow::Result<()> {
        copy_dir_recursive(&src, &dst)?;
        let old_json = dst.join(format!("{id}.json"));
        let new_json = dst.join(format!("{new_id}.json"));
        if old_json.is_file() {
            let mut json: crate::core::minecraft::VersionJson =
                serde_json::from_str(&std::fs::read_to_string(&old_json)?)?;
            json.id = new_id.clone();
            std::fs::write(&new_json, serde_json::to_string_pretty(&json)?)?;
            let _ = std::fs::remove_file(&old_json);
        }
        let old_jar = dst.join(format!("{id}.jar"));
        if old_jar.is_file() {
            let _ = std::fs::rename(&old_jar, dst.join(format!("{new_id}.jar")));
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            app.refresh_local_versions();
            let new_mc_version = app
                .local_versions
                .iter()
                .find(|v| v.id == new_id)
                .map(|v| v.mc_version.clone())
                .unwrap_or_else(|| new_id.clone());
            app.versions_ui.detail = Some(new_mc_version.clone());
            app.success(format!("已复制为 {new_id}"));
        }
        Err(err) => app.error(format!("复制失败：{err}")),
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let relative = path.strip_prefix(src).unwrap_or(path);
        let out = dst.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &out)?;
        }
    }
    Ok(())
}

fn delete_modal(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.versions_ui.confirm_delete.clone() else {
        return;
    };
    let p = app.palette;
    let mut open = true;
    let mut confirmed = false;
    let ctx = ui.ctx().clone();
    let title = format!("删除 {id}");

    widgets::modal(&ctx, &p, &mut open, &title, 440.0, |ui| {
        ui.label(
            egui::RichText::new("该版本目录下的存档、模组与配置会一并删除，且无法恢复。")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::danger("确认删除")).clicked() {
                    confirmed = true;
                }
                if ui::button(ui, &p, ButtonOpts::secondary("取消")).clicked() {
                    app.versions_ui.confirm_delete = None;
                }
            });
        });
    });

    if confirmed {
        let dir = version::version_dir(&app.cfg.game_dir, &id);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {
                app.success(format!("已删除 {id}"));
                if app.cfg.selected_version.as_deref() == Some(id.as_str()) {
                    app.cfg.selected_version = None;
                }
                app.cfg.version_overrides.remove(&id);
                app.cfg.version_stats.remove(&id);
                app.save_config();
                app.versions_ui.detail = None;
                app.refresh_local_versions();
            }
            Err(err) => app.error(format!("删除失败：{err}")),
        }
        app.versions_ui.confirm_delete = None;
    }
    if !open {
        app.versions_ui.confirm_delete = None;
    }
}
