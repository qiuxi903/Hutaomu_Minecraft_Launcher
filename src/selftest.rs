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
            println!("      | {}", line.text);
        }
        last_len = lines.len();
        if let Some(code) = process.try_exit() {
            println!("      游戏进程退出，代码 {code:?}");
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
        // 批量取项目：本地列表靠它拿图标
        {
            let slugs = vec!["sodium".to_string(), "iris".to_string()];
            let projects = crate::core::content::modrinth::projects_by_slugs(&dl2, &slugs)?;
            let with_icon = projects
                .iter()
                .filter(|p| p.icon_url.is_some())
                .count();
            println!(
                "      批量取项目 {} 个，其中有图标 {} 个",
                projects.len(),
                with_icon
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

    println!("[6.2/7] CurseForge 内容接口 ...");
    {
        use crate::core::content::curseforge;
        use crate::core::content::ContentKind;
        let key = crate::core::config::DownloadSettings::default().curseforge_key();
        let dl2 = Downloader::new(DownloadSource::Official, 30)?
            .with_curseforge_key(key);
        match curseforge::search(
            &dl2,
            ContentKind::Mod,
            "sodium",
            Some("1.20.1"),
            Some("fabric"),
            0,
            5,
        ) {
            Ok(result) => {
                println!(
                    "      搜索到 {} 条（总计 {}）",
                    result.hits.len(),
                    result.total_hits
                );
                for hit in result.hits.iter().take(5) {
                    println!("      {} | {} | 下载 {}", hit.title, hit.author, hit.downloads);
                }
                // 详情、文件列表、下载地址、依赖、指纹反查
                if let Some(first) = result.hits.first() {
                    let project = curseforge::project(&dl2, &first.project_id, ContentKind::Mod)?;
                    println!("      详情：{}（{}）", project.title, project.slug);
                    let versions = curseforge::versions(
                        &dl2,
                        &first.project_id,
                        Some("1.20.1"),
                        Some("fabric"),
                    )?;
                    println!("      可安装版本 {} 个", versions.len());
                    if let Some(version) = versions.first() {
                        if let (Some(mod_id), Some(file_id)) =
                            (version.cf_mod_id, version.cf_file_id)
                        {
                            let deps = curseforge::dependencies(&dl2, mod_id, file_id)?;
                            println!("      依赖 {} 个", deps.len());
                            let url = curseforge::download_url(&dl2, mod_id, file_id)?;
                            println!("      下载地址：{}", &url[..url.len().min(72)]);
                            // 真下一个文件下来，算指纹再反查回去
                            if let Some(file) = version.primary_file() {
                                let temp_dir = paths::cache_dir().join("selftest-curseforge");
                                let _ = std::fs::remove_dir_all(&temp_dir);
                                paths::ensure_dir(&temp_dir)?;
                                let dest = temp_dir.join(&file.filename);
                                if let Err(err) = dl2.download(
                                    &crate::core::download::FileTask::new(url.clone(), dest.clone())
                                        .with_size(file.size),
                                    &DownloadProgress::new(),
                                ) {
                                    println!("      下载失败：{err}");
                                } else if let Some(fingerprint) =
                                    curseforge::file_fingerprint(&dest)
                                {
                                    // CurseForge 自己也会给出文件指纹，对一下算得对不对
                                    let reported = curseforge::file_fingerprint_of(&dl2, mod_id, file_id);
                                    match reported {
                                        Some(reported) if reported == fingerprint => {
                                            println!("      指纹 {fingerprint} 与 CurseForge 给的一致")
                                        }
                                        Some(reported) => println!(
                                            "      指纹 {fingerprint} 与 CurseForge 给的 {reported} 不一致"
                                        ),
                                        None => println!("      指纹 {fingerprint}（接口没给参考值）"),
                                    }
                                    let hits = curseforge::fingerprints(&dl2, &[fingerprint])?;
                                    let matched = hits
                                        .iter()
                                        .any(|(_, project_id)| *project_id == mod_id);
                                    println!(
                                        "      指纹反查：{}",
                                        if matched {
                                            "认出了同一个项目"
                                        } else {
                                            "没对上"
                                        }
                                    );
                                    // 反查回来的项目要带着图标，列表里才能显示
                                    let brief = curseforge::projects_by_ids(&dl2, &[mod_id])?
                                        .into_iter()
                                        .next()
                                        .map(|(_, project)| project);
                                    match brief {
                                        Some(project) => println!(
                                            "      反查项目：{}，图标 {}",
                                            project.slug,
                                            if project.icon_url.is_some() {
                                                "有"
                                            } else {
                                                "无"
                                            }
                                        ),
                                        None => println!("      反查项目：没取到"),
                                    }
                                }
                                let _ = std::fs::remove_dir_all(&temp_dir);
                            }
                        } else {
                            println!("      这个版本没带上 CurseForge 编号");
                        }
                    }
                }
            }
            Err(err) => println!("      CurseForge 搜索失败：{err}"),
        }
    }

    println!("[6.3/7] 本地光影与资源包反查线上项目（图标能不能配上）...");
    {
        use crate::core::content::curseforge;
        let key = crate::core::config::DownloadSettings::default().curseforge_key();
        if key.is_none() {
            println!("      没配 CurseForge API Key，跳过");
        } else {
            let dl2 = Downloader::new(DownloadSource::Official, 120)?.with_curseforge_key(key);
            // 光影、资源包、数据包都放在实例目录下，一个一个实例扫过去
            let game_dir = paths::default_game_dir();
            let mut roots: Vec<std::path::PathBuf> = vec![game_dir.clone()];
            if let Ok(entries) = std::fs::read_dir(paths::versions_dir(&game_dir)) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        roots.push(entry.path());
                    }
                }
            }
            let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
            for root in &roots {
                for sub in ["shaderpacks", "resourcepacks", "datapacks"] {
                    let Ok(entries) = std::fs::read_dir(root.join(sub)) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_file() {
                            continue;
                        }
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        files.push((name, path));
                    }
                }
            }
            if files.is_empty() {
                println!("      本地没有光影与资源包文件，跳过");
            } else {
                match crate::core::content::curseforge_projects(&dl2, &files) {
                    Ok(map) => {
                        let with_icon = map.values().filter(|p| p.icon_url.is_some()).count();
                        println!(
                            "      本地 {} 个文件，认出 {} 个线上项目，其中有图标 {} 个",
                            files.len(),
                            map.len(),
                            with_icon
                        );
                        for (name, project) in map.iter().take(5) {
                            println!("      {} → {}", name, project.slug);
                        }
                    }
                    Err(err) => println!("      反查失败：{err}"),
                }
            }
        }
    }

    println!("[6.4/7] CurseForge 整合包（下载、解压、读清单） ...");
    {
        use crate::core::content::curseforge;
        use crate::core::content::ContentKind;
        let key = crate::core::config::DownloadSettings::default().curseforge_key();
        let dl2 = Downloader::new(DownloadSource::Official, 120)?
            .with_curseforge_key(key);
        match curseforge::search(
            &dl2,
            ContentKind::Modpack,
            "all the mods 9",
            None,
            None,
            0,
            5,
        ) {
            Ok(result) => {
                println!("      整合包搜索到 {} 条", result.hits.len());
                if let Some(hit) = result.hits.first() {
                    println!("      选中：{}（{}）", hit.title, hit.project_id);
                    match curseforge::versions(&dl2, &hit.project_id, None, None) {
                        Ok(versions) => {
                            println!("      可安装版本 {} 个", versions.len());
                            // 取最小的那个，别下太大的包
                            if let Some(version) = versions
                                .iter()
                                .min_by_key(|v| v.primary_file().map(|f| f.size).unwrap_or(0))
                            {
                                println!("      试装：{}", version.name);
                                match curseforge::fetch_modpack(
                                    &dl2,
                                    version,
                                    &DownloadProgress::new(),
                                ) {
                                    Ok((dir, deps)) => {
                                        println!(
                                            "      已解压到 {}",
                                            dir.display()
                                        );
                                        println!(
                                            "      清单：Minecraft {} · 加载器 {}",
                                            deps.minecraft,
                                            deps.loader.clone().unwrap_or_default()
                                        );
                                        match curseforge::read_manifest(&dir) {
                                            Ok(manifest) => println!(
                                                "      清单文件 {} 个，overrides {}",
                                                manifest.files.len(),
                                                manifest.overrides.clone().unwrap_or_default()
                                            ),
                                            Err(err) => println!("      读清单失败：{err}"),
                                        }
                                    }
                                    Err(err) => println!("      整合包装载失败：{err}"),
                                }
                            }
                        }
                        Err(err) => println!("      取整合包版本失败：{err}"),
                    }
                }
            }
            Err(err) => println!("      整合包搜索失败：{err}"),
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
            println!("      | {}", line.text);
        }
        last_len = lines.len();
        if let Some(code) = process.try_exit() {
            println!("      游戏进程退出，代码 {code:?}");
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