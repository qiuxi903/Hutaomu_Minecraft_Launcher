use anyhow::{anyhow, Context, Result};
use image::GenericImageView;

use crate::core::paths;

/// 皮肤贴图（RGBA8）
#[derive(Debug, Clone)]
pub struct SkinImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub type Rgba = [u8; 4];

impl SkinImage {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> Rgba {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let idx = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[idx],
            self.rgba[idx + 1],
            self.rgba[idx + 2],
            self.rgba[idx + 3],
        ]
    }

    pub fn set(&mut self, x: u32, y: u32, color: Rgba) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.rgba[idx..idx + 4].copy_from_slice(&color);
    }

    pub fn fill(&mut self, x: u32, y: u32, w: u32, h: u32, color: Rgba) {
        for dy in 0..h {
            for dx in 0..w {
                self.set(x + dx, y + dy, color);
            }
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let img = image::load_from_memory(bytes).context("无法解析皮肤图片")?;
        let (width, height) = img.dimensions();
        if width != 64 || (height != 64 && height != 32) {
            return Err(anyhow!(
                "皮肤尺寸必须是 64x64 或 64x32，当前为 {width}x{height}"
            ));
        }
        let rgba = img.to_rgba8();
        Ok(Self {
            width,
            height,
            rgba: rgba.into_raw(),
        })
    }

    pub fn load(path: &std::path::Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("读取皮肤文件失败: {}", path.display()))?;
        Self::from_bytes(&bytes)
    }

    /// 纤细模型检测：经典模型在 (46,52) 处有像素，纤细模型为透明
    pub fn detect_slim(&self) -> bool {
        if self.height < 64 {
            return false;
        }
        self.get(46, 52)[3] == 0 && self.get(54, 20)[3] == 0
    }

    pub fn to_color_image(&self) -> egui::ColorImage {
        let mut rgba = self.rgba.clone();
        // 透明处的颜色铺开，免得线性过滤在边缘拉出黑边
        bleed_alpha(
            &mut rgba,
            self.width as usize,
            self.height as usize,
            1,
        );
        egui::ColorImage::from_rgba_unmultiplied(
            [self.width as usize, self.height as usize],
            &rgba,
        )
    }

    /// 第二层专用的「镂空」版本
    ///
    /// Minecraft 渲染皮肤第二层用的是 cutout：alpha 低于阈值的像素直接丢掉，
    /// 不做半透明混合。直接画原图的话，第二层的半透明像素会和第一层混在一起，
    /// 整个模型看着发灰。这里把半透明像素抹成全透明，还原游戏里的效果。
    pub fn to_cutout_color_image(&self) -> egui::ColorImage {
        let mut rgba = self.rgba.clone();
        for px in rgba.chunks_exact_mut(4) {
            if px[3] < 128 {
                px[3] = 0;
            }
        }
        // 被抹成全透明的像素也要铺上颜色，不然边缘会镶黑边
        bleed_alpha(
            &mut rgba,
            self.width as usize,
            self.height as usize,
            128,
        );
        egui::ColorImage::from_rgba_unmultiplied([self.width as usize, self.height as usize], &rgba)
    }

    /// 头像：8x8 脸部 + 帽子叠加层，最近邻放大
    pub fn head_icon(&self, size: u32) -> egui::ColorImage {
        let mut out = egui::ColorImage::new(
            [size as usize, size as usize],
            vec![egui::Color32::TRANSPARENT; (size * size) as usize],
        );
        let scale = size as f32 / 8.0;
        for y in 0..8u32 {
            for x in 0..8u32 {
                let base = self.get(8 + x, 8 + y);
                let overlay = if self.height >= 64 {
                    self.get(40 + x, 8 + y)
                } else {
                    [0, 0, 0, 0]
                };
                let color = blend(base, overlay);
                let px0 = (x as f32 * scale) as usize;
                let py0 = (y as f32 * scale) as usize;
                let px1 = ((x + 1) as f32 * scale) as usize;
                let py1 = ((y + 1) as f32 * scale) as usize;
                for py in py0..py1.min(size as usize) {
                    for px in px0..px1.min(size as usize) {
                        out.pixels[py * size as usize + px] = egui::Color32::from_rgba_unmultiplied(
                            color[0], color[1], color[2], color[3],
                        );
                    }
                }
            }
        }
        out
    }
}

/// 把透明处的颜色用最近的实心像素铺开
///
/// 线性过滤会在相邻像素之间插值。透明像素原本留着什么颜色
/// （多半是黑），边缘就会镶上一圈脏边，看着毛糙。
/// 这里把透明处填成邻居的颜色，插值时就不会拉进黑色。
///
/// `opaque_threshold` 是「算实心」的 alpha 门槛：
/// 传 1 表示只有全透明的算空，传 128 则半透明的也算空。
fn bleed_alpha(rgba: &mut [u8], width: usize, height: usize, opaque_threshold: u8) {
    if width == 0 || height == 0 {
        return;
    }
    let sources: Vec<bool> = rgba
        .chunks_exact(4)
        .map(|px| px[3] >= opaque_threshold)
        .collect();
    let mut filled = sources.clone();
    // 空白区可能连着几十个像素，多铺几轮才能铺满
    for _ in 0..64 {
        let mut changed = false;
        for y in 0..height {
            for x in 0..width {
                let index = y * width + x;
                if filled[index] {
                    continue;
                }
                let mut source = None;
                for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                        continue;
                    }
                    let neighbor = ny as usize * width + nx as usize;
                    // 从已经铺过的像素继续往外铺，
                    // 一次只铺一格的话，大片透明区根本铺不到
                    if filled[neighbor] {
                        source = Some(neighbor);
                        break;
                    }
                }
                if let Some(neighbor) = source {
                    let (to, from) = (index * 4, neighbor * 4);
                    rgba[to] = rgba[from];
                    rgba[to + 1] = rgba[from + 1];
                    rgba[to + 2] = rgba[from + 2];
                    filled[index] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn blend(base: Rgba, overlay: Rgba) -> Rgba {
    if overlay[3] == 0 {
        return base;
    }
    if overlay[3] == 255 {
        return overlay;
    }
    let a = overlay[3] as f32 / 255.0;
    let mix = |b: u8, o: u8| ((b as f32) * (1.0 - a) + (o as f32) * a).round() as u8;
    [
        mix(base[0], overlay[0]),
        mix(base[1], overlay[1]),
        mix(base[2], overlay[2]),
        255,
    ]
}

/// 下载皮肤到启动器缓存并返回本地路径
pub fn download_skin(
    dl: &crate::core::download::Downloader,
    url: &str,
    key: &str,
) -> Result<std::path::PathBuf> {
    let dir = paths::cache_dir().join("skins");
    paths::ensure_dir(&dir)?;
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let dest = dir.join(format!("{safe}.png"));
    let bytes = dl.get_bytes_direct(url).context("下载皮肤失败")?;
    // 校验能否解析，避免缓存坏文件
    SkinImage::from_bytes(&bytes)?;
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

/// 根据用户名生成一张默认皮肤，让每个离线账号都有可辨识的形象
pub fn generate_default_skin(name: &str) -> SkinImage {
    let mut skin = SkinImage::new(64, 64);
    let seed = fnv1a(name);

    let skin_tones: [Rgba; 5] = [
        [0xF2, 0xCE, 0xAA, 255],
        [0xE6, 0xB8, 0x90, 255],
        [0xC8, 0x96, 0x6E, 255],
        [0x9C, 0x6B, 0x48, 255],
        [0x6E, 0x46, 0x2E, 255],
    ];
    let hairs: [Rgba; 6] = [
        [0x3A, 0x2A, 0x1E, 255],
        [0x6B, 0x4A, 0x2A, 255],
        [0xC9, 0xA0, 0x54, 255],
        [0x8B, 0x3A, 0x2A, 255],
        [0x2E, 0x2E, 0x38, 255],
        [0xE8, 0xE0, 0xD0, 255],
    ];
    let shirts: [Rgba; 6] = [
        [0x4A, 0x7A, 0xB0, 255],
        [0xB0, 0x5A, 0x4A, 255],
        [0x5A, 0x8A, 0x5A, 255],
        [0x7A, 0x5A, 0xA0, 255],
        [0xD0, 0x9A, 0x3A, 255],
        [0x3A, 0x3A, 0x44, 255],
    ];
    let pants: [Rgba; 4] = [
        [0x3A, 0x44, 0x5A, 255],
        [0x4A, 0x3A, 0x2A, 255],
        [0x2E, 0x2E, 0x38, 255],
        [0x5A, 0x4A, 0x6A, 255],
    ];

    let tone = skin_tones[(seed % 5) as usize];
    let hair = hairs[((seed / 5) % 6) as usize];
    let shirt = shirts[((seed / 11) % 6) as usize];
    let pant = pants[((seed / 17) % 4) as usize];
    let shoe = darken(pant, 0.65);
    let shirt_dark = darken(shirt, 0.85);
    let hair_dark = darken(hair, 0.8);
    let eye: Rgba = [0x2A, 0x2A, 0x33, 255];
    let mouth = darken(tone, 0.7);

    // 头部六个面
    skin.fill(8, 0, 8, 8, hair); // 顶
    skin.fill(16, 0, 8, 8, hair_dark); // 底
    skin.fill(0, 8, 8, 8, hair); // 右
    skin.fill(16, 8, 8, 8, hair); // 左
    skin.fill(24, 8, 8, 8, hair); // 后
    skin.fill(8, 8, 8, 8, tone); // 前
    skin.fill(8, 8, 8, 2, hair); // 刘海
    skin.fill(8, 10, 1, 2, hair);
    skin.fill(15, 10, 1, 2, hair);
    skin.fill(10, 11, 1, 2, eye);
    skin.fill(13, 11, 1, 2, eye);
    skin.fill(11, 14, 2, 1, mouth);

    // 身体
    skin.fill(20, 16, 8, 4, shirt_dark); // 顶
    skin.fill(28, 16, 8, 4, shirt_dark); // 底
    skin.fill(16, 20, 4, 12, shirt_dark); // 右
    skin.fill(28, 20, 4, 12, shirt_dark); // 左
    skin.fill(32, 20, 8, 12, shirt_dark); // 后
    skin.fill(20, 20, 8, 12, shirt); // 前
    skin.fill(20, 20, 8, 1, darken(shirt, 0.75)); // 领口

    // 右臂
    skin.fill(44, 16, 4, 4, shirt);
    skin.fill(48, 16, 4, 4, tone);
    skin.fill(40, 20, 4, 12, shirt);
    skin.fill(44, 20, 4, 12, shirt);
    skin.fill(48, 20, 4, 12, shirt);
    skin.fill(52, 20, 4, 12, shirt);
    skin.fill(40, 24, 4, 8, tone);
    skin.fill(44, 24, 4, 8, tone);
    skin.fill(48, 24, 4, 8, tone);
    skin.fill(52, 24, 4, 8, tone);

    // 左臂
    skin.fill(36, 48, 4, 4, shirt);
    skin.fill(40, 48, 4, 4, tone);
    skin.fill(32, 52, 4, 12, shirt);
    skin.fill(36, 52, 4, 12, shirt);
    skin.fill(40, 52, 4, 12, shirt);
    skin.fill(44, 52, 4, 12, shirt);
    skin.fill(32, 56, 4, 8, tone);
    skin.fill(36, 56, 4, 8, tone);
    skin.fill(40, 56, 4, 8, tone);
    skin.fill(44, 56, 4, 8, tone);

    // 右腿
    skin.fill(4, 16, 4, 4, pant);
    skin.fill(8, 16, 4, 4, shoe);
    skin.fill(0, 20, 4, 12, pant);
    skin.fill(4, 20, 4, 12, pant);
    skin.fill(8, 20, 4, 12, pant);
    skin.fill(12, 20, 4, 12, pant);
    skin.fill(0, 30, 4, 2, shoe);
    skin.fill(4, 30, 4, 2, shoe);
    skin.fill(8, 30, 4, 2, shoe);
    skin.fill(12, 30, 4, 2, shoe);

    // 左腿
    skin.fill(20, 48, 4, 4, pant);
    skin.fill(24, 48, 4, 4, shoe);
    skin.fill(16, 52, 4, 12, pant);
    skin.fill(20, 52, 4, 12, pant);
    skin.fill(24, 52, 4, 12, pant);
    skin.fill(28, 52, 4, 12, pant);
    skin.fill(16, 62, 4, 2, shoe);
    skin.fill(20, 62, 4, 2, shoe);
    skin.fill(24, 62, 4, 2, shoe);
    skin.fill(28, 62, 4, 2, shoe);

    skin
}

fn darken(color: Rgba, factor: f32) -> Rgba {
    [
        (color[0] as f32 * factor).round() as u8,
        (color[1] as f32 * factor).round() as u8,
        (color[2] as f32 * factor).round() as u8,
        color[3],
    ]
}

fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 透明处的颜色会被邻居铺开() {
        // 4x4：最左边一列是实心红色，右边全透明
        let mut rgba = vec![0u8; 4 * 4 * 4];
        for y in 0..4 {
            let i = (y * 4) * 4;
            rgba[i] = 255;
            rgba[i + 1] = 40;
            rgba[i + 2] = 10;
            rgba[i + 3] = 255;
        }
        bleed_alpha(&mut rgba, 4, 4, 1);
        for y in 0..4 {
            for x in 1..4 {
                let i = (y * 4 + x) * 4;
                assert_eq!(rgba[i], 255, "({x},{y}) 的红色应该从左边铺过来");
                assert_eq!(rgba[i + 1], 40, "({x},{y}) 的绿色应该从左边铺过来");
                assert_eq!(rgba[i + 3], 0, "({x},{y}) 的透明度不该被改");
            }
        }
    }

    #[test]
    fn 全透明时铺开不会乱改() {
        let mut rgba = vec![0u8; 4 * 4 * 4];
        let before = rgba.clone();
        bleed_alpha(&mut rgba, 4, 4, 1);
        assert_eq!(rgba, before);
    }

    #[test]
    fn default_skin_is_valid_size() {
        let skin = generate_default_skin("Steve");
        assert_eq!(skin.width, 64);
        assert_eq!(skin.height, 64);
        // 脸部中心应当是不透明的皮肤色
        assert_eq!(skin.get(12, 12)[3], 255);
    }
}
