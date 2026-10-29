use egui::{
    Align2, Color32, CornerRadius, Id, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2,
};

use crate::icons;
use crate::theme::Palette;

use super::{font, fs};

/// 卡片内边距
///
/// 卡片里要塞东西进某一栏时，宽度要自己收一下，
/// 收掉的量就是这里的两倍。
pub const CARD_PAD: f32 = 18.0;

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
pub use super::anim::enter_progress;

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
        ui.add_space(7.0);
        ui.label(egui::RichText::new(subtitle).font(font(13.5)).color(p.text_weak));
    }
    ui.add_space(18.0);
}

pub fn section_title(ui: &mut Ui, p: &Palette, title: &str) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(title)
            .font(font(15.0))
            .color(p.text)
            .strong(),
    );
    ui.add_space(10.0);
}

/// 卡片标题
///
/// 跟 section_title 一样，但不留上下留白：卡片自己已经有内边距了。
/// 卡片标题全走这一个入口，字号才不会各处不一样。
pub fn card_title(ui: &mut Ui, p: &Palette, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .font(font(15.0))
            .color(p.text)
            .strong(),
    );
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
    card_padded(ui, p, CARD_PAD as i8, add)
}

pub fn card_padded(ui: &mut Ui, p: &Palette, padding: i8, add: impl FnOnce(&mut Ui)) -> Response {
    // 容器类卡片不响应悬停：整块面板跟着鼠标变深很奇怪
    let radius = p.corner(0);
    let response = egui::Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius)
        .inner_margin(Margin::same(padding))
        .show(ui, add)
        .response;
    // 阴影自己画，浓度不跟着页面的淡入变淡，切换时不会晃
    super::anim::shadow(ui.painter(), response.rect, radius, p.shadow.color.a());
    response
}

pub fn card_flat(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) -> Response {
    let id = ui.id().with("card_flat");
    let frame = egui::Frame::NONE
        .fill(p.surface)
        .corner_radius(p.corner(0))
        .inner_margin(Margin::same(18));
    let response = frame.show(ui, add).response;
    paint_card_state(ui, p, &response, id, false);
    response
}

/// 卡片的状态：边框颜色、阴影随悬停平滑过渡
///
/// 边框由我们自己画（而不是交给 Frame），
/// 这样才能在悬停时把颜色一点点染上主色，而不是啪地跳过去。
fn paint_card_state(ui: &Ui, p: &Palette, response: &Response, id: Id, with_shadow: bool) {
    let rect = response.rect;
    if !ui.is_rect_visible(rect) {
        return;
    }
    let hovered = response.hovered();
    let t = super::anim::to_bool(ui.ctx(), id.with("hov"), hovered, super::anim::DUR_BASE);
    let radius = p.corner(0);
    // 边框：平时是灰的，悬停时染上主色
    let border = lerp_color(p.border, p.accent, t * 0.85);
    ui.painter()
        .rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    if with_shadow {
        super::anim::shadow(ui.painter(), rect, radius, (24.0 + 46.0 * t) as u8);
    }
}

pub fn inset(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) -> Response {
    egui::Frame::NONE
        .fill(p.surface_alt)
        .corner_radius(p.corner_sm())
        .inner_margin(Margin::same(14))
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
    let hover_t =
        super::anim::to_bool(ui.ctx(), response.id.with("hov"), hovered, super::anim::DUR_HOVER);
    let press_t =
        super::anim::to_bool(ui.ctx(), response.id.with("prs"), pressed, super::anim::DUR_PRESS);

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
    // 按下时只收一点点，收多了按钮看着在抖
    let draw_rect = rect.shrink(press_t * 0.5);

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
        // 悬停底色淡入，不要啪地一下冒出来
        let t =
            super::anim::to_bool(ui.ctx(), response.id.with("hov"), hovered, super::anim::DUR_HOVER);
        if t > 0.001 {
            ui.painter().rect_filled(
                rect,
                p.corner(-2),
                alpha(p.surface_alt, (t * 255.0) as u8),
            );
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
    let height = 44.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let hover_t =
            super::anim::to_bool(ui.ctx(), response.id.with("hov"), hovered, super::anim::DUR_HOVER);
        let sel_t =
            super::anim::to_bool(ui.ctx(), response.id.with("sel"), selected, super::anim::DUR_BASE);
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
                Pos2::new(rect.left() + 27.0, rect.center().y),
                Vec2::splat(19.0),
            ),
            color,
        );
        ui.painter().text(
            Pos2::new(rect.left() + 48.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font(14.5),
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
    let t = super::anim::to_bool(ui.ctx(), response.id.with("t"), *value, super::anim::DUR_BASE);
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

// ---------------------------------------------------------------- 勾选框

/// 勾选框
///
/// 左边一个小方框，右边一行文字，两边都能点。
pub fn checkbox(ui: &mut Ui, p: &Palette, value: &mut bool, label: &str) -> Response {
    let size = Vec2::new(18.0, 18.0);
    ui.horizontal(|ui| {
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
        let radius = CornerRadius::same(4);
        if *value {
            ui.painter().rect_filled(rect, radius, p.accent);
            // 勾
            let points = vec![
                rect.left_top() + Vec2::new(4.0, 9.0),
                rect.left_top() + Vec2::new(7.5, 12.5),
                rect.left_top() + Vec2::new(14.0, 5.0),
            ];
            ui.painter()
                .add(egui::Shape::line(points, Stroke::new(2.0, p.on_accent)));
        } else {
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(1.0, p.border_strong),
                StrokeKind::Inside,
            );
        }
        if response.clicked() {
            *value = !*value;
            response.mark_changed();
        }
        ui.add_space(7.0);
        let label_response = ui.add(
            egui::Label::new(egui::RichText::new(label).font(font(13.0)).color(p.text)),
        );
        // 点文字也算勾上
        if label_response.clicked() {
            *value = !*value;
            response.mark_changed();
        }
        response
    })
    .inner
}

// ---------------------------------------------------------------- 分段选择

/// 分段选择器
///
/// id_source 用来区分页面上的多个分段器，指示块的滑动动画按它记录状态。
pub fn segmented(
    ui: &mut Ui,
    p: &Palette,
    id_source: &str,
    index: &mut usize,
    labels: &[&str],
) -> Response {
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

    // 选中块在选项之间平滑滑动
    let picked = (*index).min(labels.len().saturating_sub(1));
    let target_x = rect.left() + 3.0 + widths[..picked].iter().sum::<f32>();
    let target_w = widths[picked];
    let anim = Id::new(("htmcl_segmented", id_source));
    let (indicator_x, indicator_w) = (
        super::anim::to_value(ui.ctx(), anim.with("x"), target_x, super::anim::DUR_BASE),
        super::anim::to_value(ui.ctx(), anim.with("w"), target_w, super::anim::DUR_BASE),
    );
    let indicator = Rect::from_min_size(
        Pos2::new(indicator_x, rect.top() + 3.0),
        Vec2::new(indicator_w, height - 6.0),
    );

    let mut x = rect.left() + 3.0;
    for (i, label) in labels.iter().enumerate() {
        let width = widths[i];
        let seg = Rect::from_min_size(
            Pos2::new(x, rect.top() + 3.0),
            Vec2::new(width, height - 6.0),
        );
        // 文字颜色跟着滑动块走，滑过时两边的文字自然过渡
        let overlap = (seg.right().min(indicator.right()) - seg.left().max(indicator.left()))
            .max(0.0)
            / seg.width().max(1.0);
        let selected = i == picked;
        if selected {
            ui.painter().rect_filled(indicator, p.corner_sm(), p.surface);
            ui.painter().rect_stroke(
                indicator,
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
            lerp_color(p.text_weak, p.text, overlap),
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
    // 展开与收起都走同一条动画：收起时缩回去，不是啪地消失
    let progress = crate::ui::anim::to_value(
        ui.ctx(),
        popup_id.with("anim"),
        if is_open { 1.0 } else { 0.0 },
        crate::ui::anim::DUR_BASE,
    );
    if progress > 0.001 {
        let max_height = 280.0;
        let mut close = false;
        let area = egui::Area::new(popup_id.with("list"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(
                rect.left(),
                rect.bottom() + 4.0 + (1.0 - progress) * 6.0,
            ))
            .show(ui.ctx(), |ui| {
                ui.set_opacity(progress);
                ui.set_width(width);
                let radius = p.corner_sm();
                let response = egui::Frame::NONE
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(radius)
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
                                    // 悬停底色淡入，不要啪地一下冒出来
                                    let hover_t = super::anim::to_bool(
                                        ui.ctx(),
                                        item_resp.id.with("hov"),
                                        item_resp.hovered(),
                                        super::anim::DUR_HOVER,
                                    );
                                    if hover_t > 0.001 {
                                        ui.painter().rect_filled(
                                            item_rect,
                                            p.corner_sm(),
                                            lerp_color(Color32::TRANSPARENT, p.surface_alt, hover_t),
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
                                    // 收起过程中的点击不算数
                                    if is_open && item_resp.clicked() {
                                        result = Some(i);
                                        close = true;
                                    }
                                }
                            });
                    });
                // 阴影自己画，打开动画时浓度不变，不会看着在闪
                super::anim::shadow(
                    ui.painter(),
                    response.response.rect,
                    radius,
                    if p.dark { 90 } else { 28 },
                );
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
    desc: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    suffix: &str,
) -> Response {
    // 标题与说明在左，滑块在右，和开关、下拉的排版一致
    let height = if desc.is_empty() { 30.0 } else { 38.0 };
    let width = ui.available_width();
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());

    let value_width = 74.0;
    let track_width = 150.0;
    let track = Rect::from_min_max(
        Pos2::new(
            (rect.right() - value_width - track_width).max(rect.left() + 120.0),
            rect.center().y - 2.0,
        ),
        Pos2::new(rect.right() - value_width, rect.center().y + 2.0),
    );

    if !label.is_empty() {
        ui.painter().text(
            Pos2::new(rect.left(), rect.top() + 9.0),
            Align2::LEFT_TOP,
            label,
            font(13.0),
            p.text,
        );
    }
    if !desc.is_empty() {
        ui.painter().text(
            Pos2::new(rect.left(), rect.top() + 24.0),
            Align2::LEFT_TOP,
            desc,
            font(11.0),
            p.text_faint,
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

// ---------------------------------------------------------------- 文本截断

/// 估算单个字符的显示宽度：中日韩全角按 1 个字宽，其余按 0.58
fn char_width(ch: char, font_size: f32) -> f32 {
    if ch == '\n' {
        return font_size * 1.5;
    }
    let code = ch as u32;
    let wide = (0x1100..=0x115F).contains(&code)      // 韩文字母
        || (0x2E80..=0x303E).contains(&code)           // 中日韩符号
        || (0x3041..=0x33FF).contains(&code)           // 假名、中日韩兼容
        || (0x3400..=0x4DBF).contains(&code)           // 中日韩扩展 A
        || (0x4E00..=0x9FFF).contains(&code)           // 中日韩统一表意
        || (0xAC00..=0xD7A3).contains(&code)           // 韩文音节
        || (0xF900..=0xFAFF).contains(&code)           // 中日韩兼容表意
        || (0xFF00..=0xFF60).contains(&code)           // 全角字符
        || (0xFFE0..=0xFFE6).contains(&code); // 全角符号
    if wide {
        font_size
    } else {
        font_size * 0.58
    }
}

/// 按显示宽度截断单行文本，超出部分用省略号代替
///
/// 用于自绘文本：egui 的 Label 即使设了 truncate，测量宽度仍是原文，
/// 会把外层布局撑开，所以卡片标题这类需要固定宽度的场合要自己截断。
pub fn ellipsize(text: &str, max_width: f32, font_size: f32) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    let mut out = String::new();
    let mut width = 0.0;
    for ch in text.chars() {
        let w = char_width(ch, font_size);
        if width + w > max_width {
            out.push('…');
            break;
        }
        width += w;
        out.push(ch);
    }
    out
}

// ---------------------------------------------------------------- 固定高度卡片

/// 绘制一张高度固定的内容卡片
///
/// 与 `card` 不同，这里的高度由调用方指定，因此一排卡片的上下边缘完全对齐。
/// 内容区域是卡片内缩后的矩形，由回调自行安排。
pub fn fixed_card(
    ui: &mut Ui,
    p: &Palette,
    size: Vec2,
    padding: f32,
    add: impl FnOnce(&mut Ui, Rect),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let radius = p.corner_sm();
    let hovered = response.hovered();
    // 悬停时边框染上主色，阴影也重一点，扫一眼就知道点的是哪张
    let t = super::anim::to_bool(ui.ctx(), response.id.with("hov"), hovered, super::anim::DUR_BASE);
    let border = lerp_color(p.border, p.accent, t * 0.8);
    ui.painter().rect_filled(rect, radius, p.surface);
    ui.painter()
        .rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    if t > 0.0 {
        super::anim::shadow(ui.painter(), rect, radius, (8.0 + 34.0 * t) as u8);
    }
    let inner = rect.shrink(padding);
    // 显式指定自上而下的布局，避免继承外层的横向排列导致内容溢出卡片
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| add(ui, inner),
    );
    response
}

// ---------------------------------------------------------------- 反馈

pub fn progress(
    ui: &mut Ui,
    p: &Palette,
    id_source: &str,
    fraction: f32,
    height: f32,
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let radius = CornerRadius::same((height / 2.0) as u8);
    ui.painter()
        .rect_filled(rect, radius, p.surface_alt);
    let target = fraction.clamp(0.0, 1.0);
    // 进度平滑过渡，不要一格一格地跳
    let t = super::anim::to_value(
        ui.ctx(),
        ui.make_persistent_id(id_source).with("htmcl_progress"),
        target,
        super::anim::DUR_BASE,
    );
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
    // 关掉动画就画一个静止的环，不再驱动重绘
    let start = if super::animations_enabled() {
        time * 4.0
    } else {
        0.0
    };
    let points: Vec<Pos2> = (0..=20)
        .map(|i| {
            let angle = start + (i as f32 / 20.0) * std::f32::consts::TAU * 0.75;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(2.0, p.accent)));
    if super::animations_enabled() {
        // 只在转的时候要下一帧，别让整个窗口跟着常驻重绘
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
    response
}

pub fn empty_state(ui: &mut Ui, p: &Palette, icon_name: &str, title: &str, desc: &str) {
    // 空状态出现时淡入，不要突然冒出来
    let t = enter_progress(
        ui.ctx(),
        ui.id().with("htmcl_empty_state"),
        true,
        super::anim::DUR_ENTER,
    );
    ui.add_space(28.0);
    ui.vertical_centered(|ui| {
        ui.set_opacity(t);
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

/// 设置项一行：左边标题与说明，右边控件
///
/// 设置页所有项都用这个排版，看的时候能对上「这一项是干什么的」，
/// 控件也统一靠右，不会东一个西一个。
pub fn setting_row(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    desc: &str,
    control: impl FnOnce(&mut Ui),
) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            let width = (ui.available_width() - 180.0).max(120.0);
            ui.set_width(width);
            ui.label(
                egui::RichText::new(title)
                    .font(font(13.5))
                    .color(p.text),
            );
            if !desc.is_empty() {
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(desc)
                            .font(font(11.5))
                            .color(p.text_faint),
                    )
                    .wrap(),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            control(ui);
        });
    });
    ui.add_space(14.0);
}

/// 设置项分组标题：一条横线加文字
pub fn setting_group(ui: &mut Ui, p: &Palette, title: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(title)
            .font(font(15.0))
            .color(p.text)
            .strong(),
    );
    ui.add_space(10.0);
}

pub fn key_value(ui: &mut Ui, p: &Palette, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(key).font(font(13.0)).color(p.text_weak));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 数值用主色标出来，扫一眼就知道重点
            ui.add(
                egui::Label::new(
                    egui::RichText::new(value)
                        .font(font(13.5))
                        .color(p.accent)
                        .strong(),
                )
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

pub fn toast_layer(
    ctx: &egui::Context,
    p: &Palette,
    toasts: &mut Vec<Toast>,
    now: f64,
    bottom_offset: f32,
    x_shift: f32,
) {
    toasts.retain(|t| now - t.created < TOAST_LIFETIME);
    if toasts.is_empty() {
        return;
    }
    // 横向让位走过渡：下载中心开着时往右挪，别压在面板上
    let x_shift = super::anim::to_value(
        ctx,
        Id::new("htmcl_toast_x"),
        x_shift,
        super::anim::DUR_BASE,
    );
    egui::Area::new(Id::new("htmcl_toasts"))
        .order(egui::Order::Tooltip)
        .anchor(
            Align2::CENTER_BOTTOM,
            Vec2::new(x_shift, -(20.0 + bottom_offset)),
        )
        .interactable(false)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                for toast in toasts.iter().rev() {
                    let elapsed = now - toast.created;
                    // 关掉动画就直接显示与消失，不做淡入淡出
                    let fade = if !super::animations_enabled() {
                        1.0
                    } else if elapsed > TOAST_LIFETIME - super::anim::DUR_FADE as f64 {
                        ((TOAST_LIFETIME - elapsed) / super::anim::DUR_FADE as f64)
                            .clamp(0.0, 1.0) as f32
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
                    let slide = if super::animations_enabled() {
                        (elapsed / super::anim::DUR_BASE as f64).clamp(0.0, 1.0) as f32
                    } else {
                        1.0
                    };
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

/// 弹窗被拖动后偏离中心多远，按标题记着
///
/// 弹窗本身是个自由函数，拿不到 App 的状态，就在这里按标题记一份。
/// 同一个弹窗下次打开还在原来的位置。
static MODAL_OFFSETS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, Vec2>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn modal_offset(title: &str) -> Vec2 {
    MODAL_OFFSETS
        .lock()
        .ok()
        .and_then(|map| map.get(title).copied())
        .unwrap_or(Vec2::ZERO)
}

fn set_modal_offset(title: &str, offset: Vec2) {
    if let Ok(mut map) = MODAL_OFFSETS.lock() {
        map.insert(title.to_string(), offset);
    }
}

/// 标题栏可以拖着挪位置
///
/// 返回拖动量，交给调用方去改弹窗的落点。
pub fn drag_title_bar(ui: &mut Ui, rect: Rect, id: Id) -> Vec2 {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Vec2::ZERO;
    }
    // 右边留出关闭按钮的位置，别把按钮的点击抢走
    let drag_rect = Rect::from_min_max(
        rect.min,
        Pos2::new(rect.right() - 46.0, rect.bottom()),
    );
    let response = ui.interact(drag_rect, id, Sense::drag());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if !response.dragged() {
        return Vec2::ZERO;
    }
    // 拖着的时候换成抓手
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    response.drag_delta()
}

pub fn modal(
    ctx: &egui::Context,
    p: &Palette,
    open: &mut bool,
    title: &str,
    width: f32,
    add: impl FnOnce(&mut Ui),
) {
    let anim_id = Id::new(("htmcl_modal_anim", title));
    // 展开与收起都走同一条动画：收起时往下沉、变透明，缩完才真正不画
    let progress = crate::ui::anim::to_value(
        ctx,
        anim_id,
        if *open { 1.0 } else { 0.0 },
        crate::ui::anim::DUR_BASE,
    );
    if progress <= 0.001 {
        return;
    }
    let mut close = false;
    let screen = ctx.content_rect();
    // 拖出来的位置要在屏幕里待着
    let limit = Vec2::new(
        (screen.width() * 0.5 - 80.0).max(0.0),
        (screen.height() * 0.5 - 60.0).max(0.0),
    );
    let mut offset = modal_offset(title).clamp(-limit, limit);

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
            Vec2::new(offset.x, offset.y + (1.0 - progress) * 12.0),
        )
        .show(ctx, |ui| {
            ui.set_opacity(progress);
            let radius = p.corner(2);
            let response = egui::Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(radius)
                .inner_margin(Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(width);
                    let mut header_rect = Rect::NOTHING;
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
                        header_rect = ui.min_rect();
                    });
                    // 标题栏可以拖着挪弹窗
                    let delta = drag_title_bar(ui, header_rect, ui.id().with("modal_drag"));
                    if delta != Vec2::ZERO {
                        offset += delta;
                        offset = offset.clamp(-limit, limit);
                        set_modal_offset(title, offset);
                    }
                    ui.add_space(6.0);
                    add(ui);
                });
            // 阴影自己画，弹窗打开时浓度不变，看着不会闪
            super::anim::shadow(
                ui.painter(),
                response.response.rect,
                radius,
                if p.dark { 120 } else { 50 },
            );
        });

    if close {
        *open = false;
    }
}

/// 详情页大返回按钮等使用的圆形图标按钮
pub fn round_icon_button(ui: &mut Ui, p: &Palette, name: &str, size: f32) -> Response {
    round_icon_button_state(ui, p, name, size, false)
}

/// 圆形图标按钮，可以带一个「亮着」的状态
///
/// 亮着的按钮用主色画图标，一眼能看出当前是开着的，
/// 比如内容卡片上的银锁。
pub fn round_icon_button_state(
    ui: &mut Ui,
    p: &Palette,
    name: &str,
    size: f32,
    active: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered();
    // 悬停时底色淡入
    let t = super::anim::to_bool(ui.ctx(), response.id.with("hov"), hovered, super::anim::DUR_HOVER);
    if t > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((size / 2.0) as u8),
            alpha(p.surface_alt, (t * 255.0) as u8),
        );
    }
    icons::paint(
        ui.ctx(),
        ui.painter(),
        name,
        Rect::from_center_size(rect.center(), Vec2::splat(fs(17.0))),
        if active {
            p.accent
        } else if hovered {
            p.text
        } else {
            p.text_weak
        },
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
#[cfg(test)]
mod tests {
    use super::*;

    /// 入场进度必须真的往前走
    ///
    /// egui 的临时内存每帧会清空，如果只用临时内存记起始时间，
    /// 进度会永远停在 0，动画就等于没有。
    #[test]
    fn enter_progress_advances_over_frames() {
        let ctx = egui::Context::default();
        let id = Id::new("htmcl_test_enter");
        let mut last = 0.0f32;
        for _ in 0..12 {
            ctx.begin_pass(egui::RawInput::default());
            let t = enter_progress(&ctx, id, true, 0.16);
            assert!(t >= last, "入场进度应单调不减：{t} < {last}");
            last = t;
            ctx.end_pass();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        assert!(last > 0.9, "入场进度最终应接近 1，实际 {last}");
    }

    /// 关掉动画时进度直接到位
    #[test]
    fn enter_progress_is_zero_when_inactive() {
        let ctx = egui::Context::default();
        let id = Id::new("htmcl_test_enter_off");
        ctx.begin_pass(egui::RawInput::default());
        assert_eq!(enter_progress(&ctx, id, false, 0.16), 0.0);
        ctx.end_pass();
    }

    #[test]
    fn ease_out_is_monotonic() {
        let mut last = 0.0f32;
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let v = ease_out(t);
            assert!(v >= last);
            assert!((0.0..=1.0).contains(&v));
            last = v;
        }
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
    }
    /// 数值动画确实会向目标推进，而不是一步跳到位
    #[test]
    fn animate_value_moves_towards_target() {
        let ctx = egui::Context::default();
        let id = Id::new("htmcl_test_anim");
        let mut last = 0.0f32;
        for _ in 0..12 {
            ctx.begin_pass(egui::RawInput::default());
            let v = ctx.animate_value_with_time(id, 100.0, 0.2);
            assert!(v >= last, "动画值应单调不减：{v} < {last}");
            assert!(v <= 100.0, "动画值不应超过目标：{v}");
            last = v;
            ctx.end_pass();
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(last > 40.0, "动画应明显向目标推进，实际 {last}");
    }

    /// 关掉动画后控件直接给最终状态
    #[test]
    fn animation_switch_is_global() {
        super::super::set_animations_enabled(false);
        assert!(!super::super::animations_enabled());
        super::super::set_animations_enabled(true);
        assert!(super::super::animations_enabled());
    }

    /// 标题栏能拖着挪位置
    ///
    /// 这一条是给「悬浮窗没法调整位置」兜底：
    /// 按下、拖动，拖动量必须真的传出来。
    #[test]
    fn title_bar_can_be_dragged() {
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut header = Rect::NOTHING;
        let mut delta = Vec2::ZERO;

        // 每一帧都摆一条 300 宽的标题栏，位置保持一致
        let mut frame = |ui: &mut Ui, header: &mut Rect, delta: &mut Vec2| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(300.0, 24.0), Sense::hover());
                ui.label("下载中心");
                *header = rect;
            });
            *delta = drag_title_bar(ui, *header, ui.id().with("drag"));
        };

        // 第一帧：把标题栏摆出来
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(screen);
        let _ = ctx.run_ui(input, |ui| frame(ui, &mut header, &mut delta));

        // 第二帧：在标题栏上按下
        let press = header.center();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(screen);
        input.events.push(egui::Event::PointerMoved(press));
        input.events.push(egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        let _ = ctx.run_ui(input, |ui| frame(ui, &mut header, &mut delta));

        // 第三帧：拖着走 40×25，拖动量应该原样传出来
        let moved = press + Vec2::new(40.0, 25.0);
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(screen);
        input.events.push(egui::Event::PointerMoved(moved));
        let _ = ctx.run_ui(input, |ui| frame(ui, &mut header, &mut delta));

        assert!(
            (delta - Vec2::new(40.0, 25.0)).length() < 1.0,
            "拖动量不对，实际 {delta:?}"
        );
    }
}
