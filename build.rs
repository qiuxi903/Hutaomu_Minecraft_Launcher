use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=assets/icons");
    println!("cargo:rerun-if-changed=assets/htmcl.ico");
    println!("cargo:rerun-if-changed=assets/icon.svg");
    println!("cargo:rerun-if-changed=build.rs");

    gen_icons();
    embed_curseforge_key();

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

/// 把 CurseForge 的 API Key 编进二进制
///
/// 这个 key 不能写在源码里：仓库是公开的，源码里出现就等于公开。
/// 所以这里在编译期从两个地方取，取到就编进 exe：
/// 1. `secret/curseforge_key.txt`（本地文件，已在 .gitignore 里排除）
/// 2. 环境变量 `HTMCL_CURSEFORGE_KEY`（CI 里注入）
///
/// 两个都没有就留空，运行时由用户自己在「设置 → 内容平台」里填。
/// 这样从仓库拉下来自己编译的人不会替作者白烧 CF 的接口额度，
/// 官方发布的 exe 又是开箱即用的。
///
/// key 写进 `OUT_DIR` 里的生成文件，源码那边用 `include!` 取，
/// 这样 key 一变，crate 一定会重编，不会留着上一次的旧值。
fn embed_curseforge_key() {
    const DIR: &str = "secret";
    const FILE: &str = "secret/curseforge_key.txt";
    // 目录也要盯：文件被新建、删掉、换名时，目录的 mtime 会变，
    // 只盯文件本身会漏掉「本来没有，后来又放回来」这种情况。
    println!("cargo:rerun-if-changed={DIR}");
    println!("cargo:rerun-if-changed={FILE}");
    println!("cargo:rerun-if-env-changed=HTMCL_CURSEFORGE_KEY");

    let mut key = String::new();
    if let Ok(text) = fs::read_to_string(FILE) {
        // 只取第一行，避免文件里有多余内容把编译搞挂
        if let Some(line) = text.lines().map(str::trim).find(|l| !l.is_empty()) {
            key = line.to_string();
        }
    }
    if key.is_empty() {
        if let Ok(value) = env::var("HTMCL_CURSEFORGE_KEY") {
            key = value.trim().to_string();
        }
    }

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let dest = out.join("curseforge_key.rs");
    // 用 {:?} 生成合法的 Rust 字符串字面量，key 里有 $ / . 之类不用手写转义
    let src = format!(
        "// 由 build.rs 自动生成，请勿手改。\npub const BUILD_CURSEFORGE_KEY: &str = {:?};\n",
        key
    );
    fs::write(&dest, src).expect("write curseforge_key.rs");
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
