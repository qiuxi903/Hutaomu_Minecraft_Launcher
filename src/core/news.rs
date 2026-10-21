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

#[derive(Debug, Deserialize)]
struct NewsEnvelope {
    #[serde(default)]
    items: Vec<NewsItem>,
    #[serde(default)]
    data: Vec<NewsItem>,
    #[serde(default)]
    list: Vec<NewsItem>,
}

/// 拉取资讯。支持 JSON（数组或 {items:[]}）与 RSS / Atom。
pub fn fetch(dl: &Downloader, url: &str) -> Result<Vec<NewsItem>> {
    if url.trim().is_empty() {
        return Ok(builtin_tips());
    }
    let bytes = dl
        .get_bytes_direct(url)
        .with_context(|| format!("获取资讯失败: {url}"))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let trimmed = text.trim_start();

    if trimmed.starts_with('[') {
        let items: Vec<NewsItem> = serde_json::from_str(trimmed)
            .with_context(|| "资讯 JSON 格式不正确，应为对象数组")?;
        return Ok(normalize(items));
    }
    if trimmed.starts_with('{') {
        let envelope: NewsEnvelope = serde_json::from_str(trimmed)
            .with_context(|| "资讯 JSON 格式不正确")?;
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

fn normalize(items: Vec<NewsItem>) -> Vec<NewsItem> {
    items
        .into_iter()
        .filter(|item| !item.title.trim().is_empty())
        .take(30)
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
                summary: truncate_text(&decode_entities(&strip_tags(&summary)), 160),
                date: date.trim().to_string(),
                url: url.trim().to_string(),
                image: None,
                tag: None,
            });
        }
    }
    items
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
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
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

/// 未绑定资讯源时展示的使用提示
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
            "「设置 - 资讯」可以填入自己的 JSON 或 RSS 地址，首页就会显示你关心的内容。",
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
