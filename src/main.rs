#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// 控件库与工具层刻意保留了比当前页面用量更多的通用能力（按钮变体、格式化、
// 路径辅助等），后续页面会陆续用到，这里统一放行死代码告警。
#![allow(dead_code)]

mod app;
mod core;
mod selftest;
mod icons;
mod theme;
mod ui;
mod views;

const APP_TITLE: &str = "胡桃木 Minecraft 启动器";

fn window_icon() -> egui::IconData {
    let svg = include_str!("../assets/icon.svg");
    match icons::rasterize_to_rgba(svg, 256) {
        Some((width, height, rgba)) => egui::IconData {
            rgba,
            width,
            height,
        },
        None => egui::IconData {
            rgba: vec![0, 0, 0, 0],
            width: 1,
            height: 1,
        },
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--selftest") {
        let index = args.iter().position(|a| a == "--selftest").unwrap_or(0);
        // 版本号是 --selftest 后的第一个非选项参数
        let version = args
            .get(index + 1)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| "1.20.1".to_string());
        // 加载器是版本号之后的下一个非选项参数
        let loader = args
            .get(index + 2)
            .filter(|a| !a.starts_with("--"))
            .cloned();
        // --install-only 只安装不启动，用于验证安装管线
        let install_only = args.iter().any(|a| a == "--install-only");
        match selftest::run(&version, loader.as_deref(), install_only) {
            Ok(()) => std::process::exit(0),
            Err(err) => {
                eprintln!("自检失败: {err:#}");
                std::process::exit(1);
            }
        }
    }

    let config = core::config::Config::load();
    let size = config.ui.window_size;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_inner_size([size[0].max(960.0), size[1].max(640.0)])
            .with_min_inner_size([960.0, 640.0])
            .with_decorations(false)
            .with_resizable(true)
            .with_icon(window_icon()),
        ..Default::default()
    };

    // 崩溃信息记进日志
    //
    // 发布版没有控制台，egui 的 panic 默认只打到 stderr，
    // 出了事就只剩一个闪退，日志里留一笔才查得动。
    install_panic_hook();

    eframe::run_native(
        "htmcl",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

/// 把 panic 的内容写进日志文件
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "未知原因".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "位置未知".to_string());
        log::error!("启动器崩溃：{message}（{location}）");
        previous(info);
    }));
}
