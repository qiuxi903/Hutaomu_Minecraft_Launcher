//! 从 assets/icon.svg 生成 assets/htmcl.ico 与 assets/icon.png。
//! 用法：cargo run --example gen_icon

use std::fs;
use std::path::Path;

use resvg::tiny_skia;
use resvg::usvg;

fn render(svg: &str, size: u32) -> Vec<u8> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).expect("parse svg");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().expect("encode png")
}

fn main() {
    let svg = fs::read_to_string("assets/icon.svg").expect("assets/icon.svg");
    let sizes: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

    let mut pngs: Vec<(u32, Vec<u8>)> = Vec::new();
    for s in sizes {
        pngs.push((s, render(&svg, s)));
    }

    // 256 尺寸单独导出，供窗口图标与文档使用
    fs::write("assets/icon.png", &pngs[pngs.len() - 1].1).expect("write icon.png");

    // 组装 ICO（Vista+ 支持内嵌 PNG）
    let count = pngs.len() as u16;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type = icon
    out.extend_from_slice(&count.to_le_bytes());

    let mut offset = 6 + 16 * pngs.len() as u32;
    for (size, data) in &pngs {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.push(dim); // width
        out.push(dim); // height
        out.push(0); // palette
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bpp
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &pngs {
        out.extend_from_slice(data);
    }

    fs::write("assets/htmcl.ico", &out).expect("write ico");
    println!(
        "生成完成: assets/htmcl.ico ({} 字节), assets/icon.png",
        out.len()
    );
    assert!(Path::new("assets/htmcl.ico").exists());
}
