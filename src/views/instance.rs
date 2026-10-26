//! 实例详情页的分区
//!
//! 参考 PCL-CE 的实例页来排：概览、启动选项、加载器、导出。
//!
//! 概览分成四块，跟 PCL-CE 一样：
//! - 实例信息：用的什么版本、启动过几次、占多大地方
//! - 个性化：图标、名字、描述、分类、收藏
//! - 快捷方式：一眼打开实例里的各个文件夹
//! - 高级管理：导出、补全、重命名、删除这类不常用但要有的操作
//!
//! 路径这类内部信息不往页面上摆，需要的时候点「打开目录」自己看。

use std::path::PathBuf;

use egui::{Align, Layout, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::core::minecraft::export::{self, ExportOptions};
use crate::core::minecraft::loaders::{self, LoaderKind};
use crate::core::minecraft::shader_loader;
use crate::core::minecraft::version;
use crate::core::minecraft::version_icon;
use crate::core::minecraft::{GameInstance, LocalVersion};
use crate::core::paths;
use crate::icons;
use crate::ui::{self, widgets, ButtonOpts};

// ------------------------------------------------------------------ 概览

/// 概览：这个实例现在是什么样
pub fn overview_tab(
    app: &mut App,
    ui: &mut Ui,
    inst: &GameInstance,
    active: &LocalVersion,
) {
    let p = app.palette;
    let stat = app.cfg.stat_of(&active.id);
    // 整合包实例的 id 是自己的名字，Minecraft 版本要看标记文件里记的
    let mc_version = active
        .modpack
        .as_ref()
        .map(|m| m.mc_version.clone())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| inst.real_mc_version());
    let game_dir = app.cfg.effective_game_dir(&inst.mc_version);
    let instance_id = inst.mc_version.clone();

    // ---------------- 加载器 ----------------
    ui::card(ui, &p, |ui| {
        // 卡片宽度统一，否则短内容的卡片会缩成一小块
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "加载器");
        ui.label(
            egui::RichText::new("模组要靠加载器才能用。换了加载器，模组、光影和存档还是同一份。")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        if inst.variants.len() > 1 {
            let labels: Vec<String> = inst.variants.iter().map(|v| v.variant_label()).collect();
            let current = labels
                .iter()
                .position(|l| *l == active.variant_label())
                .unwrap_or(0);
            let current_label = labels[current].clone();
            widgets::setting_row(
                ui,
                &p,
                "当前加载器",
                "切换之后就用这个加载器启动游戏",
                |ui| {
                    if let Some(index) =
                        ui::dropdown(ui, &p, "detail_variant", &current_label, &labels, 230.0)
                    {
                        let picked = inst.variants[index].id.clone();
                        app.cfg.selected_version = Some(picked);
                        app.save_config();
                        app.refresh_local_versions();
                    }
                },
            );
        } else {
            widgets::setting_row(ui, &p, "当前加载器", "还没有别的加载器", |ui| {
                ui.label(
                    egui::RichText::new(active.variant_label())
                        .font(ui::font(13.5))
                        .color(p.accent)
                        .strong(),
                );
            });
        }

        if inst.is_modpack {
            // 整合包的加载器由整合包自己决定，换了也对不上模组
            widgets::setting_row(ui, &p, "更换加载器", "整合包的加载器由整合包决定", |ui| {
                ui.label(
                    egui::RichText::new("整合包自带")
                        .font(ui::font(12.5))
                        .color(p.text_faint),
                );
            });
        } else {
            widgets::setting_row(
                ui,
                &p,
                "更换加载器",
                "给这个版本再装一个加载器，两边共用同一份模组与存档",
                |ui| {
                    if ui::button(ui, &p, ButtonOpts::secondary("更换加载器").icon("refresh"))
                        .clicked()
                    {
                        crate::views::versions::open_loaders_page(app, &mc_version);
                    }
                },
            );
        }
    });
    ui.add_space(12.0);

    // ---------------- 光影 ----------------
    let shader = shader_loader::detect(&game_dir);
    let loader_kind = active
        .loader
        .as_deref()
        .and_then(|l| LoaderKind::from_id(&l.to_lowercase()));
    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "光影");
        match &shader {
            Some(installed) => {
                let name = if installed.version.is_empty() {
                    installed.kind.label().to_string()
                } else {
                    format!("{} {}", installed.kind.label(), installed.version)
                };
                widgets::setting_row(ui, &p, "光影加载器", "光影包要靠它才能加载", |ui| {
                    ui.label(
                        egui::RichText::new(&name)
                            .font(ui::font(13.5))
                            .color(p.accent)
                            .strong(),
                    );
                });
                widgets::setting_row(
                    ui,
                    &p,
                    "更换光影加载器",
                    "装好之后就能在左边的「光影」里挑光影包了",
                    |ui| {
                        // 控件区是右到左排的，卸载写在前面，画面上「更换」才在左边
                        if ui::button(ui, &p, ButtonOpts::danger("卸载").icon("trash")).clicked() {
                            let file = installed.file.clone();
                            crate::views::versions::uninstall_shader(app, &file);
                        }
                        if ui::button(ui, &p, ButtonOpts::secondary("更换").icon("refresh")).clicked()
                        {
                            app.versions_ui.shader_modal = Some(instance_id.clone());
                        }
                    },
                );
            }
            None => {
                if loader_kind.is_none() {
                    ui.label(
                        egui::RichText::new("原版不带光影支持，先装一个加载器再来装光影加载器。")
                            .font(ui::font(12.0))
                            .color(p.text_faint),
                    );
                } else {
                    widgets::setting_row(ui, &p, "光影加载器", "当前还没有安装", |ui| {
                        ui.label(
                            egui::RichText::new("未安装")
                                .font(ui::font(13.5))
                                .color(p.text_faint),
                        );
                    });
                    widgets::setting_row(
                        ui,
                        &p,
                        "安装光影加载器",
                        "装好之后就能在左边的「光影」里挑光影包了",
                        |ui| {
                            if ui::button(
                                ui,
                                &p,
                                ButtonOpts::secondary("安装").icon("download-arrow"),
                            )
                            .clicked()
                            {
                                app.versions_ui.shader_modal = Some(instance_id.clone());
                            }
                        },
                    );
                }
            }
        }
    });
    ui.add_space(12.0);

    // ---------------- 实例信息 ----------------
    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "实例信息");
        widgets::key_value(ui, &p, "Minecraft 版本", &mc_version);
        ui.add_space(8.0);
        widgets::key_value(
            ui,
            &p,
            "版本类型",
            crate::views::versions::version_type_label(&active.version_type),
        );
        ui.add_space(8.0);
        widgets::key_value(ui, &p, "加载器", &active.variant_label());
        ui.add_space(8.0);
        if let Some(info) = &active.modpack {
            widgets::key_value(ui, &p, "整合包", &info.name);
            ui.add_space(8.0);
        }
        widgets::key_value(ui, &p, "占用空间", &paths::format_size(inst.size()));
        ui.add_space(8.0);
        widgets::key_value(ui, &p, "启动次数", &format!("{} 次", stat.launches));
        ui.add_space(8.0);
        if stat.last_launch > 0 {
            let dt = chrono::DateTime::from_timestamp(stat.last_launch, 0)
                .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();
            widgets::key_value(ui, &p, "最后启动", &dt);
            ui.add_space(8.0);
        }
    });
    ui.add_space(12.0);

    // ---------------- 个性化 ----------------
    personalization_card(app, ui, inst);
    ui.add_space(12.0);

    // ---------------- 快捷方式 ----------------
    shortcuts_card(app, ui, inst, &game_dir);
    ui.add_space(12.0);

    // ---------------- 高级管理 ----------------
    advanced_card(app, ui, inst, active);
}

/// 个性化：图标、名字、描述、分类与收藏
fn personalization_card(app: &mut App, ui: &mut Ui, inst: &GameInstance) {
    let p = app.palette;
    let instance_id = inst.mc_version.clone();
    let mut meta = app.cfg.instance_meta(&instance_id);
    let mut changed = false;

    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "个性化");
        ui.label(
            egui::RichText::new("给这个实例起个名字、写句备注，方便自己分清楚。")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        // ---- 图标 ----
        let version_dir = version::version_dir(&app.cfg.game_dir, &instance_id);
        let icon_path = version_icon::custom_icon(&version_dir);
        widgets::setting_row(ui, &p, "实例图标", "换个图标更好认", |ui| {
            ui.horizontal(|ui| {
                // 控件区是右到左排的，先写的靠右，这里倒着写，
                // 画面上才是「预览 选择图标 恢复默认」
                if icon_path.is_some()
                    && ui::button(ui, &p, ButtonOpts::ghost("恢复默认")).clicked()
                {
                    if let Err(err) = version_icon::clear_custom_icon(&version_dir) {
                        app.error(format!("恢复默认图标失败：{err}"));
                    } else {
                        app.refresh_local_versions();
                    }
                }
                if ui::button(ui, &p, ButtonOpts::secondary("选择图标").icon("image")).clicked() {
                    pick_custom_icon(app, &version_dir);
                }
                if let Some(path) = &icon_path {
                    if let Some(texture) = app.file_icon_texture(path) {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
                        ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
                        ui.painter().image(
                            texture.id(),
                            rect.shrink(2.0),
                            Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                } else {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
                    ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
                    icons::paint(
                        ui.ctx(),
                        ui.painter(),
                        inst.category.icon(),
                        rect.shrink(7.0),
                        p.text_faint,
                    );
                }
            });
        });

        // ---- 名字 ----
        widgets::setting_row(ui, &p, "实例名字", "留空就用默认显示名", |ui| {
            if ui::text_field(ui, &p, &mut meta.name, "默认名字", 220.0, false).changed() {
                changed = true;
            }
        });

        // ---- 描述 ----
        widgets::setting_row(ui, &p, "实例描述", "写点备注，比如「和朋友一起玩的」", |ui| {
            if ui::text_field(ui, &p, &mut meta.description, "没有描述", 220.0, false).changed() {
                changed = true;
            }
        });

        // ---- 分类 ----
        widgets::setting_row(ui, &p, "分类", "给实例分个类，比如「联机」「画质」", |ui| {
            if ui::text_field(ui, &p, &mut meta.category, "没有分类", 220.0, false).changed() {
                changed = true;
            }
        });

        // ---- 收藏 ----
        let mut favorite = meta.favorite;
        widgets::setting_row(ui, &p, "收藏", "标出来后在列表里一眼看到", |ui| {
            if ui::toggle(ui, &p, &mut favorite).changed() {
                meta.favorite = favorite;
                changed = true;
            }
        });
    });

    if changed {
        app.cfg.set_instance_meta(&instance_id, meta);
        app.save_config();
    }
}

/// 选一张图片当实例图标
fn pick_custom_icon(app: &mut App, version_dir: &std::path::Path) {
    let Some(path) = rfd::FileDialog::new()
        .set_title("选择实例图标")
        .add_filter("图片", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
        .pick_file()
    else {
        return;
    };
    match version_icon::set_custom_icon(version_dir, &path) {
        Ok(()) => {
            app.refresh_local_versions();
            app.success("图标已换好");
        }
        Err(err) => app.error(format!("设置图标失败：{err}")),
    }
}

/// 快捷方式：直接打开实例里的各个文件夹
fn shortcuts_card(app: &mut App, ui: &mut Ui, inst: &GameInstance, game_dir: &std::path::Path) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "快捷方式");
        ui.label(
            egui::RichText::new("点一下直接打开对应的文件夹。")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(12.0);

        let entries: Vec<(&str, &str, PathBuf)> = vec![
            ("实例目录", "folder", game_dir.to_path_buf()),
            ("模组", "mod", game_dir.join("mods")),
            ("存档", "world", game_dir.join("saves")),
            ("配置", "settings", game_dir.join("config")),
            ("光影", "shader", game_dir.join("shaderpacks")),
            ("资源包", "resourcepack", game_dir.join("resourcepacks")),
            ("截图", "image", game_dir.join("screenshots")),
        ];

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.spacing_mut().item_spacing.y = 8.0;
            for (label, icon, path) in entries {
                let exists = path.exists();
                let button = ui::button(
                    ui,
                    &p,
                    ButtonOpts::secondary(label).icon(icon).enabled(exists),
                );
                if button.clicked() {
                    // 没有的目录顺手建出来，省得点了没反应
                    let _ = paths::ensure_dir(&path);
                    paths::reveal(&path);
                }
                if !exists {
                    button.on_hover_text("目录还不存在，点一下会创建");
                }
            }
        });
        let _ = inst;
    });
}

/// 高级管理：不常用但要有的操作
fn advanced_card(app: &mut App, ui: &mut Ui, inst: &GameInstance, active: &LocalVersion) {
    let p = app.palette;
    let instance_id = inst.mc_version.clone();
    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "高级管理");
        ui.label(
            egui::RichText::new("导出、补全、删除这些操作，出问题的时候用得上。")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.spacing_mut().item_spacing.y = 8.0;
            if ui::button(ui, &p, ButtonOpts::secondary("导出整合包").icon("upload")).clicked() {
                app.versions_ui.section = "export";
            }
            if ui::button(ui, &p, ButtonOpts::secondary("补全文件").icon("download")).clicked() {
                repair_instance(app, &active.id);
            }
            if ui::button(ui, &p, ButtonOpts::secondary("测试游戏").icon("play")).clicked() {
                app.cfg.selected_version = Some(active.id.clone());
                app.save_config();
                crate::views::launch_game(app);
            }
            if ui::button(ui, &p, ButtonOpts::secondary("重命名").icon("edit")).clicked() {
                app.versions_ui.rename_target = Some(active.id.clone());
                app.versions_ui.rename_value = active.id.clone();
            }
            if ui::button(ui, &p, ButtonOpts::secondary("复制版本").icon("copy")).clicked() {
                crate::views::versions::copy_version(app, &active.id);
            }
            if ui::button(ui, &p, ButtonOpts::secondary("版本 JSON").icon("code")).clicked() {
                let path = version::version_json_path(&app.cfg.game_dir, &active.id);
                paths::reveal(&path);
            }
            if ui::button(ui, &p, ButtonOpts::danger("删除版本").icon("trash")).clicked() {
                app.versions_ui.confirm_delete = Some(active.id.clone());
            }
        });
        let _ = instance_id;
    });
}

/// 补全实例缺掉的文件
///
/// 重新走一遍依赖库与资源的下载流程，已经有的文件会跳过，
/// 缺的那些会补回来。
fn repair_instance(app: &mut App, version_id: &str) {
    let game_dir = app.cfg.game_dir.clone();
    let opts = app.cfg.download.install_options();
    let version_id = version_id.to_string();
    app.spawn_download(format!("补全 {version_id}"), move |dl, progress, _job| {
        crate::core::minecraft::install::install_version(
            &dl,
            &game_dir,
            &version_id,
            &progress,
            opts,
        )?;
        Ok(format!("{version_id} 的文件已补齐"))
    });
}

// ------------------------------------------------------------------ 启动选项

/// 启动选项与内存分配
///
/// 这里的设置只对当前实例生效，会覆盖全局设置。
pub fn setup_tab(app: &mut App, ui: &mut Ui, inst: &GameInstance) {
    let p = app.palette;
    let instance_id = inst.mc_version.clone();

    // 内存三档：跟随全局 / 自动 / 自定义
    // 下面这些都放在状态里，改动写回去，页面切走再回来不会丢
    let mut memory_mode = app.versions_ui.detail_memory_mode.clone();
    let mut memory = app.versions_ui.detail_memory;
    let mut initial_memory = app.versions_ui.detail_initial_memory;
    let mut java_index = app.versions_ui.detail_java;
    let mut jvm_args = app.versions_ui.detail_jvm.clone();
    let mut extra_args = app.versions_ui.detail_extra_args.clone();
    let mut custom_title = app.versions_ui.detail_title.clone();
    let mut isolation = app.versions_ui.detail_isolation;
    let mut game_dir_text = app.versions_ui.detail_dir.clone();

    ui.label(
        egui::RichText::new("这里的设置只对当前实例生效，会覆盖全局设置。")
            .font(ui::font(12.0))
            .color(p.text_weak),
    );
    ui.add_space(14.0);

    // ---------------- 启动选项 ----------------
    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "启动选项");

        // ---- 版本隔离 ----
        widgets::setting_row(
            ui,
            &p,
            "版本隔离",
            "每个实例使用独立的模组、存档与配置目录",
            |ui| {
                if ui::toggle(ui, &p, &mut isolation).changed() {
                    app.versions_ui.detail_isolation = isolation;
                }
            },
        );
        let isolated_dir = version::version_dir(&app.cfg.game_dir, &instance_id);
        ui.label(
            egui::RichText::new(if isolation {
                format!("该实例的游戏目录：{}", isolated_dir.display())
            } else {
                "该实例与其它实例共用游戏目录".to_string()
            })
            .font(ui::font(11.5))
            .color(p.text_faint),
        );
        ui.add_space(14.0);

        // ---- 窗口标题 ----
        widgets::setting_row(
            ui,
            &p,
            "游戏窗口标题",
            "留空就用游戏自己的标题",
            |ui| {
                if ui::text_field(ui, &p, &mut custom_title, "留空用默认标题", 220.0, false)
                    .changed()
                {
                    app.versions_ui.detail_title = custom_title.clone();
                }
            },
        );

        // ---- Java ----
        let java_labels: Vec<String> = app
            .javas
            .iter()
            .map(|j| format!("Java {} · {} · {}", j.version, j.bits, j.vendor))
            .collect();
        widgets::setting_row(ui, &p, "游戏 Java", "这个实例用哪个 Java 启动", |ui| {
            if java_labels.is_empty() {
                ui.label(
                    egui::RichText::new("尚未搜索到 Java")
                        .font(ui::font(12.5))
                        .color(p.text_faint),
                );
            } else {
                let index = java_index.min(java_labels.len() - 1);
                let current = java_labels[index].clone();
                if let Some(selected) =
                    ui::dropdown(ui, &p, "detail_java", &current, &java_labels, 300.0)
                {
                    java_index = selected;
                    app.versions_ui.detail_java = selected;
                }
            }
        });

        // ---- 额外启动参数 ----
        widgets::setting_row(
            ui,
            &p,
            "额外启动参数",
            "跟在游戏参数后面，比如 --fullscreen",
            |ui| {
                if ui::text_field(ui, &p, &mut extra_args, "留空表示没有", 220.0, false).changed() {
                    app.versions_ui.detail_extra_args = extra_args.clone();
                }
            },
        );

        // ---- 额外 JVM 参数 ----
        widgets::setting_row(
            ui,
            &p,
            "额外 JVM 参数",
            "传给 Java 的参数，比如 -XX:+UseG1GC",
            |ui| {
                if ui::text_field(ui, &p, &mut jvm_args, "留空表示没有", 220.0, false).changed() {
                    app.versions_ui.detail_jvm = jvm_args.clone();
                }
            },
        );

        // ---- 独立游戏目录 ----
        widgets::setting_row(
            ui,
            &p,
            "独立游戏目录",
            "留空表示跟随全局 / 版本隔离",
            |ui| {
                ui.horizontal(|ui| {
                    // 这一块是右到左排的，先加的靠右，所以按钮先写
                    if ui::button(ui, &p, ButtonOpts::secondary("选择")).clicked() {
                        if let Some(dir) = rfd::FileDialog::new()
                            .set_title("选择该版本的游戏目录")
                            .pick_folder()
                        {
                            game_dir_text = dir.display().to_string();
                            app.versions_ui.detail_dir = game_dir_text.clone();
                        }
                    }
                    if ui::text_field(ui, &p, &mut game_dir_text, "留空使用默认目录", 240.0, false)
                        .changed()
                    {
                        app.versions_ui.detail_dir = game_dir_text.clone();
                    }
                });            },
        );
    });
    ui.add_space(12.0);

    // ---------------- 内存分配 ----------------
    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "内存分配");
        ui.label(
            egui::RichText::new("内存给少了游戏会卡，给多了又占着机器不放。")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        // ---- 三档 ----
        let modes = ["跟随全局", "自动配置", "自定义"];
        let mut index = match memory_mode.as_str() {
            "auto" => 1,
            "custom" => 2,
            _ => 0,
        };
        widgets::setting_row(
            ui,
            &p,
            "分配方式",
            "自动配置按机器的物理内存推算，自定义自己填",
            |ui| {
                if ui::segmented(ui, &p, "detail_memory_mode", &mut index, &modes).clicked() {
                    memory_mode = match index {
                        1 => "auto".to_string(),
                        2 => "custom".to_string(),
                        _ => "global".to_string(),
                    };
                    app.versions_ui.detail_memory_mode = memory_mode.clone();
                }
            },
        );

        let total_memory = crate::core::java::total_memory_mb();
        let effective = match memory_mode.as_str() {
            "auto" => app.cfg.recommended_memory(),
            "custom" => memory as u32,
            _ => app.cfg.java.memory_mb,
        };

        // ---- 内存条 ----
        if total_memory > 0 {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("内存占用")
                        .font(ui::font(13.0))
                        .color(p.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "机器共 {} · 这个实例分到 {}",
                            paths::format_size(total_memory as u64 * 1024 * 1024),
                            paths::format_size(effective as u64 * 1024 * 1024),
                        ))
                        .font(ui::font(12.0))
                        .color(p.text_faint),
                    );
                });
            });
            ui.add_space(6.0);
            let width = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
            ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
            let ratio = (effective as f32 / total_memory as f32).clamp(0.0, 1.0);
            if ratio > 0.0 {
                let filled = Rect::from_min_max(
                    rect.left_top(),
                    egui::pos2(rect.left() + rect.width() * ratio, rect.bottom()),
                );
                ui.painter().rect_filled(filled, p.corner_sm(), p.accent);
            }
            ui.add_space(16.0);
        }

        // ---- -Xmx ----
        if memory_mode == "custom" {
            if ui::slider(
                ui,
                &p,
                "最大内存 -Xmx",
                "游戏最多能用多少内存",
                &mut memory,
                512.0,
                32768.0,
                256.0,
                " MB",
            )
            .changed()
            {
                app.versions_ui.detail_memory = memory;
            }
            ui.add_space(6.0);
            if ui::slider(
                ui,
                &p,
                "初始内存 -Xms",
                "游戏一启动就占多少内存，0 表示不设置",
                &mut initial_memory,
                0.0,
                16384.0,
                256.0,
                " MB",
            )
            .changed()
            {
                app.versions_ui.detail_initial_memory = initial_memory;
            }
            ui.add_space(6.0);
        } else {
            ui.label(
                egui::RichText::new(match memory_mode.as_str() {
                    "auto" => "按机器的物理内存自动推算，不用自己填。",
                    _ => "用全局设置里的内存分配，在「设置 → 游戏」里改。",
                })
                .font(ui::font(12.0))
                .color(p.text_faint),
            );
            ui.add_space(12.0);
        }
    });
    ui.add_space(14.0);

    // ---------------- 保存 ----------------
    ui.horizontal(|ui| {
        if ui::button(ui, &p, ButtonOpts::primary("保存该实例设置")).clicked() {
            let java_path = app.javas.get(java_index).map(|j| j.path.clone());
            let dir_value = game_dir_text.trim().to_string();
            let override_new = crate::core::config::VersionOverride {
                memory_mb: if memory_mode == "custom" {
                    Some(memory as u32)
                } else {
                    None
                },
                memory_mode: Some(memory_mode.clone()),
                initial_memory_mb: Some(initial_memory as u32),
                java_path,
                jvm_args: Some(jvm_args.clone()),
                extra_args: Some(extra_args.clone()),
                game_dir: if dir_value.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(dir_value))
                },
                isolation: Some(isolation),
                custom_title: Some(custom_title.clone()),
            };
            app.cfg
                .version_overrides
                .insert(instance_id.clone(), override_new);
            app.save_config();
            app.success(format!("{} 的配置已保存", instance_id));
        }
        if ui::button(ui, &p, ButtonOpts::secondary("恢复全局设置")).clicked() {
            app.cfg.version_overrides.remove(&instance_id);
            app.save_config();
            crate::views::versions::load_detail(app, &instance_id);
            app.info("已恢复为全局设置");
        }
    });
}

// ------------------------------------------------------------------ 加载器

/// 加载器列表：已经装了哪些，还能装哪些
pub fn loaders_tab(
    app: &mut App,
    ui: &mut Ui,
    inst: &GameInstance,
    active: &LocalVersion,
) {
    let p = app.palette;
    let instance_id = inst.mc_version.clone();
    let mc = inst.real_mc_version();

    // ---------------- 已安装 ----------------
    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "已安装的加载器");
        ui.label(
            egui::RichText::new("同一个版本可以装好几个加载器，模组与存档还是共用一份。")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        for variant in &inst.variants {
            let is_active = variant.id == active.id;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(variant.variant_label())
                        .font(ui::font(13.5))
                        .color(if is_active { p.accent } else { p.text })
                        .strong(),
                );
                if is_active {
                    widgets::badge(ui, &p, "正在用", p.accent);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // 右到左排的，先写的靠右，这里倒着写，
                    // 画面上才是「设为当前 移除」
                    if variant.id != active.id
                        && variant.loader.is_some()
                        && ui::button(ui, &p, ButtonOpts::ghost("移除").icon("trash")).clicked()
                    {
                        remove_variant(app, &variant.id);
                    }
                    if !is_active
                        && ui::button(ui, &p, ButtonOpts::secondary("设为当前")).clicked()
                    {
                        app.cfg.selected_version = Some(variant.id.clone());
                        app.save_config();
                        app.refresh_local_versions();
                    }
                });
            });
            ui.add_space(8.0);
        }
    });
    ui.add_space(12.0);

    // ---------------- 安装新的 ----------------
    if inst.is_modpack {
        ui::card(ui, &p, |ui| {
            ui.set_width(ui.available_width());
            widgets::section_title(ui, &p, "安装加载器");
            ui.label(
                egui::RichText::new("整合包的加载器由整合包自己决定，换了会对不上里面的模组。")
                    .font(ui::font(12.5))
                    .color(p.text_faint),
            );
        });
        return;
    }

    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "安装加载器");
        ui.label(
            egui::RichText::new("新加载器会装成这个版本的一个变体，模组、存档和光影跟现在这份共用。")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        let kinds = LoaderKind::all();
        let labels: Vec<&str> = kinds.iter().map(|k| k.label()).collect();
        let kind = app.versions_ui.loader_kind.unwrap_or(kinds[0]);
        let mut index = kinds.iter().position(|k| *k == kind).unwrap_or(0);
        if ui::segmented(ui, &p, "loaders_kind", &mut index, &labels).clicked() {
            app.versions_ui.loader_kind = Some(kinds[index]);
            app.versions_ui.loader_versions.clear();
            app.versions_ui.loader_version_index = 0;
            app.versions_ui.loader_error = None;
            app.versions_ui.loader_mc.clear();
            app.versions_ui.loader_loading = false;
        }
        ui.add_space(10.0);

        let kind = app.versions_ui.loader_kind.unwrap_or(kinds[0]);
        ui.label(
            egui::RichText::new(kind.description())
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(12.0);

        let supported = loaders::supports(kind, &mc);
        if !supported {
            ui.label(
                egui::RichText::new(format!("{} 不支持 Minecraft {mc}", kind.label()))
                    .font(ui::font(12.0))
                    .color(p.warning),
            );
            ui.add_space(8.0);
        }

        // ---- 版本 ----
        widgets::setting_row(ui, &p, "加载器版本", "自动挑一个推荐的稳定版", |ui| {
            ui.vertical(|ui| {
                let modes = ["自动选择", "手动选择"];
                let mut mode = if app.versions_ui.loader_auto { 0 } else { 1 };
                if ui::segmented(ui, &p, "loaders_mode", &mut mode, &modes).clicked() {
                    app.versions_ui.loader_auto = mode == 0;
                    if mode == 1 {
                        crate::views::versions::load_loader_versions(app, &mc, kind);
                    } else {
                        app.versions_ui.loader_versions.clear();
                        app.versions_ui.loader_error = None;
                    }
                }
                ui.add_space(8.0);
                if app.versions_ui.loader_auto {
                    ui.label(
                        egui::RichText::new("装推荐的稳定版就行")
                            .font(ui::font(12.0))
                            .color(p.text_faint),
                    );
                } else {
                    crate::views::versions::loader_version_picker(app, ui, &mc, kind);
                }
            });
        });

        ui.add_space(6.0);
        let mut picked_version = None;
        if !app.versions_ui.loader_auto {
            picked_version = crate::views::versions::picked_loader_version(app);
        }
        if ui::button(
            ui,
            &p,
            ButtonOpts::primary("安装并切换")
                .icon("download-arrow")
                .enabled(supported),
        )
        .clicked()
        {
            crate::views::versions::install_loader(app, &instance_id, kind, picked_version);
        }
    });
}

/// 移除一个加载器变体
///
/// 只删加载器自己的目录。原版那一版是整个实例的底子，删了实例就没了。
fn remove_variant(app: &mut App, variant_id: &str) {
    let game_dir = app.cfg.game_dir.clone();
    let dir = version::version_dir(&game_dir, variant_id);
    if !dir.is_dir() {
        app.error("找不到这个加载器版本");
        return;
    }
    if let Err(err) = std::fs::remove_dir_all(&dir) {
        app.error(format!("删除失败：{err}"));
        return;
    }
    // 删掉的正好是当前用的，要换一个顶上
    if app.cfg.selected_version.as_deref() == Some(variant_id) {
        app.cfg.selected_version = None;
    }
    app.save_config();
    app.refresh_local_versions();
    app.success("已移除这个加载器");
}

// ------------------------------------------------------------------ 导出

/// 导出整合包
pub fn export_tab(app: &mut App, ui: &mut Ui, inst: &GameInstance, active: &LocalVersion) {
    let p = app.palette;
    let instance_id = inst.mc_version.clone();
    let game_dir = app.cfg.effective_game_dir(&instance_id);
    let display_name = inst.display_name.clone();
    // 游戏本体要导当前用的那个变体，原版和加载器的版本描述不一样
    let variant_id = active.id.clone();

    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "导出整合包");
        ui.label(
            egui::RichText::new("挑要带走的内容，打包成一个 zip，解压到游戏的 .minecraft 目录就能用。")
                .font(ui::font(12.0))
                .color(p.text_faint),
        );
        ui.add_space(14.0);

        let mut options = app.versions_ui.export_options;
        for (label, checked) in options.items() {
            let mut flag = checked;
            if widgets::checkbox(ui, &p, &mut flag, label).changed() {
                options = match label {
                    "游戏本体" => ExportOptions { game: flag, ..options },
                    "模组" => ExportOptions { mods: flag, ..options },
                    "资源包" => ExportOptions { resourcepacks: flag, ..options },
                    "光影包" => ExportOptions { shaderpacks: flag, ..options },
                    "单人存档" => ExportOptions { saves: flag, ..options },
                    "服务器列表" => ExportOptions { servers: flag, ..options },
                    _ => ExportOptions { others: flag, ..options },
                };
                app.versions_ui.export_options = options;
            }
        }
        ui.add_space(14.0);

        // ---- 目标文件 ----
        let mut path_text = app.versions_ui.export_path.clone();
        if path_text.trim().is_empty() {
            path_text = export::default_dest(&display_name).display().to_string();
        }
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("导出到")
                    .font(ui::font(13.0))
                    .color(p.text_weak),
            );
            if ui::text_field(ui, &p, &mut path_text, "选择保存位置", 420.0, false).changed() {
                app.versions_ui.export_path = path_text.clone();
            }
            if ui::button(ui, &p, ButtonOpts::secondary("选择")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("选择导出位置")
                    .set_file_name(&export::default_file_name(&display_name))
                    .save_file()
                {
                    path_text = path.display().to_string();
                    app.versions_ui.export_path = path_text.clone();
                }
            }
        });
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            if ui::button(
                ui,
                &p,
                ButtonOpts::primary("开始导出")
                    .icon("upload")
                    .enabled(!options.is_empty()),
            )
            .clicked()
            {
                let dest = PathBuf::from(path_text.trim());
                let result = export::export_instance(
                    &app.cfg.game_dir,
                    &game_dir,
                    &variant_id,
                    &display_name,
                    options,
                    &dest,
                );
                match result {
                    Ok(summary) => {
                        app.success(format!(
                            "已导出 {} 个文件（{}）到 {}",
                            summary.files,
                            paths::format_size(summary.bytes),
                            dest.display()
                        ));
                    }
                    Err(err) => app.error(format!("导出失败：{err}")),
                }
            }
            if options.is_empty() {
                ui.label(
                    egui::RichText::new("至少要选一项内容")
                        .font(ui::font(12.0))
                        .color(p.text_faint),
                );
            }
        });
    });
    ui.add_space(12.0);

    // ---- 目录里现在有什么 ----
    ui::card(ui, &p, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, &p, "这个实例里有什么");
        let rows: Vec<(&str, &str)> = vec![
            ("mods", "模组"),
            ("resourcepacks", "资源包"),
            ("shaderpacks", "光影包"),
            ("saves", "单人存档"),
            ("config", "配置"),
        ];
        for (folder, label) in rows {
            let dir = game_dir.join(folder);
            let count = std::fs::read_dir(&dir)
                .map(|entries| entries.flatten().count())
                .unwrap_or(0);
            widgets::key_value(ui, &p, label, &format!("{count} 个"));
        }
    });
}
