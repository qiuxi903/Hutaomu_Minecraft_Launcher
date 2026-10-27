pub mod about;
pub mod accounts;
pub mod download;
pub mod home;
pub mod home_config;
pub mod instance;
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

    // Minecraft 版本：整合包实例要认出它底层的原版版本，
    // 不然会拿实例名去挑 Java，26.3 的游戏被挑了 Java 8 根本起不来
    let mc_version = crate::core::minecraft::version::detect_mc_version(
        &game_dir,
        &version_id,
        &resolved.json,
    );

    // 实例目录用的 id：整合包用自己的名字，普通版本就是 Minecraft 版本
    let dir_id = if crate::core::minecraft::modpack::is_instance_dir(&game_dir, &version_id) {
        version_id.clone()
    } else {
        mc_version.clone()
    };

    // 版本 JSON 里的 javaVersion 是 Mojang 官方声明的要求，
    // 比按版本号估算的表准（26.x 要求 Java 25）
    let required_major = resolved
        .json
        .java_version
        .as_ref()
        .map(|v| v.major_version)
        .unwrap_or_else(|| crate::core::java::required_major(&mc_version));

    // Java：实例自己指定的优先，没指定才用全局的，
    // 两处都没指定就按游戏版本自动挑一个够用的
    let manual_java = app.cfg.effective_java(&dir_id);
    let Some(java) = crate::core::java::pick_for_version(
        required_major,
        manual_java.as_deref(),
        &app.javas,
    ) else {
        app.error("没有找到可用的 Java，请到「设置 - Java」搜索或手动指定");
        return;
    };
    // Java 主版本不够时游戏起不来，先拦一下，
    // 总让人对着「Unrecognized option」猜是哪里的问题
    if !app.cfg.launch.no_jvm_check && !crate::core::java::compatible(&java, required_major) {
        app.error(format!(
            "当前用的是 Java {}，这个版本需要 Java {} 以上，换一个再启动\
             （可在「设置 → 游戏」里关掉兼容性检查）",
            java.version, required_major
        ));
        return;
    }

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
    // 实例级设置按实例 id 取：普通实例的 id 就是 Minecraft 版本，
    // 整合包实例的 id 是它自己的名字（界面上也是按这个存的）
    let memory = app.cfg.effective_memory(&dir_id);
    let jvm_args = app.cfg.effective_jvm_args(&dir_id);
    // 实例自己填的启动参数与窗口标题优先，没填才用全局的
    let extra_args = app.cfg.effective_extra_args(&dir_id);
    let custom_title = app.cfg.effective_custom_title(&dir_id);
    let initial_memory_mb = app.cfg.effective_initial_memory(&dir_id);
    let fullscreen = app.cfg.launch.fullscreen;
    let width = app.cfg.launch.width;
    let height = app.cfg.launch.height;
    let authlib_jar = app.authlib_jar.clone();

    // 版本隔离：游戏目录指向该实例的目录，
    // 原版与各加载器版本共用这一份，模组与存档不会各分一份；
    // 整合包实例用自己的目录
    let effective_dir = app.cfg.effective_game_dir(&dir_id);
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
        initial_memory_mb,
        width,
        height,
        fullscreen,
        extra_jvm_args: &jvm_args,
        extra_game_args: &extra_args,
        authlib_jar: authlib_jar.as_deref(),
        server: None,
        garbage_collector: &app.cfg.launch.garbage_collector,
        no_default_jvm_args: app.cfg.launch.no_default_jvm_args,
        custom_title: &custom_title,
        no_javaw: app.cfg.launch.no_javaw,
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

    let priority = app.cfg.launch.process_priority.clone();
    let visibility = app.cfg.launch.launcher_visibility.clone();
    match crate::core::minecraft::launch::spawn(command, &version_id) {
        Ok(process) => {
            // 进程优先级
            crate::core::minecraft::launch::set_priority(process.pid, &priority);
            app.game = Some(process);
            // 启动不弹日志：日志在后台一直记着，右侧面板里能看
            app.game_log_open = false;
            app.game_starting = true;
            app.game_phase = crate::app::GamePhase::Starting;
            app.game_ready_announced = false;
            app.game_log_docked = app.cfg.ui.log_dock;
            app.cfg.record_launch(&version_id);
            app.save_config();
            // 「正在启动」不再用一闪而过的提示条：过场卡片和右侧面板里都写着
            log::info!("{version_id} 正在启动");
            if app.cfg.launch.sound {
                crate::core::sound::play_start();
            }
            // 启动器可见性：游戏开起来后按设置隐藏或退出
            app.apply_launcher_visibility(&visibility);
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
