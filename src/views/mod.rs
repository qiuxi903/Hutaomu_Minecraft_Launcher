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

    // 皮肤站的令牌有效期不长，过期后启动会直接失败，
    // 与其让游戏报一堆看不懂的错，不如先问一次皮肤站。
    // 记过密码的话，过期了就用密码自动换一张新令牌，不用用户再输一遍。
    if let crate::core::auth::Account::Authlib(a) = &account {
        let expired = a.expires_at > 0 && a.expires_at <= chrono::Utc::now().timestamp();
        // 令牌也可能被皮肤站单方面作废（比如在别处登录），
        // 这种情况下游戏能进但皮肤会变回默认角色
        let mut invalid = false;
        if let Ok(dl) = app.downloader() {
            match crate::core::auth::authlib::validate(
                &dl,
                &a.server_url,
                &a.access_token,
                &a.client_token,
            ) {
                Ok(false) => invalid = true,
                Err(err) => {
                    // 连不上皮肤站就别拦着，先进游戏再说
                    log::warn!("检查皮肤站登录状态失败：{err}");
                }
                Ok(true) => {}
            }
        }
        if expired || invalid {
            // 记住过密码就自动续期
            if !app.authlib_relogin_attempted {
                if let Some(pass) = a
                    .saved_password
                    .as_ref()
                    .and_then(|p| crate::core::auth::secret::unprotect(p))
                {
                    if !a.username.is_empty() {
                        app.authlib_relogin_attempted = true;
                        start_authlib_relogin(app, a.clone(), pass);
                        return;
                    }
                }
            }
            app.error("皮肤站登录已过期，请到「账号」页面重新登录");
            app.accounts_ui.tab = 2;
            app.accounts_ui.authlib_user = a.username.clone();
            app.page = crate::app::Page::Accounts;
            return;
        }
    }

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

    // 版本 JSON 里的 javaVersion 是 Mojang 官方声明的要求，
    // 比按版本号估算的表准（26.x 要求 Java 25）
    let required_major = resolved
        .json
        .java_version
        .as_ref()
        .map(|v| v.major_version)
        .unwrap_or_else(|| crate::core::java::required_major(&mc_version));

    let Some(java) = crate::core::java::pick_for_version(
        required_major,
        app.cfg.java.path.as_deref(),
        &app.javas,
    ) else {
        app.error("没有找到可用的 Java，请到「设置 - Java」搜索或手动指定");
        return;
    };

    let authlib_url = account.authlib_url().map(|s| s.to_string());
    if authlib_url.is_some() && app.authlib_jar.is_none() {
        // 组件还没准备好，先去下载，别让用户干等
        app.refresh_authlib_jar();
        match app.authlib_jar_error.clone() {
            Some(err) => app.error(format!("皮肤站组件下载失败：{err}")),
            None => app.info("正在准备皮肤站组件，稍等片刻再启动"),
        }
        return;
    }

    let identity = account.to_identity();
    let memory = app.cfg.effective_memory(&mc_version);
    let jvm_args = app.cfg.effective_jvm_args(&mc_version);
    let extra_args = app.cfg.launch.extra_args.clone();
    let fullscreen = app.cfg.launch.fullscreen;
    let width = app.cfg.launch.width;
    let height = app.cfg.launch.height;
    let authlib_jar = app.authlib_jar.clone();

    // 版本隔离：游戏目录指向该 Minecraft 版本的实例目录，
    // 原版与各加载器版本共用这一份，模组与存档不会各分一份
    let effective_dir = app.cfg.effective_game_dir(&mc_version);
    if let Err(err) = paths::ensure_dir(&effective_dir) {
        app.error(format!("创建游戏目录失败：{err}"));
        return;
    }
    resolved.game_dir = effective_dir;

    // 首次启动时给个默认语言，用户之后自己改的不动
    if let Err(err) = crate::core::minecraft::launch::ensure_default_lang(&resolved.game_dir) {
        log::warn!("设置游戏语言失败：{err}");
    }

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

/// 用记住的密码重新登录皮肤站，换一张新令牌
///
/// 令牌彻底过期后 refresh 接口换不了新的（它要拿旧令牌去换），
/// 只能重新走一次认证。续期成功后再继续启动游戏。
fn start_authlib_relogin(app: &mut App, account: crate::core::auth::AuthlibAccount, password: String) {
    let account_id = account.id.clone();
    let server = account.server_url.clone();
    let user = account.username.clone();
    let profile_id = account.uuid.clone();
    let timeout = app.cfg.download.timeout_secs;
    app.info("皮肤站登录已过期，正在自动续期 ...");

    app.spawn("皮肤站自动续期", true, move |job| {
        let result = (|| -> anyhow::Result<crate::core::auth::authlib::AuthlibAuth> {
            let dl = crate::core::download::Downloader::new(
                crate::core::download::DownloadSource::Official,
                timeout,
            )?;
            crate::core::auth::authlib::relogin(
                &dl,
                &server,
                &user,
                &password,
                Some(&profile_id),
            )
        })();
        job.send(crate::app::CoreEvent::AuthlibRelogin {
            account_id,
            result: result.map_err(|e| format!("{e}")),
        });
        job.done(true, "");
    });
}

/// 打开版本对应的游戏目录
pub fn open_version_dir(app: &mut App, mc_version: &str) {
    let dir = app.cfg.effective_game_dir(mc_version);
    let _ = paths::ensure_dir(&dir);
    paths::reveal(&dir);
}
