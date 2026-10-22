use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
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
        // 默认走国内镜像（BMCLAPI），直连即可，下载更快
        Self::Bmclapi
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

    /// 回滚已计入的字节。下载失败或需要从头重下时调用，避免进度虚高。
    pub fn sub_done(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let mut current = self.done.load(Ordering::Relaxed);
        loop {
            let next = current.saturating_sub(bytes);
            match self.done.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
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

/// 永久性失败（404、403 等），重试没有意义，直接放弃
#[derive(Debug)]
struct PermanentError(String);

impl std::fmt::Display for PermanentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PermanentError {}

/// 判断状态码是否为「重试也没用」的类型
fn is_permanent_status(status: reqwest::StatusCode) -> bool {
    if status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return false;
    }
    status.is_client_error()
}

static USE_SYSTEM_PROXY: AtomicBool = AtomicBool::new(true);

/// 目标文件占用表：游戏本体与加载器并行下载时，避免两个任务同时写同一个文件。
static FILE_LOCKS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
static FILE_CV: OnceLock<Condvar> = OnceLock::new();

fn file_locks() -> &'static Mutex<HashSet<PathBuf>> {
    FILE_LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn file_cv() -> &'static Condvar {
    FILE_CV.get_or_init(|| Condvar::new())
}

/// 目标文件的占用凭证，释放时唤醒等待中的任务。
pub struct FileLock {
    path: PathBuf,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        if let Ok(mut set) = file_locks().lock() {
            set.remove(&self.path);
        }
        file_cv().notify_all();
    }
}

/// 下载中的分片路径：在原文件名后加 .part
///
/// 不替换扩展名，避免同目录下 a.jar 与 a.zip 生成同一个分片文件。
fn part_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download.part".to_string());
    dest.with_file_name(format!("{name}.part"))
}

/// 等待目标文件空闲后占用它。同一文件不会被两个任务同时写入。
pub fn acquire_file(path: &Path) -> FileLock {
    let mut set = file_locks().lock().unwrap_or_else(|e| e.into_inner());
    while set.contains(path) {
        set = file_cv()
            .wait(set)
            .unwrap_or_else(|e| e.into_inner());
    }
    set.insert(path.to_path_buf());
    FileLock {
        path: path.to_path_buf(),
    }
}

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
        Self::new(DownloadSource::default(), 60)
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

    /// 认证接口使用与普通请求相同的客户端选择逻辑
    pub fn client_for_auth(&self, url: &str) -> &reqwest::blocking::Client {
        self.client_for(url)
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

    /// 下载单个文件，支持进度、取消、SHA1 校验、断点续传与重试
    pub fn download(
        &self,
        task: &FileTask,
        progress: &DownloadProgress,
    ) -> Result<()> {
        if progress.is_cancelled() {
            return Err(anyhow!("已取消"));
        }

        // 同一目标文件串行化：并行任务等待前一个写完后再校验，避免互相覆盖
        let _lock = acquire_file(&task.dest);

        // 拿到锁后重新判断：可能前一个任务刚刚下载完成
        if task.dest.is_file() && self.verify_existing(task)? {
            progress.add_done(task.size);
            progress.files_done.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        paths::ensure_parent(&task.dest)?;

        // 候选地址：镜像优先，镜像缺失时回退到原始地址
        let mut candidates = vec![self.source.rewrite(&task.url)];
        if candidates[0] != task.url {
            candidates.push(task.url.clone());
        }

        // 记录本次调用实际计入的字节，失败时精确回滚
        let mut added = 0u64;
        let mut last_err = None;
        for url in &candidates {
            for attempt in 0..3 {
                if progress.is_cancelled() {
                    progress.sub_done(added);
                    return Err(anyhow!("已取消"));
                }
                if attempt > 0 {
                    std::thread::sleep(Duration::from_millis(400 * attempt));
                }
                match self.download_once(url, task, progress, &mut added) {
                    Ok(()) => {
                        progress.files_done.fetch_add(1, Ordering::Relaxed);
                        return Ok(());
                    }
                    Err(err) => {
                        // 永久性错误不再重试
                        if err.downcast_ref::<PermanentError>().is_some() {
                            log::warn!("下载失败（不再重试）{url}: {err}");
                            last_err = Some(err);
                            break;
                        }
                        log::warn!("下载失败（第 {} 次）{url}: {err}", attempt + 1);
                        last_err = Some(err);
                    }
                }
            }
        }

        // 失败：把本次计入的字节全部退回，进度条不会虚高
        progress.sub_done(added);
        Err(last_err.unwrap_or_else(|| anyhow!("下载失败: {}", task.url)))
    }

    /// 已存在的文件是否可用（有校验值时必须匹配）
    fn verify_existing(&self, task: &FileTask) -> Result<bool> {
        match &task.sha1 {
            Some(expected) => Ok(file_sha1(&task.dest)
                .map(|actual| actual.eq_ignore_ascii_case(expected))
                .unwrap_or(false)),
            None => Ok(true),
        }
    }

    /// 单次下载尝试：支持 HTTP Range 断点续传
    fn download_once(
        &self,
        url: &str,
        task: &FileTask,
        progress: &DownloadProgress,
        added: &mut u64,
    ) -> Result<()> {
        let tmp = part_path(&task.dest);
        paths::ensure_parent(&tmp)?;

        // 已有的分片长度即续传起点
        let mut resume_from = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if task.size > 0 && resume_from > task.size {
            // 分片比预期还大，说明源已经变了，丢弃重来
            resume_from = 0;
            let _ = std::fs::remove_file(&tmp);
        }

        let mut req = self.client_for(url).get(url);
        if resume_from > 0 {
            req = req.header("Range", format!("bytes={resume_from}-"));
        }
        let mut resp = req
            .send()
            .with_context(|| format!("连接失败: {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            if is_permanent_status(status) {
                // 404 之类的错误换源重试也没意义，直接放弃
                return Err(anyhow!(PermanentError(format!(
                    "HTTP {} ({url})",
                    status
                ))));
            }
            return Err(anyhow!("HTTP {} ({url})", status));
        }

        // 服务器没有返回 206，说明不支持续传，只能从头重下
        let mut append = false;
        if resume_from > 0 {
            // 有些服务器会回 206 却仍然发送整个文件，这时追加会写坏文件
            let sent_whole_file = resp
                .content_length()
                .map(|len| task.size > 0 && len == task.size)
                .unwrap_or(false);
            if status == reqwest::StatusCode::PARTIAL_CONTENT && !sent_whole_file {
                append = true;
                // 上次会话留下的分片没有计入过进度，这里补上
                // （本次调用内已计入的部分不再重复计算）
                let uncounted = resume_from.saturating_sub(*added);
                if uncounted > 0 {
                    progress.add_done(uncounted);
                    *added += uncounted;
                }
            } else {
                log::debug!("{url} 未按范围返回，重新下载");
                // 分片作废，把本次调用已计入的字节退回
                progress.sub_done(*added);
                *added = 0;
                resume_from = 0;
                let _ = std::fs::remove_file(&tmp);
            }
        }

        let mut hasher = Sha1::new();
        let mut file = if append {
            // 续传：先把已下载部分计入哈希，再接着往下写
            let mut existing = std::fs::File::open(&tmp)?;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let read = existing.read(&mut buf)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buf[..read]);
            }
            std::fs::OpenOptions::new()
                .append(true)
                .open(&tmp)
                .with_context(|| format!("写入分片失败: {}", tmp.display()))?
        } else {
            std::fs::File::create(&tmp)
                .with_context(|| format!("写入分片失败: {}", tmp.display()))?
        };

        let mut buf = vec![0u8; 64 * 1024];
        let mut written = resume_from;

        loop {
            if progress.is_cancelled() {
                drop(file);
                // 保留分片，下次可以接着下
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
            *added += read as u64;
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
        let mut pending = tasks;
        let mut failed: Vec<FileTask> = Vec::new();
        let mut permanent: Vec<FileTask> = Vec::new();

        for round in 0..3 {
            if pending.is_empty() {
                break;
            }
            if round > 0 {
                if progress.is_cancelled() {
                    break;
                }
                progress.set_message(format!(
                    "正在重试 {} 个失败文件（第 {} 轮）...",
                    pending.len(),
                    round
                ));
                std::thread::sleep(Duration::from_millis(500 * round as u64));
            }

            let (round_failed, round_permanent, aborted) =
                self.run_pool(pending, threads, progress, abort_after);
            failed = round_failed;
            permanent = round_permanent;
            if aborted {
                // 熔断说明是系统性问题（网络/源不可用），不再重试
                break;
            }
            if failed.is_empty() && permanent.is_empty() {
                return Vec::new();
            }
            pending = failed.clone();
        }

        // 永久性失败（404 之类）不再重试，但也要如实报告
        failed.extend(permanent);
        failed
    }

    /// 把任务分给多个线程执行，返回失败列表与是否触发熔断
    fn run_pool(
        &self,
        tasks: Vec<FileTask>,
        threads: usize,
        progress: &Arc<DownloadProgress>,
        abort_after: usize,
    ) -> (Vec<FileTask>, Vec<FileTask>, bool) {
        let queue = Arc::new(Mutex::new(tasks));
        // (任务, 是否为永久性失败)
        let failed: Arc<Mutex<Vec<(FileTask, bool)>>> = Arc::new(Mutex::new(Vec::new()));
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
                    let permanent = err.downcast_ref::<PermanentError>().is_some();
                    let too_many = {
                        let mut list = failed.lock().unwrap();
                        list.push((task, permanent));
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

        let aborted = abort.load(Ordering::Relaxed);
        let mut retryable: Vec<FileTask> = Vec::new();
        let mut permanent: Vec<FileTask> = Vec::new();
        for (task, is_permanent) in failed.lock().unwrap().iter() {
            if *is_permanent {
                permanent.push(task.clone());
            } else {
                retryable.push(task.clone());
            }
        }

        if aborted {
            // 熔断后队列里没来得及处理的任务也要计入失败，否则会被静默丢弃
            let remaining = queue.lock().unwrap().drain(..).collect::<Vec<_>>();
            retryable.extend(remaining);
        }

        (retryable, permanent, aborted)
    }

    /// 严格模式：任何文件失败都返回错误
    pub fn download_batch(
        &self,
        tasks: Vec<FileTask>,
        threads: usize,
        progress: &Arc<DownloadProgress>,
    ) -> Result<()> {
        progress.files_total.store(tasks.len() as u64, Ordering::Relaxed);
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
        progress.files_total.store(tasks.len() as u64, Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_done_never_underflows() {
        let p = DownloadProgress::new();
        p.add_done(100);
        p.sub_done(500);
        assert_eq!(p.done.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn part_path_keeps_original_extension() {
        assert_eq!(
            part_path(Path::new("a/b/foo.jar")),
            PathBuf::from("a/b/foo.jar.part")
        );
        assert_eq!(
            part_path(Path::new("a/b/foo.zip")),
            PathBuf::from("a/b/foo.zip.part")
        );
    }

    /// 启动一个支持 Range 的本地文件服务器
    fn spawn_range_server(body: &'static [u8]) -> String {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let start = req
                    .lines()
                    .find_map(|l| {
                        let lower = l.to_lowercase();
                        if !lower.starts_with("range:") {
                            return None;
                        }
                        l.split(':')
                            .nth(1)
                            .and_then(|r| r.trim().trim_end_matches('-').parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let chunk = &body[start.min(body.len())..];
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                    chunk.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(chunk);
                let _ = stream.flush();
            }
        });
        format!("http://{addr}/file.bin")
    }

    /// 断点续传：只下载缺失的部分，并把已存在的分片计入进度
    #[test]
    fn download_resumes_from_partial_file() {
        let body: &'static [u8] = b"0123456789ABCDEF";
        let url = spawn_range_server(body);
        let dl = Downloader::build(DownloadSource::Official, 10, false).unwrap();

        let dir = std::env::temp_dir().join("htmcl_resume_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("file.bin");

        // 模拟上次中断：留下半个分片
        std::fs::write(part_path(&dest), &body[..8]).unwrap();

        let progress = DownloadProgress::new();
        progress.add_total(body.len() as u64);
        let task = FileTask::new(url, dest.clone()).with_size(body.len() as u64);
        dl.download(&task, &progress).unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), body);
        // 8 字节续传 + 8 字节新下载 = 完整长度，进度应为 100%
        assert_eq!(progress.done.load(Ordering::Relaxed), 16);
        assert_eq!(progress.fraction(), 1.0);
        // 分片文件在成功后应被清理
        assert!(!part_path(&dest).exists());
    }

    /// 完整文件已存在时不应重复下载
    #[test]
    fn download_skips_existing_file() {
        let body: &'static [u8] = b"0123456789ABCDEF";
        let url = spawn_range_server(body);
        let dl = Downloader::build(DownloadSource::Official, 10, false).unwrap();

        let dir = std::env::temp_dir().join("htmcl_skip_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("file.bin");
        std::fs::write(&dest, body).unwrap();

        let progress = DownloadProgress::new();
        let task = FileTask::new(url, dest.clone()).with_size(body.len() as u64);
        dl.download(&task, &progress).unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(progress.done.load(Ordering::Relaxed), 16);
    }
}
