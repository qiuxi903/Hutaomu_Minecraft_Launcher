//! 悬浮下载中心：显示所有进行中的任务、字节进度、速度与当前动作。

use egui::{Align, CornerRadius, Layout, Margin, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::{App, TaskItem};
use crate::core::paths;
use crate::icons;
use crate::ui;
use crate::ui::widgets;
use crate::theme::Palette;

struct TaskSnapshot {
    id: u64,
    title: String,
    fraction: f32,
    message: String,
    cancellable: bool,
    bytes_done: u64,
    bytes_total: u64,
    speed: f32,
    elapsed: std::time::Duration,
}

impl From<&TaskItem> for TaskSnapshot {
    fn from(task: &TaskItem) -> Self {
        Self {
            id: task.id,
            title: task.title.clone(),
            fraction: task.fraction,
            message: task.message.clone(),
            cancellable: task.cancellable,
            bytes_done: task.bytes_done,
            bytes_total: task.bytes_total,
            speed: task.speed,
            elapsed: task.started.elapsed(),
        }
    }
}

impl App {
    pub fn download_panel(&mut self, ctx: &egui::Context) {
        let active: Vec<TaskSnapshot> = self
            .tasks
            .iter()
            .filter(|task| !task.done)
            .map(TaskSnapshot::from)
            .collect();

        // 开合都走同一条动画：关闭时不是啪地消失，而是缩回去
        let target = if self.download_panel_open { 1.0 } else { 0.0 };
        let anim = crate::ui::anim::to_value(
            ctx,
            egui::Id::new("htmcl_download_panel_anim"),
            target,
            crate::ui::anim::DUR_BASE,
        );

        if anim <= 0.001 {
            // 完全收起了，只留一个胶囊提示
            if active.is_empty() {
                return;
            }
            if self.download_chip(ctx, &active) {
                self.download_panel_open = true;
                self.download_panel_auto = false;
            }
            return;
        }

        let palette = self.palette;
        let mut close = false;
        let mut cancel: Option<u64> = None;

        // 面板从侧栏「下载中心」按钮的位置滑出来：向右一点、从下往上升。
        // 收起时顺着原路缩回去，不是啪地消失。
        let shift = (1.0 - anim) * 18.0;
        // 用户拖过之后记住新位置
        let mut offset = Vec2::new(
            self.cfg.ui.download_panel_offset[0],
            self.cfg.ui.download_panel_offset[1],
        );
        let screen = ctx.content_rect();
        // 锚在左下角，往右与往上拖；拖出屏幕的量全部收掉
        let limit = Vec2::new(
            (screen.width() - 420.0).max(0.0),
            (screen.height() - 160.0).max(0.0),
        );
        // 左下角锚点：往右与往上拖
        offset = offset.clamp(Vec2::new(0.0, -limit.y), Vec2::new(limit.x, 0.0));
        let mut dragged = Vec2::ZERO;
        let sidebar_width = if self.cfg.ui.sidebar_collapsed { 64.0 } else { 186.0 };

        egui::Area::new(egui::Id::new("htmcl_download_panel"))
            .order(egui::Order::Foreground)
            .anchor(
                egui::Align2::LEFT_BOTTOM,
                Vec2::new(
                    sidebar_width + 8.0 + offset.x,
                    -12.0 + shift + offset.y,
                ),
            )
            .show(ctx, |ui| {
                ui.set_opacity(anim);
                ui.set_width(400.0);
                let radius = palette.corner(2);
                let response = egui::Frame::NONE
                    .fill(palette.surface)
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(radius)
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        let mut header_rect = Rect::NOTHING;
                        ui.horizontal(|ui| {
                            icons::paint(
                                ui.ctx(),
                                ui.painter(),
                                "download",
                                Rect::from_center_size(
                                    Pos2::new(ui.cursor().left() + 8.0, ui.cursor().center().y),
                                    Vec2::splat(16.0),
                                ),
                                palette.accent,
                            );
                            ui.add_space(24.0);
                            widgets::card_title(ui, &palette, "下载中心");
                            if !active.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!("{} 个任务", active.len()))
                                        .font(ui::font(11.5))
                                        .color(palette.text_faint),
                                );
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui::icon_button(ui, &palette, "close", "收起").clicked() {
                                    close = true;
                                }
                            });
                            header_rect = ui.min_rect();
                        });
                        // 标题栏可以拖着挪位置
                        dragged = crate::ui::widgets::drag_title_bar(
                            ui,
                            header_rect,
                            ui.id().with("download_drag"),
                        );
                        ui.add_space(8.0);

                        if active.is_empty() {
                            ui.label(
                                egui::RichText::new("暂无进行中的下载任务")
                                    .font(ui::font(12.5))
                                    .color(palette.text_weak),
                            );
                            return;
                        }

                        egui::ScrollArea::vertical()
                            .max_height(400.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                // 任务卡片依次淡入，不会整片糊上来
                                for (i, task) in active.iter().enumerate() {
                                    let delay = ui::anim::stagger(i, 0.03);
                                    let appear = ui::anim::fade_in(ui.ctx(), ui.id().with(("task", i)), delay);
                                    ui.set_opacity(appear);
                                    task_card(ui, &palette, task, &mut cancel);
                                    ui.set_opacity(1.0);
                                    ui.add_space(6.0);
                                }
                            });
                    });
                // 阴影自己画，展开动画时浓度不变，看着不会闪
                crate::ui::anim::shadow(
                    ui.painter(),
                    response.response.rect,
                    radius,
                    if palette.dark { 110 } else { 40 },
                );
            });

        if dragged != Vec2::ZERO {
            offset += dragged;
            offset = offset.clamp(Vec2::new(0.0, -limit.y), Vec2::new(limit.x, 0.0));
            self.cfg.ui.download_panel_offset = [offset.x, offset.y];
            self.save_config();
        }
        if close {
            self.download_panel_open = false;
        }
        if let Some(id) = cancel {
            self.cancel_task(id);
        }
    }

    fn download_chip(&self, ctx: &egui::Context, active: &[TaskSnapshot]) -> bool {
        let palette = self.palette;
        let fraction = if active.is_empty() {
            0.0
        } else {
            active.iter().map(|t| t.fraction).sum::<f32>() / active.len() as f32
        };
        let mut open = false;
        // 面板被拖走过以后，胶囊跟着待在同一个地方
        let offset = Vec2::new(
            self.cfg.ui.download_panel_offset[0],
            self.cfg.ui.download_panel_offset[1],
        );
        let sidebar_width = if self.cfg.ui.sidebar_collapsed { 64.0 } else { 186.0 };
        // 旧配置里的偏移是右下角锚点时代的（负值），这里按新锚点钳回屏幕内
        let chip_offset = {
            let screen = ctx.content_rect();
            Vec2::new(
                offset.x.clamp(0.0, (screen.width() - 216.0).max(0.0)),
                offset.y.clamp(-((screen.height() - 80.0).max(0.0)), 0.0),
            )
        };

        egui::Area::new(egui::Id::new("htmcl_download_chip"))
            .order(egui::Order::Foreground)
            .anchor(
                egui::Align2::LEFT_BOTTOM,
                Vec2::new(sidebar_width + 8.0 + chip_offset.x, -12.0 + chip_offset.y),
            )
            .show(ctx, |ui| {
                // 胶囊出现时淡入，不要啪地冒出来
                let appear = widgets::enter_progress(
                    ctx,
                    egui::Id::new("htmcl_download_chip_anim"),
                    true,
                    ui::anim::DUR_ENTER,
                );
                ui.set_opacity(appear);
                let (rect, response) = ui.allocate_exact_size(Vec2::new(196.0, 48.0), Sense::click());
                let radius = CornerRadius::same(24);
                let painter = ui.painter();
                painter.rect_filled(rect, radius, palette.surface);
                painter.rect_stroke(
                    rect,
                    radius,
                    Stroke::new(1.0, palette.border),
                    egui::StrokeKind::Inside,
                );
                icons::paint(
                    ui.ctx(),
                    painter,
                    "download",
                    Rect::from_center_size(
                        Pos2::new(rect.left() + 24.0, rect.center().y),
                        Vec2::splat(18.0),
                    ),
                    palette.accent,
                );
                painter.text(
                    Pos2::new(rect.left() + 42.0, rect.center().y - 7.0),
                    egui::Align2::LEFT_CENTER,
                    format!("下载中 · {} 个任务", active.len()),
                    ui::font(12.0),
                    palette.text,
                );
                painter.text(
                    Pos2::new(rect.left() + 42.0, rect.center().y + 8.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{:.0}% · 点击查看", fraction * 100.0),
                    ui::font(11.0),
                    palette.text_weak,
                );
                // 底部一条细进度，扫一眼就知道走到哪了
                let bar = Rect::from_min_size(
                    Pos2::new(rect.left() + 42.0, rect.bottom() - 11.0),
                    Vec2::new(rect.width() - 62.0, 3.0),
                );
                painter.rect_filled(bar, CornerRadius::same(2), palette.surface_alt);
                let filled = Rect::from_min_size(
                    bar.min,
                    Vec2::new(bar.width() * fraction.clamp(0.0, 1.0), bar.height()),
                );
                painter.rect_filled(filled, CornerRadius::same(2), palette.accent);
                if response.clicked() {
                    open = true;
                }
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            });

        open
    }
}

fn task_card(ui: &mut egui::Ui, palette: &Palette, task: &TaskSnapshot, cancel: &mut Option<u64>) {
    egui::Frame::NONE
        .fill(palette.surface_alt)
        .corner_radius(palette.corner_sm())
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(372.0);
            ui.horizontal(|ui| {
                ui::spinner(ui, palette, 13.0);
                ui.label(
                    egui::RichText::new(&task.title)
                        .font(ui::font(13.0))
                        .color(palette.text)
                        .strong(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if task.cancellable && ui::ghost(ui, palette, "取消").clicked() {
                        *cancel = Some(task.id);
                    }
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", task.fraction * 100.0))
                            .font(ui::font(12.0))
                            .color(palette.text_weak),
                    );
                });
            });
            ui.add_space(6.0);
            ui::progress(ui, palette, &format!("task-{}", task.id), task.fraction, 6.0);
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                if task.bytes_total > 0 {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {}",
                            paths::format_size(task.bytes_done),
                            paths::format_size(task.bytes_total)
                        ))
                        .font(ui::font(11.0))
                        .color(palette.text_weak),
                    );
                }
                if task.speed > 1024.0 {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}/s",
                            paths::format_size(task.speed as u64)
                        ))
                        .font(ui::font(11.0))
                        .color(palette.accent),
                    );
                }
                let elapsed = task.elapsed.as_secs();
                ui.label(
                    egui::RichText::new(format!("已用 {}:{:02}", elapsed / 60, elapsed % 60))
                        .font(ui::font(11.0))
                        .color(palette.text_faint),
                );
            });
            if !task.message.is_empty() {
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new(&task.message)
                        .font(ui::font(11.0))
                        .color(palette.text_faint),
                );
            }
        });
}
