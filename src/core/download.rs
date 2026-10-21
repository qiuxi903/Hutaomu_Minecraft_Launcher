use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::core::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadSource {
    /// Mojang 官方源
    Official,
    /// BMCLAPI 镜像（国内加速）
    Bmclapi,
}

impl Default for DownloadSource {
    fn default() -> Self {
        Self::Official
    }
}

impl DownloadSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Official => "官方源",
            Self::Bmclapi => "BMCLAPI 镜像",
        }
    }

    pub fn all() -> [DownloadSource; 2] {
        [Self::Official, Self::Bmclapi]
    }

    pub fn version_manifest(self) -> &'static str {
        match self {
            Self::Official => {
                "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
            }
            Self::Bmclapi => {
                "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
            }
        }
    }

    /// 把官方 URL 重写到镜像地址；官方源原样返回
    pub fn rewrite(self, url: &str) -> String {
        match self {
            Self::Official => url.to_string(),
            Self::Bmclapi => rewrite_bmclapi(url),
        }
    }

    pub fn fabric_meta(self, path: &str) -> String {
        match self {
            Self::Official => format!("https://meta.fabricmc.net{path}"),
            Self::Bmclapi => format!("https://bmclapi2.bangbang93.com/fabric-meta{path}"),
        }
    }

    pub fn quilt_meta(self, path: &str) -> String {
        match self {
            Self::Official => format!("https://meta.quiltmc.org{path}"),
            Self::Bmclapi => format!("https://bmclapi2.bangbang93.com/quilt-meta{path}"),
        }
    }

    pub fn forge_maven(self, path: &str) -> String {
        match self {
            Self::Official => format!("https://maven.minecraftforge.net/{path}"),
            Self::Bmclapi => format!("https://bmclapi2.bangbang93.com/maven/{path}"),
        }
    }

    pub fn neoforge_maven(self, path: &str) -> String {
        match self {
            Self::Official => format!("https://maven.neoforged.net/releases/{path}"),
            Self::Bmclapi => format!("https://bmclapi2.bangbang93.com/maven/{path}"),
        }
    }
}

fn rewrite_bmclapi(url: &str) -> String {
    const REPLACEMENTS: [(&str, &str); 9] = [
        (
            "https://piston-meta.mojang.com",
            "https://bmclapi2.bangbang93.com",
        ),
        (
            "https://launchermeta.mojang.com",
            "https://bmclapi2.bangbang93.com",
        ),
        (
            "https://launcher.mojang.com",
            "https://bmclapi2.bangbang93.com",
        ),
        (
            "https://piston-data.mojang.com",
            "https://bmclapi2.bangbang93.com",
        ),
        (
            "https://resources.download.minecraft.net",
            "https://bmclapi2.bangbang93.com/assets",
        ),
        (
            "https://libraries.minecraft.net",
            "https://bmclapi2.bangbang93.com/maven",
        ),
        (
            "https://maven.fabricmc.net",
            "https://bmclapi2.bangbang93.com/maven",
        ),
        (
            "https://maven.minecraftforge.net",
            "https://bmclapi2.bangbang93.com/maven",
        ),
        (
            "https://maven.neoforged.net/releases",
            "https://bmclapi2.bangbang93.com/maven",
        ),
    ];
    for (from, to) in REPLACEMENTS {
        if let Some(rest) = url.strip_prefix(from) {
            return format!("{to}{rest}");
        }
    }
    url.to_string()
}

#[derive(Debug, Default)]
pub struct DownloadProgress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub message: Mutex<String>,
    pub cancelled: AtomicBool,
    pub finished: AtomicBool,
    pub files_done: AtomicU64,
    pub files_total: AtomicU64,
}

impl DownloadProgress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_message(&self, msg: impl Into<String>) {
        if let Ok(mut guard) = self.message.lock() {
            *guard = msg.into();
        }
    }

    pub fn message(&self) -> String {
        self.message.lock().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn add_total(&self, bytes: u64) {
        self.total.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_done(&self, bytes: u64) {
        self.done.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        let done = self.done.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (done as f32 / total as f32).clamp(0.0, 1.0)
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct FileTask {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: u64,
}

impl FileTask {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            sha1: None,
            size: 0,
        }
    }

    pub fn with_sha1(mut self, sha1: impl Into<String>) -> Self {
        self.sha1 = Some(sha1.into());
        self
    }

    pub fn with_optional_sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1;
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }
}

static USE_SYSTEM_PROXY: AtomicBool = AtomicBool::new(true);

/// 是否使用系统代理（默认开启）。部分 CDN 直连会被拒绝，需要走系统代理。
pub fn set_use_system_proxy(enabled: bool) {
    USE_SYSTEM_PROXY.store(enabled, Ordering::Relaxed);
}

pub fn use_system_proxy() -> bool {
    USE_SYSTEM_PROXY.load(Ordering::Relaxed)
}

/// 读取系统代理：优先环境变量，其次 Windows 注册表中的 Internet 设置
pub fn system_proxy() -> Option<String> {
    for var in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(value) = std::env::var(var) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(normalize_proxy(value));
            }
        }
    }

    #[cfg(windows)]
    {
        const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
        let enabled = reg_query(KEY, "ProxyEnable")?;
        if !enabled.contains("0x1") {
            return None;
        }
        let server = reg_query(KEY, "ProxyServer")?;
        let server = server.trim();
        if server.is_empty() {
            return None;
        }
        let value = if server.contains('=') {
            server
                .split(';')
                .filter_map(|part| {
                    let part = part.trim();
                    part.strip_prefix("https=")
                        .or_else(|| part.strip_prefix("http="))
                })
                .next()
                .unwrap_or("")
                .to_string()
        } else {
            server.to_string()
        };
        if value.is_empty() {
            None
        } else {
            Some(normalize_proxy(&value))
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn normalize_proxy(value: &str) -> String {
    if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    }
}

#[cfg(windows)]
fn reg_query(key: &str, name: &str) -> Option<String> {
    use std::process::Command;
    let mut cmd = Command::new("reg");
    cmd.args(["query", key, "/v", name]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(name) {
            continue;
        }
        if let Some(index) = trimmed.find("REG_SZ") {
            return Some(trimmed[index + 6..].trim().to_string());
        }
        if let Some(index) = trimmed.find("REG_DWORD") {
            return Some(trimmed[index + 9..].trim().to_string());
        }
    }
    None
}

#[derive(Clone)]
pub struct Downloader {
    client: reqwest::blocking::Client,
    /// 镜像站（BMCLAPI）走直连：它本身就是为直连加速设计的，
    /// 走代理反而可能被拒绝。
    direct: reqwest::blocking::Client,
    pub source: DownloadSource,
}

impl Downloader {
    pub fn new(source: DownloadSource, timeout_secs: u64) -> Result<Self> {
        Self::build(source, timeout_secs, use_system_proxy())
    }

    pub fn build(source: DownloadSource, timeout_secs: u64, use_proxy: bool) -> Result<Self> {
        let mut builder = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "htmcl/",
                env!("CARGO_PKG_VERSION"),
                " (Hutaomu Minecraft Launcher)"
            ))
            .timeout(Duration::from_secs(timeout_secs.max(10)))
            .connect_timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(16);

        if use_proxy {
            if let Some(proxy) = system_proxy() {
                match reqwest::Proxy::all(&proxy) {
                    Ok(proxy) => {
                        log::info!("使用系统代理: {proxy:?}");
                        builder = builder.proxy(proxy);
                    }
                    Err(err) => log::warn!("系统代理配置无效（{proxy}）: {err}"),
                }
            }
        }

        let client = builder.build().context("初始化网络客户端失败")?;

        let direct_builder = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "htmcl/",
                env!("CARGO_PKG_VERSION"),
                " (Hutaomu Minecraft Launcher)"
            ))
            .timeout(Duration::from_secs(timeout_secs.max(10)))
            .connect_timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(16);
        let direct = direct_builder.build().context("初始化网络客户端失败")?;

        Ok(Self {
            client,
            direct,
            source,
        })
    }

    pub fn default_client() -> Result<Self> {
        Self::new(DownloadSource::Official, 60)
    }

    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let url = self.source.rewrite(url);
        let resp = self
            .client_for(&url)
            .get(&url)
            .send()
            .with_context(|| format!("请求失败: {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("请求失败 (HTTP {}): {url}", resp.status()));
        }
        let bytes = resp.bytes().context("读取响应失败")?;
        serde_json::from_slice(&bytes).with_context(|| format!("解析 JSON 失败: {url}"))
    }

    /// 直接按原样请求（不经过镜像重写），用于 Modrinth 等第三方 API
    pub fn get_json_direct<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("请求失败: {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("请求失败 (HTTP {}): {url}", resp.status()));
        }
        let bytes = resp.bytes().context("读取响应失败")?;
        serde_json::from_slice(&bytes).with_context(|| format!("解析 JSON 失败: {url}"))
    }

    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let url = self.source.rewrite(url);
        let resp = self
            .client_for(&url)
            .get(&url)
            .send()
            .with_context(|| format!("请求失败: {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("请求失败 (HTTP {}): {url}", resp.status()));
        }
        Ok(resp.bytes().context("读取响应失败")?.to_vec())
    }

    pub fn get_bytes_direct(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("请求失败: {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("请求失败 (HTTP {}): {url}", resp.status()));
        }
        Ok(resp.bytes().context("读取响应失败")?.to_vec())
    }

    pub fn client(&self) -> &reqwest::blocking::Client {
        &self.client
    }

    /// 根据目标主机选择客户端：镜像站直连，其余走系统代理
    fn client_for(&self, url: &str) -> &reqwest::blocking::Client {
        let host = url
            .split("://")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("");
        if host.ends_with("bangbang93.com") {
            &self.direct
        } else {
            &self.client
        }
    }

    /// 发送 JSON 请求并解析响应，非 2xx 时把响应体一并返回便于排查
    pub fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .client_for(url)
            .post(url)
            .json(body)
            .send()
            .with_context(|| format!("请求失败: {url}"))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("HTTP {status}: {}", truncate(&text, 300)));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("解析响应失败: {}", truncate(&text, 300)))
    }

    pub fn post_form<T: DeserializeOwned>(&self, url: &str, form: &[(&str, &str)]) -> Result<T> {
        let resp = self
            .client_for(url)
            .post(url)
            .form(form)
            .send()
            .with_context(|| format!("请求失败: {url}"))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("HTTP {status}: {}", truncate(&text, 300)));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("解析响应失败: {}", truncate(&text, 300)))
    }

    pub fn get_json_with_headers<T: DeserializeOwned>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<T> {
        let mut req = self.client_for(url).get(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req.send().with_context(|| format!("请求失败: {url}"))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("HTTP {status}: {}", truncate(&text, 300)));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("解析响应失败: {}", truncate(&text, 300)))
    }

    pub fn post_json_with_headers<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        url: &str,
        body: &B,
        headers: &[(&str, &str)],
    ) -> Result<T> {
        let mut req = self.client_for(url).post(url).json(body);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req.send().with_context(|| format!("请求失败: {url}"))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("HTTP {status}: {}", truncate(&text, 300)));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("解析响应失败: {}", truncate(&text, 300)))
    }

    /// 下载单个文件，支持进度、取消、SHA1 校验与重试
    pub fn download(
        &self,
        task: &FileTask,
        progress: &DownloadProgress,
    ) -> Result<()> {
        if progress.is_cancelled() {
            return Err(anyhow!("已取消"));
        }
        if task.dest.is_file() {
            let ok = match &task.sha1 {
                Some(expected) => file_sha1(&task.dest)
                    .map(|actual| actual.eq_ignore_ascii_case(expected))
                    .unwrap_or(false),
                None => true,
            };
            if ok {
                progress.add_done(task.size);
                return Ok(());
            }
        }

        paths::ensure_parent(&task.dest)?;

        // 候选地址：镜像优先，镜像缺失时回退到原始地址
        let mut candidates = vec![self.source.rewrite(&task.url)];
        if candidates[0] != task.url {
            candidates.push(task.url.clone());
        }

        let mut last_err = None;
        for url in &candidates {
            for attempt in 0..3 {
                if progress.is_cancelled() {
                    return Err(anyhow!("已取消"));
                }
                if attempt > 0 {
                    std::thread::sleep(Duration::from_millis(400 * attempt));
                }
                match self.download_once(url, task, progress) {
                    Ok(()) => {
                        progress.files_done.fetch_add(1, Ordering::Relaxed);
                        return Ok(());
                    }
                    Err(err) => {
                        log::warn!("下载失败（第 {} 次）{url}: {err}", attempt + 1);
                        last_err = Some(err);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("下载失败: {}", task.url)))
    }

    fn download_once(
        &self,
        url: &str,
        task: &FileTask,
        progress: &DownloadProgress,
    ) -> Result<()> {
        let mut resp = self
            .client_for(url)
            .get(url)
            .send()
            .with_context(|| format!("连接失败: {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("HTTP {} ({url})", resp.status()));
        }

        let tmp = task.dest.with_extension("part");
        paths::ensure_parent(&tmp)?;
        let mut file = std::fs::File::create(&tmp)?;
        let mut hasher = Sha1::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut written = 0u64;

        loop {
            if progress.is_cancelled() {
                drop(file);
                let _ = std::fs::remove_file(&tmp);
                return Err(anyhow!("已取消"));
            }
            let read = resp.read(&mut buf)?;
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
            file.write_all(&buf[..read])?;
            written += read as u64;
            progress.add_done(read as u64);
        }
        file.flush()?;
        drop(file);

        if let Some(expected) = &task.sha1 {
            let actual = hex_lower(&hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected) {
                let _ = std::fs::remove_file(&tmp);
                return Err(anyhow!("文件校验失败: {}", task.dest.display()));
            }
        }

        if written == 0 && task.size > 0 {
            let _ = std::fs::remove_file(&tmp);
            return Err(anyhow!("下载内容为空: {url}"));
        }

        if task.dest.exists() {
            let _ = std::fs::remove_file(&task.dest);
        }
        std::fs::rename(&tmp, &task.dest)?;
        Ok(())
    }

    /// 多线程批量下载：单个文件失败不会中断整批，最后统一重试。
    /// 返回仍然失败的文件列表。
    fn download_batch_inner(
        &self,
        tasks: Vec<FileTask>,
        threads: usize,
        progress: &Arc<DownloadProgress>,
        abort_after: usize,
    ) -> Vec<FileTask> {
        let queue = Arc::new(Mutex::new(tasks));
        let failed: Arc<Mutex<Vec<(FileTask, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let threads = threads.clamp(1, 32);
        let abort = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();

        for _ in 0..threads {
            let queue = queue.clone();
            let failed = failed.clone();
            let progress = progress.clone();
            let downloader = self.clone();
            let abort = abort.clone();
            handles.push(std::thread::spawn(move || loop {
                if progress.is_cancelled() || abort.load(Ordering::Relaxed) {
                    return;
                }
                let task = {
                    let mut guard = queue.lock().unwrap();
                    guard.pop()
                };
                let Some(task) = task else { break };
                if let Err(err) = downloader.download(&task, &progress) {
                    let too_many = {
                        let mut list = failed.lock().unwrap();
                        list.push((task, format!("{err}")));
                        list.len() >= abort_after
                    };
                    if too_many {
                        abort.store(true, Ordering::Relaxed);
                    }
                }
            }));
        }

        for handle in handles {
            let _ = handle.join();
        }

        // 第二轮：重试失败项。若失败数量已触发熔断，说明是系统性问题（网络/源不可用），
        // 直接返回失败列表，避免无意义的重试拖慢整体流程。
        let mut still_failed: Vec<FileTask> = Vec::new();
        let retry_list: Vec<FileTask> = {
            let mut list = failed.lock().unwrap();
            list.drain(..).map(|(task, _)| task).collect()
        };
        if abort.load(Ordering::Relaxed) {
            still_failed = retry_list;
            return still_failed;
        }
        if !retry_list.is_empty() && !progress.is_cancelled() {
            progress.set_message(format!("正在重试 {} 个失败文件 ...", retry_list.len()));
            for task in retry_list {
                if progress.is_cancelled() {
                    break;
                }
                if self.download(&task, progress).is_err() {
                    still_failed.push(task);
                }
            }
        }
        still_failed
    }

    /// 严格模式：任何文件失败都返回错误
    pub fn download_batch(
        &self,
        tasks: Vec<FileTask>,
        threads: usize,
        progress: &Arc<DownloadProgress>,
    ) -> Result<()> {
        let failed = self.download_batch_inner(tasks, threads, progress, 24);
        if !failed.is_empty() {
            let mut message = format!("{} 个文件下载失败，请检查网络或更换下载源。", failed.len());
            for task in failed.iter().take(3) {
                message.push_str("\n");
                message.push_str(&task.url);
            }
            log::error!("{message}");
            return Err(anyhow!(message));
        }
        Ok(())
    }

    /// 宽松模式：返回失败列表，不报错（用于资源文件这类可后补的内容）
    pub fn download_batch_lenient(
        &self,
        tasks: Vec<FileTask>,
        threads: usize,
        progress: &Arc<DownloadProgress>,
    ) -> Vec<FileTask> {
        self.download_batch_inner(tasks, threads, progress, 200)
    }
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

pub fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn file_sha1(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Some(hex_lower(&hasher.finalize()))
}

pub fn truncate(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        let mut out: String = trimmed.chars().take(max).collect();
        out.push('…');
        out
    }
}
