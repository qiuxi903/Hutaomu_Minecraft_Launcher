use egui::{Align, Layout, Ui};

use crate::app::{App, Page};
use crate::core::paths;
use crate::ui::{self, widgets, ButtonOpts};
use crate::views;

#[derive(Default)]
pub struct VersionsState {
    pub search: String,
    pub confirm_delete: Option<String>,
    pub detail: Option<String>,
    pub detail_memory: f32,
    pub detail_java: usize,
    pub detail_jvm: String,
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    widgets::page_title(
        ui,
        &p,
        "版本管理",
        "管理已安装的游戏版本，可以为单个版本单独配置内存与 Java",
    );

    ui.horizontal(|ui| {
        let mut search = app.versions_ui.search.clone();
        if ui::search_field(ui, &p, &mut search, "搜索版本", 240.0).changed() {
            app.versions_ui.search = search;
        }
        if ui::button(ui, &p, ButtonOpts::secondary("打开游戏目录").icon("folder")).clicked() {
            app.cfg.open_game_dir();
        }
        if ui::button(ui, &p, ButtonOpts::secondary("下载新版本").icon("download")).clicked() {
            app.page = Page::Download;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::icon_button(ui, &p, "refresh", "刷新列表").clicked() {
                app.refresh_local_versions();
            }
            ui.label(
                egui::RichText::new(format!("已安装 {} 个版本", app.local_versions.len()))
                    .font(ui::font(12.0))
                    .color(p.text_faint),
            );
        });
    });
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

    let keyword = app.versions_ui.search.trim().to_lowercase();
    let versions: Vec<_> = app
        .local_versions
        .iter()
        .filter(|v| keyword.is_empty() || v.id.to_lowercase().contains(&keyword))
        .cloned()
        .collect();

    if versions.is_empty() {
        ui::empty_state(ui, &p, "search", "没有匹配的版本", "");
        return;
    }

    let mut launch: Option<String> = None;
    let mut delete: Option<String> = None;
    let mut detail: Option<String> = None;
    let mut select: Option<String> = None;

    for version in &versions {
        let selected = app.cfg.selected_version.as_deref() == Some(version.id.as_str());
        ui::card_padded(ui, &p, 14, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&version.id)
                                .font(ui::font(15.0))
                                .color(p.text)
                                .strong(),
                        );
                        if let Some(loader) = &version.loader {
                            widgets::badge(ui, &p, loader, views::loader_color(Some(loader)));
                        }
                        widgets::badge(
                            ui,
                            &p,
                            &version.mc_version,
                            views::version_type_color(&version.version_type),
                        );
                        if selected {
                            widgets::badge(ui, &p, "当前使用", p.accent);
                        }
                    });
                    ui.add_space(3.0);
                    let modified = version
                        .last_modified
                        .map(|t| {
                            let datetime: chrono::DateTime<chrono::Local> = t.into();
                            datetime.format("%Y-%m-%d %H:%M").to_string()
                        })
                        .unwrap_or_else(|| "未知".into());
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · 最近修改 {}",
                            paths::format_size(version.size),
                            modified
                        ))
                        .font(ui::font(11.5))
                        .color(p.text_faint),
                    );
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::button(ui, &p, ButtonOpts::primary("启动").icon("play")).clicked() {
                        select = Some(version.id.clone());
                        launch = Some(version.id.clone());
                    }
                    if ui::icon_button(ui, &p, "trash", "删除该版本").clicked() {
                        delete = Some(version.id.clone());
                    }
                    if ui::icon_button(ui, &p, "sliders", "单独配置").clicked() {
                        detail = Some(version.id.clone());
                    }
                    if ui::icon_button(ui, &p, "folder", "打开目录").clicked() {
                        views::open_version_dir(app, &version.id);
                    }
                    if !selected {
                        if ui::ghost(ui, &p, "设为当前").clicked() {
                            select = Some(version.id.clone());
                        }
                    }
                });
            });
        });
        ui.add_space(8.0);
    }

    if let Some(id) = select {
        app.cfg.selected_version = Some(id);
        app.save_config();
    }
    if let Some(_id) = launch {
        views::launch_game(app);
    }
    if let Some(id) = delete {
        app.versions_ui.confirm_delete = Some(id);
    }
    if let Some(id) = detail {
        app.versions_ui.detail_memory = app.cfg.effective_memory(&id) as f32;
        app.versions_ui.detail_jvm = app.cfg.effective_jvm_args(&id);
        app.versions_ui.detail_java = app
            .javas
            .iter()
            .position(|j| Some(&j.path) == app.cfg.effective_java(&id).as_ref())
            .unwrap_or(0);
        app.versions_ui.detail = Some(id);
    }

    delete_confirm(app, ui);
    detail_modal(app, ui);
}

fn delete_confirm(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.versions_ui.confirm_delete.clone() else {
        return;
    };
    let p = app.palette;
    let mut open = true;
    let mut confirmed = false;
    let ctx = ui.ctx().clone();
    widgets::modal(&ctx, &p, &mut open, "删除版本", 420.0, |ui| {
        ui.label(
            egui::RichText::new(format!("确定要删除版本 {id} 吗？"))
                .font(ui::font(13.5))
                .color(p.text),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("该版本目录下的存档、模组与配置会一并删除，且无法恢复。")
                .font(ui::font(12.0))
                .color(p.text_weak),
        );
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::danger("确认删除")).clicked() {
                    confirmed = true;
                }
                if ui::button(ui, &p, ButtonOpts::secondary("取消")).clicked() {
                    confirmed = false;
                }
            });
        });
    });

    if confirmed {
        let dir = crate::core::minecraft::version::version_dir(&app.cfg.game_dir, &id);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {
                app.success(format!("已删除 {id}"));
                if app.cfg.selected_version.as_deref() == Some(id.as_str()) {
                    app.cfg.selected_version = None;
                    app.save_config();
                }
                app.refresh_local_versions();
            }
            Err(err) => app.error(format!("删除失败：{err}")),
        }
        app.versions_ui.confirm_delete = None;
        open = false;
    }
    if !open {
        app.versions_ui.confirm_delete = None;
    }
}

fn detail_modal(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.versions_ui.detail.clone() else {
        return;
    };
    let p = app.palette;
    let mut open = true;
    let mut save = false;
    let mut cancel = false;
    let ctx = ui.ctx().clone();
    let title = format!("{id} 的启动配置");

    let java_labels: Vec<String> = app
        .javas
        .iter()
        .map(|j| format!("Java {} · {} · {}", j.version, j.bits, j.vendor))
        .collect();

    widgets::modal(&ctx, &p, &mut open, &title, 520.0, |ui| {
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
                .font(ui::font(13.0))
                .color(p.text_weak),
        );
        ui.add_space(4.0);
        let mut jvm = app.versions_ui.detail_jvm.clone();
        ui::text_field(ui, &p, &mut jvm, "例如 -XX:+UseG1GC", 480.0, false);
        app.versions_ui.detail_jvm = jvm;

        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::primary("保存")).clicked() {
                    save = true;
                }
                if ui::button(ui, &p, ButtonOpts::secondary("取消")).clicked() {
                    cancel = true;
                }
            });
        });
    });

    if cancel {
        open = false;
    }
    if save {
        let java_path = app
            .javas
            .get(app.versions_ui.detail_java)
            .map(|j| j.path.clone());
        let override_cfg = crate::core::config::VersionOverride {
            memory_mb: Some(app.versions_ui.detail_memory as u32),
            java_path,
            jvm_args: Some(app.versions_ui.detail_jvm.clone()),
            game_dir: None,
        };
        app.cfg.version_overrides.insert(id.clone(), override_cfg);
        app.save_config();
        app.success(format!("{id} 的配置已保存"));
        open = false;
    }
    if !open {
        app.versions_ui.detail = None;
    }
}
