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
