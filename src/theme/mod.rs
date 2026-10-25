use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::config::{Background, BackgroundKind};
use crate::core::paths;

/// 主题定义（内置主题写在代码里，自定义主题放在 exe 同目录 themes/<id>/theme.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeDef {
    pub id: String,
    pub name: String,
    pub dark: bool,
    pub accent: String,
    pub bg: String,
    pub surface: String,
    pub surface_alt: String,
    pub border: String,
    pub text: String,
    pub text_weak: String,
    pub radius: u8,
    /// 相对主题目录的背景图（png / jpg / gif）
    pub background: Option<String>,
    pub background_opacity: f32,
    pub background_dim: f32,
    #[serde(skip)]
    pub builtin: bool,
    #[serde(skip)]
    pub dir: Option<PathBuf>,
}

impl Default for ThemeDef {
    fn default() -> Self {
        Self {
            id: "light".into(),
            name: "默认白".into(),
            dark: false,
            accent: "#B0703F".into(),
            bg: "#F5F6F8".into(),
            surface: "#FFFFFF".into(),
            surface_alt: "#F1F2F5".into(),
            border: "#E7E9ED".into(),
            text: "#1D2025".into(),
            text_weak: "#6E7681".into(),
            radius: 10,
            background: None,
            background_opacity: 0.35,
            background_dim: 0.55,
            builtin: true,
            dir: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: egui::Color32,
    pub surface: egui::Color32,
    pub surface_alt: egui::Color32,
    pub surface_hover: egui::Color32,
    pub border: egui::Color32,
    pub border_strong: egui::Color32,
    pub text: egui::Color32,
    pub text_weak: egui::Color32,
    pub text_faint: egui::Color32,
    pub accent: egui::Color32,
    pub accent_hover: egui::Color32,
    pub accent_active: egui::Color32,
    pub accent_weak: egui::Color32,
    pub on_accent: egui::Color32,
    pub danger: egui::Color32,
    pub success: egui::Color32,
    pub warning: egui::Color32,
    pub radius: u8,
    pub shadow: egui::epaint::Shadow,
}

impl Palette {
    pub fn from_def(def: &ThemeDef, accent_override: Option<egui::Color32>, radius: u8) -> Self {
        let accent = accent_override.unwrap_or_else(|| hex(&def.accent));
        let dark = def.dark;
        let bg = hex(&def.bg);
        let surface = hex(&def.surface);
        let surface_alt = hex(&def.surface_alt);
        let border = hex(&def.border);
        let text = hex(&def.text);
        let text_weak = hex(&def.text_weak);

        let surface_hover = if dark {
            lighten(surface_alt, 0.06)
        } else {
            darken(surface_alt, 0.03)
        };

        let on_accent = if luminance(accent) > 0.62 {
            egui::Color32::from_rgb(0x1D, 0x20, 0x25)
        } else {
            egui::Color32::WHITE
        };

        Self {
            dark,
            bg,
            surface,
            surface_alt,
            surface_hover,
            border,
            border_strong: if dark {
                lighten(border, 0.12)
            } else {
                darken(border, 0.08)
            },
            text,
            text_weak,
            text_faint: if dark {
                lighten(text_weak, 0.0).gamma_multiply(0.75)
            } else {
                text_weak.gamma_multiply(0.75)
            },
            accent,
            accent_hover: if dark {
                lighten(accent, 0.10)
            } else {
                darken(accent, 0.08)
            },
            accent_active: if dark {
                lighten(accent, 0.18)
            } else {
                darken(accent, 0.16)
            },
            accent_weak: egui::Color32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                if dark { 46 } else { 30 },
            ),
            on_accent,
            danger: if dark {
                egui::Color32::from_rgb(0xE0, 0x6C, 0x6C)
            } else {
                egui::Color32::from_rgb(0xC0, 0x45, 0x45)
            },
            success: if dark {
                egui::Color32::from_rgb(0x6F, 0xC0, 0x8A)
            } else {
                egui::Color32::from_rgb(0x35, 0x8A, 0x5A)
            },
            warning: if dark {
                egui::Color32::from_rgb(0xE0, 0xB0, 0x5C)
            } else {
                egui::Color32::from_rgb(0xB8, 0x7A, 0x1E)
            },
            radius,
            shadow: egui::epaint::Shadow {
                offset: [0, 1],
                blur: 8,
                spread: 0,
                color: if dark {
                    egui::Color32::from_black_alpha(60)
                } else {
                    egui::Color32::from_black_alpha(14)
                },
            },
        }
    }

    pub fn corner(&self, extra: i8) -> egui::CornerRadius {
        let base = self.radius as i32 + extra as i32;
        egui::CornerRadius::same(base.clamp(0, 32) as u8)
    }

    pub fn corner_sm(&self) -> egui::CornerRadius {
        egui::CornerRadius::same((self.radius.saturating_sub(3)).max(2))
    }

    /// 背景遮罩色：用于在背景图上压一层，保证内容可读
    pub fn overlay(&self, dim: f32) -> egui::Color32 {
        if self.dark {
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, (dim * 255.0) as u8)
        } else {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, (dim * 255.0) as u8)
        }
    }
}

pub fn hex(value: &str) -> egui::Color32 {
    let trimmed = value.trim().trim_start_matches('#');
    let parse = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
    match trimmed.len() {
        6 => egui::Color32::from_rgb(
            parse(&trimmed[0..2]),
            parse(&trimmed[2..4]),
            parse(&trimmed[4..6]),
        ),
        8 => egui::Color32::from_rgba_unmultiplied(
            parse(&trimmed[0..2]),
            parse(&trimmed[2..4]),
            parse(&trimmed[4..6]),
            parse(&trimmed[6..8]),
        ),
        3 => {
            let expand = |c: char| {
                let s: String = std::iter::repeat(c).take(2).collect();
                parse(&s)
            };
            let chars: Vec<char> = trimmed.chars().collect();
            egui::Color32::from_rgb(expand(chars[0]), expand(chars[1]), expand(chars[2]))
        }
        _ => egui::Color32::GRAY,
    }
}

pub fn to_hex(color: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

fn luminance(color: egui::Color32) -> f32 {
    (0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32) / 255.0
}

fn lighten(color: egui::Color32, amount: f32) -> egui::Color32 {
    let mix = |c: u8| (c as f32 + (255.0 - c as f32) * amount).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(mix(color.r()), mix(color.g()), mix(color.b()), color.a())
}

fn darken(color: egui::Color32, amount: f32) -> egui::Color32 {
    let mix = |c: u8| (c as f32 * (1.0 - amount)).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(mix(color.r()), mix(color.g()), mix(color.b()), color.a())
}

pub fn builtin_themes() -> Vec<ThemeDef> {
    let mut list = vec![
        ThemeDef {
            id: "light".into(),
            name: "默认白".into(),
            dark: false,
            accent: "#B0703F".into(),
            bg: "#F5F6F8".into(),
            surface: "#FFFFFF".into(),
            surface_alt: "#F1F2F5".into(),
            border: "#E7E9ED".into(),
            text: "#1D2025".into(),
            text_weak: "#6E7681".into(),
            radius: 10,
            ..Default::default()
        },
        ThemeDef {
            id: "walnut".into(),
            name: "胡桃木".into(),
            dark: false,
            accent: "#A9663A".into(),
            bg: "#FAF6F1".into(),
            surface: "#FFFFFF".into(),
            surface_alt: "#F5EEE6".into(),
            border: "#ECE1D5".into(),
            text: "#2A231D".into(),
            text_weak: "#7A6A5C".into(),
            radius: 10,
            ..Default::default()
        },
        ThemeDef {
            id: "mint".into(),
            name: "青竹".into(),
            dark: false,
            accent: "#3F8F6B".into(),
            bg: "#F4F8F5".into(),
            surface: "#FFFFFF".into(),
            surface_alt: "#EDF4EF".into(),
            border: "#DFE9E2".into(),
            text: "#1C2620".into(),
            text_weak: "#64756B".into(),
            radius: 10,
            ..Default::default()
        },
        ThemeDef {
            id: "ocean".into(),
            name: "深海".into(),
            dark: false,
            accent: "#3A6EA5".into(),
            bg: "#F4F7FB".into(),
            surface: "#FFFFFF".into(),
            surface_alt: "#EDF2F8".into(),
            border: "#DFE7F0".into(),
            text: "#1B222B".into(),
            text_weak: "#66717F".into(),
            radius: 10,
            ..Default::default()
        },
        ThemeDef {
            id: "sakura".into(),
            name: "樱粉".into(),
            dark: false,
            accent: "#C2607A".into(),
            bg: "#FBF6F7".into(),
            surface: "#FFFFFF".into(),
            surface_alt: "#F7EEF1".into(),
            border: "#EFE0E4".into(),
            text: "#2A1F23".into(),
            text_weak: "#7C6A70".into(),
            radius: 12,
            ..Default::default()
        },
        ThemeDef {
            id: "dark".into(),
            name: "深色".into(),
            dark: true,
            accent: "#C98A5B".into(),
            bg: "#1B1D21".into(),
            surface: "#24272C".into(),
            surface_alt: "#2C3036".into(),
            border: "#34383F".into(),
            text: "#E8EAED".into(),
            text_weak: "#9BA1A9".into(),
            radius: 10,
            ..Default::default()
        },
    ];
    for theme in list.iter_mut() {
        theme.builtin = true;
    }
    list
}

/// 扫描 themes 目录下的自定义主题
pub fn custom_themes() -> Vec<ThemeDef> {
    let dir = paths::themes_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut list = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join("theme.json");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        match serde_json::from_str::<ThemeDef>(&text) {
            Ok(mut def) => {
                if def.id.is_empty() {
                    def.id = entry.file_name().to_string_lossy().to_string();
                }
                if def.name.is_empty() {
                    def.name = def.id.clone();
                }
                def.builtin = false;
                def.dir = Some(path.clone());
                list.push(def);
            }
            Err(err) => log::warn!("主题 {} 解析失败: {err}", manifest.display()),
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

pub fn all_themes() -> Vec<ThemeDef> {
    let mut list = builtin_themes();
    list.extend(custom_themes());
    list
}

pub fn find_theme(id: &str) -> ThemeDef {
    all_themes()
        .into_iter()
        .find(|t| t.id == id)
        .unwrap_or_default()
}

/// 把主题目录 / 配置里的背景设置解析成实际文件路径
///
/// 背景也可以指向一个文件夹，这时每次打开启动器都从里面换一张。
pub fn resolve_background(
    theme: &ThemeDef,
    config_bg: &Background,
    random: bool,
) -> Option<PathBuf> {
    if config_bg.kind == BackgroundKind::Image {
        if let Some(path) = &config_bg.path {
            if path.is_file() {
                return Some(path.clone());
            }
            if path.is_dir() {
                let mut files: Vec<PathBuf> = std::fs::read_dir(path)
                    .ok()?
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|file| {
                        file.is_file()
                            && matches!(
                                file.extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| e.to_ascii_lowercase())
                                    .as_deref(),
                                Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
                            )
                    })
                    .collect();
                files.sort();
                if files.is_empty() {
                    return None;
                }
                let index = if random && files.len() > 1 {
                    use rand::Rng;
                    rand::rng().random_range(0..files.len())
                } else {
                    0
                };
                return Some(files[index].clone());
            }
        }
    }
    if let Some(relative) = &theme.background {
        if let Some(dir) = &theme.dir {
            let path = dir.join(relative);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// 加载背景帧（GIF 返回多帧），返回 (图像, 每帧时长秒)
pub fn load_background_frames(path: &Path) -> anyhow::Result<Vec<(egui::ColorImage, f32)>> {
    use image::AnimationDecoder;

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if extension == "gif" {
        let file = std::fs::File::open(path)?;
        let decoder = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file))?;
        let frames = decoder.into_frames().collect_frames()?;
        let mut out = Vec::with_capacity(frames.len());
        for frame in frames {
            let (numer, denom) = frame.delay().numer_denom_ms();
            let delay = if denom == 0 {
                0.1
            } else {
                (numer as f32 / denom as f32 / 1000.0).clamp(0.02, 5.0)
            };
            let buffer = frame.into_buffer();
            let (w, h) = (buffer.width(), buffer.height());
            out.push((
                egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    buffer.as_raw(),
                ),
                delay,
            ));
        }
        if out.is_empty() {
            anyhow::bail!("GIF 没有可用帧");
        }
        return Ok(out);
    }

    let img = image::open(path)?;
    let img = downscale(img, 2560);
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Ok(vec![(
        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw()),
        0.0,
    )])
}

fn downscale(img: image::DynamicImage, max_width: u32) -> image::DynamicImage {
    if img.width() <= max_width {
        return img;
    }
    let ratio = max_width as f32 / img.width() as f32;
    let height = (img.height() as f32 * ratio).round() as u32;
    img.resize(max_width, height, image::imageops::FilterType::Triangle)
}

/// 生成一个示例主题目录，方便用户照着改
pub fn write_example_theme() -> anyhow::Result<PathBuf> {
    let dir = paths::themes_dir().join("my-theme");
    paths::ensure_dir(&dir)?;
    let def = ThemeDef {
        id: "my-theme".into(),
        name: "我的主题".into(),
        builtin: false,
        ..Default::default()
    };
    let path = dir.join("theme.json");
    std::fs::write(&path, serde_json::to_string_pretty(&def)?)?;
    Ok(path)
}
