//! CLI tests for plugin management: install, list, disable, enable, remove.
//! These don't inject anything, so they run everywhere without the loader.

use std::path::{Path, PathBuf};
use std::process::Command;

fn ferrocord() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ferrocord"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Cli {
    data: tempfile::TempDir,
}

impl Cli {
    fn new() -> Self {
        Self { data: tempfile::tempdir().unwrap() }
    }

    fn run(&self, args: &[&str]) -> (bool, String) {
        let out = Command::new(ferrocord())
            .args(args)
            .env("FERROCORD_DIR", self.data.path())
            .output()
            .expect("run ferrocord");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    fn config(&self) -> String {
        std::fs::read_to_string(self.data.path().join("plugins.json")).unwrap_or_default()
    }
}

#[test]
fn install_list_toggle_remove() {
    let cli = Cli::new();
    let plugin = repo_root().join("examples/hello-plugin");

    let (ok, _) = cli.run(&["install", plugin.to_str().unwrap()]);
    assert!(ok);

    let (ok, out) = cli.run(&["list"]);
    assert!(ok);
    assert!(out.contains("[enabled ] hello"), "{out}");

    let (ok, _) = cli.run(&["disable", "hello"]);
    assert!(ok);
    assert!(cli.config().contains("\"hello\""), "config: {}", cli.config());
    let (_, out) = cli.run(&["list"]);
    assert!(out.contains("[disabled] hello"), "{out}");

    let (ok, _) = cli.run(&["enable", "hello"]);
    assert!(ok);
    assert!(!cli.config().contains("\"hello\""));

    // Unknown ids fail cleanly.
    let (ok, out) = cli.run(&["disable", "ghost"]);
    assert!(!ok);
    assert!(out.contains("no plugin named `ghost`"), "{out}");

    let (ok, out) = cli.run(&["remove", "hello"]);
    assert!(ok, "{out}");
    let (_, out) = cli.run(&["list"]);
    assert!(out.contains("no plugins installed"), "{out}");
}
