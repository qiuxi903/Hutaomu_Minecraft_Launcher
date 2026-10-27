//! 首页：顶部启动区 + 资讯（主内容）+ 皮肤预览与快捷入口（右栏）。
//!
//! 布局参考 PCL2 / HMCL / SJMCL 的通用做法：
//! 启动是最主要的动作，放在首屏最显眼的位置；资讯占据主内容区；
//! 皮肤预览与快捷入口作为次要信息放在右栏。

use egui::{Align, Color32, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::{App, Page};
use crate::core::news::NewsItem;
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
    launch_card(app, ui);
    ui.add_space(14.0);

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let total = ui.available_width();
        // 右边停靠了游戏日志面板，内容区会窄一截。
        // 窄到两栏都站不下就改成上下排，否则右栏跟着一起收窄。
        if total < 500.0 {
            ui.vertical(|ui| {
                news_section(app, ui);
                ui.add_space(14.0);
                skin_card(app, ui);
                ui.add_space(14.0);
                stats_card(app, ui);
                ui.add_space(14.0);
                shortcuts_card(app, ui);
            });
            return;
        }
        let right_width = (total - 320.0 - 14.0).clamp(220.0, 286.0);
        let left_width = total - right_width - 14.0;

        ui.vertical(|ui| {
            ui.set_width(left_width);
            news_section(app, ui);
        });

        ui.vertical(|ui| {
            ui.set_width(right_width);
            skin_card(app, ui);
            ui.add_space(14.0);
            stats_card(app, ui);
            ui.add_space(14.0);
            shortcuts_card(app, ui);
        });
    });
}

// ------------------------------------------------------------------ 启动区

fn launch_card(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let version_id = app.selected_version_id();
    let version = version_id
        .as_ref()
        .and_then(|id| app.local_versions.iter().find(|v| &v.id == id).cloned());
    let account = app.selected_account().cloned();
    if let Some(account) = &account {
        app.ensure_skin(account.id());
    }
    let head = account
        .as_ref()
        .and_then(|a| app.skin_of(a.id()))
        .map(|s| s.head.clone());

    let launch_rect = ui::card_padded(ui, &p, 18, |ui| {        ui.horizontal(|ui| {
            // 账号
            ui.vertical(|ui| {
                ui.set_width(190.0);
                ui.horizontal(|ui| {
                    if let Some(head) = &head {
                        let _ = ui::avatar(ui, &p, head, 40.0);
                    } else {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                        ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
                        icons::paint(ui.ctx(), ui.painter(), "user", rect.shrink(10.0), p.text_faint);
                    }
                    ui.add_space(2.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(
                                account.as_ref().map(|a| a.name()).unwrap_or("未选择账号"),
                            )
                            .font(ui::font(14.0))
                            .color(p.text)
                            .strong(),
                        );
                        ui.label(
                            egui::RichText::new(
                                account.as_ref().map(|a| a.kind_label()).unwrap_or("点击添加账号"),
                            )
                            .font(ui::font(11.0))
                            .color(p.text_faint),
                        );
                    });
                });
            });

            ui.add_space(8.0);

            // 版本选择 + 信息
            ui.vertical(|ui| {
                let width = (ui.available_width() - 210.0).clamp(220.0, 340.0);
                ui.set_width(width);
                // 按实例列出：一个实例一条，后面标出用的加载器
                let ids: Vec<String> = app
                    .instances
                    .iter()
                    .map(|inst| {
                        let loader = inst.loader_label();
                        // 独立实例用自己起的名字，默认实例还是版本号
                        let name = app.instance_display_name(inst);
                        if loader == "原版" {
                            name
                        } else {
                            format!("{} · {}", name, loader)
                        }
                    })
                    .collect();
                if ids.is_empty() {
                    if ui::button(
                        ui,
                        &p,
                        ButtonOpts::secondary("还没有安装版本，去下载")
                            .icon("download")
                            .width(width.min(320.0)),
                    )
                    .clicked()
                    {
                        app.page = Page::Download;
                    }
                } else {
                    let current = version_id
                        .clone()
                        .and_then(|id| {
                            app.local_versions
                                .iter()
                                .find(|v| v.id == id)
                                .map(|v| v.instance_id.clone())
                        })
                        .and_then(|id| {
                            app.instances
                                .iter()
                                .find(|i| i.instance_id == id)
                                .map(|i| {
                                    let loader = i.loader_label();
                                    let name = app.instance_display_name(i);
                                    if loader == "原版" {
                                        name
                                    } else {
                                        format!("{} · {}", name, loader)
                                    }
                                })
                        })
                        .unwrap_or_else(|| ids[0].clone());
                    if let Some(index) = ui::dropdown(ui, &p, "home_version", &current, &ids, width) {
                        if let Some(inst) = app.instances.get(index) {
                            if let Some(active) = inst.active_version() {
                                app.cfg.selected_version = Some(active.id.clone());
                                app.save_config();
                                app.refresh_local_versions();
                            }
                        }
                    }
                }
                ui.add_space(6.0);

                // 实例级设置按实例 id 取
                let instance_id = version
                    .as_ref()
                    .map(|v| v.instance_id.clone())
                    .unwrap_or_default();
                let java = app.java_for_current();
                let mut parts: Vec<String> = Vec::new();
                if let Some(v) = &version {
                    parts.push(format!("Minecraft {}", v.mc_version));
                    if let Some(loader) = &v.loader {
                        parts.push(loader.clone());
                    }
                }
                parts.push(match &java {
                    Some(java) => format!("Java {}", java.major),
                    None if app.java_searching => "Java 搜索中".into(),
                    None => "Java 未就绪".into(),
                });
                parts.push(format!(
                    "内存 {} MB",
                    app.cfg.effective_memory(&instance_id)
                ));
                ui.label(
                    egui::RichText::new(parts.join("  ·  "))
                        .font(ui::font(11.5))
                        .color(p.text_faint),
                );
            });

            // 启动按钮
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let running = app.game.is_some();
                let has_version = app.selected_version_id().is_some();
                let label = if running { "游戏运行中" } else { "启动游戏" };
                let clicked = ui::button(
                    ui,
                    &p,
                    ButtonOpts::primary(label)
                        .icon(if running { "pause" } else { "play" })
                        .width(186.0)
                        .height(56.0)
                        .font_size(16.5)
                        .enabled(!running && has_version),
                )
                .clicked();
                if clicked {
                    views::launch_game(app);
                }
            });
        });
    })
    .rect;

    // 启动区是首页的视觉焦点：左边一条主色条，卡片底下垫一层淡主色
    let rect = launch_rect;
    if ui.is_rect_visible(rect) {
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() + 12.0),
            egui::pos2(rect.left() + 3.0, rect.bottom() - 12.0),
        );
        ui.painter().rect_filled(bar, p.corner_sm(), p.accent);
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - 90.0),
                egui::pos2(rect.right(), rect.bottom()),
            ),
            p.corner(0),
            widgets::alpha(p.accent, if p.dark { 7 } else { 10 }),
        );
    }
}

// ------------------------------------------------------------------ 资讯

fn news_section(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("资讯")
                .font(ui::font(15.0))
                .color(p.text)
                .strong(),
        );
        if app.news_loading {
            ui::spinner(ui, &p, 14.0);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::icon_button(ui, &p, "refresh", "刷新资讯").clicked() {
                app.load_news();
            }
            if ui::ghost(ui, &p, "更换资讯源").clicked() {
                app.page = Page::Settings;
                app.settings_ui.tab = 4;
            }
            if let Some(source) = crate::core::news::preset(&app.cfg.news.source) {
                ui.label(
                    egui::RichText::new(source.name)
                        .font(ui::font(11.0))
                        .color(p.text_faint),
                );
            }
        });
    });
    ui.add_space(10.0);

    if let Some(err) = app.news_error.clone() {
        ui::inset(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui::icon(ui, "alert", 15.0, p.warning);
                ui.label(
                    egui::RichText::new(format!("资讯获取失败：{err}"))
                        .font(ui::font(12.0))
                        .color(p.text_weak),
                );
            });
        });
        ui.add_space(10.0);
    }

    let items: Vec<NewsItem> = app.news.clone();
    if items.is_empty() {
        ui::empty_state(ui, &p, "news", "暂无资讯", "可以在设置里更换资讯源");
        return;
    }

    let width = ui.available_width();
    for item in items.iter().take(12) {
        news_row(app, ui, item, width);
        ui.add_space(8.0);
    }
}

fn news_row(app: &mut App, ui: &mut Ui, item: &NewsItem, width: f32) -> egui::Response {
    let p = app.palette;
    let height = 104.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let clickable = !item.url.is_empty();
    let hovered = response.hovered() && clickable;
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), hovered, 0.12);

    let bg = widgets::lerp_color(p.surface, p.surface_alt, t * 0.75);
    let border = widgets::lerp_color(p.border, p.accent, t * 0.55);
    let painter = ui.painter();
    painter.rect_filled(rect, p.corner(0), bg);
    painter.rect_stroke(rect, p.corner(0), Stroke::new(1.0, border), StrokeKind::Inside);

    // 缩略图
    let mut text_left = rect.left() + 16.0;
    if let Some(url) = &item.image {
        let thumb = Rect::from_min_size(
            Pos2::new(rect.left() + 14.0, rect.center().y - 32.0),
            Vec2::new(112.0, 64.0),
        );
        app.ensure_image(url);
        match app.image(url) {
            Some(texture) => {
                painter.image(
                    texture.id(),
                    thumb,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                painter.rect_filled(thumb, p.corner_sm(), p.surface_alt);
                icons::paint(ui.ctx(), painter, "image", thumb.shrink(22.0), p.text_faint);
            }
        }
        text_left = thumb.right() + 14.0;
    }

    let text_right = rect.right() - 16.0;
    let text_width = (text_right - text_left).max(80.0);
    // 文本区域裁剪，避免任何情况下越界压到别的元素
    let text_clip = Rect::from_min_max(
        Pos2::new(text_left, rect.top() + 8.0),
        Pos2::new(text_right, rect.bottom() - 26.0),
    );
    let text_painter = painter.with_clip_rect(text_clip);

    let title_font = ui::font(14.0);
    let title = truncate_chars(&item.title, text_width, 14.0, 1);
    text_painter.text(
        Pos2::new(text_left, rect.top() + 20.0),
        egui::Align2::LEFT_CENTER,
        title,
        title_font,
        p.text,
    );

    if !item.summary.is_empty() {
        let summary = truncate_chars(&item.summary, text_width, 12.0, 2);
        let galley = text_painter.layout(summary, ui::font(12.0), p.text_weak, text_width);
        text_painter.galley(Pos2::new(text_left, rect.top() + 34.0), galley, p.text_weak);
    }

    // 底部：标签 + 日期 + 打开原文
    let bottom = rect.bottom() - 16.0;
    let mut x = text_left;
    if let Some(tag) = &item.tag {
        let galley = painter.layout_no_wrap(tag.clone(), ui::font(10.5), p.accent);
        let tag_rect = Rect::from_min_size(
            Pos2::new(x, bottom - 8.0),
            Vec2::new(galley.size().x + 12.0, 16.0),
        );
        painter.rect_filled(tag_rect, egui::CornerRadius::same(4), p.accent_weak);
        painter.galley(
            Pos2::new(tag_rect.left() + 6.0, tag_rect.center().y - galley.size().y / 2.0),
            galley,
            p.accent,
        );
        x = tag_rect.right() + 8.0;
    }
    if !item.date.is_empty() {
        painter.text(
            Pos2::new(x, bottom),
            egui::Align2::LEFT_CENTER,
            &item.date,
            ui::font(10.5),
            p.text_faint,
        );
    }
    if clickable {
        painter.text(
            Pos2::new(rect.right() - 16.0, bottom),
            egui::Align2::RIGHT_CENTER,
            "打开原文",
            ui::font(10.5),
            widgets::lerp_color(p.text_faint, p.accent, t),
        );
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            let _ = open::that(&item.url);
        }
    }

    response
}

/// 按可用宽度与行数粗略裁剪文本，超出部分用省略号
fn truncate_chars(text: &str, max_width: f32, font_size: f32, max_lines: usize) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let budget = max_width * max_lines as f32;
    let mut used = 0.0f32;
    let mut out = String::new();
    for ch in cleaned.chars() {
        // 中日韩字符按一个字号宽估算，西文按 0.55 估算
        let w = if ch.is_ascii() { font_size * 0.55 } else { font_size };
        if used + w > budget {
            out.push('…');
            return out;
        }
        used += w;
        out.push(ch);
    }
    out
}

// ------------------------------------------------------------------ 皮肤

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
    let cutout = skin.map(|s| s.cutout.clone());
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

        ui::skin3d::skin_preview(
            ui,
            &p,
            texture.as_ref(),
            cutout.as_ref(),
            model,
            has_overlay,
            &mut app.preview_yaw,
            236.0,
            app.cfg.ui.animations,
        );

        ui.add_space(12.0);
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

// ------------------------------------------------------------------ 快捷入口

fn shortcuts_card(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui::card(ui, &p, |ui| {
        ui.label(
            egui::RichText::new("快捷入口")
                .font(ui::font(14.5))
                .color(p.text)
                .strong(),
        );
        ui.add_space(8.0);

        let entries: [(&str, &str, Page); 4] = [
            ("download", "下载游戏与内容", Page::Download),
            ("layers", "版本管理", Page::Versions),
            ("user", "账号管理", Page::Accounts),
            ("settings", "启动器设置", Page::Settings),
        ];
        for (icon, label, page) in entries {
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
            if ui.is_rect_visible(rect) {
                let t = ui
                    .ctx()
                    .animate_bool_with_time(response.id.with("h"), response.hovered(), 0.12);
                if t > 0.0 {
                    ui.painter().rect_filled(
                        rect,
                        p.corner_sm(),
                        widgets::lerp_color(Color32::TRANSPARENT, p.surface_alt, t),
                    );
                }
                icons::paint(
                    ui.ctx(),
                    ui.painter(),
                    icon,
                    Rect::from_center_size(
                        Pos2::new(rect.left() + 18.0, rect.center().y),
                        Vec2::splat(17.0),
                    ),
                    widgets::lerp_color(p.text_weak, p.accent, t),
                );
                ui.painter().text(
                    Pos2::new(rect.left() + 38.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    ui::font(13.0),
                    widgets::lerp_color(p.text_weak, p.text, t),
                );
                icons::paint(
                    ui.ctx(),
                    ui.painter(),
                    "chevron-right",
                    Rect::from_center_size(
                        Pos2::new(rect.right() - 14.0, rect.center().y),
                        Vec2::splat(14.0),
                    ),
                    widgets::lerp_color(p.text_faint, p.accent, t),
                );
            }
            if response.clicked() {
                app.page = page;
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }

        ui.add_space(4.0);
        if ui::button(ui, &p, ButtonOpts::ghost("打开日志目录").icon("terminal")).clicked() {
            let _ = crate::core::paths::ensure_dir(&crate::core::paths::logs_dir());
            crate::core::paths::reveal(&crate::core::paths::logs_dir());
        }
    });
}

// ------------------------------------------------------------------ 概览统计

fn stats_card(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let total_size: u64 = app.instances.iter().map(|i| i.size()).sum();
    let last_launch = app
        .cfg
        .version_stats
        .values()
        .map(|s| s.last_launch)
        .max()
        .unwrap_or(0);

    ui::card(ui, &p, |ui| {
        ui.label(
            egui::RichText::new("概览")
                .font(ui::font(14.5))
                .color(p.text)
                .strong(),
        );
        ui.add_space(8.0);
        widgets::key_value(ui, &p, "已安装实例", &format!("{} 个", app.instances.len()));
        ui.add_space(4.0);
        widgets::key_value(ui, &p, "版本占用", &paths::format_size(total_size));
        ui.add_space(4.0);
        widgets::key_value(ui, &p, "账号", &format!("{} 个", app.accounts.accounts.len()));
        ui.add_space(4.0);
        widgets::key_value(
            ui,
            &p,
            "上次启动",
            &if last_launch > 0 {
                chrono::DateTime::from_timestamp(last_launch, 0)
                    .map(|t| {
                        t.with_timezone(&chrono::Local)
                            .format("%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "—".into())
            } else {
                "还没有启动过".to_string()
            },
        );
    });
}
