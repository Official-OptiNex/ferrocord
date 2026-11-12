//! SDK for writing [ferrocord](https://github.com/Official-OptiNex/ferrocord) plugins.
//!
//! It hides the raw WebAssembly ABI (manual allocation, pointer passing, JSON
//! decoding) behind a small [`Plugin`] trait and the [`export_plugin!`] macro.
//!
//! # Example
//!
//! ```ignore
//! use ferrocord_sdk::{export_plugin, log, Context, Event, Plugin};
//!
//! struct Greeter {
//!     seen: u32,
//! }
//!
//! impl Plugin for Greeter {
//!     fn init(_cx: &mut Context) -> anyhow::Result<Self> {
//!         log::info!("greeter starting");
//!         Ok(Self { seen: 0 })
//!     }
//!
//!     fn on_event(&mut self, _cx: &mut Context, event: Event) {
//!         self.seen += 1;
//!         if let Event::MessageCreate { content, .. } = event {
//!             log::info!("message #{}: {content}", self.seen);
//!         }
//!     }
//! }
//!
//! export_plugin!(Greeter);
//! ```
//!
//! Build with `cargo build --release --target wasm32-unknown-unknown` and copy
//! the resulting `.wasm` next to a `manifest.json`.

pub use ferro_plugin_api::{Event, LogLevel, API_VERSION};

pub use anyhow;

pub mod log;

#[doc(hidden)]
pub mod raw;

#[doc(hidden)]
pub mod export;

/// Services the host offers a plugin during a call. Reserved for future use
/// (settings, sending messages back to the client); passed to every method so
/// those can be added without changing plugin signatures.
#[non_exhaustive]
pub struct Context {
    _private: (),
}

impl Context {
    #[doc(hidden)]
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// A ferrocord plugin.
///
/// Implement this and pass the type to [`export_plugin!`] to generate the
/// WebAssembly entry points.
pub trait Plugin: Sized + 'static {
    /// Called once when the plugin loads. Returning `Err` aborts loading and
    /// the error is logged by the host.
    fn init(cx: &mut Context) -> anyhow::Result<Self>;

    /// Called for every [`Event`] the host sends. The default ignores them.
    fn on_event(&mut self, cx: &mut Context, event: Event) {
        let _ = (cx, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_take_roundtrip() {
        let data = b"ferrocord";
        let ptr = raw::alloc(data.len());
        assert!(!ptr.is_null());
        // SAFETY: ptr has room for data.len() bytes.
        unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len()) };
        let out = unsafe { raw::take(ptr, data.len()) };
        assert_eq!(out, data);
    }

    #[test]
    fn alloc_rejects_bad_length() {
        assert_eq!(export::alloc(-1), 0);
    }

    struct Spy {
        count: u32,
    }

    impl Plugin for Spy {
        fn init(_cx: &mut Context) -> anyhow::Result<Self> {
            log::info!("init");
            Ok(Self { count: 0 })
        }

        fn on_event(&mut self, _cx: &mut Context, event: Event) {
            self.count += 1;
            log::info!("event {} is {:?}", self.count, event);
        }
    }

    struct Failing;

    impl Plugin for Failing {
        fn init(_cx: &mut Context) -> anyhow::Result<Self> {
            anyhow::bail!("nope")
        }
    }

    #[test]
    fn init_and_dispatch_roundtrip() {
        let _ = raw::captured::take();
        let mut slot: Option<Spy> = None;
        assert_eq!(export::init(&mut slot), 0);

        export::dispatch(&mut slot, br#"{"type":"ready","client_version":null}"#);
        assert_eq!(slot.as_ref().unwrap().count, 1);

        // An event this SDK doesn't recognise is ignored, not a crash.
        export::dispatch(&mut slot, br#"{"type":"from_the_future","x":1}"#);
        assert_eq!(slot.as_ref().unwrap().count, 1);

        let logs = raw::captured::take();
        assert!(logs.iter().any(|(_, m)| m == "init"));
        assert!(logs.iter().any(|(_, m)| m.contains("Ready")));
    }

    #[test]
    fn failed_init_reports_error() {
        let mut slot: Option<Failing> = None;
        assert_eq!(export::init(&mut slot), 1);
        assert!(slot.is_none());
    }
}
