//! Minimal ferrocord plugin. Build with:
//!
//! ```sh
//! cargo build --release --target wasm32-unknown-unknown
//! cp target/wasm32-unknown-unknown/release/hello_plugin.wasm plugin.wasm
//! ferrocord install .
//! ```

mod ferro {
    #[link(wasm_import_module = "ferro")]
    extern "C" {
        #[link_name = "log"]
        fn host_log(level: i32, ptr: *const u8, len: usize);
    }

    pub fn info(msg: &str) {
        unsafe { host_log(3, msg.as_ptr(), msg.len()) }
    }
}

/// Gives the host a buffer to write event JSON into. Freed in `ferro_on_event`.
#[no_mangle]
pub extern "C" fn ferro_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn ferro_init() -> i32 {
    ferro::info("hello from a Rust plugin!");
    0
}

#[no_mangle]
pub extern "C" fn ferro_on_event(ptr: *mut u8, len: usize) {
    // SAFETY: the host wrote `len` bytes into the buffer from `ferro_alloc(len)`.
    let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
    let Ok(event) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return };
    let kind = event["type"].as_str().unwrap_or("unknown");
    ferro::info(&format!("got event `{kind}`"));
}
