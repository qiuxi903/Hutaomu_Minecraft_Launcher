use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaInfo {
    pub path: PathBuf,
    pub version: String,
    pub major: u32,
    pub vendor: String,
    pub bits: u32,
    /// 运行环境名称，例如 "OpenJDK 64-Bit Server VM"
    pub runtime: String,
}

impl JavaInfo {
    pub fn display_name(&self) -> String {
        format!("Java {} ({})", self.version, self.vendor)
    }

    pub fn short(&self) -> String {
        format!("Java {} · {}-bit · {}", self.version, self.bits, self.vendor)
    }
}

#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_cmd: &mut Command) {}

/// 并行探测多个候选路径，显著缩短搜索时间
pub fn probe_many(candidates: Vec<PathBuf>) -> Vec<JavaInfo> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let queue = std::sync::Arc::new(std::sync::Mutex::new(candidates));
    let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let threads = num_cpus::get().clamp(2, 8).min(8);

    std::thread::scope(|scope| {
        for _ in 0..threads {
            let queue = queue.clone();
            let results = results.clone();
            scope.spawn(move || loop {
                let path = {
                    let mut guard = queue.lock().unwrap();
                    guard.pop()
                };
                let Some(path) = path else { break };
                if let Some(info) = probe(&path) {
                    if let Ok(mut list) = results.lock() {
                        list.push(info);
                    }
                }
            });
        }
    });

    let mut list = results.lock().map(|l| l.clone()).unwrap_or_default();
    sort_javas(&mut list);
    list
}

/// 运行 `java -version` 并解析结果，失败返回 None
pub fn probe(path: &Path) -> Option<JavaInfo> {
    if !path.is_file() {
        return None;
    }
    let mut cmd = Command::new(path);
    cmd.arg("-version");
    hide_console(&mut cmd);
    let output = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&output.stderr).to_string();
    let text = if text.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        text
    };
    parse_version_output(path, &text)
}

pub fn parse_version_output(path: &Path, text: &str) -> Option<JavaInfo> {
    let mut version = String::new();
    let mut runtime = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(start) = line.find('"') {
            if let Some(end) = line[start + 1..].find('"') {
                let candidate = &line[start + 1..start + 1 + end];
                if candidate.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    version = candidate.to_string();
                }
            }
        }
        if line.contains("VM") && runtime.is_empty() {
            runtime = line.to_string();
        }
    }

    if version.is_empty() {
        return None;
    }

    let major = parse_major(&version)?;
    let bits = if runtime.contains("64-Bit") || text.contains("64-Bit") {
        64
    } else if text.contains("32-Bit") {
        32
    } else {
        64
    };

    Some(JavaInfo {
        path: path.to_path_buf(),
        version,
        major,
        vendor: detect_vendor(text),
        bits,
        runtime,
    })
}

fn parse_major(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '_', '-']);
    let first = parts.next()?;
    let first_num: u32 = first.parse().ok()?;
    if first_num == 1 {
        // 1.8.0_402 形式
        parts.next()?.parse().ok()
    } else {
        Some(first_num)
    }
}

fn detect_vendor(text: &str) -> String {
    let lower = text.to_lowercase();
    let table: [(&str, &str); 14] = [
        ("zulu", "Azul Zulu"),
        ("temurin", "Eclipse Temurin"),
        ("adoptopenjdk", "AdoptOpenJDK"),
        ("adoptium", "Eclipse Temurin"),
        ("microsoft", "Microsoft"),
        ("graalvm", "GraalVM"),
        ("corretto", "Amazon Corretto"),
        ("semeru", "IBM Semeru"),
        ("openj9", "IBM Semeru"),
        ("dragonwell", "Alibaba Dragonwell"),
        ("bisheng", "Huawei BiSheng"),
        ("tencent", "Tencent Kona"),
        ("oracle", "Oracle"),
        ("liberica", "BellSoft Liberica"),
    ];
    for (needle, name) in table {
        if lower.contains(needle) {
            return name.to_string();
        }
    }
    if lower.contains("openjdk") {
        "OpenJDK".into()
    } else {
        "未知".into()
    }
}

/// 物理内存总量（MB）
pub fn total_memory_mb() -> u64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
        if ok != 0 {
            return status.ullTotalPhys / 1024 / 1024;
        }
    }
    0
}

fn push_unique(list: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidate.is_file() {
        return;
    }
    let normalized = std::fs::canonicalize(&candidate).unwrap_or(candidate);
    if !list.iter().any(|p| {
        std::fs::canonicalize(p)
            .map(|c| c == normalized)
            .unwrap_or(false)
    }) {
        list.push(normalized);
    }
}

fn java_exe_in(dir: &Path) -> Option<PathBuf> {
    let exe = dir.join("bin").join("java.exe");
    if exe.is_file() {
        Some(exe)
    } else {
        let exe = dir.join("bin").join("java");
        exe.is_file().then_some(exe)
    }
}

fn scan_children(parent: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(exe) = java_exe_in(&path) {
            push_unique(out, exe);
            continue;
        }
        if depth > 0 {
            scan_children(&path, out, depth - 1);
        }
    }
}

/// 简要搜索：注册表 + 常见安装目录 + PATH + JAVA_HOME + 官方启动器自带运行时
pub fn brief_search() -> Vec<JavaInfo> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    for home in registry_java_homes() {
        if let Some(exe) = java_exe_in(&home) {
            push_unique(&mut candidates, exe);
        }
    }

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        if let Some(exe) = java_exe_in(Path::new(&java_home)) {
            push_unique(&mut candidates, exe);
        }
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for entry in std::env::split_paths(&path_var) {
            let exe = entry.join("java.exe");
            if exe.is_file() {
                push_unique(&mut candidates, exe);
            }
        }
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Ok(base) = std::env::var(var) {
            let base = PathBuf::from(base);
            for sub in [
                "Java",
                "Eclipse Adoptium",
                "Eclipse Foundation",
                "Microsoft",
                "Zulu",
                "BellSoft",
                "Amazon Corretto",
                "Semeru",
                "IBM",
                "GraalVM",
                "Common Files\\Oracle\\Java",
                "Android\\Android Studio\\jbr",
            ] {
                roots.push(base.join(sub));
            }
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        roots.push(local.join("Programs\\Eclipse Adoptium"));
        roots.push(local.join("Programs\\Microsoft"));
        roots.push(local.join("Programs\\Java"));
        roots.push(local.join("Programs\\Zulu"));
        roots.push(local.join("Packages\\Microsoft.4297127D64EC6_8wekyb3d8bbwe\\LocalCache\\Local\\runtime"));
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let profile = PathBuf::from(profile);
        roots.push(profile.join(".jdks"));
        roots.push(profile.join("scoop\\apps\\openjdk"));
        roots.push(profile.join("scoop\\apps\\temurin-jdk"));
        roots.push(profile.join(".minecraft\\runtime"));
    }

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        if let Some(exe) = java_exe_in(&root) {
            push_unique(&mut candidates, exe);
            continue;
        }
        scan_children(&root, &mut candidates, 2);
    }

    probe_many(candidates)
}

fn registry_java_homes() -> Vec<PathBuf> {
    let mut homes = Vec::new();
    #[cfg(windows)]
    {
        let queries = [
            ("HKLM\\SOFTWARE\\JavaSoft\\Java Development Kit", "/reg:64"),
            ("HKLM\\SOFTWARE\\JavaSoft\\Java Runtime Environment", "/reg:64"),
            ("HKLM\\SOFTWARE\\JavaSoft\\Java Development Kit", "/reg:32"),
            ("HKLM\\SOFTWARE\\JavaSoft\\Java Runtime Environment", "/reg:32"),
            ("HKLM\\SOFTWARE\\WOW6432Node\\JavaSoft\\Java Development Kit", "/reg:64"),
            ("HKLM\\SOFTWARE\\WOW6432Node\\JavaSoft\\Java Runtime Environment", "/reg:64"),
        ];
        for (key, reg) in queries {
            let mut cmd = Command::new("reg");
            cmd.args(["query", key, "/s", reg]);
            hide_console(&mut cmd);
            let Ok(output) = cmd.output() else { continue };
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if !trimmed.starts_with("JavaHome") {
                    continue;
                }
                if let Some(idx) = trimmed.find("REG_SZ") {
                    let value = trimmed[idx + 6..].trim();
                    if !value.is_empty() {
                        homes.push(PathBuf::from(value));
                    }
                }
            }
        }
    }
    homes
}

/// 详细搜索：遍历所有固定磁盘查找 java.exe
pub fn full_search(cancel: &AtomicBool, mut progress: impl FnMut(String)) -> Vec<JavaInfo> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut brief = brief_search();
    brief.clear();

    let drives = available_drives();
    for drive in drives {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        progress(format!("正在扫描 {} ...", drive.display()));
        let walker = walkdir::WalkDir::new(&drive)
            .max_depth(9)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| !is_skipped_dir(entry.path()));
        for entry in walker.filter_map(|e| e.ok()) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name != "java.exe" && name != "java" {
                continue;
            }
            let path = entry.path();
            let is_bin = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.eq_ignore_ascii_case("bin"))
                .unwrap_or(false);
            if !is_bin {
                continue;
            }
            push_unique(&mut candidates, path.to_path_buf());
        }
    }

    progress("正在校验 Java 版本 ...".into());
    if cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    probe_many(candidates)
}

fn available_drives() -> Vec<PathBuf> {
    let mut drives = Vec::new();
    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            let path = PathBuf::from(&root);
            if path.is_dir() {
                drives.push(path);
            }
        }
    }
    #[cfg(not(windows))]
    {
        drives.push(PathBuf::from("/"));
    }
    drives
}

fn is_skipped_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_lowercase();
    const SKIP: [&str; 22] = [
        "windows",
        "$recycle.bin",
        "system volume information",
        "node_modules",
        "winsxs",
        "servicing",
        "assembly",
        "installer",
        "temp",
        "tmp",
        "cache",
        "caches",
        "onedrivetemp",
        "packages",
        "driverstore",
        "softwaredistribution",
        "appdata\\local\\temp",
        ".git",
        "target",
        "proc",
        "sys",
        "dev",
    ];
    SKIP.contains(&lower.as_str())
}

pub fn sort_javas(list: &mut [JavaInfo]) {
    list.sort_by(|a, b| {
        b.bits
            .cmp(&a.bits)
            .then_with(|| a.major.cmp(&b.major))
            .then_with(|| a.path.cmp(&b.path))
    });
}

/// 该 Minecraft 版本建议的 Java 主版本
///
/// 这只是没有版本 JSON 时的兜底估算，
/// 真正的要求以版本 JSON 里的 `javaVersion.majorVersion` 为准。
pub fn required_major(mc_version: &str) -> u32 {
    let parts: Vec<u32> = mc_version
        .split(['.', '-'])
        .take(3)
        .filter_map(|p| p.parse().ok())
        .collect();
    if parts.is_empty() {
        return 8;
    }
    // 2026 起的 26.x 版本号方案对应 Java 25，
    // 这些版本会带上 --sun-misc-unsafe-memory-access 这类新参数，
    // 用低版本 Java 启动会直接报「Unrecognized option」。
    if parts[0] != 1 {
        return 25;
    }
    if parts.len() < 2 {
        return 8;
    }
    let minor = parts[1];
    let patch = parts.get(2).copied().unwrap_or(0);
    if minor >= 21 {
        21
    } else if minor == 20 && patch >= 5 {
        21
    } else if minor >= 18 {
        17
    } else if minor == 17 {
        16
    } else {
        8
    }
}

/// 从候选列表中挑选满足最低主版本要求的 Java
pub fn pick_for_major(required: u32, javas: &[JavaInfo]) -> Option<JavaInfo> {
    if javas.is_empty() {
        return None;
    }
    let mut best: Option<&JavaInfo> = None;
    for java in javas {
        if java.major < required {
            continue;
        }
        best = match best {
            None => Some(java),
            Some(current) => {
                let better = java.major < current.major
                    || (java.major == current.major && java.bits > current.bits);
                if better {
                    Some(java)
                } else {
                    Some(current)
                }
            }
        };
    }
    best.or_else(|| javas.iter().max_by_key(|j| (j.bits, j.major)))
        .cloned()
}

/// 从候选列表中挑选最合适的 Java
pub fn pick_for(mc_version: &str, javas: &[JavaInfo]) -> Option<JavaInfo> {
    pick_for_major(required_major(mc_version), javas)
}

/// 按最低主版本要求挑选 Java，手动指定的路径优先
///
/// 手动指定的路径最优先：用户明确指定过就不该被自动选择覆盖。
pub fn pick_for_version(
    required: u32,
    manual_path: Option<&std::path::Path>,
    javas: &[JavaInfo],
) -> Option<JavaInfo> {
    if let Some(path) = manual_path {
        if let Some(found) = javas.iter().find(|j| j.path == path) {
            return Some(found.clone());
        }
        if let Some(probed) = probe(path) {
            return Some(probed);
        }
    }
    pick_for_major(required, javas)
}
