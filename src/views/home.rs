use egui::{Align, Color32, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::{App, Page};
use crate::core::paths;
use crate::icons;
use crate::ui::{self, widgets, ButtonOpts};
use crate::views;

#[derive(Default)]
pub struct HomeState {
    pub news_expanded: Option<usize>,
    /// 「游戏配置」二级弹窗是否打开
    pub config_open: bool,
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    hero(app, ui);
    ui.add_space(14.0);

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        ui.vertical(|ui| {
            ui.set_width(292.0);
            skin_card(app, ui);
        });
        ui.vertical(|ui| {
            let width = ui.available_width();
            ui.set_width(width);
            launch_card(app, ui);
            ui.add_space(14.0);
            quick_card(app, ui);
        });
    });

    ui.add_space(14.0);
    news_section(app, ui);
}

// ------------------------------------------------------------------ 顶部横幅

fn hero(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let width = ui.available_width();
    let height = 136.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, p.corner(2), p.surface);
    painter.rect_stroke(
        rect,
        p.corner(2),
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );

    // 右侧水印：柔和光晕 + 立方体，全部裁剪在卡片内部
    let clipped = painter.with_clip_rect(rect);
    let glow = Pos2::new(rect.right() - 4.0, rect.center().y);
    clipped.circle_filled(
        glow,
        112.0,
        widgets::alpha(p.accent, if p.dark { 26 } else { 13 }),
    );
    clipped.circle_filled(
        glow,
        78.0,
        widgets::alpha(p.accent, if p.dark { 22 } else { 10 }),
    );
    icons::paint(
        ui.ctx(),
        &clipped,
        "cube",
        Rect::from_center_size(glow, Vec2::splat(118.0)),
        widgets::alpha(p.accent, if p.dark { 34 } else { 20 }),
    );

    let account = app.selected_account().cloned();
    let greeting = match &account {
        Some(acc) => format!("欢迎回来，{}", acc.name()),
        None => "欢迎使用胡桃木启动器".to_string(),
    };
    painter.text(
        Pos2::new(rect.left() + 26.0, rect.top() + 36.0),
        egui::Align2::LEFT_CENTER,
        greeting,
        ui::font(22.0),
        p.text,
    );

    let version_id = app.selected_version_id();
    let version_info = version_id
        .as_ref()
        .and_then(|id| app.local_versions.iter().find(|v| &v.id == id).cloned());

    let subtitle = match &version_info {
        Some(v) => match &v.loader {
            Some(loader) => format!("{} · {} · Minecraft {}", v.id, loader, v.mc_version),
            None => format!("{} · Minecraft {}", v.id, v.mc_version),
        },
        None => "还没有安装游戏版本，先到「下载」页面安装一个吧".to_string(),
    };
    painter.text(
        Pos2::new(rect.left() + 26.0, rect.top() + 62.0),
        egui::Align2::LEFT_CENTER,
        subtitle,
        ui::font(13.0),
        p.text_weak,
    );

    // 徽标行
    let badges_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 26.0, rect.top() + 82.0),
        Vec2::new(rect.width() - 300.0, 26.0),
    );
    let account_kind = account.as_ref().map(|a| a.kind_label()).unwrap_or("未登录");
    let java_mc = version_info
        .as_ref()
        .map(|v| v.mc_version.clone())
        .unwrap_or_default();
    let java_label = match app.java_for(&java_mc) {
        Some(java) => format!("Java {}", java.major),
        None if app.java_searching => "Java 搜索中".to_string(),
        None => "Java 未就绪".to_string(),
    };
    let memory_label = format!(
        "{} MB",
        app.cfg
            .effective_memory(version_id.as_deref().unwrap_or(""))
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(badges_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            widgets::badge(ui, &p, account_kind, p.accent);
            widgets::badge(ui, &p, &java_label, p.text_weak);
            widgets::badge(ui, &p, &memory_label, p.text_weak);
            if let Some(v) = &version_info {
                widgets::badge(ui, &p, v.version_type_label(), views::version_type_color(&v.version_type));
            }
        },
    );

    // 启动按钮
    let running = app.game.is_some();
    let button_rect = Rect::from_min_size(
        Pos2::new(rect.right() - 214.0, rect.center().y - 26.0),
        Vec2::new(186.0, 52.0),
    );
    let label = if running { "游戏运行中" } else { "启动游戏" };
    let clicked = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(button_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui::button(
                    ui,
                    &p,
                    ButtonOpts::primary(label)
                        .icon(if running { "pause" } else { "play" })
                        .width(186.0)
                        .height(52.0)
                        .font_size(16.0)
                        .enabled(!running),
                )
                .clicked()
            },
        )
        .inner;

    if clicked {
        views::launch_game(app);
    }
}

// ------------------------------------------------------------------ 皮肤预览

fn skin_card(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let account = app.selected_account().cloned();
    let account_id = account.as_ref().map(|a| a.id().to_string());
    if let Some(id) = &account_id {
        app.ensure_skin(id);
    }
    let skin = account_id.as_ref().and_then(|id| app.skin_of(id));
    let model = skin.map(|s| s.model).unwrap_or_default();
    let texture = skin.map(|s| s.texture.clone());
    let has_overlay = skin.map(|s| s.has_overlay).unwrap_or(false);

    ui::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("皮肤预览")
                    .font(ui::font(14.5))
                    .color(p.text)
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::badge(ui, &p, model.short(), p.text_weak);
            });
        });
        ui.add_space(10.0);

        let yaw = &mut app.preview_yaw;
        ui::skin3d::skin_preview(
            ui,
            &p,
            texture.as_ref(),
            model,
            has_overlay,
            yaw,
            268.0,
            app.cfg.ui.animations,
        );

        ui.add_space(12.0);
        let name = account
            .as_ref()
            .map(|a| a.name().to_string())
            .unwrap_or_else(|| "未选择账号".into());
        let kind = account
            .as_ref()
            .map(|a| a.kind_label())
            .unwrap_or("请先添加账号");
        ui.label(
            egui::RichText::new(name)
                .font(ui::font(15.0))
                .color(p.text)
                .strong(),
        );
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(kind)
                .font(ui::font(12.0))
                .color(p.text_weak),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let half = (ui.available_width() - 8.0) / 2.0;
            if ui::button(ui, &p, ButtonOpts::secondary("切换账号").width(half)).clicked() {
                app.page = Page::Accounts;
            }
            if ui::button(ui, &p, ButtonOpts::secondary("更换皮肤").width(half)).clicked() {
                app.page = Page::Accounts;
                app.accounts_ui.tab = 0;
            }
        });
    });
}

// ------------------------------------------------------------------ 游戏配置（二级入口）

fn launch_card(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("游戏配置")
                    .font(ui::font(14.5))
                    .color(p.text)
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::button(ui, &p, ButtonOpts::secondary("打开配置").icon("sliders")).clicked() {
                    app.home_ui.config_open = true;
                }
            });
        });
        ui.add_space(10.0);

        let version = app
            .selected_version_id()
            .and_then(|id| app.local_versions.iter().find(|v| v.id == id).cloned());

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("当前版本")
                    .font(ui::font(12.5))
                    .color(p.text_weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                match &version {
                    Some(v) => {
                        widgets::badge(ui, &p, &v.mc_version, p.accent);
                        if let Some(loader) = &v.loader {
                            widgets::badge(ui, &p, loader, views::loader_color(Some(loader)));
                        }
                        ui.label(
                            egui::RichText::new(&v.id)
                                .font(ui::font(13.0))
                                .color(p.text)
                                .strong(),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("尚未安装版本")
                                .font(ui::font(12.5))
                                .color(p.text_faint),
                        );
                    }
                }
            });
        });
        ui.add_space(4.0);

        let version_id = version.as_ref().map(|v| v.id.clone()).unwrap_or_default();
        let mc_version = version.as_ref().map(|v| v.mc_version.clone()).unwrap_or_default();
        widgets::key_value(
            ui,
            &p,
            "最大内存",
            &format!("{} MB", app.cfg.effective_memory(&version_id)),
        );
        widgets::key_value(
            ui,
            &p,
            "Java",
            &match app.java_for(&mc_version) {
                Some(java) => format!("Java {} · {}", java.version, java.vendor),
                None if app.java_searching => "正在搜索 ...".into(),
                None => "未找到可用的 Java".into(),
            },
        );

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("版本、内存、Java、目录等通用设置已移到「设置 - 游戏」。")
                .font(ui::font(11.0))
                .color(p.text_faint),
        );
    });
}

// ------------------------------------------------------------------ 快捷操作

fn quick_card(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        ui.label(
            egui::RichText::new("快捷操作")
                .font(ui::font(14.5))
                .color(p.text)
                .strong(),
        );
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            let actions: [(&str, &str, Page); 4] = [
                ("download", "下载游戏", Page::Download),
                ("layers", "版本管理", Page::Versions),
                ("user", "账号管理", Page::Accounts),
                ("settings", "启动器设置", Page::Settings),
            ];
            for (icon, label, page) in actions {
                if ui::button(
                    ui,
                    &p,
                    ButtonOpts::secondary(label)
                        .icon(icon)
                        .height(36.0)
                        .font_size(13.0),
                )
                .clicked()
                {
                    app.page = page;
                }
            }
            if ui::button(
                ui,
                &p,
                ButtonOpts::secondary("打开日志")
                    .icon("terminal")
                    .height(36.0)
                    .font_size(13.0),
            )
            .clicked()
            {
                let _ = paths::ensure_dir(&paths::logs_dir());
                paths::reveal(&paths::logs_dir());
            }
        });
    });
}

// ------------------------------------------------------------------ 资讯

fn news_section(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("资讯")
                .font(ui::font(14.5))
                .color(p.text)
                .strong(),
        );
        if app.news_loading {
            ui::spinner(ui, &p, 14.0);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::ghost(ui, &p, "绑定资讯源").clicked() {
                app.page = Page::Settings;
            }
            if ui::icon_button(ui, &p, "refresh", "刷新资讯").clicked() {
                app.load_news();
            }
        });
    });
    ui.add_space(8.0);

    if let Some(err) = app.news_error.clone() {
        ui::inset(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui::icon(ui, "alert", 15.0, p.warning);
                ui.label(
                    egui::RichText::new(format!("资讯获取失败：{err}"))
                        .font(ui::font(12.5))
                        .color(p.text_weak),
                );
            });
        });
        ui.add_space(8.0);
    }

    let items: Vec<_> = app.news.clone();
    if items.is_empty() {
        ui::empty_state(ui, &p, "news", "暂无资讯", "可以在设置里绑定自己的资讯源");
        return;
    }

    let width = ui.available_width();
    let columns = if width > 1000.0 { 3 } else { 2 };
    let card_width = (width - 14.0 * (columns as f32 - 1.0)) / columns as f32;

    for chunk in items.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            for item in chunk {
                ui.vertical(|ui| {
                    ui.set_width(card_width);
                    let response = ui::card_padded(ui, &p, 14, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(tag) = &item.tag {
                                widgets::badge(ui, &p, tag, p.accent);
                            }
                            if !item.date.is_empty() {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(short_date(&item.date))
                                            .font(ui::font(11.0))
                                            .color(p.text_faint),
                                    );
                                });
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(&item.title)
                                .font(ui::font(14.0))
                                .color(p.text)
                                .strong(),
                        );
                        if !item.summary.is_empty() {
                            ui.add_space(5.0);
                            ui.label(
                                egui::RichText::new(&item.summary)
                                    .font(ui::font(12.5))
                                    .color(p.text_weak),
                            );
                        }
                    });
                    if !item.url.is_empty() {
                        let clickable = response.interact(Sense::click());
                        if clickable.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if clickable.clicked() {
                            let _ = open::that(&item.url);
                        }
                    }
                });
            }
        });
        ui.add_space(14.0);
    }
}

fn short_date(date: &str) -> String {
    let trimmed = date.trim();
    if trimmed.len() >= 10 && trimmed.as_bytes()[4] == b'-' {
        trimmed[..10].to_string()
    } else {
        trimmed.chars().take(16).collect()
    }
}

impl crate::core::minecraft::LocalVersion {
    pub fn version_type_label(&self) -> &'static str {
        match self.version_type.as_str() {
            "release" => "正式版",
            "snapshot" => "快照",
            "old_beta" => "远古 Beta",
            "old_alpha" => "远古 Alpha",
            _ => "自定义",
        }
    }
}

pub fn _unused(_: Color32) {}
