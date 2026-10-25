//! 把整合包装成一个独立实例
//!
//! 整合包不是单个模组，它自带一整套模组、配置与脚本，
//! 装进来之后就是一个新的实例，跟原有实例互不干扰。
//!
//! 流程是：先按整合包声明的版本装好 Minecraft 本体与加载器，
//! 再建一个属于这个整合包的实例目录，把整合包的内容放进去。

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use crate::core::content::modrinth;
use crate::core::download::{DownloadProgress, Downloader};
use crate::core::java::JavaInfo;
use crate::core::minecraft::install::InstallOptions;
use crate::core::minecraft::{bundle, install, loaders, manifest, modpack, version};
use crate::core::paths;

/// 整合包安装结果
#[derive(Debug, Clone)]
pub struct InstalledModpack {
    /// 新实例的 id
    pub instance_id: String,
    /// Minecraft 版本
    pub mc_version: String,
    /// 加载器名
    pub loader: Option<String>,
    /// 加载器版本号
    pub loader_version: Option<String>,
}

/// 安装整合包，生成一个独立实例
///
/// `slug` 是项目在 Modrinth 上的短名，用它生成实例目录名；
/// 目录已经存在时自动加序号，不会覆盖之前装好的整合包。
pub fn install_instance(
    dl: &Downloader,
    project_id: &str,
    project_title: &str,
    slug: &str,
    pack_version: &modrinth::Version,
    game_dir: &Path,
    java: Option<JavaInfo>,
    progress: &Arc<DownloadProgress>,
    opts: InstallOptions,
) -> Result<InstalledModpack> {
    // 1. 先把整合包下载解开，拿到它声明的 Minecraft 版本与加载器
    let (extract_dir, deps) = modrinth::fetch_modpack(dl, pack_version, progress)?;

    let result = install_inner(
        dl,
        project_id,
        project_title,
        slug,
        &deps,
        &extract_dir,
        game_dir,
        java,
        progress,
        opts,
    );

    // 解压目录只是中转，装完就清掉
    let _ = std::fs::remove_dir_all(&extract_dir);
    result
}

#[allow(clippy::too_many_arguments)]
fn install_inner(
    dl: &Downloader,
    project_id: &str,
    project_title: &str,
    slug: &str,
    deps: &modrinth::ModpackDeps,
    extract_dir: &Path,
    game_dir: &Path,
    java: Option<JavaInfo>,
    progress: &Arc<DownloadProgress>,
    opts: InstallOptions,
) -> Result<InstalledModpack> {
    if deps.minecraft.trim().is_empty() {
        return Err(anyhow!("整合包没有声明 Minecraft 版本"));
    }

    // 2. 装 Minecraft 本体与整合包要求的加载器
    let loader_kind = deps.loader.as_deref().and_then(loaders::LoaderKind::from_id);
    // 整合包声明了加载器却没认出来时，宁可报错也不能悄悄按原版装，
    // 否则装出来的实例少了整套模组环境，症状还很难查
    if let Some(loader) = deps.loader.as_deref() {
        if loader_kind.is_none() {
            return Err(anyhow!("整合包声明的加载器 {loader} 暂不支持"));
        }
    }
    let manifest_data = manifest::fetch_manifest(dl.source)?;
    let entry = manifest::find(&manifest_data, &deps.minecraft)
        .cloned()
        .ok_or_else(|| anyhow!("版本清单里没有 Minecraft {}", deps.minecraft))?;
    let req = bundle::InstallRequest {
        mc_version: deps.minecraft.clone(),
        entry,
        loader: loader_kind,
        loader_version: deps.loader_version.clone(),
        loader_policy: loaders::LoaderPolicy::Stable,
        timing: bundle::LoaderTiming::After,
        game_dir: game_dir.to_path_buf(),
        java,
        opts,
    };
    let outcome = bundle::install_bundle(dl, &req, progress)?;
    let loader_id = outcome
        .loader_id
        .clone()
        .unwrap_or_else(|| outcome.game_id.clone());

    // 3. 建整合包自己的实例目录
    let taken = version::installed_versions(game_dir);
    let instance_id = modpack::unique_id(&modpack::sanitize_id(slug), &taken);
    let inst_dir = version::version_dir(game_dir, &instance_id);
    paths::ensure_dir(&inst_dir)?;

    // 版本描述沿用加载器那份，只把 id 换成实例 id：
    // 继承链、依赖库、启动参数跟加载器版本一致，
    // 客户端 jar 仍然跟原版实例共用一份，不重复下载
    match loader_kind {
        Some(_) => {
            let loader_json = version::read_local_json(game_dir, &loader_id)
                .with_context(|| format!("读取加载器版本描述失败: {loader_id}"))?;
            modpack::write_instance_json(&inst_dir, &instance_id, &loader_json, project_title)?;
            // 加载器版本目录里有时还带着自己的 jar，
            // 复制一份到实例目录，实例才能正常启动
            let loader_jar = version::version_jar_path(game_dir, &loader_id);
            if loader_jar.is_file() {
                let dest = version::version_jar_path(game_dir, &instance_id);
                if let Err(err) = std::fs::copy(&loader_jar, &dest) {
                    log::warn!("复制加载器 jar 失败: {err}");
                }
            }
        }
        None => {
            modpack::write_vanilla_json(&inst_dir, &instance_id, &deps.minecraft, project_title)?;
        }
    }

    // 标记这个目录是整合包实例，扫描器据此把它单独列成一条
    modpack::write_marker(
        &inst_dir,
        &modpack::ModpackInfo {
            name: project_title.to_string(),
            mc_version: deps.minecraft.clone(),
            loader: loader_kind.map(|k| k.label().to_string()),
            loader_version: deps.loader_version.clone(),
            project_id: Some(project_id.to_string()),
        },
    )?;

    // 4. natives 与依赖库在这个实例目录里再准备一份
    install::install_core(dl, game_dir, &instance_id, progress, opts)?;

    // 5. 整合包内容全部放进这个实例目录
    modrinth::install_modpack_files(dl, extract_dir, &inst_dir, progress)?;

    Ok(InstalledModpack {
        instance_id,
        mc_version: deps.minecraft.clone(),
        loader: loader_kind.map(|k| k.label().to_string()),
        loader_version: deps.loader_version.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_id_avoids_existing_versions() {
        let taken = vec!["all-the-mods-9".to_string()];
        let id = modpack::unique_id(&modpack::sanitize_id("All The Mods 9"), &taken);
        assert_eq!(id, "all-the-mods-9-2");
    }
}
