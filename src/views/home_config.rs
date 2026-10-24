//! 首页的二级「游戏配置」弹窗。
//!
//! 首页只保留摘要与入口按钮，版本、内存、Java、目录等通用设置
//! 统一放在「设置 - 游戏」分类里，这里提供快捷入口。

use egui::{Align, Layout, Ui};

use crate::app::{App, Page};
use crate::ui::{self, widgets, ButtonOpts};

pub fn game_config_modal(app: &mut App, ctx: &egui::Context) {
    if !app.home_ui.config_open {
        return;
    }
    let p = app.palette;
    let mut open = true;
    let mut close = false;

    widgets::modal(ctx, &p, &mut open, "游戏配置", 520.0, |ui| {
        ui.label(
            egui::RichText::new("修改会立即生效并保存。完整设置请到「设置 - 游戏」。")
                .font(ui::font(12.0))
                .color(p.text_weak),
        );
        ui.add_space(14.0);

        version_row(app, ui);
        ui.add_space(8.0);
        memory_row(app, ui);
        ui.add_space(8.0);
        java_row(app, ui, &mut close);
        ui.add_space(8.0);
        game_dir_row(app, ui);
        ui.add_space(8.0);
        toggle_rows(app, ui);

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::primary("完成")).clicked() {
                    close = true;
                }
                if ui::button(ui, &p, ButtonOpts::secondary("打开完整设置")).clicked() {
                    app.page = Page::Settings;
                    app.settings_ui.tab = 2;
                    close = true;
                }
            });
        });
    });

    app.home_ui.config_open = open && !close;
}

fn version_row(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("游戏版本")
                .font(ui::font(13.0))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // 按实例列出：一个 Minecraft 版本一条
            let ids: Vec<String> = app
                .instances
                .iter()
                .map(|inst| {
                    let loader = inst.loader_label();
                    if loader == "原版" {
                        inst.mc_version.clone()
                    } else {
                        format!("{} · {}", inst.mc_version, loader)
                    }
                })
                .collect();
            if ids.is_empty() {
                if ui::button(ui, &p, ButtonOpts::secondary("去下载版本")).clicked() {
                    app.page = Page::Download;
                }
            } else {
                let current = app
                    .selected_version()
                    .and_then(|v| {
                        app.instances
                            .iter()
                            .find(|i| i.mc_version == v.mc_version)
                            .map(|i| {
                                let loader = i.loader_label();
                                if loader == "原版" {
                                    i.mc_version.clone()
                                } else {
                                    format!("{} · {}", i.mc_version, loader)
                                }
                            })
                    })
                    .unwrap_or_else(|| ids[0].clone());
                if let Some(index) = ui::dropdown(ui, &p, "modal_version", &current, &ids, 300.0) {
                    if let Some(inst) = app.instances.get(index) {
                        if let Some(active) = inst.active_version() {
                            app.cfg.selected_version = Some(active.id.clone());
                            app.save_config();
                            app.refresh_local_versions();
                        }
                    }
                }
            }
        });
    });
}

fn memory_row(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let mc_version = app
        .selected_version()
        .map(|v| v.mc_version.clone())
        .unwrap_or_default();
    let mut memory = app.cfg.effective_memory(&mc_version) as f32;
    if ui::slider(ui, &p, "最大内存", &mut memory, 512.0, 32768.0, 256.0, " MB").changed() {
        app.cfg.java.memory_mb = memory as u32;
        app.save_config();
    }
}

fn java_row(app: &mut App, ui: &mut Ui, open: &mut bool) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Java")
                .font(ui::font(13.0))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::button(ui, &p, ButtonOpts::ghost("更换")).clicked() {
                app.page = Page::Settings;
                app.settings_ui.tab = 3;
                *open = false;
            }
            let mc_version = app
                .selected_version()
                .map(|v| v.mc_version.clone())
                .unwrap_or_default();
            match app.java_for(&mc_version) {
                Some(java) => {
                    ui.label(
                        egui::RichText::new(format!("Java {} · {}", java.version, java.vendor))
                            .font(ui::font(12.5))
                            .color(p.text),
                    );
                }
                None if app.java_searching => {
                    ui.label(
                        egui::RichText::new("正在搜索 Java ...")
                            .font(ui::font(12.5))
                            .color(p.text_weak),
                    );
                }
                None => {
                    ui.label(
                        egui::RichText::new("未找到可用的 Java")
                            .font(ui::font(12.5))
                            .color(p.danger),
                    );
                }
            }
        });
    });
}

fn game_dir_row(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("游戏目录")
                .font(ui::font(13.0))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::button(ui, &p, ButtonOpts::ghost("更改")).clicked() {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("选择游戏目录")
                    .pick_folder()
                {
                    app.cfg.set_game_dir(dir);
                    app.save_config();
                    app.refresh_local_versions();
                }
            }
            if ui::button(ui, &p, ButtonOpts::ghost("打开")).clicked() {
                app.cfg.open_game_dir();
            }
            let text = app.cfg.game_dir_display();
            let short = if text.chars().count() > 40 {
                let tail: String = text.chars().skip(text.chars().count() - 38).collect();
                format!("…{tail}")
            } else {
                text
            };
            ui.label(
                egui::RichText::new(short)
                    .font(ui::font(11.5))
                    .color(p.text_weak),
            );
        });
    });
}

fn toggle_rows(app: &mut App, ui: &mut Ui) {
    let p = app.palette;

    let mut isolation = app.cfg.launch.isolation;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("版本隔离")
                .font(ui::font(13.0))
                .color(p.text_weak),
        );
        ui.label(
            egui::RichText::new("每个版本独立的 mods、存档与配置")
                .font(ui::font(11.0))
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
}
