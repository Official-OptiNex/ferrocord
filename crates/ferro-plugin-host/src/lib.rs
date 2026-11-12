//! Loads and runs WebAssembly plugins in a sandbox (no filesystem or network
//! access unless the host explicitly exposes it).

use anyhow::{bail, Context, Result};
use ferro_plugin_api::{Event, LogLevel, Manifest, HOST_MODULE, MANIFEST_FILE, WASM_FILE};
use std::path::Path;
use wasmtime::{Caller, Engine, Instance, Linker, Memory, Module, Store, TypedFunc};

/// Fuel given to each call into a plugin, so a stuck plugin can't freeze the client.
const FUEL_PER_CALL: u64 = 50_000_000;

struct PluginState {
    id: String,
}

pub struct Plugin {
    pub manifest: Manifest,
    store: Store<PluginState>,
    #[cfg_attr(not(test), allow(dead_code))]
    instance: Instance,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    on_event: Option<TypedFunc<(i32, i32), ()>>,
}

impl Plugin {
    fn refuel(&mut self) -> Result<()> {
        self.store.set_fuel(FUEL_PER_CALL)?;
        Ok(())
    }

    /// Sends an event to the plugin, if it listens for events.
    pub fn dispatch(&mut self, event: &Event) -> Result<()> {
        let Some(on_event) = self.on_event.clone() else { return Ok(()) };
        let json = serde_json::to_vec(event)?;
        let len = i32::try_from(json.len()).context("event too large")?;
        self.refuel()?;
        let ptr = self.alloc.call(&mut self.store, len)?;
        self.memory
            .write(&mut self.store, ptr as u32 as usize, &json)
            .context("plugin returned an invalid buffer")?;
        on_event.call(&mut self.store, (ptr, len))?;
        Ok(())
    }
}

pub struct PluginHost {
    engine: Engine,
    linker: Linker<PluginState>,
    plugins: Vec<Plugin>,
}

impl PluginHost {
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        linker.func_wrap(
            HOST_MODULE,
            "log",
            |mut caller: Caller<'_, PluginState>, level: i32, ptr: i32, len: i32| {
                let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return;
                };
                let mut buf = vec![0u8; len.max(0) as usize];
                if mem.read(&caller, ptr as u32 as usize, &mut buf).is_err() {
                    return;
                }
                let msg = String::from_utf8_lossy(&buf);
                let id = &caller.data().id;
                let level = match LogLevel::from_i32(level) {
                    LogLevel::Error => log::Level::Error,
                    LogLevel::Warn => log::Level::Warn,
                    LogLevel::Info => log::Level::Info,
                    LogLevel::Debug => log::Level::Debug,
                    LogLevel::Trace => log::Level::Trace,
                };
                log::log!(target: "plugin", level, "[{id}] {msg}");
            },
        )?;
        Ok(Self { engine, linker, plugins: Vec::new() })
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    /// Instantiates a plugin from its manifest and wasm (or wat) bytes.
    pub fn load(&mut self, manifest: Manifest, wasm: &[u8]) -> Result<&Plugin> {
        manifest.validate().map_err(anyhow::Error::msg)?;
        if self.plugins.iter().any(|p| p.manifest.id == manifest.id) {
            bail!("plugin `{}` is already loaded", manifest.id);
        }
        let module = Module::new(&self.engine, wasm)
            .map_err(|e| anyhow::anyhow!("compiling plugin `{}`: {e:#}", manifest.id))?;
        let mut store = Store::new(&self.engine, PluginState { id: manifest.id.clone() });
        store.set_fuel(FUEL_PER_CALL)?;
        let instance: Instance = self.linker.instantiate(&mut store, &module)?;
        let memory = instance.get_memory(&mut store, "memory").context("plugin must export `memory`")?;
        let alloc = instance.get_typed_func::<i32, i32>(&mut store, "ferro_alloc")?;
        let init = instance.get_typed_func::<(), i32>(&mut store, "ferro_init")?;
        let on_event = instance.get_typed_func::<(i32, i32), ()>(&mut store, "ferro_on_event").ok();
        let code = init.call(&mut store, ())?;
        if code != 0 {
            bail!("plugin `{}` failed to initialise (code {code})", manifest.id);
        }
        log::info!("loaded plugin `{}` v{}", manifest.id, manifest.version);
        self.plugins.push(Plugin { manifest, store, instance, memory, alloc, on_event });
        Ok(self.plugins.last().unwrap())
    }

    /// Loads one plugin directory containing `manifest.json` and `plugin.wasm`.
    pub fn load_dir(&mut self, dir: &Path) -> Result<&Plugin> {
        let manifest: Manifest = serde_json::from_slice(
            &std::fs::read(dir.join(MANIFEST_FILE)).with_context(|| format!("reading {}", dir.display()))?,
        )?;
        let wasm = std::fs::read(dir.join(WASM_FILE))?;
        self.load(manifest, &wasm)
    }

    /// Loads every plugin directory under `root`. Failures are logged and
    /// skipped so one broken plugin can't block the rest.
    pub fn load_all(&mut self, root: &Path) -> usize {
        self.load_all_filtered(root, |_| true)
    }

    /// Like [`load_all`](Self::load_all) but skips a directory whose plugin id
    /// (its folder name) fails `enabled`.
    pub fn load_all_filtered(&mut self, root: &Path, enabled: impl Fn(&str) -> bool) -> usize {
        let Ok(entries) = std::fs::read_dir(root) else { return 0 };
        let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        dirs.iter()
            .filter(|dir| {
                let id = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if !enabled(&id) {
                    log::info!("skipping disabled plugin `{id}`");
                    return false;
                }
                match self.load_dir(dir) {
                    Ok(_) => true,
                    Err(e) => {
                        log::error!("skipping plugin at {}: {e:#}", dir.display());
                        false
                    }
                }
            })
            .count()
    }

    /// Sends an event to every plugin. A plugin that traps is unloaded.
    pub fn dispatch(&mut self, event: &Event) {
        self.plugins.retain_mut(|p| match p.dispatch(event) {
            Ok(()) => true,
            Err(e) => {
                log::error!("unloading plugin `{}` after error: {e:#}", p.manifest.id);
                false
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Logs on init; on each event, counts calls and logs the JSON it got.
    const HELLO_WAT: &str = r#"
    (module
      (import "ferro" "log" (func $log (param i32 i32 i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) "hello from wasm")
      (global $events (export "events") (mut i32) (i32.const 0))
      (func (export "ferro_alloc") (param i32) (result i32) (i32.const 1024))
      (func (export "ferro_init") (result i32)
        (call $log (i32.const 3) (i32.const 0) (i32.const 15))
        (i32.const 0))
      (func (export "ferro_on_event") (param $ptr i32) (param $len i32)
        (global.set $events (i32.add (global.get $events) (i32.const 1)))
        (call $log (i32.const 4) (local.get $ptr) (local.get $len))))
    "#;

    fn manifest(id: &str) -> Manifest {
        Manifest {
            id: id.into(),
            name: id.into(),
            version: "0.1.0".into(),
            description: String::new(),
            authors: vec![],
            api_version: 1,
        }
    }

    fn events_seen(host: &mut PluginHost, idx: usize) -> i32 {
        let p = &mut host.plugins[idx];
        let g = p.instance.get_global(&mut p.store, "events").unwrap();
        g.get(&mut p.store).i32().unwrap()
    }

    #[test]
    fn loads_and_dispatches() {
        let mut host = PluginHost::new().unwrap();
        host.load(manifest("hello"), HELLO_WAT.as_bytes()).unwrap();
        assert!(host.load(manifest("hello"), HELLO_WAT.as_bytes()).is_err());
        assert_eq!(events_seen(&mut host, 0), 0);
        host.dispatch(&Event::Ready { client_version: None });
        host.dispatch(&Event::MessageDelete { channel_id: "1".into(), message_id: "2".into() });
        assert_eq!(host.plugins().len(), 1);
        assert_eq!(events_seen(&mut host, 0), 2);
    }

    #[test]
    fn rejects_failed_init_and_unloads_trapping_plugin() {
        let mut host = PluginHost::new().unwrap();
        let failing = r#"(module (memory (export "memory") 1)
            (func (export "ferro_alloc") (param i32) (result i32) (i32.const 0))
            (func (export "ferro_init") (result i32) (i32.const 7)))"#;
        assert!(host.load(manifest("failing"), failing.as_bytes()).is_err());

        let looping = r#"(module (memory (export "memory") 1)
            (func (export "ferro_alloc") (param i32) (result i32) (i32.const 0))
            (func (export "ferro_init") (result i32) (i32.const 0))
            (func (export "ferro_on_event") (param i32 i32) (loop (br 0))))"#;
        host.load(manifest("looping"), looping.as_bytes()).unwrap();
        host.dispatch(&Event::Ready { client_version: None });
        assert!(host.plugins().is_empty(), "fuel limit should stop the infinite loop");
    }

    #[test]
    fn load_all_skips_broken_dirs() {
        let root = tempfile::tempdir().unwrap();
        let good = root.path().join("a-good");
        std::fs::create_dir(&good).unwrap();
        std::fs::write(good.join(MANIFEST_FILE), serde_json::to_vec(&manifest("good")).unwrap()).unwrap();
        std::fs::write(good.join(WASM_FILE), HELLO_WAT).unwrap();
        std::fs::create_dir(root.path().join("b-broken")).unwrap();
        let mut host = PluginHost::new().unwrap();
        assert_eq!(host.load_all(root.path()), 1);
    }
}
