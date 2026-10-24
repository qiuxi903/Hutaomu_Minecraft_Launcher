use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use crate::core::java::JavaInfo;
use crate::core::minecraft::install;
use crate::core::minecraft::version::{self, ResolvedVersion};
use crate::core::paths;

pub const LAUNCHER_NAME: &str = "HTMCL";
/// 游戏默认语言，简体中文
pub const DEFAULT_LANG: &str = "zh_cn";

/// 把游戏语言固定成简体中文
///
/// Minecraft 的语言由游戏目录里 `options.txt` 的 `lang` 决定，
/// 启动参数 `--lang` 并不管用（26.x 的参数表里没有它）。
/// 其它设置原样保留，只改语言这一项。
pub fn ensure_chinese_lang(game_dir: &Path) -> Result<()> {
    let path = game_dir.join("options.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in text.lines() {
        if line.starts_with("lang:") {
            if !found {
                lines.push(format!("lang:{DEFAULT_LANG}"));
                found = true;
            }
            continue;
        }
        lines.push(line.to_string());
    }
    if !found {
        lines.push(format!("lang:{DEFAULT_LANG}"));
    }
    std::fs::write(&path, lines.join("\n") + "\n")
        .with_context(|| format!("写入语言设置失败: {}", path.display()))?;
    Ok(())
}
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Default)]
pub struct LaunchIdentity {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    /// msa / mojang / legacy
    pub user_type: String,
    pub xuid: String,
    pub client_id: String,
    pub user_properties: String,
    /// 皮肤站地址，存在时启用 authlib-injector
    pub authlib_url: Option<String>,
}

pub struct LaunchSpec<'a> {
    pub version: &'a ResolvedVersion,
    pub identity: &'a LaunchIdentity,
    pub java: &'a JavaInfo,
    pub memory_mb: u32,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub extra_jvm_args: &'a str,
    pub extra_game_args: &'a str,
    pub authlib_jar: Option<&'a Path>,
    pub server: Option<(String, u16)>,
}

fn os_name() -> &'static str {
    "windows"
}

fn os_version() -> &'static str {
    "10.0"
}

fn arch() -> &'static str {
    "x86_64"
}

/// 展开 arguments 中的条件项（字符串或 {rules, value}）
fn collect_arguments(items: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Value::String(s) => out.push(s.clone()),
            Value::Object(map) => {
                let rules: Vec<version::Rule> = map
                    .get("rules")
                    .and_then(|r| serde_json::from_value(r.clone()).ok())
                    .unwrap_or_default();
                if !version::rules_allow(&rules) {
                    continue;
                }
                match map.get("value") {
                    Some(Value::String(s)) => out.push(s.clone()),
                    Some(Value::Array(list)) => {
                        for v in list {
                            if let Value::String(s) = v {
                                out.push(s.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// 按空格拆分用户自定义参数，支持双引号包裹
pub fn split_args(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn substitute(input: &str, vars: &[(String, String)]) -> String {
    let mut out = input.to_string();
    for (key, value) in vars {
        if out.contains(key.as_str()) {
            out = out.replace(key.as_str(), value);
        }
    }
    out
}

pub fn build_command(spec: &LaunchSpec<'_>) -> Result<Command> {
    let resolved = spec.version;
    let json = &resolved.json;

    if json.main_class.is_empty() {
        return Err(anyhow!("版本描述缺少主类，无法启动"));
    }

    // 确保 natives 已解压
    paths::ensure_dir(&resolved.natives_dir)?;
    for lib in &json.libraries {
        if let Some((rel, _)) = version::library_native(lib) {
            let jar = resolved
                .libraries_dir
                .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if jar.is_file() {
                let exclude = lib.extract.as_ref().map(|e| e.exclude.clone());
                let _ = install::extract_natives(&jar, &resolved.natives_dir, exclude.as_deref());
            }
        }
    }

    // classpath
    let mut classpath: Vec<PathBuf> = Vec::new();
    for lib in &json.libraries {
        if let Some((rel, _)) = version::library_artifact(lib) {
            let path = resolved
                .libraries_dir
                .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if path.is_file() {
                classpath.push(path);
            }
        }
    }
    if resolved.jar_path.is_file() {
        classpath.push(resolved.jar_path.clone());
    }
    if classpath.is_empty() {
        return Err(anyhow!("缺少依赖库，请先下载该版本"));
    }
    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath_str = classpath
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(separator);

    let assets_index_name = json
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .or_else(|| json.assets.clone())
        .unwrap_or_else(|| "legacy".into());

    let game_assets = if assets_index_name == "legacy" || assets_index_name.starts_with("pre-1.6") {
        resolved.assets_dir.join("virtual").join(&assets_index_name)
    } else {
        resolved.assets_dir.clone()
    };

    let log_config = json
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
        .map(|c| {
            resolved
                .assets_dir
                .join("log_configs")
                .join(&c.file.id)
        })
        .filter(|p| p.is_file());

    let vars: Vec<(String, String)> = vec![
        ("${auth_player_name}".into(), spec.identity.username.clone()),
        ("${version_name}".into(), resolved.id.clone()),
        ("${game_directory}".into(), resolved.game_dir.display().to_string()),
        ("${assets_root}".into(), resolved.assets_dir.display().to_string()),
        ("${game_assets}".into(), game_assets.display().to_string()),
        ("${assets_index_name}".into(), assets_index_name.clone()),
        ("${auth_uuid}".into(), spec.identity.uuid.clone()),
        ("${auth_access_token}".into(), spec.identity.access_token.clone()),
        ("${auth_session}".into(), spec.identity.access_token.clone()),
        ("${user_type}".into(), spec.identity.user_type.clone()),
        ("${version_type}".into(), resolved.id.clone()),
        ("${natives_directory}".into(), resolved.natives_dir.display().to_string()),
        ("${launcher_name}".into(), LAUNCHER_NAME.into()),
        ("${launcher_version}".into(), LAUNCHER_VERSION.into()),
        ("${classpath}".into(), classpath_str.clone()),
        ("${classpath_separator}".into(), separator.into()),
        ("${library_directory}".into(), resolved.libraries_dir.display().to_string()),
        ("${auth_xuid}".into(), spec.identity.xuid.clone()),
        ("${clientid}".into(), spec.identity.client_id.clone()),
        (
            "${user_properties}".into(),
            if spec.identity.user_properties.is_empty() {
                "{}".into()
            } else {
                spec.identity.user_properties.clone()
            },
        ),
        ("${resolution_width}".into(), spec.width.to_string()),
        ("${resolution_height}".into(), spec.height.to_string()),
        ("${quickPlayPath}".into(), String::new()),
        ("${quickPlaySingleplayer}".into(), String::new()),
        ("${quickPlayMultiplayer}".into(), String::new()),
        ("${quickPlayRealms}".into(), String::new()),
    ];

    // ---- JVM 参数 ----
    let mut jvm_args: Vec<String> = Vec::new();
    jvm_args.push(format!("-Xmx{}M", spec.memory_mb.max(512)));
    jvm_args.push(format!("-Xms{}M", (spec.memory_mb / 2).clamp(256, 2048)));
    jvm_args.push("-Dfile.encoding=UTF-8".into());
    jvm_args.push("-Dstdout.encoding=UTF-8".into());
    jvm_args.push("-Dstderr.encoding=UTF-8".into());
    jvm_args.push("-Dlog4j2.formatMsgNoLookups=true".into());
    jvm_args.push(format!("-Dminecraft.launcher.brand={LAUNCHER_NAME}"));
    jvm_args.push(format!("-Dminecraft.launcher.version={LAUNCHER_VERSION}"));

    if let Some(jar) = spec.authlib_jar {
        if let Some(url) = &spec.identity.authlib_url {
            jvm_args.push(format!("-javaagent:{}={}", jar.display(), url));
            jvm_args.push("-Dauthlibinjector.side=client".into());
        }
    }

    if let Some(config) = &log_config {
        if let Some(logging) = json.logging.as_ref().and_then(|l| l.client.as_ref()) {
            let arg = substitute(
                &logging.argument,
                &[("${path}".into(), config.display().to_string())],
            );
            jvm_args.push(arg);
        }
    }

    let from_json = json
        .arguments
        .as_ref()
        .map(|a| collect_arguments(&a.jvm))
        .unwrap_or_default();

    if from_json.is_empty() {
        jvm_args.push(format!(
            "-Djava.library.path={}",
            resolved.natives_dir.display()
        ));
        jvm_args.push("-cp".into());
        jvm_args.push(classpath_str.clone());
    } else {
        for arg in from_json {
            jvm_args.push(substitute(&arg, &vars));
        }
    }

    for extra in split_args(spec.extra_jvm_args) {
        jvm_args.push(extra);
    }

    // ---- 游戏参数 ----
    let mut game_args: Vec<String> = Vec::new();
    if let Some(arguments) = &json.arguments {
        for arg in collect_arguments(&arguments.game) {
            game_args.push(substitute(&arg, &vars));
        }
    } else if let Some(legacy) = &json.minecraft_arguments {
        for arg in legacy.split_whitespace() {
            game_args.push(substitute(arg, &vars));
        }
    }

    if spec.fullscreen {
        game_args.push("--fullscreen".into());
    } else {
        game_args.push("--width".into());
        game_args.push(spec.width.to_string());
        game_args.push("--height".into());
        game_args.push(spec.height.to_string());
    }

    if let Some((server, port)) = &spec.server {
        game_args.push("--server".into());
        game_args.push(server.clone());
        game_args.push("--port".into());
        game_args.push(port.to_string());
    }

    for extra in split_args(spec.extra_game_args) {
        game_args.push(extra);
    }

    let mut cmd = Command::new(&spec.java.path);
    cmd.args(&jvm_args);
    cmd.arg(&json.main_class);
    cmd.args(&game_args);
    cmd.current_dir(&resolved.game_dir);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    Ok(cmd)
}

pub fn describe_command(cmd: &Command) -> String {
    let mut text = cmd.get_program().to_string_lossy().to_string();
    for arg in cmd.get_args() {
        text.push(' ');
        let arg = arg.to_string_lossy();
        if arg.contains(' ') {
            text.push('"');
            text.push_str(&arg);
            text.push('"');
        } else {
            text.push_str(&arg);
        }
    }
    text
}

#[derive(Default)]
pub struct GameLog {
    lines: Mutex<VecDeque<String>>,
    pub exit_code: Mutex<Option<i32>>,
    pub pid: Mutex<Option<u32>>,
}

impl GameLog {
    pub fn push(&self, line: String) {
        if let Ok(mut lines) = self.lines.lock() {
            if lines.len() >= 4000 {
                lines.pop_front();
            }
            lines.push_back(line);
        }
    }

    pub fn snapshot(&self, count: usize) -> Vec<String> {
        self.lines
            .lock()
            .map(|lines| {
                let start = lines.len().saturating_sub(count);
                lines.iter().skip(start).cloned().collect()
            })
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.lines.lock().map(|l| l.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn set_exit(&self, code: i32) {
        if let Ok(mut guard) = self.exit_code.lock() {
            *guard = Some(code);
        }
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code.lock().ok().and_then(|g| *g)
    }
}

pub struct GameProcess {
    pub child: Child,
    pub pid: u32,
    pub log: Arc<GameLog>,
    pub version_id: String,
    pub started: std::time::Instant,
}

impl GameProcess {
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

/// 启动游戏进程，并把输出重定向到日志缓冲与文件
pub fn spawn(mut cmd: Command, version_id: &str) -> Result<GameProcess> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());

    let mut child = cmd.spawn().context("启动 Java 进程失败")?;
    let pid = child.id();
    let log = Arc::new(GameLog::default());
    if let Ok(mut guard) = log.pid.lock() {
        *guard = Some(pid);
    }

    let log_file = paths::logs_dir().join(format!(
        "game-{}-{}.log",
        version_id,
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    let _ = paths::ensure_dir(&paths::logs_dir());
    let file = std::fs::File::create(&log_file).ok();

    if let Some(stdout) = child.stdout.take() {
        let log = log.clone();
        let file = file.as_ref().and_then(|f| f.try_clone().ok());
        std::thread::spawn(move || {
            read_stream(stdout, log, file);
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let log = log.clone();
        let file = file.as_ref().and_then(|f| f.try_clone().ok());
        std::thread::spawn(move || {
            read_stream(stderr, log, file);
        });
    }

    Ok(GameProcess {
        child,
        pid,
        log,
        version_id: version_id.to_string(),
        started: std::time::Instant::now(),
    })
}

fn read_stream<R: std::io::Read + Send + 'static>(
    stream: R,
    log: Arc<GameLog>,
    mut file: Option<std::fs::File>,
) {
    use std::io::Write;
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(line) => {
                log.push(line.clone());
                if let Some(f) = file.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
            }
            Err(_) => break,
        }
    }
}

/// authlib-injector 在启动器缓存目录里的位置
///
/// 已经下载过且文件大小正常时返回路径，否则返回 None。
pub fn cached_authlib_injector() -> Option<PathBuf> {
    let jar = paths::cache_dir()
        .join("authlib-injector")
        .join("authlib-injector.jar");
    if jar.is_file() && std::fs::metadata(&jar).map(|m| m.len() > 100_000).unwrap_or(false) {
        Some(jar)
    } else {
        None
    }
}

/// 确保 authlib-injector 已下载到启动器缓存目录
///
/// 皮肤站账号启动游戏时需要把它挂成 javaagent，
/// 没有它就只能按普通正版账号启动，皮肤与披风都不会生效。
pub fn ensure_authlib_injector(dl: &crate::core::download::Downloader) -> Result<PathBuf> {
    if let Some(jar) = cached_authlib_injector() {
        return Ok(jar);
    }
    let dir = paths::cache_dir().join("authlib-injector");
    paths::ensure_dir(&dir)?;
    let jar = dir.join("authlib-injector.jar");
    let url = "https://authlib-injector.yushi.moe/artifact/latest.json";
    #[derive(serde::Deserialize)]
    struct Latest {
        download_url: String,
    }
    let latest: Latest = dl
        .get_json_direct(url)
        .context("获取 authlib-injector 版本信息失败")?;
    let bytes = dl
        .get_bytes_direct(&latest.download_url)
        .context("下载 authlib-injector 失败")?;
    if bytes.len() < 100_000 {
        return Err(anyhow!("下载到的 authlib-injector 文件不完整"));
    }
    std::fs::write(&jar, bytes)?;
    log::info!("authlib-injector 已更新: {}", jar.display());
    Ok(jar)
}
#[cfg(test)]
mod tests {
    use super::*;

    /// 语言固定简体中文，其它设置不能被弄丢
    #[test]
    fn chinese_lang_is_written_to_options() {
        let dir = std::env::temp_dir().join("htmcl-lang-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("应能建目录");
        let opts = dir.join("options.txt");
        std::fs::write(&opts, "version:3955\nlang:en_us\nfov:0.0\n").expect("应能写文件");

        ensure_chinese_lang(&dir).expect("应能设置语言");

        let text = std::fs::read_to_string(&opts).expect("应能读文件");
        assert!(text.contains("lang:zh_cn"), "语言应改成简体中文：{text}");
        assert!(!text.contains("en_us"), "不该残留旧语言：{text}");
        // 其它设置要留着
        assert!(text.contains("version:3955"));
        assert!(text.contains("fov:0.0"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 没有 options.txt 时也能建一个
    #[test]
    fn chinese_lang_creates_missing_options() {
        let dir = std::env::temp_dir().join("htmcl-lang-test-new");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("应能建目录");

        ensure_chinese_lang(&dir).expect("应能设置语言");

        let text = std::fs::read_to_string(dir.join("options.txt")).expect("应能读文件");
        assert!(text.contains("lang:zh_cn"), "应写入语言：{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}