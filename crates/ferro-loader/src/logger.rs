//! Minimal logger writing to `ferrocord.log` and stderr.

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct Logger {
    file: Mutex<Option<File>>,
    level: Level,
}

impl Log for Logger {
    fn enabled(&self, meta: &Metadata) -> bool {
        meta.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let line = format!("[{secs}] {:<5} {}: {}\n", record.level(), record.target(), record.args());
        eprint!("ferrocord {line}");
        if let Ok(mut file) = self.file.lock() {
            if let Some(f) = file.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            if let Some(f) = file.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

/// Installs the logger. Level comes from `FERROCORD_LOG` (default `info`).
pub fn init(path: &Path) {
    let level =
        std::env::var("FERROCORD_LOG").ok().and_then(|l| l.parse::<Level>().ok()).unwrap_or(Level::Info);
    let file = path
        .parent()
        .map(std::fs::create_dir_all)
        .and_then(|_| OpenOptions::new().create(true).append(true).open(path).ok());
    let logger = Box::new(Logger { file: Mutex::new(file), level });
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Trace.min(level.to_level_filter()));
    }
}
