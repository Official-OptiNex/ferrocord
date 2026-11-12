//! DLL injection for Windows: load `ferro_loader.dll` into a running process
//! by calling `LoadLibraryW` on a remote thread.

use anyhow::{bail, Context, Result};
use std::ffi::c_void;
use std::path::Path;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW, MODULEENTRY32W,
    PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeThread, OpenProcess, WaitForSingleObject, PROCESS_CREATE_THREAD,
    PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

/// Closes a Win32 handle when dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: we own this handle.
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn last_error() -> u32 {
    // SAFETY: always safe to call.
    unsafe { GetLastError() }
}

/// PIDs of running processes whose executable name matches `name`
/// (case-insensitive, `.exe` optional).
pub fn find_processes(name: &str) -> Result<Vec<u32>> {
    let want = name.to_ascii_lowercase();
    let want = want.strip_suffix(".exe").unwrap_or(&want).to_owned();
    // SAFETY: plain Win32 calls; the entry struct is sized before use.
    unsafe {
        let snap = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
        if snap.0 == INVALID_HANDLE_VALUE {
            bail!("cannot list processes (error {})", last_error());
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut pids = Vec::new();
        let mut ok = Process32FirstW(snap.0, &mut entry);
        while ok != 0 {
            let exe = from_wide(&entry.szExeFile).to_ascii_lowercase();
            if exe.strip_suffix(".exe").unwrap_or(&exe) == want {
                pids.push(entry.th32ProcessID);
            }
            ok = Process32NextW(snap.0, &mut entry);
        }
        Ok(pids)
    }
}

/// Whether `pid` has a module named `module` loaded. Fails while the
/// process is still starting up.
fn has_module(pid: u32, module: &str) -> Result<bool> {
    // SAFETY: plain Win32 calls; the entry struct is sized before use.
    unsafe {
        let snap = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid));
        if snap.0 == INVALID_HANDLE_VALUE {
            bail!("cannot list modules of {pid} (error {})", last_error());
        }
        let mut entry: MODULEENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<MODULEENTRY32W>() as u32;
        let mut ok = Module32FirstW(snap.0, &mut entry);
        while ok != 0 {
            if from_wide(&entry.szModule).eq_ignore_ascii_case(module) {
                return Ok(true);
            }
            ok = Module32NextW(snap.0, &mut entry);
        }
        Ok(false)
    }
}

/// Waits until the Windows loader has set up `pid` (kernel32 is mapped), so
/// `LoadLibraryW` can run in it.
pub fn wait_until_ready(pid: u32, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if let Ok(true) = has_module(pid, "kernel32.dll") {
            return Ok(());
        }
        if start.elapsed() > timeout {
            bail!("process {pid} did not finish starting within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether the loader DLL is already loaded in `pid`.
pub fn is_injected(pid: u32, dll: &Path) -> bool {
    let name = dll.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    has_module(pid, &name).unwrap_or(false)
}

/// Loads `dll` into process `pid`.
pub fn inject(pid: u32, dll: &Path) -> Result<()> {
    let path = dll.to_str().context("loader path is not valid UTF-16")?;
    // Strip the `\\?\` prefix `canonicalize` adds; LoadLibraryW handles plain paths best.
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    let path_w = wide(path);
    let bytes = path_w.len() * 2;

    // SAFETY: every pointer passed below is either from the Win32 API or
    // points at `path_w`, which outlives the calls.
    unsafe {
        let process = Handle(OpenProcess(
            PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_WRITE
                | PROCESS_VM_READ,
            0,
            pid,
        ));
        if process.0.is_null() {
            bail!("cannot open process {pid} (error {}); try running as the same user", last_error());
        }

        let remote =
            VirtualAllocEx(process.0, std::ptr::null(), bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if remote.is_null() {
            bail!("cannot allocate memory in {pid} (error {})", last_error());
        }
        let free = || VirtualFreeEx(process.0, remote, 0, MEM_RELEASE);

        let mut written = 0;
        if WriteProcessMemory(process.0, remote, path_w.as_ptr().cast(), bytes, &mut written) == 0 {
            free();
            bail!("cannot write to process {pid} (error {})", last_error());
        }

        // kernel32 is mapped at the same address in every process of a session.
        let kernel32 = GetModuleHandleW(wide("kernel32.dll").as_ptr());
        let Some(load_library) = GetProcAddress(kernel32, c"LoadLibraryW".as_ptr().cast()) else {
            free();
            bail!("LoadLibraryW not found");
        };
        let start: unsafe extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(load_library);

        let thread = Handle(CreateRemoteThread(
            process.0,
            std::ptr::null(),
            0,
            Some(start),
            remote,
            0,
            std::ptr::null_mut(),
        ));
        if thread.0.is_null() {
            free();
            bail!("cannot start a thread in {pid} (error {})", last_error());
        }
        let waited = WaitForSingleObject(thread.0, 15_000);
        let mut code = 0u32;
        GetExitCodeThread(thread.0, &mut code);
        free();
        if waited != WAIT_OBJECT_0 {
            bail!("loading the DLL in {pid} timed out");
        }
        // The exit code is the low 32 bits of the HMODULE; 0 means LoadLibraryW failed.
        if code == 0 {
            bail!(
                "LoadLibraryW failed in {pid}: check the DLL path and that it matches the process (64-bit)"
            );
        }
        Ok(())
    }
}
