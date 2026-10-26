//! 端到端自检：真实下载一个版本、构建启动命令并实际启动游戏。
//! 用法：htmcl.exe --selftest [版本号]

use std::time::Duration;

use crate::core::auth;
use crate::core::download::{DownloadProgress, DownloadSource, Downloader};
use crate::core::java;
use crate::core::minecraft::{bundle, install, launch, loaders, manifest, version};
use crate::core::paths;

pub fn run(version_id: &str, loader: Option<&str>, install_only: bool) -> anyhow::Result<()> {
    crate::core::paths::migrate_legacy_layout();
    crate::core::log::init();
    println!("=== htmcl 自检开始（版本 {version_id}）===");

    let source = DownloadSource::Bmclapi;
    let dl = Downloader::new(source, 60)?;

    println!("[1/5] 获取版本清单 ...");
    let manifest = manifest::fetch_manifest(source)?;
    println!(
        "      共 {} 个版本，最新正式版 {}",
        manifest.versions.len(),
        manifest.latest.release
    );
    let entry = manifest::find(&manifest, version_id)
        .ok_or_else(|| anyhow::anyhow!("版本 {version_id} 不在清单中"))?
        .clone();

    println!("[2/5] 搜索 Java ...");
    let javas = java::brief_search();
    for j in &javas {
        println!(
            "      {} | {} | {}-bit | {} | {}",
            j.version,
            j.vendor,
            j.bits,
            j.major,
            j.path.display()
        );
    }
    if javas.is_empty() {
        anyhow::bail!("没有找到任何 Java");
    }

    println!("[3/5] 下载并安装 {version_id} ...");
    let game_dir = paths::exe_dir().join(".minecraft");
    paths::ensure_dir(&game_dir)?;
    let progress = DownloadProgress::new();
    {
        let progress = progress.clone();
        std::thread::spawn(move || loop {
            if progress.is_cancelled() || progress.finished.load(std::sync::atomic::Ordering::Relaxed)
            {
                break;
            }
            println!(
                "      {:>5.1}%  {}",
                progress.fraction() * 100.0,
                progress.message()
            );
            std::thread::sleep(Duration::from_millis(1500));
        });
    }

    // 加载器类型：交给统一安装管线，游戏与加载器一起装
    let kind = match loader {
        None => None,
        Some(name) => Some(match name.to_lowercase().as_str() {
            "fabric" => loaders::LoaderKind::Fabric,
            "quilt" => loaders::LoaderKind::Quilt,
            "forge" => loaders::LoaderKind::Forge,
            "neoforge" => loaders::LoaderKind::NeoForge,
            other => anyhow::bail!("未知加载器 {other}"),
        }),
    };

    println!("[3.5/5] 统一安装管线（游戏 + 加载器）...");
    let req = bundle::InstallRequest {
        mc_version: version_id.to_string(),
        entry: entry.clone(),
        loader: kind,
        loader_version: None,
        loader_policy: loaders::LoaderPolicy::Stable,
        timing: bundle::LoaderTiming::Parallel,
        game_dir: game_dir.clone(),
        java: java::pick_for(version_id, &javas),
        opts: install::InstallOptions {
            threads: 8,
            install_assets: true,
        },
    };
    let outcome = bundle::install_bundle(&dl, &req, &progress)?;
    println!("      {}", outcome.report.summary());
    if let Some(loader_id) = &outcome.loader_id {
        println!("      加载器版本 id: {loader_id}");
    }

    // 原版已经在本地了，再给它追加一个加载器变体。
    // 这走的正是版本详情页「更换加载器」那条路：变体通过 inheritsFrom
    // 挂在原版下面，扫描时会自动归到同一个实例里。
    println!("[3.8/7] 给已装好的版本追加一个加载器变体 ...");
    // 换一种加载器，别跟刚装的那个重样
    let variant_kind = match kind {
        Some(loaders::LoaderKind::Fabric) => loaders::LoaderKind::Quilt,
        _ => loaders::LoaderKind::Fabric,
    };
    let mut variant_id: Option<String> = None;
    if loaders::supports(variant_kind, version_id) {
        let list = loaders::list_versions(&dl, variant_kind, version_id)?;
        if let Some(picked) = loaders::pick_version(&list, loaders::LoaderPolicy::Stable) {
            match loaders::install_variant(
                &dl,
                variant_kind,
                version_id,
                &picked,
                &game_dir,
                java::pick_for(version_id, &javas).as_ref(),
                &progress,
                install::InstallOptions {
                    threads: 8,
                    install_assets: false,
                },
            ) {
                Ok(id) => {
                    let resolved = version::resolve(&game_dir, &id)?;
                    let detected =
                        version::detect_mc_version(&game_dir, &id, &resolved.json);
                    println!(
                        "      变体 {id}（{} {picked}）归到的 Minecraft 版本：{detected}",
                        variant_kind.label()
                    );
                    if detected == version_id {
                        variant_id = Some(id);
                    } else {
                        println!("      变体没有归到 {version_id} 下面，换回原版本启动");
                    }
                }
                Err(err) => println!("      追加变体失败：{err}"),
            }
        }
    } else {
        println!("      {} 不支持 Minecraft {version_id}，跳过", variant_kind.label());
    }

    let launch_id = variant_id
        .clone()
        .unwrap_or_else(|| outcome.loader_id.clone().unwrap_or_else(|| outcome.game_id.clone()));

    progress
        .finished
        .store(true, std::sync::atomic::Ordering::Relaxed);

    println!("[4/5] 构建启动命令 ...");
    let resolved = version::resolve(&game_dir, &launch_id)?;
    let account = auth::offline::create("HtmclTest");
    let identity = account.to_identity();
    let java = java::pick_for(version_id, &javas)
        .ok_or_else(|| anyhow::anyhow!("没有满足 {version_id} 要求的 Java"))?;
    println!("      使用 Java {} ({})", java.version, java.path.display());

    let spec = launch::LaunchSpec {
        version: &resolved,
        identity: &identity,
        java: &java,
        memory_mb: 2048,
        initial_memory_mb: 0,
        width: 854,
        height: 480,
        fullscreen: false,
        extra_jvm_args: "",
        extra_game_args: "",
        authlib_jar: None,
        server: None,
        garbage_collector: "auto",
        no_default_jvm_args: false,
        custom_title: "",
        no_javaw: false,
    };
    let command = launch::build_command(&spec)?;
    println!("      命令: {}", launch::describe_command(&command));

    if install_only {
        println!("[5/5] 仅安装模式，跳过启动游戏");
        return Ok(());
    }

    println!("[5/5] 启动游戏（最多观察 90 秒）...");
    let mut process = launch::spawn(command, &launch_id)?;
    let mut last_len = 0usize;
    for _ in 0..90 {
        std::thread::sleep(Duration::from_secs(1));
        let lines = process.log.snapshot(4000);
        for line in lines.iter().skip(last_len) {
            println!("      | {line}");
        }
        last_len = lines.len();
        if let Ok(Some(status)) = process.child.try_wait() {
            println!("      游戏进程退出，代码 {:?}", status.code());
            break;
        }
    }
    if process.is_running() {
        println!("      游戏仍在运行，判定启动成功，结束进程");
        process.kill();
    }
    println!("[6/6] Modrinth 内容接口 ...");
    {
        let dl2 = crate::core::download::Downloader::new(
            crate::core::download::DownloadSource::Official,
            30,
        )?;
        let result = crate::core::content::modrinth::search(
            &dl2,
            crate::core::content::ContentKind::Mod,
            "sodium",
            Some("1.20.1"),
            Some("fabric"),
            0,
            5,
        )?;
        println!(
            "      搜索到 {} 条（总计 {}）",
            result.hits.len(),
            result.total_hits
        );
        for hit in result.hits.iter().take(5) {
            println!(
                "      {} | {} | 下载 {}",
                hit.title,
                hit.author,
                crate::core::content::modrinth::format_downloads(hit.downloads)
            );
        }
        if let Some(first) = result.hits.first() {
            let versions = crate::core::content::modrinth::versions(
                &dl2,
                &first.project_id,
                Some("1.20.1"),
                Some("fabric"),
            )?;
            println!("      {} 可用版本 {} 个", first.title, versions.len());
            if let Some(v) = versions.first() {
                if let Some(file) = v.primary_file() {
                    println!(
                        "      最新: {} · {} · {} · {}",
                        v.version_number,
                        v.loader_label(),
                        v.game_versions.join(","),
                        file.filename
                    );
                }
            }
        }
    }

    println!("[6.5/7] 光影加载器安装（装到临时目录，不动用户数据） ...");
    {
        use crate::core::minecraft::shader_loader::{self, ShaderLoader};
        let temp_dir = paths::cache_dir().join("selftest-shader");
        let _ = std::fs::remove_dir_all(&temp_dir);
        paths::ensure_dir(&temp_dir)?;
        let dl2 = crate::core::download::Downloader::new(
            crate::core::download::DownloadSource::Official,
            60,
        )?;
        let progress = DownloadProgress::new();
        match shader_loader::install(
            &dl2,
            ShaderLoader::Iris,
            "1.20.1",
            loaders::LoaderKind::Fabric,
            &temp_dir,
            &progress,
        ) {
            Ok((path, extra)) => {
                println!(
                    "      Iris 已装到 {}（前置依赖 {extra} 个）",
                    path.display()
                );
                match shader_loader::detect(&temp_dir) {
                    Some(inst) => println!(
                        "      本地认出：{} {}",
                        inst.kind.label(),
                        inst.version
                    ),
                    None => println!("      本地没认出来"),
                }
            }
            Err(err) => println!("      光影加载器安装失败：{err}"),
        }
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    println!("[7/7] 资讯源与皮肤站接口 ...");
    {
        let dl2 = crate::core::download::Downloader::new(
            crate::core::download::DownloadSource::Official,
            30,
        )?;
        for preset in crate::core::news::PRESETS {
            match crate::core::news::fetch(&dl2, preset.id, "") {
                Ok(items) => println!(
                    "      {}：{} 条（首条：{}）",
                    preset.name,
                    items.len(),
                    items.first().map(|i| i.title.clone()).unwrap_or_default()
                ),
                Err(err) => println!("      {}：失败 {}", preset.name, err),
            }
        }
        // 用错误的账号密码验证 Yggdrasil 响应解析（应返回友好错误而不是“未返回令牌”）
        match crate::core::auth::authlib::login(
            &dl2,
            crate::core::auth::authlib::DEFAULT_SERVER,
            "htmcl_selftest_not_exist",
            "wrong-password",
        ) {
            Ok(_) => println!("      皮肤站：意外登录成功"),
            Err(err) => println!("      皮肤站错误处理：{err}"),
        }
    }

    println!("=== 自检结束 ===");
    Ok(())
}
/// 直接启动本地已装好的版本，不重新下载安装
///
/// 用来验证整合包、加载器这类"装好了但起不来"的场景：
/// `htmcl.exe --selftest-local fps`
pub fn run_local(version_id: &str) -> anyhow::Result<()> {
    crate::core::paths::migrate_legacy_layout();
    crate::core::log::init();
    println!("=== htmcl 本地版本自检（{version_id}）===");

    let game_dir = paths::exe_dir().join(".minecraft");
    let resolved = version::resolve(&game_dir, version_id)
        .map_err(|e| anyhow::anyhow!("本地没有这个版本：{e}"))?;
    let mc_version = version::detect_mc_version(&game_dir, version_id, &resolved.json);
    println!(
        "      Minecraft 版本 {mc_version}（版本描述 id={} 继承={:?}）",
        resolved.json.id, resolved.json.inherits_from
    );

    let javas = java::brief_search();
    let java = java::pick_for(&mc_version, &javas)
        .ok_or_else(|| anyhow::anyhow!("没有满足 {mc_version} 要求的 Java"))?;
    println!("      使用 Java {} ({})", java.version, java.path.display());

    let account = auth::offline::create("HtmclTest");
    let identity = account.to_identity();
    let spec = launch::LaunchSpec {
        version: &resolved,
        identity: &identity,
        java: &java,
        memory_mb: 2048,
        initial_memory_mb: 0,
        width: 854,
        height: 480,
        fullscreen: false,
        extra_jvm_args: "",
        extra_game_args: "",
        authlib_jar: None,
        server: None,
        garbage_collector: "auto",
        no_default_jvm_args: false,
        custom_title: "",
        no_javaw: false,
    };
    let command = launch::build_command(&spec)?;
    println!("      命令: {}", launch::describe_command(&command));

    println!("[启动游戏] 最多观察 90 秒 ...");
    let mut process = launch::spawn(command, version_id)?;
    let mut last_len = 0usize;
    for _ in 0..90 {
        std::thread::sleep(Duration::from_secs(1));
        let lines = process.log.snapshot(4000);
        for line in lines.iter().skip(last_len) {
            println!("      | {line}");
        }
        last_len = lines.len();
        if let Ok(Some(status)) = process.child.try_wait() {
            println!("      游戏进程退出，代码 {:?}", status.code());
            return Ok(());
        }
    }
    if process.is_running() {
        println!("      游戏仍在运行，判定启动成功，结束进程");
        process.kill();
    } else {
        println!("      游戏已经退出，没有成功启动");
    }
    Ok(())
}