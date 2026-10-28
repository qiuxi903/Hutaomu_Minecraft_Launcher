//! 版本管理：一级是所有版本列表，点进去进入该版本的二级页面。
//!
//! 二级页面左侧是导航（概览 / 设置 / 存档 / 截图 / 模组），右侧是内容。
//! 二级页面里的设置只对当前版本生效，会覆盖全局设置。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::path::{Path, PathBuf};

use egui::{Align, Color32, Layout, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::{App, CoreEvent, Page};
use crate::core::minecraft::loaders::{self, LoaderKind, LoaderVersion};
use crate::core::minecraft::shader_loader::{self, ShaderLoader};
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
    /// 本地文件名 → Modrinth 上的对应项目（后台反查得到，用于跳转官网与显示图标）
    pub content_slugs: HashMap<String, crate::core::content::RemoteProject>,
    /// 已经发起过 Modrinth 反查的本地内容，键是「分类|文件名」
    pub content_looked_up: HashSet<String>,
    /// 本地文件名 → CurseForge 上的对应项目（用文件指纹反查得到）
    pub content_cf: HashMap<String, crate::core::content::RemoteProject>,
    /// 已经发起过 CurseForge 指纹反查的本地内容
    pub content_cf_looked_up: HashSet<String>,
    /// 内容列表每行放几个（1 或 2）
    pub content_columns: usize,
    pub detail_memory: f32,
    /// 内存分配方式：global / auto / custom
    pub detail_memory_mode: String,
    /// 初始内存 -Xms
    pub detail_initial_memory: f32,
    pub detail_java: usize,
    pub detail_jvm: String,
    /// 额外的游戏启动参数
    pub detail_extra_args: String,
    /// 游戏窗口标题
    pub detail_title: String,
    pub detail_dir: String,
    /// 该版本是否启用版本隔离
    pub detail_isolation: bool,
    /// 「更换加载器」弹窗的目标实例
    pub loader_modal: Option<String>,
    /// 弹窗里选中的加载器类型
    pub loader_kind: Option<crate::core::minecraft::loaders::LoaderKind>,
    /// 弹窗里加载器版本是否自动选择
    pub loader_auto: bool,
    /// 手动选择时可用的加载器版本
    pub loader_versions: Vec<crate::core::minecraft::loaders::LoaderVersion>,
    pub loader_version_index: usize,
    pub loader_loading: bool,
    pub loader_error: Option<String>,
    /// 上面这份版本列表对应的游戏版本
    pub loader_mc: String,
    /// 「更换光影加载器」弹窗的目标实例
    pub shader_modal: Option<String>,
    /// 导出整合包时勾了哪些内容
    pub export_options: crate::core::minecraft::export::ExportOptions,
    /// 导出文件的路径
    pub export_path: String,
    pub confirm_delete: Option<String>,
    pub rename_target: Option<String>,
    pub rename_value: String,
    /// 「新建实例」弹窗：以哪个实例为基础
    pub new_instance_from: Option<String>,
    /// 新实例的名字
    pub new_instance_name: String,
    /// 新实例的目录名
    pub new_instance_id: String,
    pub screenshots: HashMap<PathBuf, egui::TextureHandle>,
    pub loaded_for: Option<String>,
    /// 本地内容列表的缓存
    ///
    /// 目录没变就用上次的列表，不用每帧重扫一遍，
    /// 整合包上百个模组时差别很大。
    pub content_cache: HashMap<String, (Option<std::time::SystemTime>, Arc<Vec<LocalContent>>)>,
    /// 每个本地内容的更新状态，键是「分类|文件名」
    pub content_updates: HashMap<String, ContentUpdateState>,
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
            content_looked_up: HashSet::new(),
            content_cf: HashMap::new(),
            content_cf_looked_up: HashSet::new(),
            content_columns: 2,
            detail_memory: 4096.0,
            detail_memory_mode: "global".to_string(),
            detail_initial_memory: 0.0,
            detail_java: 0,
            detail_jvm: String::new(),
            detail_extra_args: String::new(),
            detail_title: String::new(),
            detail_dir: String::new(),
            detail_isolation: true,
            loader_modal: None,
            loader_kind: None,
            loader_auto: true,
            loader_versions: Vec::new(),
            loader_version_index: 0,
            loader_loading: false,
            loader_error: None,
            loader_mc: String::new(),
            shader_modal: None,
            export_options: crate::core::minecraft::export::ExportOptions::default(),
            export_path: String::new(),
            confirm_delete: None,
            rename_target: None,
            rename_value: String::new(),
            new_instance_from: None,
            new_instance_name: String::new(),
            new_instance_id: String::new(),
            screenshots: HashMap::new(),
            loaded_for: None,
            content_cache: HashMap::new(),
            content_updates: HashMap::new(),
        }
    }
}

impl VersionsState {
    /// 收到「更换加载器」弹窗的版本列表
    ///
    /// 游戏版本或加载器类型变了就丢弃过期结果，
    /// 不然切换加载器后看到的还是上一个的版本。
    pub fn on_loader_versions(
        &mut self,
        kind: crate::core::minecraft::loaders::LoaderKind,
        mc: &str,
        result: Result<Vec<crate::core::minecraft::loaders::LoaderVersion>, String>,
    ) {
        if self.loader_mc != mc || self.loader_kind != Some(kind) {
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
        ("loaders", "加载器", "loader"),
        ("export", "导出", "upload"),
        ("saves", "存档", "world"),
        ("screenshots", "截图", "image"),
    ];
    let has_loader = inst.variants.iter().any(|v| v.loader.is_some());
    if has_loader {
        list.push(("mod", "模组", "mod"));
        list.push(("shader", "光影", "shader"));
    }
    list.push(("resourcepack", "资源包", "resourcepack"));
    list.push(("datapack", "数据包", "datapack"));
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
    shader_modal(app, ui);
    new_instance_modal(app, ui);
}

// ------------------------------------------------------------------ 一级：版本列表

fn list_page(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(ui, &p, "版本管理", "");

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
                    || inst.instance_id.to_lowercase().contains(&keyword)
                || inst.display_name.to_lowercase().contains(&keyword)
                // 用户改过的名字也能搜到
                || app
                    .cfg
                    .instance_meta(&inst.instance_id)
                    .name
                    .to_lowercase()
                    .contains(&keyword)
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
    let card_width = (width - 16.0 * (columns as f32 - 1.0)) / columns as f32;

    let mut open: Option<String> = None;
    let mut index = 0usize;
    for chunk in instances.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            for inst in chunk {
                // 卡片逐个淡入，网格不会整片糊上来
                let delay = ui::anim::stagger(index, 0.035);
                let appear = ui::anim::fade_in(ui.ctx(), ui.id().with(("inst", index)), delay);
                ui.set_opacity(appear);
                index += 1;
                let is_current = app
                    .instances
                    .iter()
                    .find(|i| i.instance_id == inst.instance_id)
                    .and_then(|i| i.active_version())
                    .map(|v| app.cfg.selected_version.as_deref() == Some(v.id.as_str()))
                    .unwrap_or(false);
                // 版本目录里有自定义图标就用它，没有再按版本分类给一个
                let icon_texture = inst
                    .custom_icon
                    .as_ref()
                    .and_then(|path| app.file_icon_texture(path));
                // 用户改过名字就用他改的
                let display_name = app.instance_display_name(inst);
                let response =
                    instance_card(ui, &p, inst, &display_name, card_width, is_current, icon_texture);
                let clickable = response.interact(Sense::click());
                if clickable.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if clickable.clicked() {
                    open = Some(inst.instance_id.clone());
                }
                ui.set_opacity(1.0);
            }
        });
        ui.add_space(16.0);
    }

    if let Some(id) = open {
        open_detail(app, &id);
    }
}

/// 单个实例卡片
///
/// 整张卡片用 painter 绘制：egui 的 Label 即使设了 truncate，测量宽度仍是原文，
/// 版本号一长就会把整行布局撑开，所以这里自己截断文本。
///
/// 版面分三层：图标加标题、副信息、底部的占用与时间。
/// 中间用一条细线把「这是什么」和「占了多大」分开，扫一眼就能看完一张卡。
fn instance_card(
    ui: &mut Ui,
    p: &crate::theme::Palette,
    inst: &crate::core::minecraft::GameInstance,
    display_name: &str,
    card_width: f32,
    is_current: bool,
    icon_texture: Option<egui::TextureHandle>,
) -> egui::Response {
    const CARD_HEIGHT: f32 = 116.0;
    const PAD: f32 = 14.0;
    const ICON_SIZE: f32 = 38.0;
    const TITLE_SIZE: f32 = 14.5;
    const META_SIZE: f32 = 12.0;
    const FOOT_SIZE: f32 = 11.5;

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(card_width, CARD_HEIGHT), Sense::click());
    // 悬停时轻微上浮并把边框染上主色，让人知道这张卡能点
    let hovered = response.hovered();
    // 上浮与染色都走过渡，不要啪地一下跳过去
    let hover_t = crate::ui::anim::to_bool(
        ui.ctx(),
        response.id.with("hov"),
        hovered,
        crate::ui::anim::DUR_BASE,
    );
    let lift = hover_t * 2.0;
    let rect = rect.translate(Vec2::new(0.0, -lift));
    let border = if is_current {
        p.accent
    } else {
        widgets::lerp_color(p.border, p.accent, hover_t * 0.55)
    };
    ui.painter().rect_filled(
        rect,
        p.corner_sm(),
        widgets::lerp_color(p.surface, p.surface_alt, hover_t * 0.6),
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
                icon_rect.shrink(5.0),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            icons::paint(
                ui.ctx(),
                ui.painter(),
                inst.category.icon(),
                icon_rect.shrink(4.0),
                p.text_weak,
            );
        }
    }

    // 标题：自行截断，保证不超出卡片
    let text_x = icon_rect.right() + 12.0;
    // 右上角要留给「当前使用」标记，标题不能压到它
    let avail = (rect.right() - PAD - text_x - if is_current { 58.0 } else { 0.0 }).max(40.0);
    let title = widgets::ellipsize(display_name, avail, TITLE_SIZE);
    ui.painter().text(
        Pos2::new(text_x, icon_rect.top() + 10.0),
        egui::Align2::LEFT_CENTER,
        title,
        ui::font(TITLE_SIZE),
        p.text,
    );

    // 副信息：加载器 · 变体数
    let mut meta: Vec<String> = vec![inst.loader_label()];
    // 版本分类，正式版就不用重复写了
    if inst.category != crate::core::minecraft::VersionCategory::Release {
        meta.push(inst.category.label().to_string());
    }
    if inst.variant_count() > 1 {
        meta.push(format!("{} 个变体", inst.variant_count()));
    }
    ui.painter().text(
        Pos2::new(text_x, icon_rect.top() + 32.0),
        egui::Align2::LEFT_CENTER,
        widgets::ellipsize(&meta.join(" · "), avail, META_SIZE),
        ui::font(META_SIZE),
        p.text_weak,
    );

    // 当前使用中的实例在右上角标一下，跟别的卡一眼能分开
    if is_current {
        ui.painter().text(
            Pos2::new(rect.right() - PAD, icon_rect.top() + 10.0),
            egui::Align2::RIGHT_CENTER,
            "当前使用",
            ui::font(11.5),
            p.accent,
        );
    }

    // 分隔线：把「是什么」和「占了多大」隔开
    let divider_y = icon_rect.bottom() + 14.0;
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.left() + PAD, divider_y),
            Pos2::new(rect.right() - PAD, divider_y + 1.0),
        ),
        egui::CornerRadius::ZERO,
        p.border,
    );

    // 底部：左边占用，右边最后修改时间（坐标相对卡片，不能用屏幕坐标）
    let foot_y = rect.bottom() - PAD;
    ui.painter().text(
        Pos2::new(rect.left() + PAD, foot_y),
        egui::Align2::LEFT_BOTTOM,
        paths::format_size(inst.size()),
        ui::font(FOOT_SIZE),
        p.text_faint,
    );
    let modified = inst
        .last_modified()
        .map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%m-%d %H:%M").to_string()
        })
        .unwrap_or_else(|| "未知时间".into());
    ui.painter().text(
        Pos2::new(rect.right() - PAD, foot_y),
        egui::Align2::RIGHT_BOTTOM,
        modified,
        ui::font(FOOT_SIZE),
        p.text_faint,
    );

    response
}

/// 重新安装整合包
///
/// 装坏了的实例（缺加载器、版本描述不对）或者整合包更新了，
/// 都可以在这里重装一次。已经下过的文件会跳过，不会重新下几百兆。
fn reinstall_modpack(app: &mut App, instance_id: &str) {
    let dir = version::version_dir(&app.cfg.game_dir, instance_id);
    let Some(info) = crate::core::minecraft::modpack::read_marker(&dir) else {
        app.error("这个实例没有整合包信息，无法重新安装");
        return;
    };
    let (Some(project_id), Some(version_id)) =
        (info.project_id.clone(), info.pack_version_id.clone())
    else {
        app.error("这个整合包没有记录来源与版本，无法重新安装");
        return;
    };
    let name = info.name.clone();
    let game_dir = app.cfg.game_dir.clone();
    let opts = app.cfg.download.install_options();
    let java = app.java_for(&info.mc_version);
    let target = instance_id.to_string();
    // 重新安装要回到整合包原来的平台去取文件
    let source = crate::core::content::ContentSource::from_id(info.source.as_deref().unwrap_or(""));

    app.spawn_download(format!("重新安装 {name}"), move |dl, progress, job| {
        let versions = crate::core::content::versions(&dl, source, &project_id, None, None)
            .unwrap_or_default();
        let Some(version) = versions.into_iter().find(|v| v.id == version_id) else {
            return Err(anyhow::anyhow!("找不到整合包对应的版本"));
        };
        let installed = crate::core::content::modpack::install_instance(
            &dl,
            source,
            &project_id,
            &name,
            &target,
            &version,
            &game_dir,
            java,
            &progress,
            opts,
            Some(target.clone()),
        )?;
        job.send(CoreEvent::Installed(installed.instance_id.clone()));
        Ok(format!("{name} 已重新安装"))
    });
}

// ------------------------------------------------------------------ 二级：版本详情

/// 打开某个实例的详情页
pub(crate) fn open_detail(app: &mut App, instance_id: &str) {
    app.versions_ui.detail = Some(instance_id.to_string());
    app.versions_ui.section = "info";
    load_detail(app, instance_id);
}

fn detail_page(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let Some(instance_id) = app.versions_ui.detail.clone() else {
        return;
    };
    let Some(inst) = app.instances.iter().find(|i| i.instance_id == instance_id).cloned() else {
        // 版本已被删除，回到列表
        app.versions_ui.detail = None;
        return;
    };
    if app.versions_ui.loaded_for.as_deref() != Some(instance_id.as_str()) {
        load_detail(app, &instance_id);
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
        .get(&inst.instance_id)
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
    let mut reinstall: Option<String> = None;


    ui::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(54.0), Sense::hover());
            ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
            match icon_texture.as_ref() {
                Some(texture) => {
                    ui.painter().image(
                        texture.id(),
                        rect.shrink(7.0),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    icons::paint(
                        ui.ctx(),
                        ui.painter(),
                        inst.category.icon(),
                        rect.shrink(6.0),
                        p.text_weak,
                    );
                }
            }
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(app.instance_display_name(&inst))
                            .font(ui::font(20.0))
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
                ui.add_space(5.0);
                // 路径不放这里，概览里也不摆，要看目录点「打开目录」
                let mut parts: Vec<String> = Vec::new();
                if inst.variant_count() > 1 {
                    parts.push(format!("{} 个变体", inst.variant_count()));
                }
                if let Some(loader_version) = &active.loader_version {
                    parts.push(loader_version.clone());
                }
                let meta = parts.join(" · ");
                ui.label(
                    egui::RichText::new(meta)
                        .font(ui::font(12.5))
                        .color(p.text_faint),
                );
                // 用户自己写的备注
                let meta_info = app.cfg.instance_meta(&inst.instance_id);
                if !meta_info.description.trim().is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(meta_info.description.trim())
                            .font(ui::font(12.5))
                            .color(p.text_weak),
                    );
                }
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
                // 整合包可以重新装一次：装坏了的实例（缺加载器、版本描述不对）
                // 或者整合包更新了，都从这里重来
                if inst.is_modpack
                    && ui::button(ui, &p, ButtonOpts::secondary("重新安装").icon("refresh"))
                        .clicked()
                {
                    reinstall = Some(inst.instance_id.clone());
                }
                if ui::button(ui, &p, ButtonOpts::secondary("打开目录").icon("folder")).clicked() {
                    views::open_version_dir(app, &inst.instance_id);
                }
                // 同一个版本可以装多个实例，各自一份模组与存档
                if ui::button(ui, &p, ButtonOpts::secondary("新建实例").icon("plus")).clicked() {
                    start_new_instance(app, &inst.instance_id);
                }
            });
        });
        if let Some(id) = reinstall {
            reinstall_modpack(app, &id);
        }
    });
    ui.add_space(16.0);

    // ---------- 左侧导航 + 右侧内容 ----------
    //
    // 顶部的返回按钮与版本信息、左边的导航都钉在原地，
    // 只有右边的内容滚动。外层已经不再套滚动条了。
    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        ui.vertical(|ui| {
            ui.set_width(172.0);
            // 导航项会随安装的模组变多，装不下时滚动，别把后面的项挤出去
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(height)
                .show(ui, |ui| {
                    ui::card_padded(ui, &p, 10, |ui| {
                        ui.set_width(152.0);
                        // 项与项之间只留 egui 自己的间距，不再额外加，
                        // 十来项才装得下，不用动不动就多一条滚动条
                        ui.spacing_mut().item_spacing.y = 6.0;
                        for (id, label, icon) in sections.iter() {
                            let selected = app.versions_ui.section == *id;
                            if ui::nav_item(ui, &p, icon, label, selected).clicked() {
                                app.versions_ui.section = id;
                            }
                        }
                    });
                });
        });
        ui.vertical(|ui| {
            let width = ui.available_width();
            // 换子标签时内容淡入，不是一下子跳过去
            let section = app.versions_ui.section;
            let appear = ui::anim::fade_in(ui.ctx(), ui.id().with(("section", &section)), 0.0);
            ui.set_opacity(appear);
            // 内容自己滚，左边的导航与顶部不跟着走
            egui::ScrollArea::vertical()
                // 带上分区名，换分区时滚动位置回到顶部
                .id_salt(("version_detail", section))
                .auto_shrink([false, false])
                .max_height(height)
                .show(ui, |ui| {
                    ui.set_width(width);
                    match app.versions_ui.section {
                        "info" => crate::views::instance::overview_tab(app, ui, &inst, &active),
                        "settings" => crate::views::instance::setup_tab(app, ui, &inst),
                        "loaders" => {
                            crate::views::instance::loaders_tab(app, ui, &inst, &active)
                        }
                        "export" => crate::views::instance::export_tab(app, ui, &inst, &active),
                        "saves" => saves_tab(app, ui, &inst),
                        "screenshots" => screenshots_tab(app, ui, &inst),
                        "mod" => content_tab(app, ui, &inst, ContentKind::Mod),
                        "shader" => content_tab(app, ui, &inst, ContentKind::Shader),
                        "resourcepack" => content_tab(app, ui, &inst, ContentKind::ResourcePack),
                        "datapack" => content_tab(app, ui, &inst, ContentKind::DataPack),
                        section => match features.iter().find(|f| f.section == section) {
                            Some(feature) => mod_feature_tab(app, ui, &inst, feature),
                            None => crate::views::instance::overview_tab(app, ui, &inst, &active),
                        },
                    }
                });
            ui.set_opacity(1.0);
        });
    });
}

pub(crate) fn version_type_label(version_type: &str) -> &'static str {
    match version_type {
        "release" => "正式版",
        "snapshot" => "快照",
        "old_beta" => "远古 Beta",
        "old_alpha" => "远古 Alpha",
        _ => "自定义",
    }
}

// ------------------------------------------------------------------ 更换加载器

/// 打开加载器页
///
/// 顺便把加载器类型挑成这个版本支持的那个，
/// 省得用户进来还要自己点一下。
pub(crate) fn open_loaders_page(app: &mut App, mc_version: &str) {
    let kinds = LoaderKind::all();
    app.versions_ui.loader_kind = kinds
        .iter()
        .copied()
        .find(|k| loaders::supports(*k, mc_version))
        .or(Some(kinds[0]));
    app.versions_ui.loader_auto = true;
    app.versions_ui.loader_versions.clear();
    app.versions_ui.loader_version_index = 0;
    app.versions_ui.loader_error = None;
    app.versions_ui.loader_mc.clear();
    app.versions_ui.loader_loading = false;
    app.versions_ui.section = "loaders";
}

/// 加载器版本下拉框
pub(crate) fn loader_version_picker(
    app: &mut App,
    ui: &mut Ui,
    mc: &str,
    kind: LoaderKind,
) -> Option<String> {
    if app.versions_ui.loader_loading {
        ui.horizontal(|ui| {
            ui::spinner(ui, &app.palette, 14.0);
            ui.label(
                egui::RichText::new("正在获取加载器版本 ...")
                    .font(ui::font(12.0))
                    .color(app.palette.text_weak),
            );
        });
        return None;
    }
    if let Some(err) = app.versions_ui.loader_error.clone() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&err)
                    .font(ui::font(12.0))
                    .color(app.palette.danger),
            );
            if ui::ghost(ui, &app.palette, "重试").clicked() {
                load_loader_versions(app, mc, kind);
            }
        });
        return None;
    }
    if app.versions_ui.loader_versions.is_empty() {
        load_loader_versions(app, mc, kind);
        ui.label(
            egui::RichText::new("正在获取加载器版本 ...")
                .font(ui::font(12.0))
                .color(app.palette.text_faint),
        );
        return None;
    }
    let labels: Vec<String> = app
        .versions_ui
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
        .get(app.versions_ui.loader_version_index)
        .cloned()
        .unwrap_or_default();
    if let Some(index) = ui::dropdown(
        ui,
        &app.palette,
        "detail_loader_version",
        &current,
        &labels,
        320.0,
    ) {
        app.versions_ui.loader_version_index = index;
    }
    app.versions_ui
        .loader_versions
        .get(app.versions_ui.loader_version_index)
        .map(|v| v.version.clone())
}

/// 当前手动选中的加载器版本
pub(crate) fn picked_loader_version(app: &App) -> Option<String> {
    app.versions_ui
        .loader_versions
        .get(app.versions_ui.loader_version_index)
        .map(|v| v.version.clone())
}

/// 后台获取加载器版本列表
pub(crate) fn load_loader_versions(app: &mut App, mc: &str, kind: LoaderKind) {
    if app.versions_ui.loader_loading {
        return;
    }
    if app.versions_ui.loader_mc == mc && !app.versions_ui.loader_versions.is_empty() {
        return;
    }
    app.versions_ui.loader_loading = true;
    app.versions_ui.loader_error = None;
    app.versions_ui.loader_mc = mc.to_string();
    app.versions_ui.loader_kind = Some(kind);
    let source = app.cfg.download.source;
    let timeout = app.cfg.download.timeout_secs;
    let mc = mc.to_string();
    app.spawn(format!("获取 {} 版本列表", kind.label()), false, move |job| {
        let result = (|| -> anyhow::Result<Vec<LoaderVersion>> {
            let dl = crate::core::download::Downloader::new(source, timeout)?;
            loaders::list_versions(&dl, kind, &mc)
        })();
        job.send(CoreEvent::DetailLoaderVersions {
            kind,
            mc: mc.clone(),
            result: result.map_err(|e| format!("{e}")),
        });
        job.done(true, "");
    });
}

/// 给实例装一个加载器变体，装完切过去
pub(crate) fn install_loader(
    app: &mut App,
    instance_id: &str,
    kind: LoaderKind,
    version: Option<String>,
) {
    let Some(inst) = app.instances.iter().find(|i| i.instance_id == instance_id).cloned() else {
        app.error("找不到这个实例");
        return;
    };
    let mc = inst.real_mc_version();
    let game_dir = app.cfg.game_dir.clone();
    let java = app.java_for(&mc);
    let opts = app.cfg.download.install_options();
    let policy = app.cfg.download.loader_policy();
    let instance_id = instance_id.to_string();
    app.spawn_download(format!("安装 {} 加载器", kind.label()), move |dl, progress, job| {
        let version = match version {
            Some(v) => v,
            None => {
                let list = loaders::list_versions(&dl, kind, &mc)?;
                loaders::pick_version(&list, policy).ok_or_else(|| {
                    anyhow::anyhow!(
                        "没有找到适用于 Minecraft {} 的 {} 加载器",
                        mc,
                        kind.label()
                    )
                })?
            }
        };
        let id = loaders::install_variant(
            &dl,
            kind,
            &mc,
            &version,
            &game_dir,
            java.as_ref(),
            &progress,
            opts,
        )?;
        job.send(CoreEvent::LoaderSwitched {
            version_id: id.clone(),
            instance_id: instance_id.clone(),
        });
        Ok(format!("{mc} 的 {} {version} 已装好", kind.label()))
    });
}

// ------------------------------------------------------------------ 更换光影加载器

/// 「更换光影加载器」弹窗
fn shader_modal(app: &mut App, ui: &mut Ui) {
    let Some(instance_id) = app.versions_ui.shader_modal.clone() else {
        return;
    };
    let Some(inst) = app.instances.iter().find(|i| i.instance_id == instance_id).cloned() else {
        app.versions_ui.shader_modal = None;
        return;
    };
    let Some(active) = inst.active_version().cloned() else {
        app.versions_ui.shader_modal = None;
        return;
    };
    let loader = active
        .loader
        .as_deref()
        .and_then(|l| LoaderKind::from_id(&l.to_lowercase()));
    let p = app.palette;
    let mut open = true;
    let action: std::cell::Cell<Option<ShaderLoader>> = std::cell::Cell::new(None);
    let cancel = std::cell::Cell::new(false);
    let ctx = ui.ctx().clone();
    let game_dir = app.cfg.effective_game_dir(&instance_id);
    let current = shader_loader::detect(&game_dir);
    let options: Vec<ShaderLoader> = ShaderLoader::all()
        .into_iter()
        .filter(|s| s.works_with(loader))
        .collect();
    let title = format!("光影加载器 · {}", inst.display_name);

    widgets::modal(&ctx, &p, &mut open, &title, 520.0, |ui| {
        ui.label(
            egui::RichText::new("需要光影加载器才能用")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(16.0);
        if options.is_empty() {
            ui.label(
                egui::RichText::new("当前加载器没有可用的光影加载器")
                    .font(ui::font(12.5))
                    .color(p.warning),
            );
            ui.add_space(16.0);
        }
        for kind in options {
            let installed = current.as_ref().filter(|c| c.kind == kind);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(kind.label())
                            .font(ui::font(14.0))
                            .color(p.text)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(kind.description())
                            .font(ui::font(11.5))
                            .color(p.text_faint),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if installed.is_some() {
                        ui.label(
                            egui::RichText::new("已安装")
                                .font(ui::font(12.0))
                                .color(p.accent),
                        );
                    } else if ui::button(ui, &p, ButtonOpts::secondary("安装")).clicked() {
                        action.set(Some(kind));
                    }
                });
            });
            ui.add_space(14.0);
        }
        ui.horizontal(|ui| {
            if let Some(cur) = &current {
                if ui::button(ui, &p, ButtonOpts::danger("卸载当前光影加载器").icon("trash"))
                    .clicked()
                {
                    let file = cur.file.clone();
                    uninstall_shader(app, &file);
                }
            }
            if ui::button(ui, &p, ButtonOpts::secondary("关闭")).clicked() {
                cancel.set(true);
            }
        });
    });

    if cancel.get() {
        open = false;
    }
    if !open {
        app.versions_ui.shader_modal = None;
    }
    if let Some(kind) = action.take() {
        install_shader_loader(app, &instance_id, kind);
    }
}

/// 装一个光影加载器
fn install_shader_loader(app: &mut App, instance_id: &str, kind: ShaderLoader) {
    let Some(inst) = app.instances.iter().find(|i| i.instance_id == instance_id).cloned() else {
        app.error("找不到这个实例");
        return;
    };
    let Some(active) = inst.active_version().cloned() else {
        return;
    };
    let Some(loader) = active
        .loader
        .as_deref()
        .and_then(|l| LoaderKind::from_id(&l.to_lowercase()))
    else {
        app.error("原版不带光影支持，先装一个加载器");
        return;
    };
    let mc = inst.real_mc_version();
    let game_dir = app.cfg.effective_game_dir(instance_id);
    app.spawn_download(format!("安装 {}", kind.label()), move |dl, progress, _job| {
        let (path, extra) = shader_loader::install(&dl, kind, &mc, loader, &game_dir, &progress)?;
        Ok(match extra {
            0 => format!("{} 已安装到 {}", kind.label(), path.display()),
            _ => format!(
                "{} 已安装到 {}（连同 {} 个前置依赖）",
                kind.label(),
                path.display(),
                extra
            ),
        })
    });
}

/// 卸掉光影加载器
pub(crate) fn uninstall_shader(app: &mut App, file: &Path) {
    match std::fs::remove_file(file) {
        Ok(()) => app.success("光影加载器已卸载"),
        Err(err) => app.error(format!("卸载失败：{err}")),
    }
}

pub(crate) fn load_detail(app: &mut App, instance_id: &str) {
    let ov = app.cfg.version_override(instance_id);
    app.versions_ui.detail_memory = ov.memory_mb.unwrap_or(app.cfg.java.memory_mb) as f32;
    app.versions_ui.detail_memory_mode = ov
        .memory_mode
        .clone()
        .unwrap_or_else(|| "global".to_string());
    app.versions_ui.detail_initial_memory = ov
        .initial_memory_mb
        .unwrap_or(app.cfg.launch.initial_memory_mb) as f32;
    app.versions_ui.detail_jvm = ov
        .jvm_args
        .clone()
        .unwrap_or_else(|| app.cfg.java.jvm_args.clone());
    app.versions_ui.detail_extra_args = ov.extra_args.clone().unwrap_or_default();
    app.versions_ui.detail_title = ov.custom_title.clone().unwrap_or_default();
    app.versions_ui.detail_dir = ov
        .game_dir
        .as_ref()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    // 第 0 项是「自动选择」，其余按搜到的 Java 顺序往后排
    app.versions_ui.detail_java = ov
        .java_path
        .as_ref()
        .and_then(|path| app.javas.iter().position(|j| &j.path == path))
        .map(|i| i + 1)
        .unwrap_or(0);
    // 隔离开关默认跟随全局（全局默认开启）
    app.versions_ui.detail_isolation = app.cfg.isolation_for(instance_id);
    app.versions_ui.loaded_for = Some(instance_id.to_string());
}

fn saves_tab(app: &mut App, ui: &mut Ui, inst: &crate::core::minecraft::GameInstance) {
    let p = app.palette;
    let game_dir = app.cfg.effective_game_dir(&inst.instance_id);
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
    let game_dir = app.cfg.effective_game_dir(&inst.instance_id);
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
    /// 数据包，1.13 之后用来改配方、战利品这些玩法内容
    DataPack,
}

impl ContentKind {
    fn label(self) -> &'static str {
        match self {
            Self::Mod => "模组",
            Self::Shader => "光影",
            Self::ResourcePack => "资源包",
            Self::DataPack => "数据包",
        }
    }

    /// 界面上的显示名
    ///
    /// 资源包在中文圈也常叫材质包，两个名字都写上，
    /// 找东西的时候不用猜。
    fn display_label(self) -> &'static str {
        match self {
            Self::ResourcePack => "资源包（材质包）",
            _ => self.label(),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
            Self::DataPack => "datapack",
        }
    }

    /// Modrinth 的项目类型（决定主页链接的路径）
    fn modrinth_type(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
            // Modrinth 不收录数据包
            Self::DataPack => "project",
        }
    }

    /// 内容平台那边的类型
    ///
    /// 缓存、查版本这些都要用平台这一套分类，
    /// 界面这一套只管显示。
    fn core_kind(self) -> crate::core::content::modrinth::ContentKind {
        use crate::core::content::modrinth::ContentKind as Kind;
        match self {
            Self::Mod => Kind::Mod,
            Self::Shader => Kind::Shader,
            Self::ResourcePack => Kind::ResourcePack,
            Self::DataPack => Kind::DataPack,
        }
    }

    /// CurseForge 的项目类型（决定主页链接的路径）
    fn curseforge_type(self) -> crate::core::content::modrinth::ContentKind {
        self.core_kind()
    }

    /// 相对游戏目录的位置
    fn sub_dir(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::Shader => "shaderpacks",
            Self::ResourcePack => "resourcepacks",
            Self::DataPack => "datapacks",
        }
    }

    fn all() -> [ContentKind; 4] {
        [
            Self::Mod,
            Self::Shader,
            Self::ResourcePack,
            Self::DataPack,
        ]
    }

    /// 对应「下载」页里的标签页下标
    fn download_tab(self) -> usize {
        match self {
            Self::Mod => 2,
            Self::Shader => 3,
            Self::ResourcePack => 4,
            Self::DataPack => 5,
        }
    }
}

/// 一个本地内容的更新状态
#[derive(Debug, Clone, Default)]
pub enum ContentUpdateState {
    /// 还没查过
    #[default]
    Idle,
    /// 正在查线上版本
    Checking,
    /// 已经是最新
    UpToDate,
    /// 有新版本，附版本号
    Available(String),
    /// 查失败
    Failed(String),
    /// 刚更新完，附版本号
    Done(String),
}

/// 更新状态的键：分类 + 本地文件名
fn update_key(kind: ContentKind, file_name: &str) -> String {
    format!("{}|{}", kind.core_kind().facet(), file_name)
}

/// 本地内容的一个条目
pub struct LocalContent {
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
///
/// 目录没变就复用上次的列表：整合包动辄上百个模组，
/// 每帧重扫一遍目录、重新解析 jar 元数据，界面会明显发涩。
fn list_contents(
    app: &mut App,
    kind: ContentKind,
    game_dir: &Path,
) -> Arc<Vec<LocalContent>> {
    let dir = game_dir.join(kind.sub_dir());
    let key = format!("{}|{}", dir.display(), kind.label());
    let stamp = std::fs::metadata(&dir)
        .ok()
        .and_then(|m| m.modified().ok());
    if let Some((cached, items)) = app.versions_ui.content_cache.get(&key) {
        if *cached == stamp {
            return items.clone();
        }
    }
    let items = Arc::new(scan_contents(app, kind, &dir));
    app.versions_ui
        .content_cache
        .insert(key, (stamp, items.clone()));
    items
}

/// 真正去目录里扫一遍
fn scan_contents(app: &App, kind: ContentKind, dir: &Path) -> Vec<LocalContent> {
    let mut items: Vec<LocalContent> = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
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
) {
    let p = app.palette;
    let game_dir = app.cfg.effective_game_dir(&inst.instance_id);

    let dir = game_dir.join(kind.sub_dir());
    let items = list_contents(app, kind, &game_dir);

    // 反查 Modrinth 项目：模组能从 MCMod 词库拿到 slug，
    // 光影与资源包只能靠文件名猜项目名。
    // Modrinth 不收录数据包，跳过免得误认成别的项目。
    //
    // 缓存里有的直接拿来用，不再联网；
    // 只有缓存里没有的才去线上问一次，问完写回缓存，下次打开就是本地读。
    if kind != ContentKind::DataPack {
        let missing = fill_from_cache(
            app,
            kind,
            &items,
            crate::core::content::ContentSource::Modrinth,
        );
        let fresh: Vec<(String, Option<String>)> = missing
            .into_iter()
            .filter(|(name, _)| {
                app.versions_ui
                    .content_looked_up
                    .insert(looked_up_key(kind, name))
            })
            .collect();
        if !fresh.is_empty() {
            lookup_content_slugs(app, kind, fresh);
        }
    }

    // CurseForge 认文件指纹，改过名的光影、资源包也能反查出来，
    // 所以不管当前下载平台是哪个，都拿指纹问一次，能配上就配图标。
    // 模组不算：jar 自带图标，词库也能给出项目页，
    // 而一个整合包几十上百个模组，全算一遍指纹太慢。
    if kind != ContentKind::Mod && app.cfg.download.curseforge_key().is_some() {
        let missing = fill_from_cache(
            app,
            kind,
            &items,
            crate::core::content::ContentSource::CurseForge,
        );
        let fresh: Vec<(String, PathBuf)> = missing
            .into_iter()
            .filter(|(name, _)| {
                app.versions_ui
                    .content_cf_looked_up
                    .insert(looked_up_key(kind, name))
            })
            .map(|(name, _)| {
                let path = items
                    .iter()
                    .find(|i| i.file_name == name)
                    .map(|i| i.path.clone())
                    .unwrap_or_default();
                (name, path)
            })
            .collect();
        if !fresh.is_empty() {
            lookup_curseforge_projects(app, kind, fresh);
        }
    }

    ui.horizontal(|ui| {
        // 锁住的有几个，一眼能看见
        let locked_count = items
            .iter()
            .filter(|i| app.content_cache.is_locked(kind.core_kind(), &i.file_name))
            .count();
        if locked_count > 0 {
            ui::icon(ui, "lock", 12.0, p.accent);
            ui.label(
                egui::RichText::new(format!("{locked_count} 项已锁定"))
                    .font(ui::font(11.5))
                    .color(p.text_faint),
            );
        }
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
                app.download.content_install_instance = Some(inst.instance_id.clone());
                app.download.content_loaded_tab = None;
                app.page = Page::Download;
            }
            // 一键更新：锁住的不动，没认出线上项目的跳过
            if ui::button(ui, &p, ButtonOpts::secondary("全部更新").icon("refresh")).clicked() {
                update_all(app, inst, kind, &items);
            }
        });
    });
    ui.add_space(10.0);

    if items.is_empty() {
        let tip = match kind {
            ContentKind::Mod => "到「下载」页搜索模组，默认会装到当前实例",
            ContentKind::Shader => "把光影包放进这个目录即可生效",
            ContentKind::ResourcePack => "把资源包（材质包）放进这个目录即可生效",
            ContentKind::DataPack => "把数据包放进这个目录即可生效",
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
                // 反查到的线上项目：项目页链接与图标都用它
                let modrinth_project = app.versions_ui.content_slugs.get(&item.file_name).cloned();
                let cf_project = app.versions_ui.content_cf.get(&item.file_name).cloned();
                let use_curseforge =
                    app.cfg.download.content_source == crate::core::content::ContentSource::CurseForge;
                // 在线页面：模组用 MCMod 词库里的 slug，光影与资源包用后台反查的结果
                let modrinth_slug = item
                    .modrinth_slug
                    .clone()
                    .or_else(|| modrinth_project.as_ref().map(|p| p.slug.clone()));
                let cf_slug = cf_project.as_ref().map(|p| p.slug.clone());
                // 图标：文件自带的优先，其次线上项目的，最后退回分类图标。
                // CurseForge 的图标认得准（有文件指纹），排前面
                let remote_icon = cf_project
                    .as_ref()
                    .and_then(|p| p.icon_url.clone())
                    .or_else(|| modrinth_project.as_ref().and_then(|p| p.icon_url.clone()));
                // 银锁：锁住的内容不会被自动更新
                let locked = app.content_cache.is_locked(kind.core_kind(), &item.file_name);
                // 更新状态：没查过时是个暗色小图标，查出新版就变成按钮
                let update_state = app
                    .versions_ui
                    .content_updates
                    .get(&update_key(kind, &item.file_name))
                    .cloned()
                    .unwrap_or_default();
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
                        let mut texture = item
                            .icon
                            .as_ref()
                            .and_then(|bytes| app.mod_icon_texture(&icon_key, bytes));
                        if texture.is_none() {
                            // jar 里没有图标就用线上项目的图标，
                            // 光影与资源包基本都靠这一条
                            if let Some(url) = remote_icon.as_ref() {
                                app.ensure_image(url);
                                texture = app.image(url).cloned();
                            }
                        }
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
                            // 文字过长时截断，免得把卡片撑出格子。
                            // 右上角最多三个圆形按钮，得给它们留够位置，
                            // 不然长标题会被按钮压住。
                            ui.set_width((card_width - 156.0).max(100.0));
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
                                    .font(ui::font(10.0))
                                    .color(p.text_faint),
                                )
                                .truncate(),
                            )
                            .on_hover_text(format!(
                                "{}\n{}",
                                item.file_name,
                                paths::format_size(item.size)
                            ));
                        });
                        // 右上角的角标：打开所在目录、用外部浏览器看项目主页
                        let mcmod_url = item.mcmod.as_ref().map(|e| e.url());
                        // 外链角标优先指当前平台的项目页，那个没认出来就看另一个平台
                        let cf_url = cf_slug.as_ref().map(|slug| {
                            (
                                crate::core::content::curseforge::project_url(
                                    kind.curseforge_type(),
                                    slug,
                                ),
                                format!("在浏览器里打开 CurseForge：{slug}"),
                            )
                        });
                        let mr_url = modrinth_slug.as_ref().map(|slug| {
                            (
                                crate::core::content::modrinth::project_url(
                                    kind.modrinth_type(),
                                    slug,
                                ),
                                format!("在浏览器里打开 Modrinth：{slug}"),
                            )
                        });
                        let (primary_url, primary_tip): (String, String) = if use_curseforge {
                            cf_url.or(mr_url)
                        } else {
                            mr_url.or(cf_url)
                        }
                        .or_else(|| {
                            mcmod_url
                                .clone()
                                .map(|url| (url, "在浏览器里打开 MC 百科".to_string()))
                        })
                        .unwrap_or_default();
                        let lock_clicked = ui
                            .with_layout(Layout::right_to_left(Align::Min), |ui| {
                            // 打开所在目录
                            let folder = ui::round_icon_button(ui, &p, "folder-open", 20.0);
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
                            if !primary_url.is_empty() {
                                let badge = ui::round_icon_button(ui, &p, "external-link", 20.0);
                                if badge.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    let tip = primary_tip.clone();
                                    badge.clone().on_hover_ui(|ui| {
                                        ui.label(egui::RichText::new(tip).font(ui::font(11.0)));
                                    });
                                }
                                if badge.clicked() {
                                    let _ = open::that(&primary_url);
                                }
                            }
                            // 银锁：锁住的内容不会被自动更新
                            let lock = ui::round_icon_button_state(
                                ui,
                                &p,
                                if locked { "lock" } else { "lock-open" },
                                20.0,
                                locked,
                            );
                            if lock.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                let tip = if locked {
                                    "已锁定，不会被自动更新，点击解除"
                                } else {
                                    "锁定这个内容，不会被自动更新"
                                };
                                lock.clone().on_hover_ui(|ui| {
                                    ui.label(egui::RichText::new(tip).font(ui::font(11.0)));
                                });
                            }
                            lock.clicked()
                        })
                        .inner;
                        if lock_clicked {
                            app.content_cache
                                .set_locked(kind.core_kind(), &item.file_name, !locked);
                            app.content_cache.save();
                        }
                    });
                    ui.add_space(5.0);
                    // 底行：左边是文件位置，右边是更新按钮与相关网站
                    ui.horizontal(|ui| {
                        let path_width = (card_width - 250.0).max(60.0);
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
                        let update_clicked = ui
                            .with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // 更新按钮放在最右边，每张卡片的位置都一样
                            let clicked = update_control(ui, &p, &update_state, locked);
                            // 当前平台的项目页，认出来才显示
                            let mut links: Vec<(&str, String)> = Vec::new();
                            if let Some(url) = &mcmod_url {
                                links.push(("MC 百科", url.clone()));
                            }
                            if let Some(slug) = &modrinth_slug {
                                links.push((
                                    "Modrinth",
                                    crate::core::content::modrinth::project_url(
                                        kind.modrinth_type(),
                                        slug,
                                    ),
                                ));
                            }
                            if let Some(slug) = &cf_slug {
                                links.push((
                                    "CurseForge",
                                    crate::core::content::curseforge::project_url(
                                        kind.curseforge_type(),
                                        slug,
                                    ),
                                ));
                            }
                            // 链接之间用一个点隔开
                            for (index, (label, url)) in links.iter().enumerate() {
                                if index > 0 {
                                    ui.label(
                                        egui::RichText::new("·")
                                            .font(ui::font(10.5))
                                            .color(p.text_faint),
                                    );
                                }
                                if site_link(ui, &p, label).clicked() {
                                    let _ = open::that(url);
                                }
                            }
                            clicked
                        })
                        .inner;
                        if update_clicked {
                            update_one(app, inst, kind, item);
                        }
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
    let game_dir = app.cfg.effective_game_dir(&inst.instance_id);

    let mut files: Vec<FeatureFile> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for sub in feature.dirs {
        let dir = game_dir.join(sub);
        list_feature_files(&dir, feature.exts, &mut files);
        dirs.push(dir);
    }
    files.sort_by(|a, b| a.rel.to_lowercase().cmp(&b.rel.to_lowercase()));

    ui.horizontal(|ui| {
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

/// 反查记录的键：分类 + 本地文件名
fn looked_up_key(kind: ContentKind, file_name: &str) -> String {
    format!("{}|{}", kind.core_kind().facet(), file_name)
}

/// 找出这个本地内容对应的线上项目
///
/// 优先用当前下载平台那边的项目，另一个平台认出来了也能用。
fn remote_of(
    app: &App,
    file_name: &str,
) -> Option<(
    crate::core::content::ContentSource,
    crate::core::content::RemoteProject,
)> {
    let prefer_cf = app.cfg.download.content_source
        == crate::core::content::ContentSource::CurseForge;
    let mr = app.versions_ui.content_slugs.get(file_name);
    let cf = app.versions_ui.content_cf.get(file_name);
    let pick = |first: Option<&crate::core::content::RemoteProject>,
                first_source: crate::core::content::ContentSource,
                second: Option<&crate::core::content::RemoteProject>,
                second_source: crate::core::content::ContentSource| {
        first
            .cloned()
            .map(|p| (first_source, p))
            .or_else(|| second.cloned().map(|p| (second_source, p)))
    };
    if prefer_cf {
        pick(
            cf,
            crate::core::content::ContentSource::CurseForge,
            mr,
            crate::core::content::ContentSource::Modrinth,
        )
    } else {
        pick(
            mr,
            crate::core::content::ContentSource::Modrinth,
            cf,
            crate::core::content::ContentSource::CurseForge,
        )
    }
}

/// 取实例的 MC 版本与加载器，更新时按它筛线上版本
fn instance_filter(app: &App, instance_id: &str) -> (Option<String>, Option<String>) {
    app.instances
        .iter()
        .find(|i| i.instance_id == instance_id)
        .map(|i| {
            let mc = i.real_mc_version();
            let loader = i.loader_label_opt().map(|l| l.to_lowercase());
            (
                if mc.trim().is_empty() { None } else { Some(mc) },
                loader.filter(|l| !l.trim().is_empty()),
            )
        })
        .unwrap_or((None, None))
}

/// 更新一个内容到线上最新版
fn update_one(
    app: &mut App,
    inst: &crate::core::minecraft::GameInstance,
    kind: ContentKind,
    item: &LocalContent,
) {
    let Some((source, project)) = remote_of(app, &item.file_name) else {
        app.error("这个内容还没找到对应的线上项目，更新不了");
        return;
    };
    let Some(project_id) = project.project_id.clone().filter(|id| !id.is_empty()) else {
        app.error("这个内容还没找到对应的线上项目，更新不了");
        return;
    };
    if app.content_cache.is_locked(kind.core_kind(), &item.file_name) {
        app.info("这个内容已锁定，先解锁再更新");
        return;
    }
    let game_dir = app.cfg.effective_game_dir(&inst.instance_id);
    let (mc, loader) = instance_filter(app, &inst.instance_id);
    let target = crate::core::content::update::Target {
        kind: kind.core_kind(),
        source,
        project_id,
        file_name: item.file_name.clone(),
        path: item.path.clone(),
    };
    let key = update_key(kind, &item.file_name);
    let title = item.display.clone();
    app.versions_ui
        .content_updates
        .insert(key.clone(), ContentUpdateState::Checking);
    app.spawn_download(format!("更新 {title}"), move |dl, progress, job| {
        let outcome = crate::core::content::update::update(
            &dl,
            &target,
            &game_dir,
            mc.as_deref(),
            loader.as_deref(),
            &progress,
        )?;
        job.send(CoreEvent::ContentUpdated {
            key,
            kind: target.kind,
            old_file: target.file_name.clone(),
            new_file: outcome
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string()),
            version: outcome.version.clone(),
            updated: outcome.updated,
        });
        Ok(if outcome.updated {
            format!("{title} 已更新到 {}", outcome.version)
        } else {
            format!("{title} 已经是最新")
        })
    });
}

/// 把这一页能更新的内容全更新一遍
///
/// 锁住的不动，没认出线上项目的也跳过。
/// 查线上最新版是网络往返，一百多个模组一个个问要三十多秒，
/// 这里并发地问完，再把要更新的一个个下下来。
fn update_all(
    app: &mut App,
    inst: &crate::core::minecraft::GameInstance,
    kind: ContentKind,
    items: &[LocalContent],
) {
    use crate::core::content::update::Checked;

    let mut targets: Vec<crate::core::content::update::Target> = Vec::new();
    for item in items {
        if app.content_cache.is_locked(kind.core_kind(), &item.file_name) {
            continue;
        }
        let Some((source, project)) = remote_of(app, &item.file_name) else {
            continue;
        };
        let Some(project_id) = project.project_id.clone().filter(|id| !id.is_empty()) else {
            continue;
        };
        targets.push(crate::core::content::update::Target {
            kind: kind.core_kind(),
            source,
            project_id,
            file_name: item.file_name.clone(),
            path: item.path.clone(),
        });
    }
    if targets.is_empty() {
        app.info("这一页没有可以更新的内容");
        return;
    }
    let game_dir = app.cfg.effective_game_dir(&inst.instance_id);
    let (mc, loader) = instance_filter(app, &inst.instance_id);
    // 先把状态标成「检查中」，不然点了没反应
    for target in &targets {
        app.versions_ui
            .content_updates
            .insert(update_key(kind, &target.file_name), ContentUpdateState::Checking);
    }
    app.spawn_download("全部更新", move |dl, progress, job| {
        // 第一步：并发地问线上最新版
        let checked = crate::core::content::update::check_all(
            &dl,
            &targets,
            mc.as_deref(),
            loader.as_deref(),
            crate::core::content::update::CHECK_CONCURRENCY,
            &progress,
        );

        // 第二步：把要更新的下下来。下载一个一个来，
        // 进度条才看得出在动哪个。
        let to_download = checked
            .iter()
            .filter(|item| matches!(item, Checked::Stale(_)))
            .count();
        let mut updated = 0usize;
        let mut newest = 0usize;
        let mut failed = 0usize;
        let mut downloading = 0usize;
        for (target, result) in targets.iter().zip(checked.iter()) {
            let key = update_key(kind, &target.file_name);
            match result {
                Checked::Current(version) => {
                    newest += 1;
                    job.send(CoreEvent::ContentUpdated {
                        key,
                        kind: target.kind,
                        old_file: target.file_name.clone(),
                        new_file: Some(target.file_name.clone()),
                        version: version.version_number.clone(),
                        updated: false,
                    });
                }
                Checked::Stale(version) => {
                    downloading += 1;
                    progress.set_message(format!(
                        "正在下载 {downloading}/{to_download} · {}",
                        target.file_name
                    ));
                    match crate::core::content::update::apply(
                        &dl,
                        target,
                        version,
                        &game_dir,
                        &progress,
                    ) {
                        Ok(outcome) => {
                            updated += 1;
                            job.send(CoreEvent::ContentUpdated {
                                key,
                                kind: target.kind,
                                old_file: target.file_name.clone(),
                                new_file: outcome
                                    .path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string()),
                                version: outcome.version.clone(),
                                updated: outcome.updated,
                            });
                        }
                        Err(err) => {
                            failed += 1;
                            job.send(CoreEvent::ContentUpdateFailed {
                                key,
                                error: err.to_string(),
                            });
                        }
                    }
                }
                Checked::Failed(error) => {
                    failed += 1;
                    job.send(CoreEvent::ContentUpdateFailed {
                        key,
                        error: error.clone(),
                    });
                }
            }
        }
        Ok(format!(
            "更新完成：{updated} 个已更新，{newest} 个已是最新，{failed} 个失败"
        ))
    });
}

/// 卡片上的更新控件
///
/// 没查过时是个暗色小图标，查出新版就变成显眼的按钮。
/// 锁住的内容换成锁的图标，一眼就知道它不会被更新。
fn update_control(
    ui: &mut Ui,
    p: &Palette,
    state: &ContentUpdateState,
    locked: bool,
) -> bool {
    match state {
        ContentUpdateState::Checking => {
            ui::spinner(ui, p, 14.0);
            false
        }
        ContentUpdateState::UpToDate => {
            ui.label(
                egui::RichText::new("已是最新")
                    .font(ui::font(11.0))
                    .color(p.text_faint),
            );
            false
        }
        ContentUpdateState::Done(version) => {
            ui.label(
                egui::RichText::new(format!("已更新 {version}"))
                    .font(ui::font(11.0))
                    .color(p.success),
            );
            false
        }
        ContentUpdateState::Failed(error) => {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
            icons::paint(ui.ctx(), ui.painter(), "alert", rect, p.warning);
            response.on_hover_text(format!("检查失败：{error}"));
            false
        }
        ContentUpdateState::Idle => {
            let name = if locked { "lock" } else { "download-arrow" };
            let button = ui::round_icon_button(ui, p, name, 22.0);
            if button.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                let tip = if locked {
                    "已锁定，不会被自动更新，点击卡片右上角的锁解除"
                } else {
                    "检查并更新到线上最新版"
                }
                .to_string();
                button.clone().on_hover_ui(|ui| {
                    ui.label(egui::RichText::new(tip).font(ui::font(11.0)));
                });
            }
            button.clicked()
        }
        ContentUpdateState::Available(version) => {
            let tip = format!("线上有新版本 {version}，点击更新");
            ui::button(
                ui,
                p,
                ButtonOpts::primary("更新")
                    .font_size(12.0)
                    .height(26.0)
                    .tooltip(&tip),
            )
            .clicked()
        }
    }
}

/// 后台反查 Modrinth 项目，结果回来后再显示官网链接与图标
///
/// 模组在 MCMod 词库里能直接拿到 slug，一次批量就能取回图标；
/// 光影、资源包这些只能靠文件名猜项目名，再一个个查。
fn lookup_content_slugs(app: &mut App, kind: ContentKind, items: Vec<(String, Option<String>)>) {
    if items.is_empty() {
        return;
    }
    let source = app.cfg.download.source;
    let timeout = app.cfg.download.timeout_secs;
    app.spawn("查找项目页面", false, move |job| {
        let result = (|| -> anyhow::Result<
            std::collections::HashMap<String, crate::core::content::RemoteProject>,
        > {
            let dl = crate::core::download::Downloader::new(source, timeout.max(20))?;
            let mut found = std::collections::HashMap::new();

            // 已经知道 slug 的，一次批量取回，省得一个个请求
            let known: Vec<(String, String)> = items
                .iter()
                .filter_map(|(name, slug)| slug.as_ref().map(|s| (name.clone(), s.clone())))
                .collect();
            if !known.is_empty() {
                let slugs: Vec<String> = known.iter().map(|(_, slug)| slug.clone()).collect();
                let projects = crate::core::content::modrinth::projects_by_slugs(&dl, &slugs)
                    .unwrap_or_default();
                for project in projects {
                    for (name, slug) in &known {
                        if slug == &project.slug {
                            found.insert(
                                name.clone(),
                                crate::core::content::RemoteProject {
                                    slug: project.slug.clone(),
                                    // 项目图标一并带回来，列表里直接拿来显示
                                    icon_url: project.icon_url.clone(),
                                    // Modrinth 用 slug 当项目编号
                                    project_id: Some(project.slug.clone()),
                                },
                            );
                        }
                    }
                }
            }

            // 没 slug 的靠文件名猜项目名
            for (name, slug) in items.iter() {
                if slug.is_some() || found.contains_key(name) {
                    continue;
                }
                let guess = guess_project_name(name);
                for candidate in slug_candidates(&guess) {
                    if let Ok(Some(project)) =
                        crate::core::content::modrinth::find_by_slug(&dl, &candidate)
                    {
                        found.insert(
                            name.clone(),
                            crate::core::content::RemoteProject {
                                slug: project.slug.clone(),
                                icon_url: project.icon_url.clone(),
                                project_id: Some(project.slug.clone()),
                            },
                        );
                        break;
                    }
                }
            }
            Ok(found)
        })();
        match result {
            Ok(map) => job.send(CoreEvent::ContentLookup {
                kind: kind.core_kind(),
                source: crate::core::content::ContentSource::Modrinth,
                asked: items.iter().map(|(name, _)| name.clone()).collect(),
                map,
            }),
            Err(err) => {
                log::warn!("反查 Modrinth 项目失败: {err}");
                job.send(CoreEvent::ContentLookupFailed(kind.core_kind()));
            }
        }
        job.done(true, "");
    });
}

/// 后台用文件指纹反查 CurseForge 项目
///
/// CurseForge 认的是文件指纹，算出来去问接口，
/// 就能知道本地这个文件在 CurseForge 上属于哪个项目。
fn lookup_curseforge_projects(
    app: &mut App,
    kind: ContentKind,
    files: Vec<(String, PathBuf)>,
) {
    if files.is_empty() {
        return;
    }
    let timeout = app.cfg.download.timeout_secs;
    let key = app.cfg.download.curseforge_key();
    let mirror = app.cfg.download.source;
    app.spawn("查找 CurseForge 项目", false, move |job| {
        let result = (|| -> anyhow::Result<
            std::collections::HashMap<String, crate::core::content::RemoteProject>,
        > {
            let dl = crate::core::download::Downloader::new(mirror, timeout.max(20))?
                .with_curseforge_key(key);
            crate::core::content::curseforge_projects(&dl, &files)
        })();
        match result {
            Ok(map) => job.send(CoreEvent::ContentLookup {
                kind: kind.core_kind(),
                source: crate::core::content::ContentSource::CurseForge,
                asked: files.iter().map(|(name, _)| name.clone()).collect(),
                map,
            }),
            Err(err) => {
                log::warn!("反查 CurseForge 项目失败: {err}");
                job.send(CoreEvent::ContentLookupFailed(kind.core_kind()));
            }
        }
        job.done(true, "");
    });
}

/// 用本地缓存补齐线上项目，返回缓存里还没有、需要联网去查的文件
///
/// 缓存是这个功能的关键：查过一次就记在本地，
/// 下次打开这一页直接读缓存，不再联网问一遍。
fn fill_from_cache(
    app: &mut App,
    kind: ContentKind,
    items: &[LocalContent],
    source: crate::core::content::ContentSource,
) -> Vec<(String, Option<String>)> {
    let mut missing = Vec::new();
    for item in items {
        let cached = app
            .content_cache
            .get(source, kind.core_kind(), &item.file_name)
            .cloned();
        match cached {
            // 问过没找到的，短期内不再问
            Some(entry) if entry.is_miss() && !entry.is_stale() => {}
            Some(entry) if entry.is_miss() => {
                missing.push((item.file_name.clone(), item.modrinth_slug.clone()));
            }
            Some(entry) => {
                let project = crate::core::content::RemoteProject {
                    slug: entry.slug.clone(),
                    icon_url: entry.icon_url.clone(),
                    project_id: Some(entry.project_id.clone()),
                };
                match source {
                    crate::core::content::ContentSource::CurseForge => {
                        app.versions_ui
                            .content_cf
                            .insert(item.file_name.clone(), project);
                    }
                    crate::core::content::ContentSource::Modrinth => {
                        app.versions_ui
                            .content_slugs
                            .insert(item.file_name.clone(), project);
                    }
                }
            }
            None => {
                missing.push((item.file_name.clone(), item.modrinth_slug.clone()));
            }
        }
    }
    missing
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
                let new_instance_id = app
                    .local_versions
                    .iter()
                    .find(|v| v.id == new_id)
                    .map(|v| v.instance_id.clone())
                    .unwrap_or_else(|| new_id.clone());
                app.versions_ui.detail = Some(new_instance_id);
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

pub(crate) fn copy_version(app: &mut App, id: &str) {
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
            let new_instance_id = app
                .local_versions
                .iter()
                .find(|v| v.id == new_id)
                .map(|v| v.instance_id.clone())
                .unwrap_or_else(|| new_id.clone());
            app.versions_ui.detail = Some(new_instance_id.clone());
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

// ------------------------------------------------------------------ 新建实例

/// 新实例默认的目录名
///
/// 拿版本号当底，撞上已有的目录就往后加序号：
/// 1.20.1 装了第二个实例，那第三个就叫 1.20.1-2。
fn default_new_instance_id(app: &App, inst: &crate::core::minecraft::GameInstance) -> String {
    let taken: Vec<String> = app.local_versions.iter().map(|v| v.id.clone()).collect();
    crate::core::minecraft::instance::unique_id(&inst.mc_version, &taken)
}

/// 打开「新建实例」弹窗
///
/// 以某个已有实例为基础复制一份。版本管理页与下载页都走这里。
pub(crate) fn start_new_instance(app: &mut App, instance_id: &str) {
    let Some(inst) = app
        .instances
        .iter()
        .find(|i| i.instance_id == instance_id)
        .cloned()
    else {
        app.error("找不到这个实例");
        return;
    };
    app.versions_ui.new_instance_from = Some(inst.instance_id.clone());
    // 整合包只复制游戏本体，名字按真实的 Minecraft 版本来，
    // 免得出现「某某整合包 副本」这种名不副实的名字
    let base = if inst.is_modpack {
        inst.real_mc_version()
    } else {
        inst.display_name.clone()
    };
    app.versions_ui.new_instance_name = format!("{base} 副本");
    app.versions_ui.new_instance_id = default_new_instance_id(app, &inst);
}

/// 「新建实例」弹窗
///
/// 以当前实例为基础复制一份：客户端 jar、natives 与版本描述都复制过去，
/// 模组、存档与配置不复制，新实例是干净的。
pub(crate) fn new_instance_modal(app: &mut App, ui: &mut Ui) {
    let Some(from_id) = app.versions_ui.new_instance_from.clone() else {
        return;
    };
    let p = app.palette;
    let mut open = true;
    let mut create = false;
    let ctx = ui.ctx().clone();

    // 源实例：找不到就关掉弹窗
    let Some(inst) = app.instances.iter().find(|i| i.instance_id == from_id).cloned() else {
        app.versions_ui.new_instance_from = None;
        return;
    };
    let Some(active) = inst.active_version().cloned() else {
        app.versions_ui.new_instance_from = None;
        return;
    };

    let title = format!("新建实例 · {}", inst.display_name);
    // 整合包实例带的是模组，复制不过去，得说清楚
    let note = if inst.is_modpack {
        format!(
            "以 {} 为基础新建一个独立实例。只复制游戏本体，\
             整合包的模组与配置不会带过去，新实例是干净的。",
            active.variant_label()
        )
    } else {
        format!(
            "以 {} 为基础新建一个独立实例。游戏本体与加载器会复制过去，\
             模组、存档与配置不复制，新实例是干净的。",
            active.variant_label()
        )
    };
    widgets::modal(&ctx, &p, &mut open, &title, 460.0, |ui| {
        ui.label(
            egui::RichText::new(note.clone())
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(16.0);
        let mut name = app.versions_ui.new_instance_name.clone();
        if ui::text_field(ui, &p, &mut name, "实例名称", 360.0, false).changed() {
            app.versions_ui.new_instance_name = name;
        }
        ui.add_space(8.0);
        let mut id = app.versions_ui.new_instance_id.clone();
        if ui::text_field(ui, &p, &mut id, "目录名", 360.0, false).changed() {
            app.versions_ui.new_instance_id = id;
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::primary("创建")).clicked() {
                    create = true;
                }
                if ui::button(ui, &p, ButtonOpts::secondary("取消")).clicked() {
                    app.versions_ui.new_instance_from = None;
                }
            });
        });
    });

    if create {
        let new_id = app.versions_ui.new_instance_id.trim().to_string();
        let new_name = app.versions_ui.new_instance_name.trim().to_string();
        match create_new_instance(app, &inst, &active, &new_id, &new_name) {
            Ok(()) => {
                app.success(format!("已创建实例 {new_name}"));
                app.versions_ui.detail = Some(new_id);
                app.versions_ui.new_instance_from = None;
                app.refresh_local_versions();
            }
            Err(err) => app.error(format!("创建实例失败：{err}")),
        }
    }
    if !open {
        app.versions_ui.new_instance_from = None;
    }
}

/// 按源实例复制出一个新的独立实例
fn create_new_instance(
    app: &mut App,
    inst: &crate::core::minecraft::GameInstance,
    active: &crate::core::minecraft::LocalVersion,
    new_id: &str,
    new_name: &str,
) -> anyhow::Result<()> {
    if new_id.is_empty() {
        anyhow::bail!("目录名不能为空");
    }
    if new_id.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        anyhow::bail!("目录名不能包含路径分隔符或特殊字符");
    }
    if new_name.is_empty() {
        anyhow::bail!("实例名称不能为空");
    }
    let game_dir = app.cfg.game_dir.clone();
    // 变体目录：版本描述与 natives 都在这儿
    let variant_dir = version::version_dir(&game_dir, &active.id);
    let json = version::read_local_json(&game_dir, &active.id)?;
    // 客户端 jar 跟着原版走：解析继承链才知道 jar 实际在哪个目录
    let (base_dir, base_id) = match version::resolve(&game_dir, &active.id) {
        Ok(resolved) => (
            version::version_dir(&game_dir, &resolved.jar_id),
            resolved.jar_id,
        ),
        // 解析不了就按版本号猜，复制不到 jar 时游戏会报错，但目录还是建好了
        Err(_) => (
            version::version_dir(&game_dir, &inst.real_mc_version()),
            inst.real_mc_version(),
        ),
    };
    let source = crate::core::minecraft::instance::InstanceSource {
        base_dir,
        base_id,
        variant_dir,
        json,
    };
    crate::core::minecraft::instance::create_instance(
        &game_dir,
        &source,
        new_id,
        // 标记里记的是真正的 Minecraft 版本，整合包实例不能记它自己的名字
        &inst.real_mc_version(),
        new_name,
    )?;
    Ok(())
}
