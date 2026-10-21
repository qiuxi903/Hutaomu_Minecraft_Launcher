use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};

include!(concat!(env!("OUT_DIR"), "/icons_generated.rs"));

pub fn source(name: &str) -> Option<&'static str> {
    ICON_SOURCES
        .iter()
        .find(|(icon_name, _)| *icon_name == name)
        .map(|(_, svg)| *svg)
}

pub fn exists(name: &str) -> bool {
    source(name).is_some()
}

pub fn names() -> Vec<&'static str> {
    ICON_SOURCES.iter().map(|(name, _)| *name).collect()
}

type Key = (String, [u8; 4], u32);

static CACHE: OnceLock<Mutex<HashMap<Key, TextureHandle>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<Key, TextureHandle>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn options() -> &'static resvg::usvg::Options<'static> {
    static OPTIONS: OnceLock<resvg::usvg::Options<'static>> = OnceLock::new();
    OPTIONS.get_or_init(|| resvg::usvg::Options::default())
}

fn hex_of(color: Color32) -> String {
    if color.a() == 255 {
        format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
    } else {
        // 保留透明度，否则半透明图标会被渲染成实心
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            color.r(),
            color.g(),
            color.b(),
            color.a()
        )
    }
}

fn rasterize(svg: &str, px: u32, color: Color32) -> Option<ColorImage> {
    let tinted = svg.replace("currentColor", &hex_of(color));
    let tree = resvg::usvg::Tree::from_str(&tinted, options()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let size = tree.size();
    let scale = px as f32 / size.width().max(1.0);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(ColorImage::from_rgba_premultiplied(
        [px as usize, px as usize],
        pixmap.data(),
    ))
}

/// 取得（并缓存）指定颜色与像素尺寸的图标纹理
pub fn icon(ctx: &Context, name: &str, size: f32, color: Color32) -> Option<TextureHandle> {
    let ppp = ctx.pixels_per_point().clamp(1.0, 3.0);
    let px = ((size * ppp).ceil() as u32).clamp(8, 512);
    let key = (
        name.to_string(),
        [color.r(), color.g(), color.b(), color.a()],
        px,
    );

    if let Ok(map) = cache().lock() {
        if let Some(handle) = map.get(&key) {
            return Some(handle.clone());
        }
    }

    let svg = source(name)?;
    let image = rasterize(svg, px, color)?;
    let handle = ctx.load_texture(
        format!("icon/{name}/{px}/{:02X}{:02X}{:02X}", color.r(), color.g(), color.b()),
        image,
        TextureOptions::LINEAR,
    );
    if let Ok(mut map) = cache().lock() {
        map.insert(key, handle.clone());
    }
    Some(handle)
}

/// 把图标画到指定矩形内，返回是否成功
pub fn paint(
    ctx: &Context,
    painter: &egui::Painter,
    name: &str,
    rect: egui::Rect,
    color: Color32,
) -> bool {
    let size = rect.width().max(rect.height());
    let Some(handle) = icon(ctx, name, size, color) else {
        return false;
    };
    painter.image(
        handle.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    true
}

/// 在指定位置画一个居中的图标
pub fn paint_centered(
    ctx: &Context,
    painter: &egui::Painter,
    name: &str,
    center: egui::Pos2,
    size: f32,
    color: Color32,
) -> bool {
    let rect = egui::Rect::from_center_size(center, egui::vec2(size, size));
    paint(ctx, painter, name, rect, color)
}

/// 预加载常用图标，避免首帧闪烁
pub fn warmup(ctx: &Context, color: Color32, size: f32) {
    for name in [
        "home", "download", "layers", "user", "settings", "info", "play", "search", "refresh",
        "folder", "plus", "trash", "check", "close", "chevron-down", "chevron-right", "loader",
        "win-minimize", "win-maximize", "win-close", "win-restore",
    ] {
        let _ = icon(ctx, name, size, color);
    }
}

/// 把 SVG 光栅化为 RGBA8 像素（用于窗口图标）
pub fn rasterize_to_rgba(svg: &str, px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let tree = resvg::usvg::Tree::from_str(svg, options()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let scale = px as f32 / tree.size().width().max(1.0);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some((px, px, pixmap.take()))
}
