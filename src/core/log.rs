use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::Local;

use crate::core::paths;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub time: String,
    pub level: log::Level,
    pub target: String,
    pub message: String,
}

static RECENT: OnceLock<Mutex<VecDeque<LogLine>>> = OnceLock::new();
const MAX_RECENT: usize = 500;

fn recent_buf() -> &'static Mutex<VecDeque<LogLine>> {
    RECENT.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_RECENT)))
}

struct LauncherLogger {
    file: Mutex<Option<File>>,
}

impl log::Log for LauncherLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let now = Local::now();
        let line = LogLine {
            time: now.format("%H:%M:%S").to_string(),
            level: record.level(),
            target: record.target().to_string(),
            message: record.args().to_string(),
        };

        if let Ok(mut buf) = recent_buf().lock() {
            if buf.len() >= MAX_RECENT {
                buf.pop_front();
            }
            buf.push_back(line.clone());
        }

        if let Ok(mut guard) = self.file.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = writeln!(
                    file,
                    "{} [{:<5}] {}: {}",
                    now.format("%Y-%m-%d %H:%M:%S"),
                    line.level.as_str(),
                    line.target,
                    line.message
                );
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

pub fn log_file_path() -> PathBuf {
    paths::logs_dir().join(format!("htmcl-{}.log", Local::now().format("%Y-%m-%d")))
}

pub fn init() {
    let file = paths::ensure_dir(&paths::logs_dir())
        .ok()
        .and_then(|_| File::options().create(true).append(true).open(log_file_path()).ok());

    let logger = LauncherLogger {
        file: Mutex::new(file),
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    log::info!("===== htmcl {} 启动 =====", env!("CARGO_PKG_VERSION"));
}

pub fn recent(limit: usize) -> Vec<LogLine> {
    recent_buf()
        .lock()
        .map(|buf| buf.iter().rev().take(limit).rev().cloned().collect())
        .unwrap_or_default()
}

pub fn clear_recent() {
    if let Ok(mut buf) = recent_buf().lock() {
        buf.clear();
    }
}

pub fn level_color(level: log::Level) -> egui::Color32 {
    match level {
        log::Level::Error => egui::Color32::from_rgb(0xD0, 0x45, 0x45),
        log::Level::Warn => egui::Color32::from_rgb(0xC8, 0x85, 0x1E),
        log::Level::Info => egui::Color32::from_rgb(0x35, 0x7A, 0x4F),
        log::Level::Debug | log::Level::Trace => egui::Color32::from_rgb(0x7A, 0x82, 0x8C),
    }
}
