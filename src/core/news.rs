use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::download::Downloader;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    #[serde(default)]
    pub title: String,
    #[serde(default, alias = "description", alias = "desc")]
    pub summary: String,
    #[serde(default, alias = "pubDate", alias = "published", alias = "time")]
    pub date: String,
    #[serde(default, alias = "link", alias = "href")]
    pub url: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetKind {
    /// Minecraft 官网中文资讯（与 PCL2-CE 同源）
    Official,
    /// 通用 RSS / Atom
    Rss,
    /// 中文 Minecraft Wiki 最近更改
    Wiki,
}

pub struct NewsPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub kind: PresetKind,
}

pub const PRESETS: &[NewsPreset] = &[
    NewsPreset {
        id: "official_zh",
        name: "Minecraft 官方资讯",
        description: "Minecraft 官网中文新闻（与 PCL2-CE 同源）",
        url: "https://net-secondary.web.minecraft-services.net/api/v1.0/zh-cn/search?pageSize=24&sortType=Recent&category=News&newsOnly=true&page=1",
        kind: PresetKind::Official,
    },
    NewsPreset {
        id: "klpbbs",
        name: "苦力怕论坛",
        description: "国内 Minecraft 社区资讯（RSS）",
        url: "https://www.klpbbs.com/forum.php?mod=rss",
        kind: PresetKind::Rss,
    },
    NewsPreset {
        id: "wiki",
        name: "中文 Minecraft Wiki",
        description: "Wiki 最近更改，适合追新版本与机制变动",
        url: "https://zh.minecraft.wiki/api.php?action=query&list=recentchanges&rcprop=title%7Ctimestamp%7Ccomment%7Cids&rclimit=20&format=json",
        kind: PresetKind::Wiki,
    },
];

pub fn preset(id: &str) -> Option<&'static NewsPreset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// 拉取资讯。source 为内置源 id 或 "custom"（使用 custom_url 自动识别 JSON / RSS）。
pub fn fetch(dl: &Downloader, source: &str, custom_url: &str) -> Result<Vec<NewsItem>> {
    if source == "custom" {
        if custom_url.trim().is_empty() {
            return Ok(builtin_tips());
        }
        return fetch_custom(dl, custom_url);
    }

    let Some(preset) = preset(source) else {
        return Ok(builtin_tips());
    };

    match preset.kind {
        PresetKind::Official => {
            let response: OfficialResponse = dl
                .get_json_direct(preset.url)
                .with_context(|| "获取 Minecraft 官方资讯失败")?;
            Ok(normalize(
                response
                    .result
                    .results
                    .into_iter()
                    .map(|item| NewsItem {
                        title: decode_entities(&item.title),
                        summary: truncate_text(&decode_entities(&strip_tags(&item.description)), 180),
                        date: unix_date(item.time),
                        url: item.url,
                        image: if item.image.is_empty() {
                            None
                        } else {
                            Some(item.image)
                        },
                        tag: Some(if item.author.is_empty() {
                            "官方资讯".into()
                        } else {
                            format!("官方 · {}", item.author)
                        }),
                    })
                    .collect(),
            ))
        }
        PresetKind::Rss => {
            let bytes = dl.get_bytes_direct(preset.url)?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            Ok(normalize(parse_feed(&text)))
        }
        PresetKind::Wiki => {
            let response: WikiResponse = dl
                .get_json_direct(preset.url)
                .with_context(|| "获取 Minecraft Wiki 最近更改失败")?;
            Ok(normalize(
                response
                    .query
                    .recentchanges
                    .into_iter()
                    .map(|change| NewsItem {
                        title: change.title,
                        summary: strip_tags(&change.comment),
                        date: change.timestamp.chars().take(10).collect(),
                        url: format!("https://zh.minecraft.wiki/?diff={}", change.revid),
                        image: None,
                        tag: Some("Wiki".into()),
                    })
                    .collect(),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
struct OfficialResponse {
    #[serde(default)]
    result: OfficialResult,
}

#[derive(Debug, Default, Deserialize)]
struct OfficialResult {
    #[serde(default)]
    results: Vec<OfficialItem>,
}

#[derive(Debug, Deserialize)]
struct OfficialItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    time: i64,
}

#[derive(Debug, Deserialize)]
struct WikiResponse {
    #[serde(default)]
    query: WikiQuery,
}

#[derive(Debug, Default, Deserialize)]
struct WikiQuery {
    #[serde(default)]
    recentchanges: Vec<WikiChange>,
}

#[derive(Debug, Deserialize)]
struct WikiChange {
    #[serde(default)]
    title: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    revid: i64,
}

fn unix_date(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn fetch_custom(dl: &Downloader, url: &str) -> Result<Vec<NewsItem>> {
    let bytes = dl
        .get_bytes_direct(url)
        .with_context(|| format!("获取资讯失败: {url}"))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let trimmed = text.trim_start();

    if trimmed.starts_with('[') {
        let items: Vec<NewsItem> =
            serde_json::from_str(trimmed).with_context(|| "资讯 JSON 格式不正确，应为对象数组")?;
        return Ok(normalize(items));
    }
    if trimmed.starts_with('{') {
        let envelope: NewsEnvelope =
            serde_json::from_str(trimmed).with_context(|| "资讯 JSON 格式不正确")?;
        let items = if !envelope.items.is_empty() {
            envelope.items
        } else if !envelope.data.is_empty() {
            envelope.data
        } else {
            envelope.list
        };
        return Ok(normalize(items));
    }
    if trimmed.contains("<rss") || trimmed.contains("<feed") || trimmed.contains("<channel") {
        return Ok(normalize(parse_feed(&text)));
    }

    Err(anyhow::anyhow!(
        "无法识别的资讯格式，请提供 JSON 数组或 RSS 地址"
    ))
}

#[derive(Debug, Deserialize)]
struct NewsEnvelope {
    #[serde(default)]
    items: Vec<NewsItem>,
    #[serde(default)]
    data: Vec<NewsItem>,
    #[serde(default)]
    list: Vec<NewsItem>,
}

fn normalize(items: Vec<NewsItem>) -> Vec<NewsItem> {
    items
        .into_iter()
        .filter(|item| !item.title.trim().is_empty())
        .take(40)
        .collect()
}

fn parse_feed(xml: &str) -> Vec<NewsItem> {
    let mut items = Vec::new();
    let is_atom = xml.contains("<entry");
    let (open, close) = if is_atom {
        ("<entry", "</entry>")
    } else {
        ("<item", "</item>")
    };

    let mut rest = xml;
    while let Some(start) = rest.find(open) {
        let after = &rest[start..];
        let Some(end) = after.find(close) else { break };
        let block = &after[..end];
        rest = &after[end + close.len()..];

        let title = extract_tag(block, "title").unwrap_or_default();
        let summary = extract_tag(block, "description")
            .or_else(|| extract_tag(block, "summary"))
            .or_else(|| extract_tag(block, "content"))
            .unwrap_or_default();
        let date = extract_tag(block, "pubDate")
            .or_else(|| extract_tag(block, "published"))
            .or_else(|| extract_tag(block, "updated"))
            .unwrap_or_default();
        let url = extract_tag(block, "link")
            .or_else(|| extract_attr(block, "link", "href"))
            .unwrap_or_default();

        if !title.trim().is_empty() {
            items.push(NewsItem {
                title: decode_entities(&strip_tags(&title)),
                summary: truncate_text(&decode_entities(&strip_tags(&summary)), 180),
                date: short_date(&date),
                url: url.trim().to_string(),
                image: extract_attr(block, "enclosure", "url")
                    .or_else(|| extract_media_url(block)),
                tag: None,
            });
        }
    }
    items
}

fn short_date(date: &str) -> String {
    let trimmed = date.trim();
    if trimmed.len() >= 16 && trimmed.contains(':') {
        trimmed.chars().take(16).collect()
    } else {
        trimmed.chars().take(10).collect()
    }
}

fn extract_media_url(block: &str) -> Option<String> {
    for needle in ["media:content", "media:thumbnail"] {
        if let Some(pos) = block.find(needle) {
            let head = &block[pos..];
            if let Some(start) = head.find("url=\"") {
                let rest = &head[start + 5..];
                if let Some(end) = rest.find('"') {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
}

fn extract_tag(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = block.find(&open)?;
    let after = &block[start..];
    let gt = after.find('>')?;
    let content_start = gt + 1;
    let close = format!("</{tag}>");
    let end = after[content_start..].find(&close)?;
    Some(after[content_start..content_start + end].to_string())
}

fn extract_attr(block: &str, tag: &str, attr: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = block.find(&open)?;
    let after = &block[start..];
    let gt = after.find('>')?;
    let head = &after[..gt];
    let needle = format!("{attr}=\"");
    let attr_start = head.find(&needle)? + needle.len();
    let attr_end = head[attr_start..].find('"')?;
    Some(head[attr_start..attr_start + attr_end].to_string())
}

fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

fn decode_entities(text: &str) -> String {
    let mut out = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&#8217;", "'")
        .replace("&#8216;", "'")
        .replace("&#8220;", "\"")
        .replace("&#8221;", "\"")
        .replace("&#8212;", "—")
        .replace("&#8230;", "…");
    while let Some(start) = out.find("&#") {
        let Some(end) = out[start..].find(';') else { break };
        let entity = &out[start + 2..start + end];
        let code = if let Some(hex) = entity.strip_prefix('x').or_else(|| entity.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            entity.parse::<u32>().ok()
        };
        match code.and_then(char::from_u32) {
            Some(ch) => out.replace_range(start..start + end + 1, &ch.to_string()),
            None => break,
        }
    }
    out
}

fn truncate_text(text: &str, max: usize) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= max {
        cleaned
    } else {
        let mut out: String = cleaned.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// 未启用资讯或源不可用时展示的使用提示
pub fn builtin_tips() -> Vec<NewsItem> {
    let tips: [(&str, &str); 6] = [
        (
            "先做一次 Java 搜索",
            "在「设置 - Java」里点简要搜索，启动器会自动挑选匹配当前版本的 Java；找不到时再用详细搜索全盘扫描。",
        ),
        (
            "下载慢就换镜像",
            "「设置 - 下载」里可以切换到 BMCLAPI 镜像，游戏本体、依赖库和资源文件都会走国内节点。",
        ),
        (
            "保持版本隔离",
            "版本隔离开启后，每个版本的 mods、存档与配置互不影响，装整合包时不会互相污染。",
        ),
        (
            "皮肤站账号即插即用",
            "皮肤站账号登录后，启动时会自动挂载 authlib-injector，皮肤和披风直接生效，不需要手动改参数。",
        ),
        (
            "整合包装到哪",
            "整合包会写入当前选中版本的目录，安装前先确认版本选择正确，避免装错位置。",
        ),
        (
            "绑定自己的资讯源",
            "「设置 - 资讯」可以在内置源之间切换，也可以填入自己的 JSON 或 RSS 地址。",
        ),
    ];
    tips.iter()
        .map(|(title, summary)| NewsItem {
            title: (*title).to_string(),
            summary: (*summary).to_string(),
            date: String::new(),
            url: String::new(),
            image: None,
            tag: Some("使用提示".into()),
        })
        .collect()
}
