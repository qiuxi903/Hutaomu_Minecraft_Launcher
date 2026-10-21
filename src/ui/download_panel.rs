//! 悬浮下载中心：显示所有进行中的任务、字节进度、速度与当前动作。

use egui::{Align, Color32, Layout, Margin, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::{App, TaskItem};
use crate::core::paths;
use crate::icons;
use crate::ui;
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

        if !self.download_panel_open {
            if active.is_empty() {
                return;
            }
            if self.download_chip(ctx, &active) {
                self.download_panel_open = true;
            }
            return;
        }

        let palette = self.palette;
        let mut close = false;
        let mut cancel: Option<u64> = None;

        egui::Area::new(egui::Id::new("htmcl_download_panel"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -20.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(palette.surface)
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(palette.corner(2))
                    .inner_margin(Margin::same(14))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(if palette.dark { 110 } else { 40 }),
                    })
                    .show(ui, |ui| {
                        ui.set_width(400.0);
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
                            ui.label(
                                egui::RichText::new("下载中心")
                                    .font(ui::font(14.5))
                                    .color(palette.text)
                                    .strong(),
                            );
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
                        });
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
                                for task in &active {
                                    task_card(ui, &palette, task, &mut cancel);
                                    ui.add_space(6.0);
                                }
                            });
                    });
            });

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

        egui::Area::new(egui::Id::new("htmcl_download_chip"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -20.0))
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(Vec2::new(200.0, 46.0), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(rect, palette.corner(0), palette.surface);
                painter.rect_stroke(
                    rect,
                    palette.corner(0),
                    Stroke::new(1.0, palette.border),
                    egui::StrokeKind::Inside,
                );
                icons::paint(
                    ui.ctx(),
                    painter,
                    "download",
                    Rect::from_center_size(
                        Pos2::new(rect.left() + 22.0, rect.center().y),
                        Vec2::splat(18.0),
                    ),
                    palette.accent,
                );
                painter.text(
                    Pos2::new(rect.left() + 40.0, rect.center().y - 7.0),
                    egui::Align2::LEFT_CENTER,
                    format!("下载中 · {} 个任务", active.len()),
                    ui::font(12.0),
                    palette.text,
                );
                painter.text(
                    Pos2::new(rect.left() + 40.0, rect.center().y + 8.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{:.0}% · 点击查看", fraction * 100.0),
                    ui::font(11.0),
                    palette.text_weak,
                );
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
            ui::progress(ui, palette, task.fraction, 6.0);
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
