//! The host-side imports and the allocator the host calls into.
//!
//! Plugin authors use [`crate::log`] instead of calling these directly.

use alloc::vec::Vec;
use core::alloc::Layout;

extern crate alloc;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "ferro")]
extern "C" {
    #[link_name = "log"]
    fn host_log(level: i32, ptr: *const u8, len: usize);
}

/// Sends a log line to the host. `level` is a [`crate::LogLevel`] value.
#[cfg(target_arch = "wasm32")]
pub fn log(level: i32, message: &str) {
    // SAFETY: `host_log` only reads `len` bytes starting at `ptr`.
    unsafe { host_log(level, message.as_ptr(), message.len()) }
}

/// Off-wasm there is no host, so logs are captured instead. This lets plugin
/// authors (and this crate) unit-test plugin logic with plain `cargo test`.
#[cfg(not(target_arch = "wasm32"))]
pub fn log(level: i32, message: &str) {
    captured::push(level, message);
}

/// Native-only capture of log lines emitted through [`log`].
#[cfg(not(target_arch = "wasm32"))]
pub mod captured {
    use std::cell::RefCell;

    thread_local! {
        static LINES: RefCell<Vec<(i32, String)>> = const { RefCell::new(Vec::new()) };
    }

    pub(crate) fn push(level: i32, message: &str) {
        LINES.with(|l| l.borrow_mut().push((level, message.to_owned())));
    }

    /// Takes and clears the captured `(level, message)` lines.
    pub fn take() -> Vec<(i32, String)> {
        LINES.with(|l| core::mem::take(&mut *l.borrow_mut()))
    }
}

/// Allocation header: we store the layout size just before the returned
/// pointer so the matching free can rebuild the `Layout`.
const HEADER: usize = core::mem::size_of::<usize>();

/// Called by the host to get a buffer it can write `len` bytes into. The
/// buffer is reclaimed in [`crate::export::on_event`] after the event is read.
///
/// # Safety
/// Returns a pointer to at least `len` writable bytes, or null on overflow.
pub fn alloc(len: usize) -> *mut u8 {
    let Some(total) = len.checked_add(HEADER) else { return core::ptr::null_mut() };
    let Ok(layout) = Layout::from_size_align(total, core::mem::align_of::<usize>()) else {
        return core::ptr::null_mut();
    };
    // SAFETY: layout has non-zero size (HEADER > 0).
    let base = unsafe { alloc::alloc::alloc(layout) };
    if base.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `base` has room for the header plus `len` bytes.
    unsafe {
        (base as *mut usize).write(len);
        base.add(HEADER)
    }
}

/// Reclaims a buffer from [`alloc`] and copies its contents out.
///
/// # Safety
/// `ptr` must be a pointer returned by [`alloc`] and not yet freed, and `len`
/// must be the length that was requested.
pub unsafe fn take(ptr: *mut u8, len: usize) -> Vec<u8> {
    let base = ptr.sub(HEADER);
    let stored = (base as *const usize).read();
    debug_assert_eq!(stored, len, "host freed a buffer with the wrong length");
    let out = core::slice::from_raw_parts(ptr, len).to_vec();
    let layout = Layout::from_size_align_unchecked(len + HEADER, core::mem::align_of::<usize>());
    alloc::alloc::dealloc(base, layout);
    out
}
