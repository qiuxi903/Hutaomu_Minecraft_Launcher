use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=assets/icons");
    println!("cargo:rerun-if-changed=assets/htmcl.ico");
    println!("cargo:rerun-if-changed=assets/icon.svg");
    println!("cargo:rerun-if-changed=build.rs");

    gen_icons();

    #[cfg(windows)]
    {
        if Path::new("assets/htmcl.ico").exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/htmcl.ico");
            res.set("ProductName", "Hutaomu Minecraft Launcher");
            res.set("FileDescription", "胡桃木 Minecraft 启动器");
            res.set("LegalCopyright", "AGPL-3.0 (c) qiuxi903");
            res.set("OriginalFilename", "htmcl.exe");
            let _ = res.compile();
        }
    }
}

/// 扫描 assets/icons 下的 SVG，生成 (名称, 源码) 静态表，供 include_str! 内嵌进 exe。
fn gen_icons() {
    let dir = Path::new("assets/icons");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let mut entries: Vec<(String, String)> = Vec::new();

    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("svg") {
                continue;
            }
            let name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let abs = fs::canonicalize(&path).unwrap_or(path.clone());
            let abs = abs.to_string_lossy().replace('\\', "/");
            let abs = abs.strip_prefix("//?/").unwrap_or(&abs).to_string();
            entries.push((name, abs));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut src = String::from("// 由 build.rs 自动生成，请勿手改。\n");
    src.push_str("pub static ICON_SOURCES: &[(&str, &str)] = &[\n");
    for (name, abs) in &entries {
        src.push_str(&format!("    ({:?}, include_str!(r\"{}\")),\n", name, abs));
    }
    src.push_str("];\n");

    let dest = out.join("icons_generated.rs");
    fs::write(&dest, src).expect("write icons_generated.rs");
}
