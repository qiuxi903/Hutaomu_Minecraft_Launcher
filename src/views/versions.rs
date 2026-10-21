//! 版本管理：左侧版本列表 + 右侧详情（概览 / 设置 / 存档 / 截图）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Align, Layout, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::{App, Page};
use crate::core::minecraft::version;
use crate::core::paths;
use crate::icons;
use crate::ui::{self, widgets, ButtonOpts};
use crate::views;

pub struct VersionsState {
    pub search: String,
    /// 0 全部 / 1 正式版 / 2 快照 / 3 加载器 / 4 其他
    pub filter: usize,
    pub selected: Option<String>,
    /// 0 概览 / 1 设置 / 2 存档 / 3 截图
    pub tab: usize,
    pub detail_memory: f32,
    pub detail_java: usize,
    pub detail_jvm: String,
    pub detail_dir: String,
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
            selected: None,
            tab: 0,
            detail_memory: 4096.0,
            detail_java: 0,
            detail_jvm: String::new(),
            detail_dir: String::new(),
            confirm_delete: None,
            rename_target: None,
            rename_value: String::new(),
            screenshots: HashMap::new(),
            loaded_for: None,
        }
    }
}

const FILTERS: [&str; 5] = ["全部", "正式版", "快照", "加载器", "其他"];
const TABS: [&str; 4] = ["概览", "设置", "存档", "截图"];

pub fn ui(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(
        ui,
        &p,
        "版本管理",
        "查看版本信息、单独配置启动参数，管理存档与截图",
    );

    toolbar(app, ui);
    ui.add_space(12.0);

    if app.local_versions.is_empty() {
        ui::empty_state(
            ui,
            &p,
            "layers",
            "还没有安装任何版本",
            "到「下载」页面安装游戏本体或加载器",
        );
        return;
    }

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.vertical(|ui| {
            ui.set_width(320.0);
            version_list(app, ui);
        });
        ui.vertical(|ui| {
            let width = ui.available_width();
            ui.set_width(width);
            version_detail(app, ui);
        });
    });

    delete_modal(app, ui);
    rename_modal(app, ui);
}

// ------------------------------------------------------------------ 工具栏

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
            if ui::button(ui, &p, ButtonOpts::secondary("下载新版本").icon("download")).clicked() {
                app.page = Page::Download;
            }
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let mut filter = app.versions_ui.filter;
        if ui::segmented(ui, &p, &mut filter, &FILTERS).clicked() {
            app.versions_ui.filter = filter;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("共 {} 个版本", app.local_versions.len()))
                    .font(ui::font(11.5))
                    .color(p.text_faint),
            );
        });
    });
}

// ------------------------------------------------------------------ 左侧列表

fn version_list(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let keyword = app.versions_ui.search.trim().to_lowercase();
    let filter = app.versions_ui.filter;
    let versions: Vec<_> = app
        .local_versions
        .iter()
        .filter(|v| match filter {
            1 => v.loader.is_none() && v.version_type == "release",
            2 => v.version_type == "snapshot",
            3 => v.loader.is_some(),
            4 => v.loader.is_none() && v.version_type != "release" && v.version_type != "snapshot",
            _ => true,
        })
        .filter(|v| keyword.is_empty() || v.id.to_lowercase().contains(&keyword))
        .cloned()
        .collect();

    if app.versions_ui.selected.is_none() {
        app.versions_ui.selected = app
            .cfg
            .selected_version
            .clone()
            .or_else(|| versions.first().map(|v| v.id.clone()));
    }

    ui::card_padded(ui, &p, 8, |ui| {
        ui.set_width(304.0);
        if versions.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("没有匹配的版本")
                        .font(ui::font(12.5))
                        .color(p.text_weak),
                );
            });
            ui.add_space(20.0);
            return;
        }

        egui::ScrollArea::vertical()
            .max_height(560.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(296.0);
                let mut select: Option<String> = None;
                for version in &versions {
                    let selected = app.versions_ui.selected.as_deref() == Some(version.id.as_str());
                    let is_current = app.cfg.selected_version.as_deref() == Some(version.id.as_str());
                    let response = ui.scope(|ui| {
                        egui::Frame::NONE
                            .fill(if selected { p.accent_weak } else { p.surface })
                            .stroke(egui::Stroke::new(
                                1.0,
                                if selected { p.accent } else { p.border },
                            ))
                            .corner_radius(p.corner_sm())
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (rect, _) =
                                        ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                                    ui.painter().rect_filled(
                                        rect,
                                        p.corner_sm(),
                                        if selected { p.surface } else { p.surface_alt },
                                    );
                                    icons::paint(
                                        ui.ctx(),
                                        ui.painter(),
                                        if version.loader.is_some() { "layers" } else { "cube" },
                                        rect.shrink(8.0),
                                        if selected { p.accent } else { p.text_weak },
                                    );
                                    ui.add_space(2.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(&version.id)
                                                    .font(ui::font(13.0))
                                                    .color(p.text)
                                                    .strong(),
                                            );
                                            if is_current {
                                                widgets::badge(ui, &p, "当前", p.accent);
                                            }
                                        });
                                        ui.add_space(2.0);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            widgets::badge(
                                                ui,
                                                &p,
                                                &version.mc_version,
                                                views::version_type_color(&version.version_type),
                                            );
                                            if let Some(loader) = &version.loader {
                                                widgets::badge(
                                                    ui,
                                                    &p,
                                                    loader,
                                                    views::loader_color(Some(loader)),
                                                );
                                            }
                                            ui.label(
                                                egui::RichText::new(paths::format_size(version.size))
                                                    .font(ui::font(10.5))
                                                    .color(p.text_faint),
                                            );
                                        });
                                    });
                                });
                            })
                            .response
                    })
                    .inner;
                    let clickable = response.interact(Sense::click());
                    if clickable.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if clickable.clicked() {
                        select = Some(version.id.clone());
                    }
                    ui.add_space(6.0);
                }
                if let Some(id) = select {
                    load_detail(app, &id);
                    app.versions_ui.selected = Some(id);
                }
            });
    });
}

// ------------------------------------------------------------------ 右侧详情

fn version_detail(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let Some(id) = app.versions_ui.selected.clone() else {
        ui::card(ui, &p, |ui| {
            ui::empty_state(ui, &p, "layers", "选择一个版本", "右侧会显示版本详情与操作");
        });
        return;
    };
    let Some(info) = app.local_versions.iter().find(|v| v.id == id).cloned() else {
        app.versions_ui.selected = None;
        return;
    };
    if app.versions_ui.loaded_for.as_deref() != Some(id.as_str()) {
        load_detail(app, &id);
    }

    ui::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&info.id)
                            .font(ui::font(19.0))
                            .color(p.text)
                            .strong(),
                    );
                    if let Some(loader) = &info.loader {
                        widgets::badge(ui, &p, loader, views::loader_color(Some(loader)));
                    }
                    widgets::badge(
                        ui,
                        &p,
                        &info.mc_version,
                        views::version_type_color(&info.version_type),
                    );
                    if app.cfg.selected_version.as_deref() == Some(info.id.as_str()) {
                        widgets::badge(ui, &p, "当前使用", p.accent);
                    }
                });
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {} 个文件 · {}",
                        paths::format_size(info.size),
                        count_files(&version::version_dir(&app.cfg.game_dir, &info.id)),
                        info.last_modified
                            .map(|t| {
                                let dt: chrono::DateTime<chrono::Local> = t.into();
                                dt.format("%Y-%m-%d %H:%M").to_string()
                            })
                            .unwrap_or_else(|| "未知时间".into())
                    ))
                    .font(ui::font(11.5))
                    .color(p.text_faint),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::primary("启动").icon("play")).clicked() {
                    app.cfg.selected_version = Some(info.id.clone());
                    app.save_config();
                    views::launch_game(app);
                }
                if app.cfg.selected_version.as_deref() != Some(info.id.as_str()) {
                    if ui::button(ui, &p, ButtonOpts::secondary("设为当前")).clicked() {
                        app.cfg.selected_version = Some(info.id.clone());
                        app.save_config();
                    }
                }
            });
        });
        ui.add_space(12.0);

        let mut tab = app.versions_ui.tab;
        if ui::segmented(ui, &p, &mut tab, &TABS).clicked() {
            app.versions_ui.tab = tab;
        }
        ui.add_space(12.0);

        match app.versions_ui.tab {
            0 => overview_tab(app, ui, &info),
            1 => settings_tab(app, ui, &info),
            2 => saves_tab(app, ui, &info),
            _ => screenshots_tab(app, ui, &info),
        }
    });
}

fn count_files(dir: &Path) -> usize {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count()
}

fn overview_tab(app: &mut App, ui: &mut Ui, info: &crate::core::minecraft::LocalVersion) {
    let p = app.palette;
    let dir = version::version_dir(&app.cfg.game_dir, &info.id);
    let json_path = version::version_json_path(&app.cfg.game_dir, &info.id);
    let inherits = version::read_local_json(&app.cfg.game_dir, &info.id)
        .ok()
        .and_then(|json| json.inherits_from)
        .unwrap_or_default();
    let stat = app.cfg.stat_of(&info.id);
    let game_dir = app.cfg.effective_game_dir(&info.id);

    widgets::key_value(ui, &p, "版本 ID", &info.id);
    ui.add_space(6.0);
    widgets::key_value(ui, &p, "Minecraft 版本", &info.mc_version);
    ui.add_space(6.0);
    widgets::key_value(
        ui,
        &p,
        "加载器",
        info.loader.as_deref().unwrap_or("原版"),
    );
    ui.add_space(6.0);
    if !inherits.is_empty() {
        widgets::key_value(ui, &p, "继承自", &inherits);
        ui.add_space(6.0);
    }
    widgets::key_value(ui, &p, "版本类型", info.version_type.as_str());
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
    widgets::key_value(ui, &p, "占用空间", &paths::format_size(info.size));
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
            app.versions_ui.rename_target = Some(info.id.clone());
            app.versions_ui.rename_value = info.id.clone();
        }
        if ui::button(ui, &p, ButtonOpts::secondary("复制版本").icon("copy")).clicked() {
            copy_version(app, &info.id);
        }
        if ui::button(ui, &p, ButtonOpts::danger("删除版本").icon("trash")).clicked() {
            app.versions_ui.confirm_delete = Some(info.id.clone());
        }
    });
}

fn load_detail(app: &mut App, id: &str) {
    let ov = app.cfg.version_override(id);
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
    app.versions_ui.loaded_for = Some(id.to_string());
}

fn settings_tab(app: &mut App, ui: &mut Ui, info: &crate::core::minecraft::LocalVersion) {
    let p = app.palette;

    ui.label(
        egui::RichText::new("这里的设置只对当前版本生效，会覆盖全局设置。")
            .font(ui::font(12.0))
            .color(p.text_weak),
    );
    ui.add_space(14.0);

    let mut memory = app.versions_ui.detail_memory;
    if ui::slider(ui, &p, "最大内存", &mut memory, 512.0, 32768.0, 256.0, " MB").changed() {
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
        if ui::button(ui, &p, ButtonOpts::primary("保存该版本设置")).clicked() {
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
            };
            app.cfg.version_overrides.insert(info.id.clone(), override_new);
            app.save_config();
            app.success(format!("{} 的配置已保存", info.id));
        }
        if ui::button(ui, &p, ButtonOpts::secondary("恢复全局设置")).clicked() {
            app.cfg.version_overrides.remove(&info.id);
            app.save_config();
            load_detail(app, &info.id);
            app.info("已恢复为全局设置");
        }
    });
}

fn saves_tab(app: &mut App, ui: &mut Ui, info: &crate::core::minecraft::LocalVersion) {
    let p = app.palette;
    let game_dir = app.cfg.effective_game_dir(&info.id);
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

fn screenshots_tab(app: &mut App, ui: &mut Ui, info: &crate::core::minecraft::LocalVersion) {
    let p = app.palette;
    let game_dir = app.cfg.effective_game_dir(&info.id);
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
                app.versions_ui.selected = Some(new_id.clone());
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
    if let Some(mut ov) = app.cfg.version_overrides.remove(old_id) {
        if let Some(dir) = ov.game_dir.take() {
            if dir.starts_with(&old_dir) {
                ov.game_dir = Some(new_dir.join(dir.strip_prefix(&old_dir).unwrap_or(&dir)));
            } else {
                ov.game_dir = Some(dir);
            }
        }
        app.cfg.version_overrides.insert(new_id.to_string(), ov);
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
            app.versions_ui.selected = Some(new_id.clone());
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
                app.versions_ui.selected = None;
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
