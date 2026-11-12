//! IDA-style byte pattern parsing and scanning, e.g. `"48 8B ?? ?? 0F 85"`.

use crate::{Error, Result};

/// A byte pattern where `None` is a wildcard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    bytes: Vec<Option<u8>>,
}

impl Pattern {
    /// Parses space-separated hex bytes; `?` or `??` is a wildcard.
    pub fn parse(src: &str) -> Result<Self> {
        let bytes = src
            .split_whitespace()
            .map(|tok| match tok {
                "?" | "??" => Ok(None),
                hex if hex.len() == 2 => u8::from_str_radix(hex, 16)
                    .map(Some)
                    .map_err(|_| Error::InvalidPattern(format!("bad byte `{hex}` in `{src}`"))),
                other => Err(Error::InvalidPattern(format!("bad token `{other}` in `{src}`"))),
            })
            .collect::<Result<Vec<_>>>()?;
        if bytes.is_empty() {
            return Err(Error::InvalidPattern("empty pattern".into()));
        }
        if bytes.iter().all(Option::is_none) {
            return Err(Error::InvalidPattern("pattern is all wildcards".into()));
        }
        Ok(Self { bytes })
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn matches_at(&self, haystack: &[u8], at: usize) -> bool {
        self.bytes
            .iter()
            .zip(&haystack[at..at + self.bytes.len()])
            .all(|(p, b)| p.map_or(true, |p| p == *b))
    }

    /// Offsets of every match in `haystack`.
    pub fn find_all(&self, haystack: &[u8]) -> Vec<usize> {
        if haystack.len() < self.len() {
            return Vec::new();
        }
        // Anchor on the first concrete byte to skip most positions cheaply.
        let (anchor_idx, anchor) = self
            .bytes
            .iter()
            .enumerate()
            .find_map(|(i, b)| b.map(|b| (i, b)))
            .expect("parse guarantees a concrete byte");
        (0..=haystack.len() - self.len())
            .filter(|&at| haystack[at + anchor_idx] == anchor && self.matches_at(haystack, at))
            .collect()
    }

    /// Offset of the first match in `haystack`.
    pub fn find(&self, haystack: &[u8]) -> Option<usize> {
        self.find_all(haystack).into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_rejects() {
        assert_eq!(Pattern::parse("48 ?? 8b ?").unwrap().len(), 4);
        assert!(Pattern::parse("").is_err());
        assert!(Pattern::parse("?? ??").is_err());
        assert!(Pattern::parse("4G").is_err());
        assert!(Pattern::parse("488B").is_err());
    }

    #[test]
    fn finds_with_wildcards() {
        let hay = [0x00, 0x48, 0x8B, 0x05, 0x10, 0x48, 0x8B, 0x0D, 0x20];
        let p = Pattern::parse("48 8B ?? 10").unwrap();
        assert_eq!(p.find(&hay), Some(1));
        let p = Pattern::parse("48 8B").unwrap();
        assert_eq!(p.find_all(&hay), vec![1, 5]);
        assert_eq!(Pattern::parse("FF").unwrap().find(&hay), None);
    }

    #[test]
    fn handles_short_haystack_and_edges() {
        let p = Pattern::parse("?? 20").unwrap();
        assert_eq!(p.find(&[0x20]), None);
        assert_eq!(p.find(&[0x0D, 0x20]), Some(0));
    }
}
