//! End-to-end test: install the example plugin, launch a stand-in client with
//! the loader, and wait for the plugin to receive the `ready` event.
//!
//! Needs the loader and example binaries built first, so it is ignored by default:
//!
//! ```sh
//! cargo build --workspace --examples
//! cargo test -p ferro-launcher --test e2e -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn ferrocord() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ferrocord"))
}

fn fake_client() -> PathBuf {
    let dir = ferrocord().parent().unwrap().join("examples");
    dir.join(format!("fake_client{}", std::env::consts::EXE_SUFFIX))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(data: &Path, args: &[&str]) -> std::process::Output {
    let out = Command::new(ferrocord())
        .args(args)
        .env("FERROCORD_DIR", data)
        .env("FERROCORD_FORCE", "1")
        .output()
        .expect("failed to run ferrocord");
    assert!(
        out.status.success(),
        "ferrocord {args:?} failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

#[test]
#[ignore = "needs `cargo build --workspace --examples` first"]
fn launch_loads_plugins() {
    let data = tempfile::tempdir().unwrap();
    let plugin = repo_root().join("examples/hello-plugin");
    run(data.path(), &["install", plugin.to_str().unwrap()]);
    let doctor = String::from_utf8(run(data.path(), &["doctor"]).stdout).unwrap();
    assert!(doctor.contains("[ok  ] loader"), "loader missing:\n{doctor}");
    assert!(doctor.contains("Hello v0.1.0"), "plugin missing:\n{doctor}");

    run(data.path(), &["launch", fake_client().to_str().unwrap(), "--", "5"]);

    let log = data.path().join("ferrocord.log");
    let start = Instant::now();
    loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.contains("[hello] got event `ready`") {
            assert!(text.contains("[hello] hello from a Rust plugin!"));
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "plugin never became ready; log:\n{text}");
        std::thread::sleep(Duration::from_millis(200));
    }
}
