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
