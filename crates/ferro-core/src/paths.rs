//! Where ferrocord keeps plugins, signatures and logs.
//!
//! Override everything with the `FERROCORD_DIR` environment variable.

use std::env;
use std::path::PathBuf;

pub const ENV_DIR: &str = "FERROCORD_DIR";

/// Root data directory:
/// - `$FERROCORD_DIR` if set
/// - Linux: `$XDG_DATA_HOME/ferrocord` or `~/.local/share/ferrocord`
/// - macOS: `~/Library/Application Support/ferrocord`
/// - Windows: `%APPDATA%\ferrocord`
pub fn data_dir() -> PathBuf {
    if let Some(dir) = env::var_os(ENV_DIR).filter(|d| !d.is_empty()) {
        return dir.into();
    }
    platform_dir().join("ferrocord")
}

#[cfg(windows)]
fn platform_dir() -> PathBuf {
    env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn platform_dir() -> PathBuf {
    home().join("Library/Application Support")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
}

#[cfg(unix)]
fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn plugins_dir() -> PathBuf {
    data_dir().join("plugins")
}

pub fn signatures_file() -> PathBuf {
    data_dir().join("signatures.txt")
}

pub fn log_file() -> PathBuf {
    data_dir().join("ferrocord.log")
}
