//! 内容缓存
//!
//! 打开「模组 / 光影 / 资源包」页时，启动器要去线上查每个本地文件属于哪个项目，
//! 才知道它的图标和项目页链接。以前每次打开都要重新联网问一遍，
//! 几十个模组就是几十个请求，又慢又费流量。
//!
//! 这里把查到的结果落到本地 `cache/content-cache.json`，
//! 下次打开直接读文件，不再联网。缓存里没有的才去线上查，查完再写回来。
//!
//! 每个条目上带一把「银锁」：
//! - 锁住的条目不会被自动重新反查，一直用缓存里那份；
//! - 「全部更新」也会跳过它，想更新得先解锁，或者单独点它自己的「更新」。
//!
//! 锁按「分类 + 本地文件名」记，跟平台无关，
//! 同一个文件在 Modrinth 与 CurseForge 上都认同一把锁。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::content::modrinth::ContentKind;
use crate::core::content::ContentSource;
use crate::core::paths;

/// 缓存文件的格式版本
///
/// 以后改了结构就把版本号加一。读到不认识的版本就当没有缓存，重新查一遍。
const FORMAT_VERSION: u32 = 1;

/// 缓存文件在缓存目录里的名字
const FILE_NAME: &str = "content-cache.json";

/// 缓存里放太久的条目会自动清掉，免得文件越攒越大
const KEEP_DAYS: u64 = 180;

/// 「查过但没找到」的条目留多久
///
/// 线上后来才补上这个项目是常事，所以留一个月，
/// 一个月后再去问一次。
const MISS_KEEP_DAYS: u64 = 30;

/// 本地内容缓存
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentCache {
    /// 格式版本
    #[serde(default)]
    pub version: u32,
    /// 线上项目映射，键是「平台|分类|本地文件名」
    #[serde(default)]
    pub entries: HashMap<String, Entry>,
    /// 银锁，键是「分类|本地文件名」
    #[serde(default)]
    pub locks: HashSet<String>,
}

/// 缓存里的一条记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// 内容分类
    pub kind: ContentKind,
    /// 来自哪个平台
    pub source: ContentSource,
    /// 平台项目标识：Modrinth 是 slug，CurseForge 是数字编号
    pub project_id: String,
    /// 项目页地址用的 slug
    #[serde(default)]
    pub slug: String,
    /// 项目图标
    #[serde(default)]
    pub icon_url: Option<String>,
    /// 本地对应的文件名
    pub file_name: String,
    /// 查到的项目名
    #[serde(default)]
    pub title: String,
    /// 上次联网反查到的时刻（Unix 秒）
    #[serde(default)]
    pub checked_at: u64,
}

impl Entry {
    /// 用查到的线上项目填一条记录
    pub fn from_remote(
        kind: ContentKind,
        source: ContentSource,
        file_name: &str,
        project: &crate::core::content::RemoteProject,
    ) -> Self {
        Self {
            kind,
            source,
            project_id: project.project_id.clone().unwrap_or_else(|| project.slug.clone()),
            slug: project.slug.clone(),
            icon_url: project.icon_url.clone(),
            file_name: file_name.to_string(),
            title: String::new(),
            checked_at: now_secs(),
        }
    }

    /// 记一条「问过了，没找到」
    ///
    /// 光影、资源包这类只能靠文件名猜项目名，
    /// 猜不到是常事。把「没找到」也记下来，
    /// 下次打开就不会又去问一遍。
    pub fn miss(kind: ContentKind, source: ContentSource, file_name: &str) -> Self {
        Self {
            kind,
            source,
            project_id: String::new(),
            slug: String::new(),
            icon_url: None,
            file_name: file_name.to_string(),
            title: String::new(),
            checked_at: now_secs(),
        }
    }

    /// 这条是不是「没找到」
    pub fn is_miss(&self) -> bool {
        self.project_id.is_empty()
    }

    /// 这条记录是不是过期了，该重新问一次
    pub fn is_stale(&self) -> bool {
        let days = if self.is_miss() { MISS_KEEP_DAYS } else { KEEP_DAYS };
        let limit = days * 24 * 60 * 60;
        let now = now_secs();
        self.checked_at != 0 && now.saturating_sub(self.checked_at) >= limit
    }
}

/// 当前的 Unix 秒
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl ContentCache {
    /// 缓存文件的位置
    pub fn path() -> PathBuf {
        paths::cache_dir().join(FILE_NAME)
    }

    /// 读缓存
    ///
    /// 文件不存在、坏了或者格式对不上，都当没有缓存，重新去线上查一遍。
    pub fn load() -> Self {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(cache) if cache.version == FORMAT_VERSION => cache,
            Ok(_) => {
                log::info!("内容缓存格式已更新，重新反查一次");
                Self::default()
            }
            Err(err) => {
                log::warn!("内容缓存读取失败，重新反查: {err}");
                Self::default()
            }
        }
    }

    /// 写缓存
    pub fn save(&self) {
        let path = Self::path();
        if let Err(err) = paths::ensure_parent(&path) {
            log::warn!("内容缓存目录准备失败: {err}");
            return;
        }
        let mut copy = self.clone();
        copy.version = FORMAT_VERSION;
        copy.prune_old();
        let text = match serde_json::to_string_pretty(&copy) {
            Ok(text) => text,
            Err(err) => {
                log::warn!("内容缓存序列化失败: {err}");
                return;
            }
        };
        if let Err(err) = std::fs::write(&path, text) {
            log::warn!("内容缓存写入失败: {err}");
        }
    }

    /// 线上项目映射的键
    pub fn entry_key(source: ContentSource, kind: ContentKind, file_name: &str) -> String {
        format!("{}|{}|{}", source.id(), kind.facet(), file_name)
    }

    /// 银锁的键
    pub fn lock_key(kind: ContentKind, file_name: &str) -> String {
        format!("{}|{}", kind.facet(), file_name)
    }

    /// 取一条记录
    pub fn get(
        &self,
        source: ContentSource,
        kind: ContentKind,
        file_name: &str,
    ) -> Option<&Entry> {
        self.entries.get(&Self::entry_key(source, kind, file_name))
    }

    /// 写一条记录
    pub fn set(&mut self, entry: Entry) {
        let key = Self::entry_key(entry.source, entry.kind, &entry.file_name);
        self.entries.insert(key, entry);
    }

    /// 删掉一条记录
    pub fn remove(&mut self, source: ContentSource, kind: ContentKind, file_name: &str) {
        self.entries
            .remove(&Self::entry_key(source, kind, file_name));
    }

    /// 这条内容是不是锁着的
    pub fn is_locked(&self, kind: ContentKind, file_name: &str) -> bool {
        self.locks.contains(&Self::lock_key(kind, file_name))
    }

    /// 上锁或解锁，返回锁完的状态
    pub fn set_locked(&mut self, kind: ContentKind, file_name: &str, locked: bool) -> bool {
        let key = Self::lock_key(kind, file_name);
        if locked {
            self.locks.insert(key);
        } else {
            self.locks.remove(&key);
        }
        locked
    }

    /// 清掉放太久的条目
    fn prune_old(&mut self) {
        self.entries.retain(|_, entry| !entry.is_stale());
    }

    /// 文件换名了，把它的缓存记录跟着挪过去
    ///
    /// 更新会把 `jei-1.20.1.jar` 换成 `jei-1.21.jar`，
    /// 记录还挂在旧名字上就白查了。
    pub fn rename_entry(&mut self, kind: ContentKind, old_file: &str, new_file: &str) {
        let old_key = Self::lock_key(kind, old_file);
        let mut moved: Vec<Entry> = Vec::new();
        self.entries.retain(|_, entry| {
            if entry.kind == kind && entry.file_name == old_file {
                moved.push(entry.clone());
                false
            } else {
                true
            }
        });
        for mut entry in moved {
            entry.file_name = new_file.to_string();
            let key = Self::entry_key(entry.source, entry.kind, &entry.file_name);
            self.entries.insert(key, entry);
        }
        // 锁也跟着挪，不然更新完就锁不住了
        if self.locks.remove(&old_key) {
            self.locks.insert(Self::lock_key(kind, new_file));
        }
    }

    /// 缓存里有多少条记录
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 缓存是不是空的
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(file: &str) -> Entry {
        Entry {
            kind: ContentKind::Mod,
            source: ContentSource::Modrinth,
            project_id: "jei".into(),
            slug: "jei".into(),
            icon_url: None,
            file_name: file.into(),
            title: String::new(),
            checked_at: now_secs(),
        }
    }

    #[test]
    fn 条目按平台与文件名分开存() {
        let mut cache = ContentCache::default();
        cache.set(entry("a.jar"));
        assert!(cache.get(ContentSource::Modrinth, ContentKind::Mod, "a.jar").is_some());
        assert!(cache.get(ContentSource::CurseForge, ContentKind::Mod, "a.jar").is_none());
        assert!(cache.get(ContentSource::Modrinth, ContentKind::Shader, "a.jar").is_none());
    }

    #[test]
    fn 银锁按分类与文件名记_与平台无关() {
        let mut cache = ContentCache::default();
        assert!(!cache.is_locked(ContentKind::Mod, "a.jar"));
        cache.set_locked(ContentKind::Mod, "a.jar", true);
        assert!(cache.is_locked(ContentKind::Mod, "a.jar"));
        // 换个分类就不算锁着
        assert!(!cache.is_locked(ContentKind::Shader, "a.jar"));
        cache.set_locked(ContentKind::Mod, "a.jar", false);
        assert!(!cache.is_locked(ContentKind::Mod, "a.jar"));
    }

    #[test]
    fn 缓存可以存到磁盘再读回来() {
        let mut cache = ContentCache::default();
        cache.set(entry("b.jar"));
        cache.set_locked(ContentKind::Mod, "b.jar", true);
        let path = ContentCache::path();
        let _ = std::fs::remove_file(&path);
        cache.save();
        let loaded = ContentCache::load();
        assert_eq!(loaded.len(), 1);
        assert!(loaded.is_locked(ContentKind::Mod, "b.jar"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn 缓存文件坏了就当没有缓存() {
        let path = ContentCache::path();
        let _ = std::fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, "{ 这不是 json").unwrap();
        let loaded = ContentCache::load();
        assert!(loaded.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn 没找到的条目一个月内不再问() {
        let miss = Entry::miss(ContentKind::Mod, ContentSource::Modrinth, "a.jar");
        assert!(miss.is_miss());
        assert!(!miss.is_stale());
        // 一个月前的记录就该重新问了
        let mut old = miss.clone();
        old.checked_at = now_secs() - 31 * 24 * 60 * 60;
        assert!(old.is_stale());
    }

    #[test]
    fn 文件换名后缓存记录跟着挪() {
        let mut cache = ContentCache::default();
        cache.set(entry("old.jar"));
        cache.set_locked(ContentKind::Mod, "old.jar", true);
        cache.rename_entry(ContentKind::Mod, "old.jar", "new.jar");
        assert!(cache.get(ContentSource::Modrinth, ContentKind::Mod, "old.jar").is_none());
        assert!(cache.get(ContentSource::Modrinth, ContentKind::Mod, "new.jar").is_some());
        // 锁也跟着挪过去
        assert!(cache.is_locked(ContentKind::Mod, "new.jar"));
        assert!(!cache.is_locked(ContentKind::Mod, "old.jar"));
    }
}
