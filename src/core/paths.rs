use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static EXE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 启动器所在目录。所有配置、账号、主题、日志、游戏数据都以此目录为基准，
/// 保证「单文件 + 同目录配置文件」的形态。
pub fn exe_dir() -> PathBuf {
    EXE_DIR
        .get_or_init(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .clone()
}

/// 启动器数据目录：exe 同目录下的 `htmcl/`
pub fn launcher_dir() -> PathBuf {
    exe_dir().join("htmcl")
}

pub fn config_file() -> PathBuf {
    launcher_dir().join("htmcl.json")
}

pub fn accounts_file() -> PathBuf {
    launcher_dir().join("accounts.json")
}

pub fn themes_dir() -> PathBuf {
    launcher_dir().join("themes")
}

pub fn logs_dir() -> PathBuf {
    launcher_dir().join("logs")
}

/// 启动器自身的缓存（authlib-injector、皮肤、图片、安装器等）
pub fn cache_dir() -> PathBuf {
    launcher_dir().join("cache")
}

pub fn default_game_dir() -> PathBuf {
    exe_dir().join(".minecraft")
}

pub fn versions_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("versions")
}

pub fn libraries_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("libraries")
}

pub fn assets_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("assets")
}

pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(path)
}

pub fn ensure_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)
    } else {
        Ok(())
    }
}

/// 在资源管理器中打开目录或选中文件
pub fn reveal(path: &Path) {
    if path.is_file() {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer")
                .arg(format!("/select,{}", path.display()))
                .spawn();
            return;
        }
        #[cfg(not(windows))]
        {
            if let Some(p) = path.parent() {
                let _ = open::that(p);
            }
            return;
        }
    }
    let _ = open::that(path);
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    total
}

pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// 校验 Minecraft 用户名：3-16 位，仅字母数字下划线
pub fn validate_player_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("用户名不能为空".into());
    }
    if !(3..=16).contains(&name.chars().count()) {
        return Err("用户名长度需为 3-16 个字符".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("用户名只能包含字母、数字和下划线".into());
    }
    Ok(())
}

/// 旧版本布局（配置直接放在 exe 同目录）迁移到 `htmcl/` 目录。
/// 只在目标不存在时移动，避免覆盖新数据。
pub fn migrate_legacy_layout() {
    let root = exe_dir();
    let target = launcher_dir();
    if root == target {
        return;
    }
    let _ = ensure_dir(&target);

    let files = [
        ("htmcl.json", target.join("htmcl.json")),
        ("accounts.json", target.join("accounts.json")),
        ("htmcl.json.bak", target.join("htmcl.json.bak")),
    ];
    for (name, dest) in files {
        let src = root.join(name);
        if src.is_file() && !dest.exists() {
            if std::fs::rename(&src, &dest).is_err() {
                let _ = std::fs::copy(&src, &dest);
                let _ = std::fs::remove_file(&src);
            }
            log::info!("已迁移 {} -> {}", src.display(), dest.display());
        }
    }

    let dirs = [
        ("themes", target.join("themes")),
        ("logs", target.join("logs")),
        (".htmcl", target.join("cache")),
    ];
    for (name, dest) in dirs {
        let src = root.join(name);
        if src.is_dir() && !dest.exists() {
            if std::fs::rename(&src, &dest).is_err() {
                copy_dir(&src, &dest);
                let _ = std::fs::remove_dir_all(&src);
            }
            log::info!("已迁移目录 {} -> {}", src.display(), dest.display());
        }
    }
}

fn copy_dir(src: &std::path::Path, dest: &std::path::Path) {
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let relative = path.strip_prefix(src).unwrap_or(path);
        let out = dest.join(relative);
        if entry.file_type().is_dir() {
            let _ = ensure_dir(&out);
        } else {
            let _ = ensure_parent(&out);
            let _ = std::fs::copy(path, &out);
        }
    }
}
