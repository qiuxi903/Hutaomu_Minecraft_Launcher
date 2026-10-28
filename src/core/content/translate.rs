//! Mod 名称与简介的中文翻译
//!
//! 参考 PCL2 / HMCL / SJMCL 的做法：
//! - Modrinth 搜索本身支持中文关键词，社区维护的中文标题与简介会直接出现在结果里；
//! - 纯英文的项目通过 MC 镜像站的翻译接口取中文简介；
//! - 翻译结果缓存在本地，避免重复请求。
//!
//! 翻译接口属于第三方服务，可能不可用。所有调用失败都静默回退到原文，
//! 不影响浏览与安装。

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::download::Downloader;
use crate::core::paths;

/// MC 镜像站的翻译接口（SJMCL 使用的是同一个服务）
const TRANSLATE_API: &str = "https://mod.mcimirror.top/translate/modrinth/";
/// 缓存文件名
const CACHE_FILE: &str = "translations.json";
/// 缓存保留天数
const CACHE_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// 中文简介
    pub translated: String,
    /// 写入时间（Unix 秒）
    pub timestamp: i64,
}

/// 本地翻译缓存
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cache {
    #[serde(default)]
    pub entries: HashMap<String, Entry>,
}

impl Cache {
    pub fn path() -> PathBuf {
        paths::cache_dir().join(CACHE_FILE)
    }

    /// 读取缓存，文件不存在或损坏时返回空缓存
    pub fn load() -> Self {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Cache>(&text) {
            Ok(cache) => cache,
            Err(err) => {
                log::warn!("翻译缓存解析失败，已忽略: {err}");
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Err(err) = paths::ensure_dir(path.parent().unwrap_or(&paths::cache_dir())) {
            log::warn!("创建翻译缓存目录失败: {err}");
            return;
        }
        if let Err(err) = std::fs::write(&path, serde_json::to_string_pretty(self).unwrap_or_default())
        {
            log::warn!("保存翻译缓存失败: {err}");
        }
    }

    /// 取缓存中的中文简介，过期视为没有
    pub fn get(&self, project_id: &str) -> Option<&str> {
        let entry = self.entries.get(project_id)?;
        if expired(entry.timestamp) {
            return None;
        }
        let text = entry.translated.trim();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    /// 写入一条翻译
    pub fn put(&mut self, project_id: &str, translated: &str) {
        self.entries.insert(
            project_id.to_string(),
            Entry {
                translated: translated.to_string(),
                timestamp: chrono::Utc::now().timestamp(),
            },
        );
    }
}

fn expired(timestamp: i64) -> bool {
    let now = chrono::Utc::now().timestamp();
    now - timestamp > CACHE_DAYS * 24 * 60 * 60
}

#[derive(Debug, Deserialize)]
struct TranslateResponse {
    translated: String,
}

/// CurseForge 的翻译接口（同一个服务，按数字项目编号查）
const TRANSLATE_API_CF: &str = "https://mod.mcimirror.top/translate/curseforge/";

/// 请求翻译接口
///
/// 服务不可用时返回 None，调用方应回退到原文而不是报错。
pub fn translate(dl: &Downloader, project_id: &str) -> Option<String> {
    if project_id.is_empty() {
        return None;
    }
    let url = format!("{TRANSLATE_API}{project_id}");
    let resp = dl.get_json_direct::<TranslateResponse>(&url).ok()?;
    let text = resp.translated.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// 带缓存的翻译：先查缓存，没有再请求接口
pub fn translate_cached(
    dl: &Downloader,
    project_id: &str,
    cache: &mut Cache,
) -> Option<String> {
    if let Some(text) = cache.get(project_id) {
        return Some(text.to_string());
    }
    let translated = translate(dl, project_id)?;
    cache.put(project_id, &translated);
    Some(translated)
}

/// CurseForge 的单个翻译（按数字项目编号）
///
/// 缓存键带 `cf:` 前缀，与 Modrinth 的项目编号区分开。
pub fn translate_curseforge_cached(
    dl: &Downloader,
    mod_id: u32,
    cache: &mut Cache,
) -> Option<String> {
    let key = format!("cf:{mod_id}");
    if let Some(text) = cache.get(&key) {
        return Some(text.to_string());
    }
    let url = format!("{TRANSLATE_API_CF}{mod_id}");
    let resp = dl.get_json_direct::<TranslateResponse>(&url).ok()?;
    let text = resp.translated.trim();
    if text.is_empty() {
        return None;
    }
    cache.put(&key, text);
    Some(text.to_string())
}

// ---------------------------------------------------------------- 批量

#[derive(Debug, Deserialize)]
struct BatchEntry {
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    modid: u32,
    #[serde(default)]
    translated: String,
}

/// 批量取 Modrinth 项目的中文简介
///
/// 一次请求问一批，返回「项目编号 → 中文简介」。
/// 接口没收录的项目不会出现在返回里，调用方按原文兜底。
pub fn translate_batch_modrinth(dl: &Downloader, project_ids: &[String]) -> HashMap<String, String> {
    if project_ids.is_empty() {
        return HashMap::new();
    }
    let body = serde_json::json!({ "project_ids": project_ids });
    let resp: Vec<BatchEntry> = match dl
        .post_json("https://mod.mcimirror.top/translate/modrinth", &body)
    {
        Ok(resp) => resp,
        Err(err) => {
            log::debug!("批量中文简介请求失败: {err}");
            return HashMap::new();
        }
    };
    resp.into_iter()
        .filter(|entry| !entry.project_id.is_empty() && !entry.translated.trim().is_empty())
        .map(|entry| (entry.project_id, entry.translated))
        .collect()
}

/// 批量取 CurseForge 项目的中文简介
pub fn translate_batch_curseforge(dl: &Downloader, mod_ids: &[u32]) -> HashMap<u32, String> {
    if mod_ids.is_empty() {
        return HashMap::new();
    }
    let body = serde_json::json!({ "modids": mod_ids });
    let resp: Vec<BatchEntry> = match dl
        .post_json("https://mod.mcimirror.top/translate/curseforge", &body)
    {
        Ok(resp) => resp,
        Err(err) => {
            log::debug!("批量中文简介请求失败: {err}");
            return HashMap::new();
        }
    };
    resp.into_iter()
        .filter(|entry| entry.modid != 0 && !entry.translated.trim().is_empty())
        .map(|entry| (entry.modid, entry.translated))
        .collect()
}
