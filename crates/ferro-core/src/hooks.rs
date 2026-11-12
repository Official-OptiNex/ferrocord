//! Named function detours backed by `retour`.

use crate::{Error, Result};
use retour::RawDetour;
use std::collections::HashMap;

/// Owns every installed detour so they can be listed, toggled and removed.
#[derive(Default)]
pub struct HookManager {
    hooks: HashMap<String, RawDetour>,
}

impl HookManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Detours `target` to `detour` and enables it. Returns a pointer to a
    /// trampoline that calls the original function.
    ///
    /// # Safety
    /// Both pointers must be functions with identical signatures and calling
    /// conventions, and no thread may be executing `target`'s prologue.
    pub unsafe fn install(&mut self, name: &str, target: *const (), detour: *const ()) -> Result<*const ()> {
        if self.hooks.contains_key(name) {
            return Err(Error::HookExists(name.to_owned()));
        }
        let hook = RawDetour::new(target, detour)?;
        hook.enable()?;
        let trampoline = hook.trampoline() as *const ();
        self.hooks.insert(name.to_owned(), hook);
        log::info!("hook `{name}` installed at {target:p}");
        Ok(trampoline)
    }

    /// # Safety
    /// See [`HookManager::install`].
    pub unsafe fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let hook = self.hooks.get(name).ok_or_else(|| Error::HookMissing(name.to_owned()))?;
        if enabled {
            hook.enable()?
        } else {
            hook.disable()?
        }
        Ok(())
    }

    /// Disables and drops a hook.
    ///
    /// # Safety
    /// See [`HookManager::install`].
    pub unsafe fn remove(&mut self, name: &str) -> Result<()> {
        let hook = self.hooks.remove(name).ok_or_else(|| Error::HookMissing(name.to_owned()))?;
        hook.disable()?;
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.hooks.keys().map(String::as_str)
    }

    pub fn is_enabled(&self, name: &str) -> Option<bool> {
        self.hooks.get(name).map(RawDetour::is_enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TRAMPOLINE: AtomicUsize = AtomicUsize::new(0);

    #[inline(never)]
    #[no_mangle]
    extern "C" fn ferro_test_add(a: i32, b: i32) -> i32 {
        std::hint::black_box(a + b)
    }

    extern "C" fn doubled_add(a: i32, b: i32) -> i32 {
        let original: extern "C" fn(i32, i32) -> i32 =
            unsafe { std::mem::transmute(TRAMPOLINE.load(Ordering::SeqCst)) };
        original(a, b) * 2
    }

    #[test]
    fn detours_and_restores() {
        let call = std::hint::black_box(ferro_test_add as extern "C" fn(i32, i32) -> i32);
        assert_eq!(call(2, 3), 5);
        let mut mgr = HookManager::new();
        unsafe {
            let t = mgr.install("add", ferro_test_add as *const (), doubled_add as *const ()).unwrap();
            TRAMPOLINE.store(t as usize, Ordering::SeqCst);
            assert_eq!(call(2, 3), 10);
            assert!(matches!(
                mgr.install("add", ferro_test_add as *const (), doubled_add as *const ()),
                Err(Error::HookExists(_))
            ));
            mgr.set_enabled("add", false).unwrap();
            assert_eq!(call(2, 3), 5);
            mgr.set_enabled("add", true).unwrap();
            assert_eq!(call(2, 3), 10);
            mgr.remove("add").unwrap();
        }
        assert_eq!(call(2, 3), 5);
    }
}
