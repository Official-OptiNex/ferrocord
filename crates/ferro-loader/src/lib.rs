//! The library loaded into the Discord process.
//!
//! On load it starts a background thread that:
//! 1. sets up logging,
//! 2. loads plugins from the plugins directory,
//! 3. resolves signatures from `signatures.txt` against the client binary,
//! 4. tells plugins the client is ready.
//!
//! Nothing heavy runs inside the load-time constructor itself.

mod logger;

use ferro_core::{memory, paths, HookManager, PluginConfig, SignatureSet};
use ferro_plugin_api::Event;
use ferro_plugin_host::PluginHost;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared state for the lifetime of the process.
pub struct Runtime {
    pub hooks: HookManager,
    pub plugins: PluginHost,
    /// Signature name -> resolved address.
    pub addresses: HashMap<String, usize>,
}

static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();

/// The global runtime, once initialisation has finished.
pub fn runtime() -> Option<&'static Mutex<Runtime>> {
    RUNTIME.get()
}

/// Only activate inside the Discord client. `LD_PRELOAD` is inherited by
/// child processes, so without this check every helper process would load
/// plugins too. Set `FERROCORD_FORCE=1` to override (used in tests).
pub fn should_activate(exe_name: &str) -> bool {
    std::env::var("FERROCORD_FORCE").is_ok_and(|v| v == "1")
        || exe_name.to_ascii_lowercase().contains("discord")
}

#[ctor::ctor(unsafe)]
fn on_load() {
    let Ok(exe) = memory::current_exe_name() else { return };
    if !should_activate(&exe) {
        return;
    }
    let _ = std::thread::Builder::new().name("ferrocord-init".into()).spawn(move || {
        if let Err(e) = init(&exe) {
            log::error!("initialisation failed: {e:#}");
        }
    });
}

fn init(exe: &str) -> anyhow::Result<()> {
    logger::init(&paths::log_file());
    log::info!("ferrocord v{VERSION} starting in `{exe}` (pid {})", std::process::id());
    log::info!("data directory: {}", paths::data_dir().display());

    let mut plugins = PluginHost::new()?;
    let plugins_dir = paths::plugins_dir();
    std::fs::create_dir_all(&plugins_dir)?;
    let config = PluginConfig::load(&paths::plugins_config_file()).unwrap_or_else(|e| {
        log::error!("bad plugins config, treating all as enabled: {e}");
        PluginConfig::default()
    });
    let loaded = plugins.load_all_filtered(&plugins_dir, |id| config.is_enabled(id));
    log::info!("{loaded} plugin(s) loaded from {}", plugins_dir.display());

    let addresses = resolve_signatures(exe);

    let mut rt = Runtime { hooks: HookManager::new(), plugins, addresses };
    rt.plugins.dispatch(&Event::Ready { client_version: None });
    if RUNTIME.set(Mutex::new(rt)).is_err() {
        log::warn!("runtime was already initialised");
    }
    log::info!("ready");
    Ok(())
}

fn resolve_signatures(exe: &str) -> HashMap<String, usize> {
    let path = paths::signatures_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            log::info!("no signatures file at {}; hooks disabled", path.display());
            return HashMap::new();
        }
    };
    let set = match SignatureSet::parse(&text) {
        Ok(s) => s,
        Err(e) => {
            log::error!("bad signatures file: {e}");
            return HashMap::new();
        }
    };
    let regions = match memory::module_regions(exe) {
        Ok(r) => r,
        Err(e) => {
            log::error!("cannot read client memory: {e}");
            return HashMap::new();
        }
    };
    // SAFETY: regions come from the live memory map of this process.
    let results = unsafe { set.resolve(&regions) };
    results
        .into_iter()
        .filter_map(|(name, res)| match res {
            Ok(addr) => {
                log::info!("signature `{name}` -> {addr:#x}");
                Some((name, addr))
            }
            Err(e) => {
                log::warn!("{e}");
                None
            }
        })
        .collect()
}

/// Returns the loader version as a NUL-terminated string. Handy for checking
/// the library is loaded (e.g. with `gdb` or `dlsym`).
#[no_mangle]
pub extern "C" fn ferrocord_version() -> *const std::ffi::c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_check() {
        assert!(should_activate("Discord"));
        assert!(should_activate("discord-canary"));
        assert!(should_activate("DiscordPTB.exe"));
        if std::env::var("FERROCORD_FORCE").is_err() {
            assert!(!should_activate("bash"));
        }
    }
}
