//! Build/packaging helper for Markd, run via `cargo dist`.
//!
//! It builds the release binary, generates the installer icon from the app PNG,
//! locates (or installs) the Inno Setup compiler, and compiles a single-file
//! standalone installer into `dist/`.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if let Err(err) = run() {
        eprintln!("\nerror: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let root = workspace_root();
    let version = read_version(&root.join("Cargo.toml"))?;
    println!("Markd version: {version}");

    // 1. Build the release binary (statically linked via .cargo/config.toml).
    println!("Building release binary...");
    let ok = Command::new(cargo())
        .current_dir(&root)
        .args(["build", "--release", "--package", "markd"])
        .status()
        .map_err(|e| format!("failed to launch cargo: {e}"))?
        .success();
    if !ok {
        return Err("`cargo build --release` failed".into());
    }
    let exe = root.join("target/release/markd.exe");
    if !exe.exists() {
        return Err(format!("release binary not found: {}", exe.display()));
    }

    // 2. Generate the installer icon from the app PNG.
    let png = root.join("src/assets/icon.png");
    let ico = root.join("installer/markd.ico");
    println!("Generating {} ...", ico.display());
    let png_bytes = fs::read(&png).map_err(|e| format!("read {}: {e}", png.display()))?;
    write_ico(&ico, &png_bytes).map_err(|e| format!("write {}: {e}", ico.display()))?;

    // 3. Locate the Inno Setup compiler (install via winget if missing).
    let iscc = find_iscc()?;
    println!("Using ISCC: {}", iscc.display());

    // 4. Compile the installer.
    let iss = root.join("installer/markd.iss");
    let ok = Command::new(&iscc)
        .arg(format!("/DMyAppVersion={version}"))
        .arg(&iss)
        .status()
        .map_err(|e| format!("failed to launch ISCC: {e}"))?
        .success();
    if !ok {
        return Err("ISCC failed to compile the installer".into());
    }

    let out = root.join(format!("dist/Markd-Setup-{version}.exe"));
    println!("\nInstaller built: {}", out.display());
    Ok(())
}

fn cargo() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

// CARGO_MANIFEST_DIR points at `xtask/`; the workspace root is its parent.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent directory")
        .to_path_buf()
}

// Read the first `version = "..."` (the [package] version) from Cargo.toml.
fn read_version(cargo_toml: &Path) -> Result<String, String> {
    let text = fs::read_to_string(cargo_toml).map_err(|e| format!("read {}: {e}", cargo_toml.display()))?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("version") {
            if let Some(value) = rest.trim_start().strip_prefix('=') {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    return Ok(value.to_string());
                }
            }
        }
    }
    Err("could not find a package version in Cargo.toml".into())
}

// Find ISCC.exe on PATH or in the usual install locations; if absent, try to
// install Inno Setup with winget and look again.
fn find_iscc() -> Result<PathBuf, String> {
    if let Some(path) = locate_iscc() {
        return Ok(path);
    }

    eprintln!("Inno Setup not found - installing via winget...");
    let installed = Command::new("winget")
        .args([
            "install",
            "--exact",
            "--id",
            "JRSoftware.InnoSetup",
            "--accept-source-agreements",
            "--accept-package-agreements",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    locate_iscc().ok_or_else(|| {
        if installed {
            "Inno Setup installed but ISCC.exe was not found; re-run `cargo dist`.".into()
        } else {
            "Inno Setup is required. Install it from https://jrsoftware.org/isdl.php \
             (or `winget install JRSoftware.InnoSetup`), then re-run `cargo dist`."
                .into()
        }
    })
}

fn locate_iscc() -> Option<PathBuf> {
    if let Some(path) = which("iscc") {
        return Some(path);
    }
    let mut candidates = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(base) = env::var_os(var) {
            let mut p = PathBuf::from(base);
            if var == "LOCALAPPDATA" {
                p.push("Programs");
            }
            p.push("Inno Setup 6");
            p.push("ISCC.exe");
            candidates.push(p);
        }
    }
    candidates.into_iter().find(|p| p.exists())
}

// Minimal PATH search for an executable (with and without `.exe`).
fn which(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        for candidate in [dir.join(name), dir.join(format!("{name}.exe"))] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

// Wrap a single 256x256 PNG in a minimal ICONDIR so it becomes a valid .ico.
// Inno Setup 6 and the Windows shell (Vista+) accept PNG-compressed icon
// entries and scale this entry down for the smaller shell sizes.
fn write_ico(path: &Path, png: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    let len = png.len() as u32;

    // ICONDIR header.
    file.write_all(&0u16.to_le_bytes())?; // reserved
    file.write_all(&1u16.to_le_bytes())?; // type: 1 = icon
    file.write_all(&1u16.to_le_bytes())?; // image count

    // ICONDIRENTRY.
    file.write_all(&[0])?; // width  (0 => 256)
    file.write_all(&[0])?; // height (0 => 256)
    file.write_all(&[0])?; // color count (0 => truecolor)
    file.write_all(&[0])?; // reserved
    file.write_all(&1u16.to_le_bytes())?; // color planes
    file.write_all(&32u16.to_le_bytes())?; // bits per pixel
    file.write_all(&len.to_le_bytes())?; // size of image data
    file.write_all(&22u32.to_le_bytes())?; // offset (6 + 16)

    file.write_all(png)?;
    Ok(())
}
