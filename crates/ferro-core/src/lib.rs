//! Core runtime for ferrocord: finding code in the target process and hooking it.
//!
//! The native Discord client ships without symbols, so functions are located by
//! byte signatures (see [`scanner`]) inside the executable's memory (see
//! [`memory`]) and then detoured via [`hooks`].

pub mod config;
pub mod hooks;
pub mod memory;
pub mod paths;
pub mod scanner;
pub mod signatures;

pub use config::PluginConfig;
pub use hooks::HookManager;
pub use scanner::Pattern;
pub use signatures::{Signature, SignatureSet};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("invalid PE image: {0}")]
    InvalidPe(String),
    #[error("module not found: {0}")]
    ModuleNotFound(String),
    #[error("signature `{0}` not found")]
    SignatureNotFound(String),
    #[error("signature `{name}` is ambiguous ({count} matches)")]
    AmbiguousSignature { name: String, count: usize },
    #[error("hook `{0}` already installed")]
    HookExists(String),
    #[error("hook `{0}` not installed")]
    HookMissing(String),
    #[error("detour error: {0}")]
    Detour(#[from] retour::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported on this platform: {0}")]
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
