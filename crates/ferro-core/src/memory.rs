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

/// Executable sections of a PE image as `(rva, size)` pairs, read from its
/// headers (`image` must start at the DOS header).
pub fn pe_exec_sections(image: &[u8]) -> Result<Vec<(usize, usize)>> {
    const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
    let bad = |what: &str| Error::InvalidPe(what.to_owned());
    let u16_at = |off: usize| -> Result<u16> {
        image.get(off..off + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| bad("truncated"))
    };
    let u32_at = |off: usize| -> Result<u32> {
        image
            .get(off..off + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| bad("truncated"))
    };
    if image.get(0..2) != Some(b"MZ") {
        return Err(bad("missing MZ signature"));
    }
    let pe = u32_at(0x3C)? as usize;
    if image.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(bad("missing PE signature"));
    }
    let coff = pe + 4;
    let sections = u16_at(coff + 2)? as usize;
    let optional_size = u16_at(coff + 16)? as usize;
    let table = coff + 20 + optional_size;
    (0..sections)
        .map(|i| table + i * 40)
        .filter_map(|sec| match u32_at(sec + 36) {
            Ok(flags) if flags & IMAGE_SCN_MEM_EXECUTE != 0 => {
                Some(u32_at(sec + 12).and_then(|rva| Ok((rva as usize, u32_at(sec + 8)? as usize))))
            }
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .collect()
}

/// Executable regions of `module` in the current process.
#[cfg(windows)]
pub fn module_regions(module: &str) -> Result<Vec<Region>> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    let wide: Vec<u16> = module.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string.
    let base = unsafe { GetModuleHandleW(wide.as_ptr()) } as usize;
    if base == 0 {
        return Err(Error::ModuleNotFound(module.to_owned()));
    }
    // The headers always fit in the first page, which is mapped read-only.
    // SAFETY: `base` is the start of a loaded image.
    let headers = unsafe { std::slice::from_raw_parts(base as *const u8, 0x1000) };
    let regions: Vec<Region> = pe_exec_sections(headers)?
        .into_iter()
        .map(|(rva, size)| Region { start: base + rva, end: base + rva + size })
        .collect();
    if regions.is_empty() {
        return Err(Error::ModuleNotFound(module.to_owned()));
    }
    Ok(regions)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn module_regions(_module: &str) -> Result<Vec<Region>> {
    // macOS: use _dyld_image_count / getsectiondata for `__TEXT,__text`.
    Err(Error::Unsupported("module_regions is not implemented on this platform yet"))
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

    /// Builds a minimal PE header with the given `(rva, size, flags)` sections.
    fn fake_pe(sections: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut img = vec![0u8; 0x400];
        img[0..2].copy_from_slice(b"MZ");
        img[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        img[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        img[coff + 2..coff + 4].copy_from_slice(&(sections.len() as u16).to_le_bytes());
        img[coff + 16..coff + 18].copy_from_slice(&0xF0u16.to_le_bytes());
        let table = coff + 20 + 0xF0;
        for (i, (rva, size, flags)) in sections.iter().enumerate() {
            let s = table + i * 40;
            img[s + 8..s + 12].copy_from_slice(&size.to_le_bytes());
            img[s + 12..s + 16].copy_from_slice(&rva.to_le_bytes());
            img[s + 36..s + 40].copy_from_slice(&flags.to_le_bytes());
        }
        img
    }

    #[test]
    fn reads_executable_pe_sections() {
        let img = fake_pe(&[
            (0x1000, 0x5000, 0x6000_0020), // .text: code | execute | read
            (0x6000, 0x2000, 0x4000_0040), // .rdata
            (0x9000, 0x100, 0x6000_0020),  // second code section
        ]);
        assert_eq!(pe_exec_sections(&img).unwrap(), vec![(0x1000, 0x5000), (0x9000, 0x100)]);
        assert!(pe_exec_sections(b"not a pe").is_err());
        let mut broken = img.clone();
        broken[0x80] = b'X';
        assert!(pe_exec_sections(&broken).is_err());
        assert!(pe_exec_sections(&img[..0x100]).is_err());
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn finds_own_executable() {
        let name = current_exe_name().unwrap();
        let regions = module_regions(&name).unwrap();
        assert!(regions.iter().all(|r| !r.is_empty()));
    }
}
