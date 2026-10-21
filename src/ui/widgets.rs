use egui::{
    Align2, Color32, CornerRadius, Id, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};

use crate::icons;
use crate::theme::Palette;

use super::{font, fs};

// ---------------------------------------------------------------- 颜色工具

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

pub fn alpha(color: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 缓出曲线，用于入场动画
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// 以首次渲染时间为基准计算 0→1 的入场进度；active 为 false 时清除状态
pub fn enter_progress(ctx: &egui::Context, id: Id, active: bool, duration: f32) -> f32 {
    if !active {
        ctx.memory_mut(|m| m.data.remove::<f64>(id));
        return 0.0;
    }
    let now = ctx.input(|i| i.time);
    let start = ctx
        .memory(|m| m.data.get_temp::<f64>(id))
        .unwrap_or_else(|| {
            ctx.memory_mut(|m| m.data.insert_temp(id, now));
            now
        });
    ease_out(((now - start) as f32 / duration).clamp(0.0, 1.0))
}

// ---------------------------------------------------------------- 文本工具

pub fn text(ui: &mut Ui, size: f32, color: Color32, content: &str) -> Response {
    ui.label(egui::RichText::new(content).font(font(size)).color(color))
}

pub fn strong(ui: &mut Ui, p: &Palette, size: f32, content: &str) -> Response {
    ui.label(
        egui::RichText::new(content)
            .font(font(size))
            .color(p.text)
            .strong(),
    )
}

pub fn weak(ui: &mut Ui, p: &Palette, size: f32, content: &str) -> Response {
    ui.label(egui::RichText::new(content).font(font(size)).color(p.text_weak))
}

pub fn page_title(ui: &mut Ui, p: &Palette, title: &str, subtitle: &str) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(title)
            .font(font(24.0))
            .color(p.text)
            .strong(),
    );
    if !subtitle.is_empty() {
        ui.add_space(3.0);
        ui.label(egui::RichText::new(subtitle).font(font(13.0)).color(p.text_weak));
    }
    ui.add_space(14.0);
}

pub fn section_title(ui: &mut Ui, p: &Palette, title: &str) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(title)
            .font(font(14.5))
            .color(p.text)
            .strong(),
    );
    ui.add_space(8.0);
}

pub fn divider(ui: &mut Ui, p: &Palette) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, p.border);
}

pub fn spacer(ui: &mut Ui, height: f32) {
    ui.add_space(height);
}

// ---------------------------------------------------------------- 图标

pub fn icon(ui: &mut Ui, name: &str, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        icons::paint(ui.ctx(), ui.painter(), name, rect, color);
    }
    response
}

pub fn icon_text(
    ui: &mut Ui,
    name: &str,
    content: &str,
    size: f32,
    color: Color32,
) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(content.to_string(), font(size), color);
    let width = size + 7.0 + galley.size().x;
    let height = size.max(galley.size().y);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if ui.is_rect_visible(rect) {
        icons::paint(
            ui.ctx(),
            ui.painter(),
            name,
            Rect::from_center_size(
                Pos2::new(rect.left() + size / 2.0, rect.center().y),
                Vec2::splat(size),
            ),
            color,
        );
        ui.painter().galley(
            Pos2::new(rect.left() + size + 7.0, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    }
    response
}

// ---------------------------------------------------------------- 容器

pub fn card(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) -> Response {
    card_padded(ui, p, 16, add)
}

pub fn card_padded(ui: &mut Ui, p: &Palette, padding: i8, add: impl FnOnce(&mut Ui)) -> Response {
    egui::Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(p.corner(0))
        .inner_margin(Margin::same(padding))
        .shadow(p.shadow)
        .show(ui, add)
        .response
}

pub fn card_flat(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) -> Response {
    egui::Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(p.corner(0))
        .inner_margin(Margin::same(16))
        .show(ui, add)
        .response
}

pub fn inset(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) -> Response {
    egui::Frame::NONE
        .fill(p.surface_alt)
        .corner_radius(p.corner_sm())
        .inner_margin(Margin::same(12))
        .show(ui, add)
        .response
}

// ---------------------------------------------------------------- 按钮

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
    Subtle,
}

#[derive(Debug, Clone)]
pub struct ButtonOpts<'a> {
    pub kind: ButtonKind,
    pub text: &'a str,
    pub icon: Option<&'a str>,
    pub enabled: bool,
    pub width: Option<f32>,
    pub height: f32,
    pub font_size: f32,
    pub icon_size: f32,
    pub tooltip: Option<&'a str>,
}

impl<'a> Default for ButtonOpts<'a> {
    fn default() -> Self {
        Self {
            kind: ButtonKind::Secondary,
            text: "",
            icon: None,
            enabled: true,
            width: None,
            height: 34.0,
            font_size: 14.0,
            icon_size: 16.0,
            tooltip: None,
        }
    }
}

impl<'a> ButtonOpts<'a> {
    pub fn primary(text: &'a str) -> Self {
        Self {
            kind: ButtonKind::Primary,
            text,
            ..Default::default()
        }
    }

    pub fn secondary(text: &'a str) -> Self {
        Self {
            kind: ButtonKind::Secondary,
            text,
            ..Default::default()
        }
    }

    pub fn ghost(text: &'a str) -> Self {
        Self {
            kind: ButtonKind::Ghost,
            text,
            ..Default::default()
        }
    }

    pub fn danger(text: &'a str) -> Self {
        Self {
            kind: ButtonKind::Danger,
            text,
            ..Default::default()
        }
    }

    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn tooltip(mut self, tip: &'a str) -> Self {
        self.tooltip = Some(tip);
        self
    }
}

pub fn button(ui: &mut Ui, p: &Palette, opts: ButtonOpts<'_>) -> Response {
    let font_id = font(opts.font_size);
    let galley = if opts.text.is_empty() {
        None
    } else {
        Some(
            ui.painter()
                .layout_no_wrap(opts.text.to_string(), font_id.clone(), p.text),
        )
    };
    let text_width = galley.as_ref().map(|g| g.size().x).unwrap_or(0.0);
    let gap = if opts.icon.is_some() && text_width > 0.0 {
        7.0
    } else {
        0.0
    };
    let icon_width = if opts.icon.is_some() {
        opts.icon_size
    } else {
        0.0
    };
    let content_width = icon_width + gap + text_width;
    let width = opts.width.unwrap_or(content_width + 30.0).max(28.0);
    let height = opts.height;

    let sense = if opts.enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let hovered = response.hovered() && opts.enabled;
    let pressed = response.is_pointer_button_down_on() && opts.enabled;
    let radius = p.corner(-2);

    // 悬停与按压都做插值，避免颜色突变
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hov"), hovered, 0.10);
    let press_t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("prs"), pressed, 0.06);

    let (base, hover_bg, active_bg, fg, border) = match opts.kind {
        ButtonKind::Primary => (
            if opts.enabled { p.accent } else { alpha(p.accent, 90) },
            p.accent_hover,
            p.accent_active,
            if opts.enabled { p.on_accent } else { p.on_accent.gamma_multiply(0.6) },
            None,
        ),
        ButtonKind::Secondary => (
            p.surface,
            p.surface_hover,
            p.surface_hover,
            if opts.enabled { p.text } else { p.text_faint },
            Some(p.border_strong),
        ),
        ButtonKind::Ghost => (
            Color32::TRANSPARENT,
            alpha(p.text, if p.dark { 12 } else { 8 }),
            p.surface_alt,
            if opts.enabled { p.text_weak } else { p.text_faint },
            None,
        ),
        ButtonKind::Subtle => (
            p.surface_alt,
            p.surface_hover,
            p.surface_hover,
            if opts.enabled { p.text } else { p.text_faint },
            None,
        ),
        ButtonKind::Danger => (
            if opts.enabled { p.danger } else { alpha(p.danger, 70) },
            p.danger.gamma_multiply(0.92),
            p.danger.gamma_multiply(0.85),
            Color32::WHITE,
            None,
        ),
    };

    let bg = lerp_color(lerp_color(base, hover_bg, hover_t), active_bg, press_t);
    // 按下时轻微内缩，给出物理反馈
    let draw_rect = rect.shrink(press_t * 1.0);

    let painter = ui.painter();
    if bg.a() > 0 {
        painter.rect_filled(draw_rect, radius, bg);
    }
    if let Some(border) = border {
        painter.rect_stroke(draw_rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    }

    let mut x = draw_rect.center().x - content_width / 2.0;
    if let Some(icon_name) = opts.icon {
        icons::paint(
            ui.ctx(),
            painter,
            icon_name,
            Rect::from_min_size(
                Pos2::new(x, draw_rect.center().y - opts.icon_size / 2.0),
                Vec2::splat(opts.icon_size),
            ),
            fg,
        );
        x += icon_width + gap;
    }
    if let Some(galley) = galley {
        painter.galley(
            Pos2::new(x, draw_rect.center().y - galley.size().y / 2.0),
            galley,
            fg,
        );
    }

    if let Some(tip) = opts.tooltip {
        response.on_hover_text(tip)
    } else {
        response
    }
}

pub fn primary(ui: &mut Ui, p: &Palette, content: &str) -> Response {
    button(ui, p, ButtonOpts::primary(content))
}

pub fn primary_icon(ui: &mut Ui, p: &Palette, icon: &str, content: &str) -> Response {
    button(ui, p, ButtonOpts::primary(content).icon(icon))
}

pub fn secondary(ui: &mut Ui, p: &Palette, content: &str) -> Response {
    button(ui, p, ButtonOpts::secondary(content))
}

pub fn ghost(ui: &mut Ui, p: &Palette, content: &str) -> Response {
    button(ui, p, ButtonOpts::ghost(content))
}

pub fn icon_button(ui: &mut Ui, p: &Palette, name: &str, tip: &str) -> Response {
    let size = 30.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, p.corner(-2), p.surface_alt);
        }
        icons::paint(
            ui.ctx(),
            ui.painter(),
            name,
            Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
            if hovered { p.text } else { p.text_weak },
        );
    }
    if tip.is_empty() {
        response
    } else {
        response.on_hover_text(tip)
    }
}

/// 侧边栏导航项
pub fn nav_item(ui: &mut Ui, p: &Palette, name: &str, label: &str, selected: bool) -> Response {
    let height = 40.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let hover_t = ui
            .ctx()
            .animate_bool_with_time(response.id.with("hov"), hovered, 0.12);
        let sel_t = ui
            .ctx()
            .animate_bool_with_time(response.id.with("sel"), selected, 0.16);
        let hover_bg = if p.dark {
            alpha(Color32::WHITE, 8)
        } else {
            alpha(Color32::BLACK, 5)
        };
        let mut bg = lerp_color(hover_bg, p.accent_weak, sel_t);
        let strength = hover_t.max(sel_t);
        bg = Color32::from_rgba_unmultiplied(
            bg.r(),
            bg.g(),
            bg.b(),
            (bg.a() as f32 * strength) as u8,
        );
        if bg.a() > 0 {
            ui.painter().rect_filled(rect, p.corner(-2), bg);
        }
        if sel_t > 0.01 {
            let bar = Rect::from_min_max(
                Pos2::new(rect.left(), rect.center().y - 9.0 * sel_t),
                Pos2::new(rect.left() + 3.0, rect.center().y + 9.0 * sel_t),
            );
            ui.painter()
                .rect_filled(bar, CornerRadius::same(2), p.accent);
        }
        let color = lerp_color(p.text_weak, p.accent, sel_t);
        icons::paint(
            ui.ctx(),
            ui.painter(),
            name,
            Rect::from_center_size(
                Pos2::new(rect.left() + 26.0, rect.center().y),
                Vec2::splat(18.0),
            ),
            color,
        );
        ui.painter().text(
            Pos2::new(rect.left() + 46.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font(14.0),
            lerp_color(p.text_weak, p.text, sel_t),
        );
    }
    response
}

// ---------------------------------------------------------------- 输入框

pub fn text_field(
    ui: &mut Ui,
    p: &Palette,
    value: &mut String,
    hint: &str,
    width: f32,
    password: bool,
) -> Response {
    field_impl(ui, p, value, hint, width, password, None)
}

pub fn search_field(
    ui: &mut Ui,
    p: &Palette,
    value: &mut String,
    hint: &str,
    width: f32,
) -> Response {
    field_impl(ui, p, value, hint, width, false, Some("search"))
}

fn field_impl(
    ui: &mut Ui,
    p: &Palette,
    value: &mut String,
    hint: &str,
    width: f32,
    password: bool,
    leading_icon: Option<&str>,
) -> Response {
    let height = 34.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let radius = p.corner_sm();

    // 背景随焦点变化
    let focus_id = ui.make_persistent_id(("htmcl_field", hint, width as i32));
    let focused_before = ui
        .memory(|m| m.data.get_temp::<bool>(focus_id))
        .unwrap_or(false);
    let bg = if focused_before {
        p.surface
    } else {
        p.surface_alt
    };
    ui.painter().rect_filled(rect, radius, bg);

    let left_pad = if leading_icon.is_some() { 32.0 } else { 11.0 };
    if let Some(icon_name) = leading_icon {
        icons::paint(
            ui.ctx(),
            ui.painter(),
            icon_name,
            Rect::from_center_size(
                Pos2::new(rect.left() + 17.0, rect.center().y),
                Vec2::splat(15.0),
            ),
            p.text_faint,
        );
    }

    let edit_rect = Rect::from_min_max(
        Pos2::new(rect.left() + left_pad, rect.top() + 5.0),
        Pos2::new(rect.right() - 10.0, rect.bottom() - 5.0),
    );
    let mut edit = egui::TextEdit::singleline(value)
        .hint_text(hint)
        .font(font(13.5))
        .text_color(p.text)
        .desired_width(edit_rect.width())
        .frame(egui::Frame::NONE)
        .margin(Margin::ZERO);
    if password {
        edit = edit.password(true);
    }
    let response = ui.put(edit_rect, edit);
    // 注意：has_focus() 会读取 Context，必须在取得 memory 写锁之前求值，否则会死锁
    let focused = response.has_focus();
    ui.memory_mut(|m| m.data.insert_temp(focus_id, focused));

    let border = if response.has_focus() {
        Stroke::new(1.5, p.accent)
    } else {
        Stroke::new(1.0, p.border)
    };
    ui.painter()
        .rect_stroke(rect, radius, border, StrokeKind::Inside);
    response
}

pub fn text_area(
    ui: &mut Ui,
    p: &Palette,
    value: &mut String,
    hint: &str,
    width: f32,
    rows: usize,
) -> Response {
    let height = 18.0 * rows as f32 + 20.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let radius = p.corner_sm();
    ui.painter().rect_filled(rect, radius, p.surface_alt);
    let edit_rect = rect.shrink2(Vec2::new(10.0, 8.0));
    let edit = egui::TextEdit::multiline(value)
        .hint_text(hint)
        .font(font(13.5))
        .text_color(p.text)
        .desired_width(edit_rect.width())
        .desired_rows(rows)
        .frame(egui::Frame::NONE);
    let response = ui.put(edit_rect, edit);
    let border = if response.has_focus() {
        Stroke::new(1.5, p.accent)
    } else {
        Stroke::new(1.0, p.border)
    };
    ui.painter()
        .rect_stroke(rect, radius, border, StrokeKind::Inside);
    response
}

// ---------------------------------------------------------------- 开关

pub fn toggle(ui: &mut Ui, p: &Palette, value: &mut bool) -> Response {
    let size = Vec2::new(38.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("t"), *value, 0.14);
    let bg = lerp_color(p.surface_alt, p.accent, t);
    ui.painter().rect_filled(rect, CornerRadius::same(11), bg);
    if t < 0.5 {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(11),
            Stroke::new(1.0, p.border_strong),
            StrokeKind::Inside,
        );
    }
    let knob_x = lerp(rect.left() + 11.0, rect.right() - 11.0, t);
    let knob = Pos2::new(knob_x, rect.center().y);
    ui.painter()
        .circle_filled(knob + Vec2::new(0.0, 1.0), 9.0, alpha(Color32::BLACK, 20));
    ui.painter().circle_filled(knob, 9.0, Color32::WHITE);
    response
}

// ---------------------------------------------------------------- 分段选择

pub fn segmented(ui: &mut Ui, p: &Palette, index: &mut usize, labels: &[&str]) -> Response {
    let font_id = font(13.5);
    let widths: Vec<f32> = labels
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap((*label).to_string(), font_id.clone(), p.text)
                .size()
                .x
                + 30.0
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let height = 34.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(total, height), Sense::click());
    let radius = p.corner(-1);
    ui.painter()
        .rect_filled(rect, radius, p.surface_alt);

    let mut x = rect.left() + 3.0;
    for (i, label) in labels.iter().enumerate() {
        let width = widths[i];
        let seg = Rect::from_min_size(
            Pos2::new(x, rect.top() + 3.0),
            Vec2::new(width, height - 6.0),
        );
        let selected = i == *index;
        if selected {
            ui.painter().rect_filled(seg, p.corner_sm(), p.surface);
            ui.painter().rect_stroke(
                seg,
                p.corner_sm(),
                Stroke::new(1.0, p.border),
                StrokeKind::Inside,
            );
        }
        ui.painter().text(
            seg.center(),
            Align2::CENTER_CENTER,
            *label,
            font_id.clone(),
            if selected { p.text } else { p.text_weak },
        );

        if response.hovered()
            && ui
                .input(|i| i.pointer.hover_pos())
                .map(|pos| seg.contains(pos))
                .unwrap_or(false)
        {
            ui.painter().rect_filled(
                seg,
                p.corner_sm(),
                if selected {
                    Color32::TRANSPARENT
                } else {
                    alpha(p.text, if p.dark { 10 } else { 6 })
                },
            );
        }

        if response.clicked()
            && ui
                .input(|i| i.pointer.interact_pos())
                .map(|pos| seg.contains(pos))
                .unwrap_or(false)
        {
            *index = i;
        }
        x += width;
    }
    response
}

// ---------------------------------------------------------------- 下拉选择

pub fn dropdown(
    ui: &mut Ui,
    p: &Palette,
    id_source: &str,
    current: &str,
    items: &[String],
    width: f32,
) -> Option<usize> {
    let id = ui.make_persistent_id(id_source);
    let popup_id = id.with("popup");
    let mut is_open = ui.memory(|m| m.data.get_temp::<bool>(popup_id).unwrap_or(false));

    let height = 34.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let radius = p.corner_sm();
    let hovered = response.hovered();
    ui.painter().rect_filled(
        rect,
        radius,
        if hovered { p.surface_hover } else { p.surface_alt },
    );
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(if is_open { 1.5 } else { 1.0 }, if is_open { p.accent } else { p.border }),
        StrokeKind::Inside,
    );
    ui.painter().text(
        Pos2::new(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        current,
        font(13.5),
        p.text,
    );
    icons::paint(
        ui.ctx(),
        ui.painter(),
        if is_open { "chevron-up" } else { "chevron-down" },
        Rect::from_center_size(
            Pos2::new(rect.right() - 15.0, rect.center().y),
            Vec2::splat(16.0),
        ),
        p.text_weak,
    );

    if response.clicked() {
        is_open = !is_open;
        ui.memory_mut(|m| m.data.insert_temp(popup_id, is_open));
    }

    let mut result = None;
    if is_open {
        let max_height = 280.0;
        let mut close = false;
        let progress = enter_progress(ui.ctx(), popup_id.with("anim"), true, 0.12);
        let area = egui::Area::new(popup_id.with("list"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(
                rect.left(),
                rect.bottom() + 4.0 + (1.0 - progress) * 6.0,
            ))
            .show(ui.ctx(), |ui| {
                ui.set_opacity(progress);
                ui.set_width(width);
                egui::Frame::NONE
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(p.corner_sm())
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 16,
                        spread: 0,
                        color: alpha(Color32::BLACK, if p.dark { 90 } else { 28 }),
                    })
                    .inner_margin(Margin::same(4))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(max_height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(width - 8.0);
                                for (i, item) in items.iter().enumerate() {
                                    let selected = item == current;
                                    let (item_rect, item_resp) = ui.allocate_exact_size(
                                        Vec2::new(width - 8.0, 30.0),
                                        Sense::click(),
                                    );
                                    if item_resp.hovered() {
                                        ui.painter().rect_filled(
                                            item_rect,
                                            p.corner_sm(),
                                            p.surface_alt,
                                        );
                                    }
                                    if selected {
                                        icons::paint(
                                            ui.ctx(),
                                            ui.painter(),
                                            "check",
                                            Rect::from_center_size(
                                                Pos2::new(
                                                    item_rect.right() - 14.0,
                                                    item_rect.center().y,
                                                ),
                                                Vec2::splat(14.0),
                                            ),
                                            p.accent,
                                        );
                                    }
                                    ui.painter().text(
                                        Pos2::new(item_rect.left() + 9.0, item_rect.center().y),
                                        Align2::LEFT_CENTER,
                                        item,
                                        font(13.5),
                                        if selected { p.accent } else { p.text },
                                    );
                                    if item_resp.clicked() {
                                        result = Some(i);
                                        close = true;
                                    }
                                }
                            });
                    });
            });

        // 点击外部关闭
        let clicked_outside = ui.input(|i| i.pointer.any_pressed())
            && ui
                .input(|i| i.pointer.interact_pos())
                .map(|pos| {
                    !area.response.rect.contains(pos)
                        && !rect.contains(pos)
                })
                .unwrap_or(false);
        if close || clicked_outside {
            ui.memory_mut(|m| m.data.insert_temp(popup_id, false));
        }
    }

    result
}

// ---------------------------------------------------------------- 滑块

pub fn slider(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    suffix: &str,
) -> Response {
    let height = 30.0;
    let width = ui.available_width();
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());

    let label_width = if label.is_empty() { 0.0 } else { 92.0 };
    let value_width = 74.0;
    let track = Rect::from_min_max(
        Pos2::new(rect.left() + label_width, rect.center().y - 2.0),
        Pos2::new(rect.right() - value_width, rect.center().y + 2.0),
    );

    if !label.is_empty() {
        ui.painter().text(
            Pos2::new(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font(13.5),
            p.text_weak,
        );
    }

    if response.dragged() || response.clicked() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let raw = min + t * (max - min);
            let stepped = if step > 0.0 {
                (raw / step).round() * step
            } else {
                raw
            };
            if (*value - stepped).abs() > f32::EPSILON {
                *value = stepped.clamp(min, max);
                response.mark_changed();
            }
        }
    }

    let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), p.surface_alt);
    let filled = Rect::from_min_max(
        track.left_top(),
        Pos2::new(track.left() + track.width() * t, track.bottom()),
    );
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), p.accent);
    let knob = Pos2::new(filled.right(), track.center().y);
    ui.painter().circle_filled(knob, 8.0, p.surface);
    ui.painter()
        .circle_stroke(knob, 8.0, Stroke::new(2.0, p.accent));
    if response.hovered() || response.dragged() {
        ui.painter().circle_filled(knob, 4.0, p.accent);
    }

    let value_text = if step >= 1.0 {
        format!("{:.0}{}", *value, suffix)
    } else {
        format!("{:.1}{}", *value, suffix)
    };
    ui.painter().text(
        Pos2::new(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        value_text,
        font(13.0),
        p.text_weak,
    );
    response
}

// ---------------------------------------------------------------- 反馈

pub fn progress(ui: &mut Ui, p: &Palette, fraction: f32, height: f32) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let radius = CornerRadius::same((height / 2.0) as u8);
    ui.painter()
        .rect_filled(rect, radius, p.surface_alt);
    let t = fraction.clamp(0.0, 1.0);
    if t > 0.0 {
        let filled = Rect::from_min_size(
            rect.min,
            Vec2::new((rect.width() * t).max(height), rect.height()),
        );
        ui.painter().rect_filled(filled, radius, p.accent);
    }
    response
}

pub fn badge(ui: &mut Ui, p: &Palette, text: &str, color: Color32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font(11.5), color);
    let width = galley.size().x + 14.0;
    let height = 20.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), alpha(color, if p.dark { 36 } else { 26 }));
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        color,
    );
    response
}

pub fn spinner(ui: &mut Ui, p: &Palette, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let time = ui.input(|i| i.time) as f32;
    let center = rect.center();
    let radius = size / 2.0 - 2.0;
    let start = time * 4.0;
    let points: Vec<Pos2> = (0..=20)
        .map(|i| {
            let angle = start + (i as f32 / 20.0) * std::f32::consts::TAU * 0.75;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(2.0, p.accent)));
    ui.ctx().request_repaint();
    response
}

pub fn empty_state(ui: &mut Ui, p: &Palette, icon_name: &str, title: &str, desc: &str) {
    ui.add_space(28.0);
    ui.vertical_centered(|ui| {
        ui.add_space(4.0);
        icon(ui, icon_name, 40.0, p.text_faint);
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(title)
                .font(font(15.0))
                .color(p.text_weak)
                .strong(),
        );
        if !desc.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(desc).font(font(12.5)).color(p.text_faint));
        }
        ui.add_space(20.0);
    });
}

pub fn key_value(ui: &mut Ui, p: &Palette, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(key).font(font(13.0)).color(p.text_weak));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 超长内容（路径等）截断显示，避免与左侧标签重叠
            ui.add(
                egui::Label::new(egui::RichText::new(value).font(font(13.0)).color(p.text))
                    .truncate(),
            );
        });
    });
}

pub fn avatar(ui: &mut Ui, p: &Palette, texture: &egui::TextureHandle, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    response
}

// ---------------------------------------------------------------- 提示条

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub created: f64,
}

impl Toast {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Info,
            created: 0.0,
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Success,
            created: 0.0,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Error,
            created: 0.0,
        }
    }
}

pub const TOAST_LIFETIME: f64 = 3.8;

pub fn toast_layer(ctx: &egui::Context, p: &Palette, toasts: &mut Vec<Toast>, now: f64) {
    toasts.retain(|t| now - t.created < TOAST_LIFETIME);
    if toasts.is_empty() {
        return;
    }
    egui::Area::new(Id::new("htmcl_toasts"))
        .order(egui::Order::Tooltip)
        .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -20.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                for toast in toasts.iter().rev() {
                    let elapsed = now - toast.created;
                    let fade = if elapsed > TOAST_LIFETIME - 0.45 {
                        ((TOAST_LIFETIME - elapsed) / 0.45).clamp(0.0, 1.0) as f32
                    } else {
                        1.0
                    };
                    let (icon_name, color) = match toast.kind {
                        ToastKind::Info => ("info", p.text_weak),
                        ToastKind::Success => ("check-circle", p.success),
                        ToastKind::Error => ("error", p.danger),
                    };
                    let galley = ui.painter().layout(
                        toast.text.clone(),
                        font(13.0),
                        p.text.gamma_multiply(fade),
                        ui.available_width().min(330.0) - 52.0,
                    );
                    let width = (galley.size().x + 52.0).max(160.0).min(360.0);
                    let height = galley.size().y + 22.0;
                    let (slot, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                    // 入场时从右侧滑入
                    let slide = (elapsed / 0.18).clamp(0.0, 1.0) as f32;
                    let rect = slot.translate(Vec2::new((1.0 - ease_out(slide)) * 26.0, 0.0));
                    ui.painter().rect_filled(rect, p.corner(-2), p.surface);
                    ui.painter().rect_stroke(
                        rect,
                        p.corner(-2),
                        Stroke::new(1.0, p.border),
                        StrokeKind::Inside,
                    );
                    icons::paint(
                        ui.ctx(),
                        ui.painter(),
                        icon_name,
                        Rect::from_center_size(
                            Pos2::new(rect.left() + 19.0, rect.center().y),
                            Vec2::splat(16.0),
                        ),
                        color.gamma_multiply(fade),
                    );
                    ui.painter().galley(
                        Pos2::new(rect.left() + 34.0, rect.center().y - galley.size().y / 2.0),
                        galley,
                        p.text,
                    );
                }
            });
        });
}

// ---------------------------------------------------------------- 模态框

pub fn modal(
    ctx: &egui::Context,
    p: &Palette,
    open: &mut bool,
    title: &str,
    width: f32,
    add: impl FnOnce(&mut Ui),
) {
    let anim_id = Id::new(("htmcl_modal_anim", title));
    let progress = enter_progress(ctx, anim_id, *open, 0.16);
    if !*open {
        return;
    }
    let mut close = false;
    let screen = ctx.content_rect();

    egui::Area::new(Id::new("htmcl_modal_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
            let base = if p.dark { 130.0 } else { 70.0 };
            ui.painter().rect_filled(
                rect,
                CornerRadius::ZERO,
                Color32::from_black_alpha((base * progress) as u8),
            );
            if response.clicked() {
                close = true;
            }
        });

    egui::Area::new(Id::new("htmcl_modal_card"))
        .order(egui::Order::Foreground)
        .anchor(
            Align2::CENTER_CENTER,
            Vec2::new(0.0, (1.0 - progress) * 12.0),
        )
        .show(ctx, |ui| {
            ui.set_opacity(progress);
            egui::Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(p.corner(2))
                .inner_margin(Margin::same(20))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 10],
                    blur: 32,
                    spread: 0,
                    color: Color32::from_black_alpha(if p.dark { 120 } else { 50 }),
                })
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(title)
                                .font(font(17.0))
                                .color(p.text)
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, p, "close", "关闭").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    add(ui);
                });
        });

    if close {
        *open = false;
    }
}

/// 详情页大返回按钮等使用的圆形图标按钮
pub fn round_icon_button(ui: &mut Ui, p: &Palette, name: &str, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((size / 2.0) as u8),
            p.surface_alt,
        );
    }
    icons::paint(
        ui.ctx(),
        ui.painter(),
        name,
        Rect::from_center_size(rect.center(), Vec2::splat(fs(17.0))),
        if hovered { p.text } else { p.text_weak },
    );
    response
}

/// 单选行：左侧圆形单选标记 + 标题 + 说明，整行可点击
pub fn radio_row(ui: &mut Ui, p: &Palette, selected: bool, title: &str, subtitle: &str) -> bool {
    let width = ui.available_width();
    let height = if subtitle.is_empty() { 34.0 } else { 46.0 };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, p.corner_sm(), p.surface_alt);
        }
        let center = Pos2::new(rect.left() + 16.0, rect.center().y);
        ui.painter().circle_stroke(
            center,
            7.0,
            Stroke::new(1.5, if selected { p.accent } else { p.border_strong }),
        );
        if selected {
            ui.painter().circle_filled(center, 3.8, p.accent);
        }
        let text_left = rect.left() + 32.0;
        if subtitle.is_empty() {
            ui.painter().text(
                Pos2::new(text_left, rect.center().y),
                Align2::LEFT_CENTER,
                title,
                font(13.0),
                if selected { p.text } else { p.text_weak },
            );
        } else {
            ui.painter().text(
                Pos2::new(text_left, rect.center().y - 9.0),
                Align2::LEFT_CENTER,
                title,
                font(13.0),
                if selected { p.text } else { p.text_weak },
            );
            ui.painter().text(
                Pos2::new(text_left, rect.center().y + 9.0),
                Align2::LEFT_CENTER,
                subtitle,
                font(11.0),
                p.text_faint,
            );
        }
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}
