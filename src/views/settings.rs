use egui::{Align, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::App;
use crate::core::config::BackgroundKind;
use crate::core::download::DownloadSource;
use crate::core::minecraft::bundle::LoaderTiming;
use crate::core::minecraft::loaders::{LoaderKind, LoaderPolicy};
use crate::core::paths;
use crate::icons;
use crate::theme;
use crate::ui::{self, widgets, ButtonOpts};

#[derive(Default)]
pub struct SettingsState {
    pub java_manual: String,
    pub news_url: String,
    /// 0 外观 / 1 下载 / 2 游戏 / 3 Java / 4 资讯 / 5 其他
    pub tab: usize,
}

const ACCENT_PRESETS: [&str; 8] = [
    "#B0703F", "#A9663A", "#3F8F6B", "#3A6EA5", "#C2607A", "#7A5AA0", "#C0803A", "#4A4A52",
];

const CATEGORIES: [(&str, &str); 6] = [
    ("palette", "外观"),
    ("download", "下载"),
    ("cube", "游戏"),
    ("java", "Java"),
    ("news", "资讯"),
    ("sliders", "其他"),
];

pub fn ui(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(ui, &p, "设置", "外观、下载、游戏、Java、资讯与文件");

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;

        ui.vertical(|ui| {
            ui.set_width(164.0);
            ui::card_padded(ui, &p, 8, |ui| {
                ui.set_width(148.0);
                for (index, (icon, label)) in CATEGORIES.iter().enumerate() {
                    let selected = app.settings_ui.tab == index;
                    if ui::nav_item(ui, &p, icon, label, selected).clicked() {
                        app.settings_ui.tab = index;
                    }
                    ui.add_space(2.0);
                }
            });
        });

        ui.vertical(|ui| {
            let width = ui.available_width();
            ui.set_width(width);
            match app.settings_ui.tab {
                0 => appearance(app, ui),
                1 => download_section(app, ui),
                2 => game_section(app, ui),
                3 => java_section(app, ui),
                4 => news_section(app, ui),
                _ => misc_section(app, ui),
            }
        });
    });
}

// ------------------------------------------------------------------ 外观

fn appearance(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "外观");

        ui.label(
            egui::RichText::new("主题")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(8.0);

        let themes = app.themes.clone();
        let current = app.cfg.ui.theme.clone();
        let width = ui.available_width();
        let columns = ((width + 12.0) / 172.0).floor().max(2.0) as usize;
        let card_width = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;

        let mut apply: Option<String> = None;
        for chunk in themes.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for def in chunk {
                    ui.vertical(|ui| {
                        ui.set_width(card_width);
                        if theme_card(ui, def, def.id == current, card_width) {
                            apply = Some(def.id.clone());
                        }
                    });
                }
            });
            ui.add_space(12.0);
        }

        if let Some(id) = apply {
            app.set_theme(&id);
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("打开主题目录").icon("folder")).clicked() {
                let _ = paths::ensure_dir(&paths::themes_dir());
                paths::reveal(&paths::themes_dir());
            }
            if ui::button(ui, &p, ButtonOpts::secondary("生成示例主题")).clicked() {
                match theme::write_example_theme() {
                    Ok(path) => {
                        app.refresh_themes();
                        app.success(format!("已生成 {}", path.display()));
                    }
                    Err(err) => app.error(format!("生成失败：{err}")),
                }
            }
            if ui::button(ui, &p, ButtonOpts::secondary("重新扫描主题")).clicked() {
                app.refresh_themes();
                app.info("主题列表已刷新");
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("自定义主题：在 themes 目录下新建文件夹，放入 theme.json 与背景图即可。")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );

        ui.add_space(16.0);
        widgets::divider(ui, &p);
        ui.add_space(14.0);

        // 主题色
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("主题色")
                    .font(ui::font(12.5))
                    .color(p.text_weak),
            );
            ui.add_space(6.0);
            let follow = app.cfg.ui.accent.is_none();
            if accent_swatch(ui, &p, None, follow, "跟随主题") {
                app.cfg.ui.accent = None;
                app.apply_theme();
                app.save_config();
            }
            for preset in ACCENT_PRESETS {
                let selected = app.cfg.ui.accent.as_deref() == Some(preset);
                if accent_swatch(ui, &p, Some(theme::hex(preset)), selected, preset) {
                    app.cfg.ui.accent = Some(preset.to_string());
                    app.apply_theme();
                    app.save_config();
                }
            }
        });

        ui.add_space(14.0);
        let mut radius = app.cfg.ui.radius as f32;
        if ui::slider(ui, &p, "圆角大小", &mut radius, 0.0, 20.0, 1.0, " px").changed() {
            app.cfg.ui.radius = radius as u8;
            app.apply_theme();
            app.save_config();
        }
        let mut scale = app.cfg.ui.font_scale;
        if ui::slider(ui, &p, "字体缩放", &mut scale, 0.85, 1.4, 0.05, "x").changed() {
            app.cfg.ui.font_scale = scale;
            app.apply_theme();
            app.save_config();
        }

        let mut animations = app.cfg.ui.animations;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("界面动画")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut animations).changed() {
                    app.cfg.ui.animations = animations;
                    app.save_config();
                }
            });
        });

        ui.add_space(16.0);
        widgets::divider(ui, &p);
        ui.add_space(14.0);

        // 背景
        ui.label(
            egui::RichText::new("背景图 / 动态背景")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("选择图片或 GIF").icon("image")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("图片", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
                    .set_title("选择背景图片")
                    .pick_file()
                {
                    app.cfg.ui.background.kind = BackgroundKind::Image;
                    app.cfg.ui.background.path = Some(path);
                    app.reload_background();
                    app.save_config();
                }
            }
            if app.cfg.ui.background.path.is_some() {
                if ui::button(ui, &p, ButtonOpts::secondary("清除背景")).clicked() {
                    app.cfg.ui.background.kind = BackgroundKind::None;
                    app.cfg.ui.background.path = None;
                    app.reload_background();
                    app.save_config();
                }
            }
            match &app.background_path {
                Some(path) => {
                    let frames = app.background_frames.len();
                    let label = if frames > 1 {
                        format!("{} · {} 帧动图", path.display(), frames)
                    } else {
                        path.display().to_string()
                    };
                    ui.label(
                        egui::RichText::new(label)
                            .font(ui::font(11.5))
                            .color(p.text_faint),
                    );
                }
                None => {
                    ui.label(
                        egui::RichText::new("未设置背景，使用主题纯色")
                            .font(ui::font(11.5))
                            .color(p.text_faint),
                    );
                }
            }
        });

        if app.cfg.ui.background.path.is_some() {
            ui.add_space(8.0);
            let mut opacity = app.cfg.ui.background.opacity;
            if ui::slider(ui, &p, "背景不透明度", &mut opacity, 0.05, 1.0, 0.05, "").changed() {
                app.cfg.ui.background.opacity = opacity;
                app.save_config();
            }
            let mut dim = app.cfg.ui.background.dim;
            if ui::slider(ui, &p, "内容遮罩", &mut dim, 0.0, 0.95, 0.05, "").changed() {
                app.cfg.ui.background.dim = dim;
                app.save_config();
            }
            ui.label(
                egui::RichText::new("遮罩用于压暗或提亮背景，保证文字清晰。")
                    .font(ui::font(11.5))
                    .color(p.text_faint),
            );
        }
    });
}

fn theme_card(ui: &mut Ui, def: &theme::ThemeDef, selected: bool, width: f32) -> bool {
    let height = 84.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let bg = theme::hex(&def.bg);
    let surface = theme::hex(&def.surface);
    let border = theme::hex(&def.border);
    let accent = theme::hex(&def.accent);
    let text = theme::hex(&def.text);

    let painter = ui.painter();
    let radius = egui::CornerRadius::same(8);
    painter.rect_filled(rect, radius, bg);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected { accent } else { border },
        ),
        StrokeKind::Inside,
    );

    // 迷你界面示意
    let inner = rect.shrink(10.0);
    let sidebar = Rect::from_min_size(inner.min, Vec2::new(26.0, inner.height()));
    painter.rect_filled(sidebar, egui::CornerRadius::same(4), surface);
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(sidebar.right() + 6.0, inner.top()),
            Vec2::new(inner.width() - 32.0, 12.0),
        ),
        egui::CornerRadius::same(3),
        accent,
    );
    for i in 0..3 {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(
                    sidebar.right() + 6.0,
                    inner.top() + 18.0 + i as f32 * 12.0,
                ),
                Vec2::new(inner.width() - 32.0 - i as f32 * 12.0, 7.0),
            ),
            egui::CornerRadius::same(2),
            text.gamma_multiply(0.18),
        );
    }

    painter.text(
        Pos2::new(rect.left() + 10.0, rect.bottom() - 12.0),
        egui::Align2::LEFT_CENTER,
        &def.name,
        ui::font(12.0),
        text,
    );
    if !def.builtin {
        painter.text(
            Pos2::new(rect.right() - 10.0, rect.bottom() - 12.0),
            egui::Align2::RIGHT_CENTER,
            "自定义",
            ui::font(10.5),
            accent,
        );
    }
    if selected {
        icons::paint(
            ui.ctx(),
            painter,
            "check-circle",
            Rect::from_center_size(Pos2::new(rect.right() - 16.0, rect.top() + 16.0), Vec2::splat(16.0)),
            accent,
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

fn accent_swatch(
    ui: &mut Ui,
    p: &theme::Palette,
    color: Option<egui::Color32>,
    selected: bool,
    tip: &str,
) -> bool {
    let size = 26.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    match color {
        Some(color) => {
            ui.painter().circle_filled(rect.center(), 11.0, color);
        }
        None => {
            ui.painter()
                .circle_filled(rect.center(), 11.0, p.surface_alt);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "自动",
                ui::font(9.5),
                p.text_weak,
            );
        }
    }
    if selected {
        ui.painter().circle_stroke(
            rect.center(),
            12.5,
            Stroke::new(2.0, p.text),
        );
    }
    response.on_hover_text(tip).clicked()
}

// ------------------------------------------------------------------ 下载

fn download_section(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "下载");

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("下载源")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            let sources = DownloadSource::all();
            let labels: Vec<&str> = sources.iter().map(|s| s.label()).collect();
            let mut index = sources
                .iter()
                .position(|s| *s == app.cfg.download.source)
                .unwrap_or(0);
            if ui::segmented(ui, &p, &mut index, &labels).clicked() {
                app.cfg.download.source = sources[index];
                app.save_config();
                app.load_manifest();
            }
            ui.label(
                egui::RichText::new("镜像源对国内网络更友好")
                    .font(ui::font(11.5))
                    .color(p.text_faint),
            );
        });
        ui.add_space(10.0);

        let mut threads = app.cfg.download.threads as f32;
        if ui::slider(ui, &p, "并发线程", &mut threads, 1.0, 32.0, 1.0, "").changed() {
            app.cfg.download.threads = threads as usize;
            app.save_config();
        }
        let mut timeout = app.cfg.download.timeout_secs as f32;
        if ui::slider(ui, &p, "超时时间", &mut timeout, 10.0, 180.0, 5.0, " 秒").changed() {
            app.cfg.download.timeout_secs = timeout as u64;
            app.save_config();
        }

        let mut use_proxy = app.cfg.download.use_system_proxy;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("使用系统代理")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.label(
                egui::RichText::new("镜像站始终直连")
                    .font(ui::font(11.5))
                    .color(p.text_faint),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut use_proxy).changed() {
                    app.cfg.download.use_system_proxy = use_proxy;
                    crate::core::download::set_use_system_proxy(use_proxy);
                    app.save_config();
                }
            });
        });
        ui.add_space(10.0);

        let mut auto_deps = app.cfg.download.auto_install_dependencies;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("自动安装 Mod 前置依赖")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut auto_deps).changed() {
                    app.cfg.download.auto_install_dependencies = auto_deps;
                    app.save_config();
                }
            });
        });
    });

    loader_section(app, ui);
}

/// 加载器与安装行为设置
fn loader_section(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "加载器");

        // 默认加载器
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("默认加载器")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let labels = ["不安装", "Fabric", "Quilt", "Forge", "NeoForge"];
                let mut index = match app.cfg.download.default_loader_kind() {
                    None => 0,
                    Some(LoaderKind::Fabric) => 1,
                    Some(LoaderKind::Quilt) => 2,
                    Some(LoaderKind::Forge) => 3,
                    Some(LoaderKind::NeoForge) => 4,
                };
                if ui::segmented(ui, &p, &mut index, &labels).clicked() {
                    app.cfg.download.default_loader = match index {
                        1 => "fabric",
                        2 => "quilt",
                        3 => "forge",
                        4 => "neoforge",
                        _ => "none",
                    }
                    .to_string();
                    app.save_config();
                }
            });
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("安装游戏时自动带上该加载器，无需再单独安装")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        // 加载器版本策略
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("加载器版本")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let policies = LoaderPolicy::all();
                let labels: Vec<&str> = policies.iter().map(|v| v.label()).collect();
                let mut index = policies
                    .iter()
                    .position(|v| *v == app.cfg.download.loader_policy())
                    .unwrap_or(0);
                if ui::segmented(ui, &p, &mut index, &labels).clicked() {
                    app.cfg.download.loader_policy = policies[index].id().to_string();
                    app.save_config();
                }
            });
        });
        ui.add_space(4.0);
        let policy_hint = match app.cfg.download.loader_policy() {
            LoaderPolicy::Stable => "自动挑选带推荐标记的稳定版本",
            LoaderPolicy::Latest => "自动挑选最新版本",
            LoaderPolicy::Ask => "每次安装都由你手动挑选版本",
        };
        ui.label(
            egui::RichText::new(policy_hint)
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        // 安装时机
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("安装时机")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let timings = LoaderTiming::all();
                let labels: Vec<&str> = timings.iter().map(|v| v.label()).collect();
                let mut index = timings
                    .iter()
                    .position(|v| *v == app.cfg.download.loader_timing())
                    .unwrap_or(0);
                if ui::segmented(ui, &p, &mut index, &labels).clicked() {
                    app.cfg.download.loader_timing = timings[index].id().to_string();
                    app.save_config();
                }
            });
        });
        ui.add_space(4.0);
        let timing_hint = match app.cfg.download.loader_timing() {
            LoaderTiming::Parallel => "加载器与游戏同时下载安装，整体耗时更短",
            LoaderTiming::After => "游戏装好后再装加载器，步骤更清晰",
            LoaderTiming::Manual => "不自动安装加载器，需要时到「下载 - 加载器」页手动安装",
        };
        ui.label(
            egui::RichText::new(timing_hint)
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
    });

    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "安装行为");

        let mut install_assets = app.cfg.download.install_assets;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("安装资源文件")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.label(
                egui::RichText::new("音效、语言、材质等，占下载量的大头")
                    .font(ui::font(11.5))
                    .color(p.text_faint),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut install_assets).changed() {
                    app.cfg.download.install_assets = install_assets;
                    app.save_config();
                }
            });
        });
        ui.add_space(10.0);

        let mut auto_select = app.cfg.download.auto_select_after_install;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("安装后设为当前版本")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut auto_select).changed() {
                    app.cfg.download.auto_select_after_install = auto_select;
                    app.save_config();
                }
            });
        });
    });
}

// ------------------------------------------------------------------ 启动

fn game_section(app: &mut App, ui: &mut Ui) {
    let p = app.palette;

    // ---------------- 通用 ----------------
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "通用");

        let mut isolation = app.cfg.launch.isolation;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("版本隔离")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.label(
                egui::RichText::new("每个版本使用独立的 mods、存档与配置")
                    .font(ui::font(11.5))
                    .color(p.text_faint),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut isolation).changed() {
                    app.cfg.launch.isolation = isolation;
                    app.save_config();
                }
            });
        });

        let mut fullscreen = app.cfg.launch.fullscreen;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("全屏启动")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut fullscreen).changed() {
                    app.cfg.launch.fullscreen = fullscreen;
                    app.save_config();
                }
            });
        });

        if !app.cfg.launch.fullscreen {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("窗口尺寸")
                        .font(ui::font(13.0))
                        .color(p.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut height = app.cfg.launch.height.to_string();
                    if ui::text_field(ui, &p, &mut height, "720", 84.0, false).changed() {
                        if let Ok(value) = height.trim().parse::<u32>() {
                            app.cfg.launch.height = value.clamp(240, 4320);
                            app.save_config();
                        }
                    }
                    ui.label(
                        egui::RichText::new("×")
                            .font(ui::font(12.0))
                            .color(p.text_faint),
                    );
                    let mut width = app.cfg.launch.width.to_string();
                    if ui::text_field(ui, &p, &mut width, "1280", 84.0, false).changed() {
                        if let Ok(value) = width.trim().parse::<u32>() {
                            app.cfg.launch.width = value.clamp(320, 7680);
                            app.save_config();
                        }
                    }
                });
            });
        }

        let mut show_log = app.cfg.launch.show_game_log;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("启动后显示游戏日志")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut show_log).changed() {
                    app.cfg.launch.show_game_log = show_log;
                    app.save_config();
                }
            });
        });
    });
    ui.add_space(14.0);

    // ---------------- 内存 ----------------
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "内存");
        let mut memory = app.cfg.java.memory_mb as f32;
        if ui::slider(ui, &p, "最大内存", &mut memory, 512.0, 32768.0, 256.0, " MB").changed() {
            app.cfg.java.memory_mb = memory as u32;
            app.save_config();
        }
        let total = crate::core::java::total_memory_mb();
        ui.label(
            egui::RichText::new(format!(
                "物理内存 {:.1} GB。原版建议 2-4 GB，装模组或光影建议 4-8 GB，不建议超过物理内存的一半。",
                total as f64 / 1024.0
            ))
            .font(ui::font(11.5))
            .color(p.text_faint),
        );
        ui.add_space(6.0);
        if ui::button(ui, &p, ButtonOpts::secondary("按物理内存推荐")).clicked() {
            app.cfg.java.memory_mb = crate::core::config::default_memory();
            app.save_config();
        }
    });
    ui.add_space(14.0);

    // ---------------- 启动参数 ----------------
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "启动参数");
        ui.label(
            egui::RichText::new("额外 JVM 参数")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(4.0);
        let mut jvm = app.cfg.java.jvm_args.clone();
        if ui::text_field(
            ui,
            &p,
            &mut jvm,
            "例如 -XX:+UseG1GC -Dfile.encoding=UTF-8",
            520.0,
            false,
        )
        .changed()
        {
            app.cfg.java.jvm_args = jvm;
            app.save_config();
        }
        ui.add_space(10.0);

        ui.label(
            egui::RichText::new("额外游戏参数")
                .font(ui::font(12.5))
                .color(p.text_weak),
        );
        ui.add_space(4.0);
        let mut extra = app.cfg.launch.extra_args.clone();
        if ui::text_field(
            ui,
            &p,
            &mut extra,
            "例如 --server example.com --port 25565",
            520.0,
            false,
        )
        .changed()
        {
            app.cfg.launch.extra_args = extra;
            app.save_config();
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("单版本参数可在「版本管理 - 设置」中单独覆盖。")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
    });
    ui.add_space(14.0);

    // ---------------- 游戏目录 ----------------
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "游戏目录");
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("目录位置")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::secondary("更改")).clicked() {
                    if let Some(dir) = rfd::FileDialog::new()
                        .set_title("选择游戏目录")
                        .pick_folder()
                    {
                        app.cfg.set_game_dir(dir);
                        app.save_config();
                        app.refresh_local_versions();
                    }
                }
                if ui::button(ui, &p, ButtonOpts::secondary("打开")).clicked() {
                    app.cfg.open_game_dir();
                }
                ui.label(
                    egui::RichText::new(app.cfg.game_dir_display())
                        .font(ui::font(12.0))
                        .color(p.text_weak),
                );
            });
        });
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(match app.instances.len() {
                0 => "当前目录下还没有已安装的版本".to_string(),
                n => format!("当前目录下已安装 {n} 个实例"),
            })
            .font(ui::font(11.5))
            .color(p.text_faint),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("查看版本").icon("layers")).clicked() {
                app.page = crate::app::Page::Versions;
            }
            if ui::button(ui, &p, ButtonOpts::secondary("安装新版本").icon("download")).clicked() {
                app.page = crate::app::Page::Download;
            }
        });
    });
}


// ------------------------------------------------------------------ Java

fn java_section(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "Java");

        ui.horizontal(|ui| {
            if ui::button(
                ui,
                &p,
                ButtonOpts::secondary("简要搜索")
                    .icon("search")
                    .enabled(!app.java_searching),
            )
            .clicked()
            {
                app.start_java_search(false);
            }
            if ui::button(
                ui,
                &p,
                ButtonOpts::secondary("详细搜索（全盘）")
                    .icon("hard-drive")
                    .enabled(!app.java_searching),
            )
            .clicked()
            {
                app.start_java_search(true);
            }
            if app.java_searching {
                ui::spinner(ui, &p, 16.0);
                if ui::button(ui, &p, ButtonOpts::ghost("取消")).clicked() {
                    app.cancel_java_search();
                }
            }
            if ui::button(ui, &p, ButtonOpts::secondary("手动添加").icon("plus")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Java 可执行文件", &["exe"])
                    .set_title("选择 java.exe")
                    .pick_file()
                {
                    match crate::core::java::probe(&path) {
                        Some(info) => {
                            app.cfg.java.path = Some(info.path.clone());
                            app.cfg.java.auto = false;
                            if !app.javas.iter().any(|j| j.path == info.path) {
                                app.javas.push(info.clone());
                                app.cfg.java_cache = app.javas.clone();
                            }
                            app.save_config();
                            app.success(format!("已添加 {}", info.display_name()));
                        }
                        None => app.error("该文件不是可用的 Java"),
                    }
                }
            }
        });

        if app.java_searching {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(&app.java_search_message)
                    .font(ui::font(12.0))
                    .color(p.text_weak),
            );
        }

        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("简要搜索会检查注册表、常见安装目录与 PATH；详细搜索会遍历所有磁盘。")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
        ui.add_space(12.0);

        if app.javas.is_empty() {
            ui::inset(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    ui::icon(ui, "alert", 15.0, p.warning);
                    ui.label(
                        egui::RichText::new("还没有搜索到 Java，启动游戏前需要至少一个可用的 Java。")
                            .font(ui::font(12.5))
                            .color(p.text_weak),
                    );
                });
            });
            return;
        }

        let auto = app.cfg.java.path.is_none();
        let mut select: Option<Option<std::path::PathBuf>> = None;
        ui.horizontal(|ui| {
            let selected = auto;
            if ui::radio_row(ui, &p, selected, "自动选择", "根据游戏版本自动挑选合适的 Java") {
                select = Some(None);
            }
        });
        for java in app.javas.clone() {
            let selected = app.cfg.java.path.as_deref() == Some(java.path.as_path());
            let subtitle = format!(
                "{} · {} · {}",
                java.path.display(),
                java.bits,
                java.vendor
            );
            ui.horizontal(|ui| {
                if ui::radio_row(ui, &p, selected, &java.display_name(), &subtitle) {
                    select = Some(Some(java.path.clone()));
                }
            });
        }
        if let Some(choice) = select {
            app.cfg.java.path = choice;
            app.cfg.java.auto = app.cfg.java.path.is_none();
            app.save_config();
        }
    });
}

// ------------------------------------------------------------------ 资讯

fn news_section(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "资讯");

        let mut enabled = app.cfg.news.enabled;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("在首页显示资讯")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::toggle(ui, &p, &mut enabled).changed() {
                    app.cfg.news.enabled = enabled;
                    app.save_config();
                    app.load_news();
                }
            });
        });
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("资讯源")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            let mut ids: Vec<String> = crate::core::news::PRESETS
                .iter()
                .map(|preset| preset.id.to_string())
                .collect();
            let mut labels: Vec<String> = crate::core::news::PRESETS
                .iter()
                .map(|preset| preset.name.to_string())
                .collect();
            ids.push("custom".into());
            labels.push("自定义地址".into());
            let current_index = ids
                .iter()
                .position(|id| id == &app.cfg.news.source)
                .unwrap_or(0);
            let current = labels[current_index].clone();
            if let Some(index) = ui::dropdown(ui, &p, "news_source", &current, &labels, 220.0) {
                app.cfg.news.source = ids[index].clone();
                app.save_config();
                app.load_news();
            }
            if let Some(preset) = crate::core::news::preset(&app.cfg.news.source) {
                ui.label(
                    egui::RichText::new(preset.description)
                        .font(ui::font(11.5))
                        .color(p.text_faint),
                );
            }
            if ui::button(ui, &p, ButtonOpts::secondary("立即刷新")).clicked() {
                app.load_news();
            }
        });
        ui.add_space(10.0);

        if app.cfg.news.source == "custom" {
            ui.label(
                egui::RichText::new("资讯源地址（JSON 数组或 RSS / Atom）")
                    .font(ui::font(12.5))
                    .color(p.text_weak),
            );
            ui.add_space(4.0);
            let mut url = app.cfg.news.url.clone();
            if ui::text_field(ui, &p, &mut url, "https://example.com/news.json", 460.0, false)
                .changed()
            {
                app.cfg.news.url = url;
                app.save_config();
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "JSON 格式示例：[{\"title\":\"标题\",\"summary\":\"摘要\",\"date\":\"2026-01-01\",\"url\":\"https://…\",\"image\":\"https://…\"}]",
                )
                .font(ui::font(11.0))
                .color(p.text_faint),
            );
            ui.add_space(10.0);
        }

        let mut minutes = app.cfg.news.refresh_minutes as f32;
        if ui::slider(ui, &p, "刷新间隔", &mut minutes, 5.0, 360.0, 5.0, " 分钟").changed() {
            app.cfg.news.refresh_minutes = minutes as u32;
            app.save_config();
        }
    });
}

// ------------------------------------------------------------------ 其他

fn misc_section(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        widgets::section_title(ui, &p, "文件与日志");

        ui.horizontal_wrapped(|ui| {
            if ui::button(ui, &p, ButtonOpts::secondary("打开启动器目录").icon("folder")).clicked() {
                paths::reveal(&paths::exe_dir());
            }
            if ui::button(ui, &p, ButtonOpts::secondary("打开日志目录").icon("terminal")).clicked() {
                let _ = paths::ensure_dir(&paths::logs_dir());
                paths::reveal(&paths::logs_dir());
            }
            if ui::button(ui, &p, ButtonOpts::secondary("打开配置文件").icon("save")).clicked() {
                paths::reveal(&paths::config_file());
            }
            if ui::button(ui, &p, ButtonOpts::secondary("打开缓存目录").icon("hard-drive")).clicked() {
                let _ = paths::ensure_dir(&paths::cache_dir());
                paths::reveal(&paths::cache_dir());
            }
        });
        ui.add_space(12.0);
        widgets::divider(ui, &p);
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("启动器数据")
                    .font(ui::font(12.5))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let config_size = std::fs::metadata(paths::config_file())
                    .map(|m| m.len())
                    .unwrap_or(0);
                let accounts_size = std::fs::metadata(paths::accounts_file())
                    .map(|m| m.len())
                    .unwrap_or(0);
                ui.label(
                    egui::RichText::new(format!(
                        "配置 {} · 账号 {}",
                        paths::format_size(config_size),
                        paths::format_size(accounts_size)
                    ))
                    .font(ui::font(11.5))
                    .color(p.text_faint),
                );
            });
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("所有配置、账号与主题都保存在启动器同目录，删除启动器文件夹即可完全卸载。")
                .font(ui::font(11.5))
                .color(p.text_faint),
        );
    });
}
