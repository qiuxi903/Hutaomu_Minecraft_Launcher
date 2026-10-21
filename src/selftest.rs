//! 端到端自检：真实下载一个版本、构建启动命令并实际启动游戏。
//! 用法：htmcl.exe --selftest [版本号]

use std::time::Duration;

use crate::core::auth;
use crate::core::download::{DownloadProgress, DownloadSource, Downloader};
use crate::core::java;
use crate::core::minecraft::{install, launch, manifest, version};
use crate::core::paths;

pub fn run(version_id: &str, loader: Option<&str>) -> anyhow::Result<()> {
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
    install::ensure_version_json(&dl, &game_dir, &entry)?;
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
    match install::install_version(&dl, &game_dir, version_id, &progress) {
        Ok(report) => println!("      {}", report.summary()),
        Err(err) => println!("      资源下载未完全成功：{err}"),
    }
    progress
        .finished
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let mut launch_id = version_id.to_string();
    if let Some(loader_name) = loader {
        println!("[3.5/5] 安装加载器 {loader_name} ...");
        let kind = match loader_name.to_lowercase().as_str() {
            "fabric" => crate::core::minecraft::loaders::LoaderKind::Fabric,
            "quilt" => crate::core::minecraft::loaders::LoaderKind::Quilt,
            "forge" => crate::core::minecraft::loaders::LoaderKind::Forge,
            "neoforge" => crate::core::minecraft::loaders::LoaderKind::NeoForge,
            other => anyhow::bail!("未知加载器 {other}"),
        };
        let versions = crate::core::minecraft::loaders::list_versions(&dl, kind, version_id)?;
        println!("      可用版本 {} 个，最新 {}", versions.len(), versions.first().map(|v| v.version.clone()).unwrap_or_default());
        let target = versions
            .iter()
            .find(|v| v.stable)
            .or_else(|| versions.first())
            .ok_or_else(|| anyhow::anyhow!("没有可用的 {loader_name} 版本"))?;
        let java = java::pick_for(version_id, &javas)
            .ok_or_else(|| anyhow::anyhow!("没有满足要求的 Java"))?;
        let installed = crate::core::minecraft::loaders::install(
            &dl,
            kind,
            version_id,
            &target.version,
            &game_dir,
            &java,
            &progress,
        )?;
        println!("      加载器版本 id: {installed}");
        launch_id = installed;
    }

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
        width: 854,
        height: 480,
        fullscreen: false,
        extra_jvm_args: "",
        extra_game_args: "",
        authlib_jar: None,
        server: None,
    };
    let command = launch::build_command(&spec)?;
    println!("      命令: {}", launch::describe_command(&command));

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

    println!("=== 自检结束 ===");
    Ok(())
}
