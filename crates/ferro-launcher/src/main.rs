//! `ferrocord`: launch the client with the loader, and manage plugins.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ferro_core::{paths, SignatureSet};
use ferro_plugin_api::{Manifest, MANIFEST_FILE, WASM_FILE};
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
const LOADER_FILE: &str = "libferro_loader.so";
#[cfg(target_os = "macos")]
const LOADER_FILE: &str = "libferro_loader.dylib";
#[cfg(windows)]
const LOADER_FILE: &str = "ferro_loader.dll";

#[derive(Parser)]
#[command(name = "ferrocord", version, about = "Client mod loader for the native Discord app")]
struct Cli {
    /// Defaults to `launch` (find the client and start it with ferrocord).
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the client with ferrocord loaded.
    Launch {
        /// Path to the client executable (auto-detected if omitted).
        client: Option<PathBuf>,
        /// Loader library to inject (defaults to the one next to this binary).
        #[arg(long, env = "FERROCORD_LOADER")]
        loader: Option<PathBuf>,
        /// Extra arguments for the client.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Load ferrocord into an already running client (Windows).
    Inject {
        /// Process ID or executable name.
        #[arg(default_value = "Discord.exe")]
        target: String,
        /// Loader library to inject (defaults to the one next to this binary).
        #[arg(long, env = "FERROCORD_LOADER")]
        loader: Option<PathBuf>,
    },
    /// Install a plugin directory (containing manifest.json and plugin.wasm).
    Install { plugin: PathBuf },
    /// Check the setup and list installed plugins.
    Doctor,
    /// Print the data, plugins, signatures and log paths.
    Paths,
}

fn main() -> std::process::ExitCode {
    // Started without arguments, e.g. by double-clicking ferrocord.exe.
    let double_clicked = std::env::args_os().len() == 1;
    let result = run(Cli::parse());
    let code = match &result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::ExitCode::FAILURE
        }
    };
    if result.is_err() && double_clicked && cfg!(windows) {
        eprintln!("\nPress Enter to close.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    code
}

/// Shows a path without the `\\?\` prefix Windows adds when canonicalising.
fn show(path: &Path) -> String {
    let s = path.display().to_string();
    s.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(s)
}

fn run(cli: Cli) -> Result<()> {
    let command = cli.command.unwrap_or(Command::Launch { client: None, loader: None, args: Vec::new() });
    match command {
        Command::Launch { client, loader, args } => {
            let client = match client {
                Some(c) => c,
                None => find_client().context(
                    "could not find the client automatically; pass its path: ferrocord launch <path>",
                )?,
            };
            launch(&client, loader, &args)
        }
        Command::Inject { target, loader } => inject(&target, loader),
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
    eprintln!("launching {} with {}", client.display(), show(&loader));
    let err = std::process::Command::new(client).args(args).env("LD_PRELOAD", preload).exec();
    Err(err).context("failed to start the client")
}

#[cfg(windows)]
fn launch(client: &Path, loader: Option<PathBuf>, args: &[String]) -> Result<()> {
    use std::time::Duration;

    let loader = find_loader(loader)?;
    if !client.is_file() {
        bail!("client not found at {}", client.display());
    }
    eprintln!("launching {}", client.display());
    let child =
        std::process::Command::new(client).args(args).spawn().context("failed to start the client")?;
    let pid = child.id();
    windows::wait_until_ready(pid, Duration::from_secs(30))?;
    windows::inject(pid, &loader)?;
    eprintln!("injected {} into pid {pid}", show(&loader));
    Ok(())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn launch(_client: &Path, loader: Option<PathBuf>, _args: &[String]) -> Result<()> {
    let _ = find_loader(loader);
    bail!("launching is not supported on this platform yet; macOS is on the roadmap")
}

#[cfg(windows)]
fn inject(target: &str, loader: Option<PathBuf>) -> Result<()> {
    let loader = find_loader(loader)?;
    let pids = match target.parse::<u32>() {
        Ok(pid) => vec![pid],
        Err(_) => windows::find_processes(target)?,
    };
    if pids.is_empty() {
        bail!("no running process named `{target}`; start the client first or use `ferrocord launch`");
    }
    let mut failures = 0;
    for pid in pids {
        if windows::is_injected(pid, &loader) {
            println!("pid {pid}: already loaded");
            continue;
        }
        match windows::inject(pid, &loader) {
            Ok(()) => println!("pid {pid}: injected"),
            Err(e) => {
                failures += 1;
                eprintln!("pid {pid}: {e:#}");
            }
        }
    }
    if failures > 0 {
        bail!("{failures} process(es) could not be injected");
    }
    Ok(())
}

#[cfg(not(windows))]
fn inject(_target: &str, _loader: Option<PathBuf>) -> Result<()> {
    bail!("`inject` is Windows-only; on Linux use `ferrocord launch`")
}

/// Best guess at where the client is installed.
fn find_client() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // %LOCALAPPDATA%\Discord\app-<version>\Discord.exe, newest version first.
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Discord");
        let mut apps: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(&root)
            .ok()?
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let version =
                    name.strip_prefix("app-")?.split('.').map(|n| n.parse().ok()).collect::<Option<_>>()?;
                Some((version, e.path().join("Discord.exe")))
            })
            .filter(|(_, exe)| exe.is_file())
            .collect();
        apps.sort();
        apps.pop().map(|(_, exe)| exe)
    }
    #[cfg(not(windows))]
    {
        ["/usr/bin/discord", "/usr/share/discord/Discord", "/opt/discord/Discord", "/opt/Discord/Discord"]
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
    }
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
