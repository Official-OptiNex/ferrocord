//! The stable contract between ferrocord and plugins.
//!
//! Plugins are WebAssembly modules, so they keep working across Discord
//! updates as long as the host can still find its hooks. Data crosses the
//! boundary as JSON.
//!
//! # ABI (version [`API_VERSION`])
//!
//! A plugin module **exports**:
//! - `memory`
//! - `ferro_alloc(len: i32) -> i32`: returns a buffer the host can write into
//! - `ferro_init() -> i32`: called once after loading; non-zero means failure
//! - `ferro_on_event(ptr: i32, len: i32)` *(optional)*: receives an [`Event`] as JSON
//!
//! A plugin may **import** from module `"ferro"`:
//! - `log(level: i32, ptr: i32, len: i32)`: UTF-8 message; level is [`LogLevel`]
//!
//! A plugin directory contains `manifest.json` ([`Manifest`]) and `plugin.wasm`.

use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;
pub const HOST_MODULE: &str = "ferro";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const WASM_FILE: &str = "plugin.wasm";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Unique, lowercase, e.g. `"message-logger"`.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// Lowest [`API_VERSION`] this plugin works with.
    #[serde(default = "default_api")]
    pub api_version: u32,
}

fn default_api() -> u32 {
    API_VERSION
}

impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        let id_ok = !self.id.is_empty()
            && self.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !id_ok {
            return Err(format!("invalid plugin id `{}` (use a-z, 0-9, -)", self.id));
        }
        if self.api_version > API_VERSION {
            return Err(format!(
                "plugin `{}` needs API v{}, host provides v{API_VERSION}",
                self.id, self.api_version
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl LogLevel {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Error,
            2 => Self::Warn,
            3 => Self::Info,
            4 => Self::Debug,
            _ => Self::Trace,
        }
    }
}

/// Events delivered to plugins. Variants will grow as hooks into the native
/// client are found; unknown variants should be ignored by plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Event {
    /// The client finished starting and hooks are installed.
    Ready {
        client_version: Option<String>,
    },
    MessageCreate {
        channel_id: String,
        message_id: String,
        author_id: String,
        content: String,
    },
    MessageDelete {
        channel_id: String,
        message_id: String,
    },
    /// Fallback for experimental hooks: arbitrary name + JSON payload.
    Custom {
        name: String,
        payload: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip_and_validation() {
        let m: Manifest = serde_json::from_str(r#"{"id":"hello","name":"Hello","version":"0.1.0"}"#).unwrap();
        assert_eq!(m.api_version, API_VERSION);
        assert!(m.validate().is_ok());
        let bad = Manifest { id: "Bad Id".into(), ..m.clone() };
        assert!(bad.validate().is_err());
        let future = Manifest { api_version: API_VERSION + 1, ..m };
        assert!(future.validate().is_err());
    }

    #[test]
    fn event_json_shape() {
        let e = Event::MessageDelete { channel_id: "1".into(), message_id: "2".into() };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"type":"message_delete","channel_id":"1","message_id":"2"}"#
        );
    }
}
