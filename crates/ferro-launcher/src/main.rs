//! `ferrocord`: launch the client with the loader, and manage plugins.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ferro_core::{paths, SignatureSet};
use ferro_plugin_api::{Manifest, MANIFEST_FILE, WASM_FILE};
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
const LOADER_FILE: &str = "libferro_loader.so";
#[cfg(target_os = "macos")]
const LOADER_FILE: &str = "libferro_loader.dylib";
#[cfg(windows)]
const LOADER_FILE: &str = "ferro_loader.dll";

#[derive(Parser)]
#[command(name = "ferrocord", version, about = "Client mod loader for the native Discord app")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the client with ferrocord loaded.
    Launch {
        /// Path to the client executable.
        client: PathBuf,
        /// Loader library to inject (defaults to the one next to this binary).
        #[arg(long, env = "FERROCORD_LOADER")]
        loader: Option<PathBuf>,
        /// Extra arguments for the client.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Install a plugin directory (containing manifest.json and plugin.wasm).
    Install { plugin: PathBuf },
    /// Check the setup and list installed plugins.
    Doctor,
    /// Print the data, plugins, signatures and log paths.
    Paths,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Launch { client, loader, args } => launch(&client, loader, &args),
        Command::Install { plugin } => install(&plugin),
        Command::Doctor => doctor(),
        Command::Paths => {
            println!("data        {}", paths::data_dir().display());
            println!("plugins     {}", paths::plugins_dir().display());
            println!("signatures  {}", paths::signatures_file().display());
            println!("log         {}", paths::log_file().display());
            Ok(())
        }
    }
}

fn default_loader() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(LOADER_FILE))
}

fn find_loader(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let loader = explicit.or_else(default_loader).context("cannot locate the loader library")?;
    if !loader.is_file() {
        bail!(
            "loader not found at {}\nbuild it with `cargo build --release` or pass --loader",
            loader.display()
        );
    }
    Ok(std::fs::canonicalize(loader)?)
}

#[cfg(target_os = "linux")]
fn launch(client: &Path, loader: Option<PathBuf>, args: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let loader = find_loader(loader)?;
    if !client.is_file() {
        bail!("client not found at {}", client.display());
    }
    let preload = match std::env::var("LD_PRELOAD") {
        Ok(existing) if !existing.is_empty() => format!("{}:{existing}", loader.display()),
        _ => loader.display().to_string(),
    };
    eprintln!("launching {} with {}", client.display(), loader.display());
    let err = std::process::Command::new(client).args(args).env("LD_PRELOAD", preload).exec();
    Err(err).context("failed to start the client")
}

#[cfg(not(target_os = "linux"))]
fn launch(_client: &Path, loader: Option<PathBuf>, _args: &[String]) -> Result<()> {
    let _ = find_loader(loader);
    bail!("launching is only supported on Linux so far; Windows and macOS are on the roadmap")
}

fn read_manifest(dir: &Path) -> Result<Manifest> {
    let raw = std::fs::read(dir.join(MANIFEST_FILE))
        .with_context(|| format!("no {MANIFEST_FILE} in {}", dir.display()))?;
    let manifest: Manifest = serde_json::from_slice(&raw).context("invalid manifest")?;
    manifest.validate().map_err(anyhow::Error::msg)?;
    if !dir.join(WASM_FILE).is_file() {
        bail!("no {WASM_FILE} in {}", dir.display());
    }
    Ok(manifest)
}

fn install(src: &Path) -> Result<()> {
    let manifest = read_manifest(src)?;
    let dest = paths::plugins_dir().join(&manifest.id);
    std::fs::create_dir_all(&dest)?;
    for file in [MANIFEST_FILE, WASM_FILE] {
        std::fs::copy(src.join(file), dest.join(file))?;
    }
    println!("installed {} v{} -> {}", manifest.name, manifest.version, dest.display());
    Ok(())
}

fn doctor() -> Result<()> {
    let ok = |b: bool| if b { "ok  " } else { "MISS" };
    let loader = default_loader().filter(|p| p.is_file());
    println!(
        "[{}] loader      {}",
        ok(loader.is_some()),
        loader.map_or_else(|| format!("{LOADER_FILE} not next to this binary"), |p| p.display().to_string())
    );

    let sig_path = paths::signatures_file();
    match std::fs::read_to_string(&sig_path) {
        Ok(text) => match SignatureSet::parse(&text) {
            Ok(set) => println!("[ok  ] signatures  {} defined in {}", set.len(), sig_path.display()),
            Err(e) => println!("[FAIL] signatures  {e}"),
        },
        Err(_) => println!("[MISS] signatures  none at {} (hooks stay off)", sig_path.display()),
    }

    let plugins_dir = paths::plugins_dir();
    let mut dirs: Vec<_> = std::fs::read_dir(&plugins_dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    dirs.sort();
    println!("[{}] plugins     {} in {}", ok(!dirs.is_empty()), dirs.len(), plugins_dir.display());
    for dir in dirs {
        match read_manifest(&dir) {
            Ok(m) => println!("         - {} v{} ({})", m.name, m.version, m.id),
            Err(e) => println!("         - {}: {e:#}", dir.display()),
        }
    }
    Ok(())
}
