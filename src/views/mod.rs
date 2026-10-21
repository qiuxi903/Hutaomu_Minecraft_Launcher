pub mod about;
pub mod accounts;
pub mod download;
pub mod home;
pub mod home_config;
pub mod settings;
pub mod versions;

use egui::Color32;

use crate::app::App;
use crate::core::paths;

/// 加载器标签配色
pub fn loader_color(loader: Option<&str>) -> Color32 {
    match loader {
        Some("Fabric") => Color32::from_rgb(0x9A, 0x7A, 0x4E),
        Some("Quilt") => Color32::from_rgb(0x8A, 0x5A, 0xA8),
        Some("Forge") => Color32::from_rgb(0x4A, 0x6E, 0xA8),
        Some("NeoForge") => Color32::from_rgb(0xC0, 0x6A, 0x3A),
        Some("OptiFine") => Color32::from_rgb(0x3E, 0x8A, 0x8A),
        Some(other) => {
            let _ = other;
            Color32::from_rgb(0x6E, 0x76, 0x81)
        }
        None => Color32::from_rgb(0x6E, 0x76, 0x81),
    }
}

pub fn version_type_color(version_type: &str) -> Color32 {
    match version_type {
        "release" => Color32::from_rgb(0x35, 0x8A, 0x5A),
        "snapshot" => Color32::from_rgb(0xB8, 0x7A, 0x1E),
        _ => Color32::from_rgb(0x8A, 0x6A, 0x9A),
    }
}

/// 启动游戏：所有前置检查在 UI 线程完成，构建命令后立即返回
pub fn launch_game(app: &mut App) {
    if app.game.is_some() {
        app.error("游戏已经在运行中");
        return;
    }

    let Some(version_id) = app.selected_version_id() else {
        app.error("还没有安装任何游戏版本，请先到「下载」页面安装");
        return;
    };
    let Some(account) = app.selected_account().cloned() else {
        app.error("请先在「账号」页面添加并选择一个账号");
        return;
    };

    let game_dir = app.cfg.game_dir.clone();
    let mut resolved = match crate::core::minecraft::resolve(&game_dir, &version_id) {
        Ok(resolved) => resolved,
        Err(err) => {
            app.error(format!("版本文件不完整：{err}"));
            return;
        }
    };

    let mc_version = crate::core::minecraft::version::detect_mc_version(
        &game_dir,
        &version_id,
        &resolved.json,
    );

    let Some(java) = app.java_for(&mc_version) else {
        app.error("没有找到可用的 Java，请到「设置 - Java」搜索或手动指定");
        return;
    };

    let authlib_url = account.authlib_url().map(|s| s.to_string());
    if authlib_url.is_some() && app.authlib_jar.is_none() {
        app.error("皮肤站组件尚未就绪，请稍后再试");
        return;
    }

    let identity = account.to_identity();
    let memory = app.cfg.effective_memory(&version_id);
    let jvm_args = app.cfg.effective_jvm_args(&version_id);
    let extra_args = app.cfg.launch.extra_args.clone();
    let fullscreen = app.cfg.launch.fullscreen;
    let width = app.cfg.launch.width;
    let height = app.cfg.launch.height;
    let authlib_jar = app.authlib_jar.clone();

    // 版本隔离：把游戏目录指向 versions/<id>
    let effective_dir = app.cfg.effective_game_dir(&version_id);
    if let Err(err) = paths::ensure_dir(&effective_dir) {
        app.error(format!("创建游戏目录失败：{err}"));
        return;
    }
    resolved.game_dir = effective_dir;

    let spec = crate::core::minecraft::launch::LaunchSpec {
        version: &resolved,
        identity: &identity,
        java: &java,
        memory_mb: memory,
        width,
        height,
        fullscreen,
        extra_jvm_args: &jvm_args,
        extra_game_args: &extra_args,
        authlib_jar: authlib_jar.as_deref(),
        server: None,
    };

    let command = match crate::core::minecraft::launch::build_command(&spec) {
        Ok(command) => command,
        Err(err) => {
            app.error(format!("构建启动命令失败：{err}"));
            return;
        }
    };

    log::info!(
        "启动 {} / Java {} / 内存 {}MB",
        version_id,
        java.version,
        memory
    );
    log::debug!("命令: {}", crate::core::minecraft::launch::describe_command(&command));

    match crate::core::minecraft::launch::spawn(command, &version_id) {
        Ok(process) => {
            app.game = Some(process);
            app.game_log_open = app.cfg.launch.show_game_log;
            app.cfg.record_launch(&version_id);
            app.save_config();
            app.success(format!("{version_id} 已启动"));
        }
        Err(err) => app.error(format!("启动失败：{err}")),
    }
}

/// 打开版本对应的游戏目录
pub fn open_version_dir(app: &mut App, version_id: &str) {
    let dir = app.cfg.effective_game_dir(version_id);
    let _ = paths::ensure_dir(&dir);
    paths::reveal(&dir);
}
