//! MCMod（MC 百科）Mod 元数据
//!
//! PCL2 / PCL2-CE / SJMCL 都会在 Mod 上直接标注 MCMod 链接，
//! 三者用的都是同一份数据源：MCMod 上的中文名、英文名、缩写与外站链接。
//!
//! - PCL2-CE 把数据打包成压缩二进制 `mcmod.buf` 随程序分发；
//! - SJMCL 用 CI 抓取 MCMod 生成 `mod_data.csv` 随程序分发。
//!
//! 这里采用与 SJMCL 相同的 CSV 字段格式，运行时按需下载并缓存到本地：
//! `mcmod_id, curseforge_slug, modrinth_slug, name, subname, abbr`
//!
//! 下载失败时静默退化（沿用 Modrinth 的原始名称与链接），不影响浏览与安装。

use std::collections::HashMap;
use std::path::PathBuf;

use crate::core::download::Downloader;
use crate::core::paths;

/// MCMod 元数据源（字段格式与 SJMCL 一致，内容来自 MCMod 抓取）
const DATA_URL: &str =
    "https://raw.githubusercontent.com/UNIkeEN/SJMCL/main/src-tauri/assets/db/mod_data.csv";
/// 本地缓存文件名
const CACHE_FILE: &str = "mcmod.csv";
/// 缓存有效期（天）
const CACHE_DAYS: i64 = 7;
/// MCMod 详情页地址
const MCMOD_BASE: &str = "https://www.mcmod.cn/class/";

/// 一条 Mod 的 MCMod 元数据
#[derive(Debug, Clone, Default)]
pub struct Entry {
    /// MCMod 的数字 id
    pub mcmod_id: u32,
    /// 对应的 Modrinth 项目 slug
    pub modrinth_slug: String,
    /// 中文名
    pub name: String,
    /// 英文名
    pub subname: String,
    /// 缩写，如 JEI、IC2
    pub abbr: String,
}

impl Entry {
    /// MCMod 详情页链接
    pub fn url(&self) -> String {
        format!("{MCMOD_BASE}{}.html", self.mcmod_id)
    }

    /// 显示用名称：有中文名用中文名，否则用英文名
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.subname
        } else {
            &self.name
        }
    }

    /// 带缩写前缀的显示名，如「[JEI] JEI物品管理器」
    pub fn display_with_abbr(&self) -> String {
        if self.abbr.is_empty() {
            self.display_name().to_string()
        } else {
            format!("[{}] {}", self.abbr, self.display_name())
        }
    }
}

/// MCMod 元数据库
#[derive(Debug, Clone, Default)]
pub struct Database {
    /// Modrinth slug → 条目
    by_modrinth: HashMap<String, Entry>,
    /// 英文名（小写）→ 条目，用于匹配 jar 元数据里的名称
    by_english: HashMap<String, Entry>,
    /// 条目总数
    pub count: usize,
}

impl Database {
    /// 解析 CSV
    pub fn parse(csv: &str) -> Self {
        let mut db = Database::default();
        for (index, line) in csv.lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            if index == 0 {
                // 跳过表头（首列可能带 BOM）
                continue;
            }
            let fields = split_csv_line(line);
            if fields.len() < 6 {
                continue;
            }
            let mcmod_id: u32 = fields[0].trim().parse().unwrap_or(0);
            let modrinth_slug = fields[2].trim();
            // 只要有 MC 百科的编号就收下：没有 Modrinth 对照的模组，
            // 照样能给出中文名和 MC 百科页面
            if mcmod_id == 0 {
                continue;
            }
            let entry = Entry {
                mcmod_id,
                modrinth_slug: modrinth_slug.to_string(),
                name: fields[3].trim().to_string(),
                subname: fields[4].trim().to_string(),
                abbr: fields[5].trim().to_string(),
            };
            if !modrinth_slug.is_empty() {
                db.by_modrinth
                    .insert(modrinth_slug.to_lowercase(), entry.clone());
            }
            if !entry.subname.is_empty() {
                db.by_english
                    .entry(entry.subname.to_lowercase())
                    .or_insert(entry);
            }
            db.count += 1;
        }
        db
    }

    /// 按 Modrinth slug 查找
    pub fn lookup_modrinth(&self, slug: &str) -> Option<&Entry> {
        self.by_modrinth.get(&slug.to_lowercase())
    }

    /// 按英文名查找
    ///
    /// 用于把 jar 元数据里的英文名对上 MCMod 的中文名。
    pub fn lookup_english(&self, name: &str) -> Option<&Entry> {
        let key = name.trim().to_lowercase();
        if key.is_empty() {
            return None;
        }
        self.by_english.get(&key)
    }

    /// 用中文名或缩写反查英文关键词
    ///
    /// 做法与 SJMCL 一致：先用 MCMod 的中文名库匹配，再把得到的英文名
    /// 拿去搜 Modrinth。缩写精确匹配最可靠，其次是中文名包含。
    pub fn english_hint(&self, query: &str) -> Option<String> {
        let q = query.trim();
        if q.is_empty() {
            return None;
        }
        // 缩写精确匹配
        for entry in self.by_modrinth.values() {
            if !entry.abbr.is_empty() && entry.abbr.eq_ignore_ascii_case(q) {
                return Some(if entry.subname.is_empty() {
                    entry.abbr.clone()
                } else {
                    entry.subname.clone()
                });
            }
        }
        // 中文名包含，取最贴近的一条
        let mut best: Option<&Entry> = None;
        for entry in self.by_modrinth.values() {
            if entry.name.is_empty() || !entry.name.contains(q) {
                continue;
            }
            match best {
                Some(current) if current.name.len() <= entry.name.len() => {}
                _ => best = Some(entry),
            }
        }
        best.and_then(|entry| {
            if !entry.subname.is_empty() {
                Some(entry.subname.clone())
            } else if !entry.abbr.is_empty() {
                Some(entry.abbr.clone())
            } else {
                None
            }
        })
    }

    pub fn path() -> PathBuf {
        paths::cache_dir().join(CACHE_FILE)
    }

    /// 读取本地缓存，文件不存在或已过期时返回空库
    pub fn load_cached() -> Option<Self> {
        let path = Self::path();
        let Ok(meta) = std::fs::metadata(&path) else {
            return None;
        };
        let age = meta
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs() as i64 / 86_400)
            .unwrap_or(CACHE_DAYS + 1);
        if age > CACHE_DAYS {
            return None;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            return None;
        };
        let db = Database::parse(&text);
        if db.count == 0 {
            None
        } else {
            Some(db)
        }
    }

    /// 写入本地缓存
    pub fn save(&self, csv: &str) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            if let Err(err) = paths::ensure_dir(parent) {
                log::warn!("创建 MCMod 缓存目录失败: {err}");
                return;
            }
        }
        if let Err(err) = std::fs::write(&path, csv) {
            log::warn!("保存 MCMod 缓存失败: {err}");
        }
    }
}

/// 判断文本里是否含有中文字符
pub fn has_chinese(text: &str) -> bool {
    text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 下载最新的 MCMod 元数据并写入缓存
///
/// 失败时返回 None，调用方沿用旧数据。
pub fn refresh(dl: &Downloader) -> Option<Database> {
    let text = dl.get_bytes_direct(DATA_URL).ok()?;
    let text = String::from_utf8_lossy(&text).to_string();
    let db = Database::parse(&text);
    if db.count == 0 {
        log::warn!("MCMod 元数据解析结果为空");
        return None;
    }
    db.save(&text);
    log::info!("MCMod 元数据已更新，共 {} 条", db.count);
    Some(db)
}

/// 拆分一行 CSV，支持双引号包裹与转义
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => fields.push(std::mem::take(&mut field)),
                _ => field.push(c),
            }
        }
    }
    fields.push(field);
    // 去掉表头可能带的 BOM
    if let Some(first) = fields.first_mut() {
        *first = first.trim_start_matches('\u{feff}').to_string();
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_slug_and_names() {
        let csv = "mcmod_id,curseforge_slug,modrinth_slug,name,subname,abbr\n\
                   459,jei,jei,JEI物品管理器,Just Enough Items,JEI\n\
                   1,,,我的世界原版,Minecraft,MC\n";
        let db = Database::parse(csv);
        // 两条都有 MC 百科编号，都收下
        assert_eq!(db.count, 2);
        let entry = db.lookup_modrinth("jei").expect("应能按 slug 找到");
        assert_eq!(entry.mcmod_id, 459);
        assert_eq!(entry.name, "JEI物品管理器");
        assert_eq!(entry.subname, "Just Enough Items");
        assert_eq!(entry.abbr, "JEI");
        assert_eq!(entry.url(), "https://www.mcmod.cn/class/459.html");
        // 没有 Modrinth 对照的模组，仍能按英文名找到，MC 百科链接照常用
        let vanilla = db.lookup_english("Minecraft").expect("应能按英文名找到");
        assert_eq!(vanilla.mcmod_id, 1);
        assert!(vanilla.modrinth_slug.is_empty());
        assert_eq!(vanilla.url(), "https://www.mcmod.cn/class/1.html");
        // 查不到时返回 None
        assert!(db.lookup_modrinth("sodium").is_none());
    }

    #[test]
    fn parse_handles_quoted_fields() {
        let csv = "mcmod_id,curseforge_slug,modrinth_slug,name,subname,abbr\n\
                   12,,demo,\"某个, 带逗号的名字\",Demo,D\n";
        let db = Database::parse(csv);
        let entry = db.lookup_modrinth("demo").expect("应能按 slug 找到");
        assert_eq!(entry.name, "某个, 带逗号的名字");
    }

    #[test]
    fn display_name_falls_back_to_english() {
        let entry = Entry {
            mcmod_id: 1,
            modrinth_slug: String::new(),
            name: String::new(),
            subname: "Minecraft".into(),
            abbr: "MC".into(),
        };
        assert_eq!(entry.display_name(), "Minecraft");
    }

    #[test]
    fn english_hint_matches_abbr_first() {
        let csv = "mcmod_id,curseforge_slug,modrinth_slug,name,subname,abbr\n\
                   459,jei,jei,JEI物品管理器,Just Enough Items,JEI\n\
                   87,ae1,ae1,应用能源,Applied Energistics 1,AE\n";
        let db = Database::parse(csv);
        // 缩写精确命中
        assert_eq!(db.english_hint("JEI").as_deref(), Some("Just Enough Items"));
        // 中文名包含
        assert_eq!(db.english_hint("应用能源").as_deref(), Some("Applied Energistics 1"));
        // 查不到时返回 None
        assert_eq!(db.english_hint("不存在的名字"), None);
        assert_eq!(db.english_hint(""), None);
    }

    #[test]
    fn has_chinese_detects_cjk() {
        assert!(has_chinese("物品管理器"));
        assert!(has_chinese("JEI物品管理器"));
        assert!(!has_chinese("sodium"));
        assert!(!has_chinese(""));
    }
}
