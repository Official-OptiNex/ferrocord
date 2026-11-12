//! Named signatures that resolve to function addresses in the target.
//!
//! Each Discord build will need its own signature set; keeping them as data
//! (instead of hard-coded offsets) lets the community update them without
//! recompiling plugins.

use crate::memory::Region;
use crate::{Error, Pattern, Result};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Signature {
    pub name: String,
    pub pattern: Pattern,
    /// Added to the match offset, e.g. when the pattern starts mid-function.
    pub offset: isize,
}

impl Signature {
    pub fn new(name: impl Into<String>, pattern: &str, offset: isize) -> Result<Self> {
        Ok(Self { name: name.into(), pattern: Pattern::parse(pattern)?, offset })
    }
}

#[derive(Debug, Default, Clone)]
pub struct SignatureSet {
    sigs: Vec<Signature>,
}

impl SignatureSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, sig: Signature) -> &mut Self {
        self.sigs.push(sig);
        self
    }

    pub fn len(&self) -> usize {
        self.sigs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sigs.is_empty()
    }

    /// Parses a signature file. Each non-empty, non-`#` line is
    /// `name offset pattern...`, for example:
    ///
    /// ```text
    /// # name           offset  pattern
    /// send_message     0       55 48 89 E5 41 57 ?? ?? 48 83 EC
    /// render_message  -4       0F 85 ?? ?? ?? ?? 48 8B 7D
    /// ```
    pub fn parse(text: &str) -> Result<Self> {
        let mut set = Self::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let err = |msg: &str| Error::InvalidPattern(format!("line {}: {msg}", n + 1));
            let mut parts = line.splitn(3, char::is_whitespace);
            let name = parts.next().ok_or_else(|| err("missing name"))?;
            let offset = parts
                .next()
                .and_then(|o| o.trim().parse::<isize>().ok())
                .ok_or_else(|| err("missing or invalid offset"))?;
            let pattern = parts.next().ok_or_else(|| err("missing pattern"))?;
            if set.sigs.iter().any(|s| s.name == name) {
                return Err(err(&format!("duplicate signature `{name}`")));
            }
            set.add(Signature::new(name, pattern, offset).map_err(|e| err(&e.to_string()))?);
        }
        Ok(set)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Signature> {
        self.sigs.iter()
    }

    /// Resolves each signature against `(base_address, bytes)` regions. A
    /// signature must match exactly once across all regions.
    pub fn resolve_in<'a>(
        &self,
        regions: impl IntoIterator<Item = (usize, &'a [u8])> + Clone,
    ) -> HashMap<String, Result<usize>> {
        self.sigs
            .iter()
            .map(|sig| {
                let hits: Vec<usize> = regions
                    .clone()
                    .into_iter()
                    .flat_map(|(base, bytes)| {
                        sig.pattern.find_all(bytes).into_iter().map(move |off| base + off)
                    })
                    .collect();
                let res = match hits.as_slice() {
                    [] => Err(Error::SignatureNotFound(sig.name.clone())),
                    [addr] => Ok(addr.wrapping_add_signed(sig.offset)),
                    _ => Err(Error::AmbiguousSignature { name: sig.name.clone(), count: hits.len() }),
                };
                (sig.name.clone(), res)
            })
            .collect()
    }

    /// Resolves against live memory regions of the current process.
    ///
    /// # Safety
    /// `regions` must describe mapped, readable memory.
    pub unsafe fn resolve(&self, regions: &[Region]) -> HashMap<String, Result<usize>> {
        self.resolve_in(regions.iter().map(|r| (r.start, r.as_slice())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_signature_file() {
        let set = SignatureSet::parse(
            "# comment\n\nsend_message 0 55 48 89 E5 # trailing\nrender -4 0F 85 ?? ??\n",
        )
        .unwrap();
        let sigs: Vec<_> = set.iter().map(|s| (s.name.as_str(), s.offset, s.pattern.len())).collect();
        assert_eq!(sigs, vec![("send_message", 0, 4), ("render", -4, 4)]);
        assert!(SignatureSet::parse("nooffset").is_err());
        assert!(SignatureSet::parse("a x 55").is_err());
        assert!(SignatureSet::parse("a 0 55\na 0 66").is_err());
        assert!(SignatureSet::parse("a 0 5G").is_err());
    }

    #[test]
    fn resolves_unique_missing_and_ambiguous() {
        let a = [0x90, 0x55, 0x48, 0x89, 0xE5, 0x90];
        let b = [0xCC, 0xCC, 0x55, 0x48];
        let mut set = SignatureSet::new();
        set.add(Signature::new("prologue", "55 48 89 E5", -1).unwrap())
            .add(Signature::new("missing", "DE AD", 0).unwrap())
            .add(Signature::new("dupe", "55 48", 0).unwrap());
        let out = set.resolve_in([(0x1000, &a[..]), (0x2000, &b[..])]);
        assert_eq!(*out["prologue"].as_ref().unwrap(), 0x1000);
        assert!(matches!(out["missing"], Err(Error::SignatureNotFound(_))));
        assert!(matches!(out["dupe"], Err(Error::AmbiguousSignature { count: 2, .. })));
    }
}
