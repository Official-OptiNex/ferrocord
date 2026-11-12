//! Helpers the [`crate::export_plugin!`] macro calls from the generated
//! `ferro_alloc`, `ferro_init` and `ferro_on_event` entry points.

use crate::{Context, Event, Plugin};

/// Backs `ferro_alloc`. Negative lengths (which can't come from a valid host)
/// yield a null pointer, reported as `0`.
pub fn alloc(len: i32) -> i32 {
    match usize::try_from(len) {
        Ok(len) => crate::raw::alloc(len) as i32,
        Err(_) => 0,
    }
}

/// Backs `ferro_init`. Returns `0` on success, `1` on failure.
pub fn init<P: Plugin>(slot: &mut Option<P>) -> i32 {
    let mut cx = Context::new();
    match P::init(&mut cx) {
        Ok(plugin) => {
            *slot = Some(plugin);
            0
        }
        Err(e) => {
            crate::error!("plugin init failed: {e:#}");
            1
        }
    }
}

/// Backs `ferro_on_event`: reclaims the buffer, decodes the event and hands it
/// to the plugin. Events this SDK version doesn't recognise are ignored.
pub fn on_event<P: Plugin>(slot: &mut Option<P>, ptr: i32, len: i32) {
    let Ok(len) = usize::try_from(len) else { return };
    if ptr == 0 {
        return;
    }
    // SAFETY: the host passes back a (ptr, len) from our own `ferro_alloc`.
    let bytes = unsafe { crate::raw::take(ptr as *mut u8, len) };
    dispatch(slot, &bytes);
}

/// Decodes one event and hands it to the plugin. Split out from [`on_event`]
/// so it can be unit-tested without going through the 32-bit pointer ABI.
pub(crate) fn dispatch<P: Plugin>(slot: &mut Option<P>, bytes: &[u8]) {
    let Some(plugin) = slot.as_mut() else {
        crate::warn!("event arrived before init");
        return;
    };
    match serde_json::from_slice::<Event>(bytes) {
        Ok(event) => {
            let mut cx = Context::new();
            plugin.on_event(&mut cx, event);
        }
        Err(e) => crate::trace!("ignoring unrecognised event: {e}"),
    }
}

/// Defines the WebAssembly entry points for a [`Plugin`] type.
///
/// Call once at the crate root:
///
/// ```ignore
/// ferrocord_sdk::export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($ty:ty) => {
        thread_local! {
            static __FERRO_PLUGIN: ::core::cell::RefCell<::core::option::Option<$ty>> =
                ::core::cell::RefCell::new(::core::option::Option::None);
        }

        /// Host-called allocator for event buffers.
        #[no_mangle]
        pub extern "C" fn ferro_alloc(len: i32) -> i32 {
            $crate::export::alloc(len)
        }

        #[no_mangle]
        pub extern "C" fn ferro_init() -> i32 {
            __FERRO_PLUGIN.with(|slot| $crate::export::init::<$ty>(&mut slot.borrow_mut()))
        }

        #[no_mangle]
        pub extern "C" fn ferro_on_event(ptr: i32, len: i32) {
            __FERRO_PLUGIN.with(|slot| $crate::export::on_event::<$ty>(&mut slot.borrow_mut(), ptr, len))
        }
    };
}
