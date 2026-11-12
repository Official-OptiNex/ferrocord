//! Locating the executable code of a module loaded in the current process.

use crate::{Error, Result};

/// A readable+executable range of memory belonging to a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub start: usize,
    pub end: usize,
}

impl Region {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// # Safety
    /// The region must be mapped and readable for the lifetime of the slice.
    pub unsafe fn as_slice(&self) -> &'static [u8] {
        std::slice::from_raw_parts(self.start as *const u8, self.len())
    }
}

/// Parses `/proc/<pid>/maps` text and returns executable regions whose path
/// ends with `module` (e.g. `"Discord"` or `"libfoo.so"`).
pub fn parse_proc_maps(maps: &str, module: &str) -> Vec<Region> {
    maps.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let range = parts.next()?;
            let perms = parts.next()?;
            let path = parts.nth(3)?; // skip offset, dev, inode
            if !perms.starts_with("r-x") && !perms.starts_with("r-xp") {
                return None;
            }
            let file_name = path.rsplit('/').next()?;
            if file_name != module {
                return None;
            }
            let (start, end) = range.split_once('-')?;
            Some(Region {
                start: usize::from_str_radix(start, 16).ok()?,
                end: usize::from_str_radix(end, 16).ok()?,
            })
        })
        .collect()
}

/// Executable regions of `module` in the current process.
#[cfg(target_os = "linux")]
pub fn module_regions(module: &str) -> Result<Vec<Region>> {
    let maps = std::fs::read_to_string("/proc/self/maps")?;
    let regions = parse_proc_maps(&maps, module);
    if regions.is_empty() {
        return Err(Error::ModuleNotFound(module.to_owned()));
    }
    Ok(regions)
}

/// Name of the main executable of the current process.
pub fn current_exe_name() -> Result<String> {
    let exe = std::env::current_exe()?;
    exe.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or(Error::ModuleNotFound("current executable".into()))
}

#[cfg(not(target_os = "linux"))]
pub fn module_regions(_module: &str) -> Result<Vec<Region>> {
    // Windows: walk PE section headers of GetModuleHandleW(module) for `.text`.
    // macOS: use _dyld_image_count / getsectiondata for `__TEXT,__text`.
    Err(Error::Unsupported("module_regions is only implemented on Linux so far"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAPS: &str = "\
55d0c0000000-55d0c0010000 r--p 00000000 08:01 123 /opt/discord/Discord
55d0c0010000-55d0c0200000 r-xp 00010000 08:01 123 /opt/discord/Discord
55d0c0200000-55d0c0210000 rw-p 00200000 08:01 123 /opt/discord/Discord
7f0000000000-7f0000100000 r-xp 00000000 08:01 456 /usr/lib/libc.so.6
7f0000200000-7f0000201000 rw-p 00000000 00:00 0
";

    #[test]
    fn picks_executable_regions_of_module() {
        assert_eq!(
            parse_proc_maps(MAPS, "Discord"),
            vec![Region { start: 0x55d0c0010000, end: 0x55d0c0200000 }]
        );
        assert_eq!(parse_proc_maps(MAPS, "libc.so.6").len(), 1);
        assert!(parse_proc_maps(MAPS, "cord").is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn finds_own_executable() {
        let name = current_exe_name().unwrap();
        let regions = module_regions(&name).unwrap();
        assert!(regions.iter().all(|r| !r.is_empty()));
    }
}
