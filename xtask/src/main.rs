// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo xtask bundle <crate> --release` → target/bundled/<name>.vst3 (nih-plug's bundler)
//! `cargo xtask dist [--headless] [--no-archive] [crate…]` → dist/: every pedal built and put away.
//!
//! dist builds the pedals of bundler.toml one at a time (a few reuse another pedal's processor without
//! its plugin feature: built together, cargo would unify the features and link two VST3 entry points
//! into one library) and copies each bundle into
//!
//!   dist/vst3/<platform>/            with the editor (gfx-ui), for a DAW; a pedal that has no `gui`
//!                                    feature yet is built as it is, and listed at the end
//!   dist/vst3-headless/<platform>/   --headless: without, what golem plays (the release of golem-binaries)
//!
//! then, when every pedal was asked for, packs the folder as dist/golem-fx-<version>-<platform>-editor.tar.gz
//! (or -headless), the bundles at the archive's root, and writes dist/SHA256SUMS. dist/ is not committed.
//! gfx-gapmeter is a measuring probe, not a block: built only when named.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

fn main() -> nih_plug_xtask::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("dist") {
        return dist(&args[1..]);
    }
    nih_plug_xtask::main()
}

fn fail<T>(message: String) -> nih_plug_xtask::Result<T> {
    Err(io::Error::other(message).into())
}

/// bundler.toml's pedals: (crate, the bundle's name).
fn pedals() -> io::Result<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in fs::read_to_string("bundler.toml")?.lines() {
        let line = line.trim();
        if let Some(krate) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push((krate.to_string(), krate.to_string()));
        } else if let (Some(name), Some(last)) = (line.strip_prefix("name").and_then(|l| l.trim_start().strip_prefix('=')), out.last_mut()) {
            last.1 = name.trim().trim_matches('"').to_string();
        }
    }
    Ok(out)
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

fn output(program: &str, args: &[&str], dir: &Path) -> Option<String> {
    let out = Command::new(program).args(args).current_dir(dir).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn dist(args: &[String]) -> nih_plug_xtask::Result<()> {
    nih_plug_xtask::chdir_workspace_root()?;
    let headless = args.iter().any(|a| a == "--headless");
    let no_archive = args.iter().any(|a| a == "--no-archive");
    if let Some(unknown) = args.iter().find(|a| a.starts_with("--") && !["--headless", "--no-archive"].contains(&a.as_str())) {
        return fail(format!("dist: unknown option {unknown} (--headless, --no-archive)"));
    }
    let asked: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let all = pedals()?;
    let chosen: Vec<&(String, String)> = if asked.is_empty() {
        all.iter().filter(|(krate, _)| krate != "gfx-gapmeter").collect()
    } else {
        let mut chosen = Vec::new();
        for krate in &asked {
            match all.iter().find(|(k, _)| k == *krate) {
                Some(p) => chosen.push(p),
                None => return fail(format!("dist: {krate} is not a pedal of bundler.toml")),
            }
        }
        chosen
    };

    let platform = match (env::consts::OS, env::consts::ARCH) {
        ("macos", _) => "macos-universal".to_string(),
        (os, arch) => format!("{os}-{arch}"),
    };
    let variant = if headless { "headless" } else { "editor" };
    let out = PathBuf::from("dist").join(if headless { "vst3-headless" } else { "vst3" }).join(&platform);
    fs::create_dir_all(&out)?;
    let bundle = if env::consts::OS == "macos" { "bundle-universal" } else { "bundle" };

    let mut bare = Vec::new();
    for (n, (krate, name)) in chosen.iter().enumerate() {
        let gui = !headless && fs::read_to_string(Path::new(krate).join("Cargo.toml"))?.lines().any(|l| l.starts_with("gui ="));
        if !headless && !gui {
            bare.push(krate.as_str());
        }
        eprintln!("[{}/{}] {krate}{}", n + 1, chosen.len(), if gui { " (editor)" } else { "" });
        let mut call = vec![bundle.to_string(), krate.clone(), "--release".to_string()];
        if gui {
            call.extend(["--features".to_string(), "gui".to_string()]);
        }
        nih_plug_xtask::main_with_args("cargo xtask", call)?;
        let file = format!("{name}.vst3");
        let (built, dest) = (Path::new("target/bundled").join(&file), out.join(&file));
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        copy_dir(&built, &dest)?;
    }
    eprintln!("{} pedal(s) in {}", chosen.len(), out.display());
    if !bare.is_empty() {
        eprintln!("without an editor yet ({}): {}", bare.len(), bare.join(" "));
    }
    if !asked.is_empty() || no_archive {
        return Ok(());
    }

    // The archive holds the pedals just built and nothing else the folder may still hold.
    let version = output("git", &["describe", "--tags", "--always", "--dirty"], Path::new(".")).unwrap_or_else(|| "dev".to_string());
    let archive = format!("golem-fx-{version}-{platform}-{variant}.tar.gz");
    let names: Vec<String> = chosen.iter().map(|(_, name)| format!("{name}.vst3")).collect();
    let status = Command::new("tar").arg("czf").arg(Path::new("dist").canonicalize()?.join(&archive)).arg("-C").arg(&out).args(&names).status()?;
    if !status.success() {
        return fail(format!("dist: tar failed on {archive}"));
    }
    let dist = Path::new("dist");
    let mut archives: Vec<String> = fs::read_dir(dist)?.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|f| f.starts_with("golem-fx-") && f.ends_with(".tar.gz")).collect();
    archives.sort();
    let list: Vec<&str> = archives.iter().map(String::as_str).collect();
    match output("sha256sum", &list, dist) {
        Some(sums) => fs::write(dist.join("SHA256SUMS"), sums + "\n")?,
        None => eprintln!("sha256sum not found: dist/SHA256SUMS not written"),
    }
    eprintln!("dist/{archive}");
    Ok(())
}
