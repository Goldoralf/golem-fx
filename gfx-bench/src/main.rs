// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-bench command line.
//!
//!   gfx-bench list
//!   gfx-bench check <effect> [--sr 48000]
//!   gfx-bench render <effect> --in <wav> --out <wav> [--set id=value]… [--stereo] [--block 512]

use gfx_bench::checks;
use gfx_bench::{apply_settings, fresh, render, wav, EFFECTS};
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  gfx-bench list\n  gfx-bench check <effect> [--sr <rate>]\n  gfx-bench render <effect> --in <wav> --out <wav> [--set id=value]... [--stereo] [--block n]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") => {
            for e in EFFECTS {
                println!("{e}");
            }
            ExitCode::SUCCESS
        }
        Some("check") => check(&args[1..]),
        Some("render") => render_cmd(&args[1..]),
        _ => usage(),
    }
}

fn check(args: &[String]) -> ExitCode {
    let Some(name) = args.first() else { return usage() };
    if gfx_bench::make(name).is_none() {
        eprintln!("unknown effect {name}");
        return ExitCode::from(2);
    }
    let mut opts = gfx_bench::options(name);
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--sr" && i + 1 < args.len() {
            opts.sample_rates = vec![args[i + 1].parse().unwrap_or(48000.0)];
            i += 1;
        }
        i += 1;
    }
    println!("# gfx-bench check {name}\n");
    println!("| Check | Rate | Result | Detail |\n|---|---|---|---|");
    let results = checks::run_all(name, &opts);
    let mut failed = 0;
    for r in &results {
        if !r.pass {
            failed += 1;
        }
        println!("| {} | {} | {} | {} |", r.name, r.sample_rate, if r.pass { "pass" } else { "**FAIL**" }, r.detail);
    }
    let (mono, stereo) = checks::cpu(name);
    println!("\nCPU at 48 kHz, blocks of 128: {mono:.1} ns / sample mono, {stereo:.1} ns / frame stereo");
    println!("\n{} checks, {} failed", results.len(), failed);
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn render_cmd(args: &[String]) -> ExitCode {
    let Some(name) = args.first() else { return usage() };
    let (mut input, mut output, mut settings, mut stereo, mut block) = (None, None, Vec::new(), false, 512usize);
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--in" => {
                input = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--out" => {
                output = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--set" => {
                if let Some((k, v)) = args.get(i + 1).and_then(|s| s.split_once('=')) {
                    settings.push((k.to_string(), v.parse::<f32>().unwrap_or(0.0)));
                }
                i += 1;
            }
            "--block" => {
                block = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(512);
                i += 1;
            }
            "--stereo" => stereo = true,
            _ => return usage(),
        }
        i += 1;
    }
    let (Some(input), Some(output)) = (input, output) else { return usage() };
    let mut audio = match wav::read(&input) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}: {e}", input.display());
            return ExitCode::FAILURE;
        }
    };
    if stereo && audio.channels.len() == 1 {
        audio.channels.push(audio.channels[0].clone());
    }
    let mut e = fresh(name, audio.sample_rate as f32, audio.channels.len(), block);
    apply_settings(e.as_mut(), &settings);
    let out = render(e.as_mut(), &audio.channels, block);
    if let Err(err) = wav::write(&output, &wav::Audio { sample_rate: audio.sample_rate, channels: out }) {
        eprintln!("{}: {err}", output.display());
        return ExitCode::FAILURE;
    }
    println!("{} → {}", input.display(), output.display());
    ExitCode::SUCCESS
}
