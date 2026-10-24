pub mod anim;
pub mod download_panel;
pub mod skin3d;
pub mod widgets;

pub use widgets::*;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::theme::Palette;

static FONT_SCALE: AtomicU32 = AtomicU32::new(1000);
/// 全局动效开关，跟「设置 - 动画」联动
static ANIMATIONS: AtomicBool = AtomicBool::new(true);

/// 开关所有动效
///
/// 关掉后控件直接跳到最终状态，不做任何过渡。
pub fn set_animations_enabled(enabled: bool) {
    ANIMATIONS.store(enabled, Ordering::Relaxed);
}

/// 当前是否开启动效
pub fn animations_enabled() -> bool {
    ANIMATIONS.load(Ordering::Relaxed)
}

pub fn set_font_scale(scale: f32) {
    FONT_SCALE.store((scale.clamp(0.8, 1.6) * 1000.0) as u32, Ordering::Relaxed);
}

pub fn font_scale() -> f32 {
    FONT_SCALE.load(Ordering::Relaxed) as f32 / 1000.0
}

pub fn fs(size: f32) -> f32 {
    (size * font_scale() * 2.0).round() / 2.0
}

pub fn font(size: f32) -> egui::FontId {
    egui::FontId::proportional(fs(size))
}

pub fn mono(size: f32) -> egui::FontId {
    egui::FontId::monospace(fs(size))
}

fn load_cjk_font() -> Option<(Vec<u8>, u32)> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let candidates = [
        (format!("{windir}\\Fonts\\msyh.ttc"), 0u32),
        (format!("{windir}\\Fonts\\msyh.ttf"), 0),
        (format!("{windir}\\Fonts\\Deng.ttf"), 0),
        (format!("{windir}\\Fonts\\simhei.ttf"), 0),
        (format!("{windir}\\Fonts\\simsun.ttc"), 0),
        ("C:\\Windows\\Fonts\\msyh.ttc".into(), 0),
        ("C:\\Windows\\Fonts\\simhei.ttf".into(), 0),
    ];
    for (path, index) in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            if bytes.len() > 1024 {
                log::info!("已加载界面字体: {path}");
                return Some((bytes, index));
            }
        }
    }
    log::warn!("未找到中文字体，中文可能显示为方块");
    None
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some((bytes, index)) = load_cjk_font() {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        fonts
            .font_data
            .insert("cjk".to_string(), std::sync::Arc::new(data));
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "cjk".to_string());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("cjk".to_string());
    }
    ctx.set_fonts(fonts);
}

pub fn install_style(ctx: &egui::Context, p: &Palette, font_scale: f32) {
    set_font_scale(font_scale);
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.visuals = visuals(p);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size = egui::vec2(40.0, 32.0);
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;
    style.animation_time = 0.12;
    style.text_styles = [
        (egui::TextStyle::Heading, font(20.0)),
        (egui::TextStyle::Body, font(14.0)),
        (egui::TextStyle::Button, font(14.0)),
        (egui::TextStyle::Small, font(12.0)),
        (egui::TextStyle::Monospace, mono(13.0)),
    ]
    .into();
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
    ctx.set_theme(if p.dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
}

fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface_alt;
    v.faint_bg_color = p.surface_alt;
    v.window_stroke = egui::Stroke::new(1.0, p.border);
    v.window_corner_radius = p.corner(2);
    v.menu_corner_radius = p.corner(0);
    v.selection.bg_fill = p.accent_weak;
    v.selection.stroke = egui::Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;
    v.window_shadow = p.shadow;
    v.popup_shadow = p.shadow;
    v.slider_trailing_fill = true;

    let radius = p.corner(0);
    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.weak_bg_fill = p.surface_alt;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, p.text);
    v.widgets.noninteractive.corner_radius = radius;

    v.widgets.inactive.bg_fill = p.surface_alt;
    v.widgets.inactive.weak_bg_fill = p.surface_alt;
    v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, p.text);
    v.widgets.inactive.corner_radius = radius;

    v.widgets.hovered.bg_fill = p.surface_hover;
    v.widgets.hovered.weak_bg_fill = p.surface_hover;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, p.border_strong);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, p.text);
    v.widgets.hovered.corner_radius = radius;

    v.widgets.active.bg_fill = p.accent_weak;
    v.widgets.active.weak_bg_fill = p.accent_weak;
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, p.accent);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, p.text);
    v.widgets.active.corner_radius = radius;

    v.widgets.open.bg_fill = p.surface_alt;
    v.widgets.open.weak_bg_fill = p.surface_alt;
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0, p.border);
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0, p.text);
    v.widgets.open.corner_radius = radius;
    v
}
