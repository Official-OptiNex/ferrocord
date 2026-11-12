//! Which installed plugins are turned off. Stored as JSON next to the plugins.

use crate::Result;
use std::collections::BTreeSet;
use std::path::Path;

/// Persisted plugin settings. Plugins are enabled unless listed in `disabled`,
/// so a freshly installed plugin runs without any config edit.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PluginConfig {
    disabled: BTreeSet<String>,
}

impl PluginConfig {
    pub fn is_enabled(&self, id: &str) -> bool {
        !self.disabled.contains(id)
    }

    /// Returns `true` if the state changed.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if enabled {
            self.disabled.remove(id)
        } else {
            self.disabled.insert(id.to_owned())
        }
    }

    pub fn disabled(&self) -> impl Iterator<Item = &str> {
        self.disabled.iter().map(String::as_str)
    }

    /// Parses the config file. A missing or empty file is an empty config; a
    /// malformed one is rejected so a typo can't silently enable everything.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Self::default());
        }
        // Minimal hand-rolled reader for {"disabled":["a","b"]} so ferro-core
        // stays free of a JSON dependency. Order and whitespace are flexible.
        let body = text
            .strip_prefix('{')
            .and_then(|t| t.trim().strip_prefix("\"disabled\""))
            .and_then(|t| t.trim_start().strip_prefix(':'))
            .and_then(|t| t.trim_start().strip_prefix('['))
            .and_then(|t| t.rsplit_once(']'))
            .map(|(inside, _)| inside)
            .ok_or_else(|| crate::Error::InvalidPattern("malformed plugins config".into()))?;
        let mut disabled = BTreeSet::new();
        for raw in body.split(',') {
            let item = raw.trim();
            if item.is_empty() {
                continue;
            }
            let id = item
                .strip_prefix('"')
                .and_then(|i| i.strip_suffix('"'))
                .ok_or_else(|| crate::Error::InvalidPattern("unquoted plugin id in config".into()))?;
            disabled.insert(id.to_owned());
        }
        Ok(Self { disabled })
    }

    pub fn to_json(&self) -> String {
        let ids = self.disabled.iter().map(|id| format!("\"{id}\"")).collect::<Vec<_>>().join(", ");
        format!("{{\"disabled\": [{ids}]}}\n")
    }

    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_by_default_and_toggles() {
        let mut cfg = PluginConfig::default();
        assert!(cfg.is_enabled("hello"));
        assert!(cfg.set_enabled("hello", false));
        assert!(!cfg.is_enabled("hello"));
        assert!(!cfg.set_enabled("hello", false)); // no change
        assert!(cfg.set_enabled("hello", true));
        assert!(cfg.is_enabled("hello"));
    }

    #[test]
    fn json_roundtrip() {
        let mut cfg = PluginConfig::default();
        cfg.set_enabled("b", false);
        cfg.set_enabled("a", false);
        let json = cfg.to_json();
        assert_eq!(json.trim(), r#"{"disabled": ["a", "b"]}"#);
        assert_eq!(PluginConfig::parse(&json).unwrap(), cfg);
    }

    #[test]
    fn parses_edge_cases_and_rejects_garbage() {
        assert_eq!(PluginConfig::parse("").unwrap(), PluginConfig::default());
        assert_eq!(PluginConfig::parse("{\"disabled\":[]}").unwrap(), PluginConfig::default());
        let one = PluginConfig::parse(" { \"disabled\" : [ \"x\" ] } ").unwrap();
        assert_eq!(one.disabled().collect::<Vec<_>>(), vec!["x"]);
        assert!(PluginConfig::parse("not json").is_err());
        assert!(PluginConfig::parse("{\"disabled\":[bare]}").is_err());
    }
}
