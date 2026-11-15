// SPDX-License-Identifier: GPL-3.0-or-later
//! Generates the static tables as const data (README §2.5, §2.2, §2.7): nothing is computed when the
//! plugin turns on.
//! - the vocal make-up of each compensated type at d = 0 … 48 dB: the RMS ratio in / out of the
//!   reference vowel ('a', 187.3 Hz, −18 dBFS RMS) through the type at 2× + ADAA, after its DC block;
//! - the small-signal gain S(type, d) the guard reads: pink noise at −90 dBFS through the type and its
//!   make-up (the README's −60 dBFS under-reads the high-gain types: the guard's cut takes the signal
//!   far lower, where a fuzz at 48 dB gains 5 dB more);
//! - per object, the pink level table over `position` × `decay` × `size`, the largest peak over
//!   `decay` × `size`, and the voice constant.
//!
//! The type code (src/types.rs), the reference signals (src/voice.rs) and the objects
//! (src/objects.rs) are the plugin's own, included here.

#[path = "src/voice.rs"]
mod voice;
#[path = "src/types.rs"]
mod types;
#[path = "src/objects.rs"]
mod objects;

use std::fmt::Write;
use types::{render_type, rms_db, SlotParams, Type, MAKEUP_MAX_DB, MAKEUP_MIN_DB, SPUTTER_CAP_DB, TABLE_POINTS, TYPES};

const FS: f64 = 48000.0;
/// The small-signal table's level (dBFS RMS).
const SMALL_DBFS: f64 = -90.0;

fn table<T: std::fmt::Debug>(s: &mut String, name: &str, ty: &str, rows: &[(String, Vec<T>)]) {
    writeln!(s, "pub static {name}: [[{ty}; {}]; {}] = [", rows[0].1.len(), rows.len()).unwrap();
    for (label, r) in rows {
        write!(s, "    // {label}\n    [").unwrap();
        for v in r {
            write!(s, "{v:?}, ").unwrap();
        }
        writeln!(s, "],").unwrap();
    }
    writeln!(s, "];").unwrap();
}

/// The largest gain (dB) of `y` over `x` in third-octave bands 50 Hz – 16 kHz (2nd-order band-passes).
fn band_max_gain(x: &[f64], y: &[f64], from: usize) -> f64 {
    use gfx_dsp::svf::{Svf, SvfMode};
    let mut best = f64::NEG_INFINITY;
    for k in 0..26 {
        let fc = 50.0 * 2f64.powf(k as f64 / 3.0);
        let q = 1.0 / (2f64.powf(1.0 / 6.0) - 2f64.powf(-1.0 / 6.0));
        let band = |s: &[f64]| {
            let mut a = Svf::new();
            let mut b = Svf::new();
            a.set(SvfMode::BandPass, fc as f32, q as f32, 0.0, FS as f32);
            b.set(SvfMode::BandPass, fc as f32, q as f32, 0.0, FS as f32);
            let v: Vec<f64> = s.iter().map(|&v| b.process(a.process(v as f32)) as f64).collect();
            rms_db(&v, from)
        };
        best = best.max(band(y) - band(x));
    }
    best
}

fn main() {
    for f in ["build.rs", "src/voice.rs", "src/types.rs", "src/objects.rs", "../gfx-dsp/src/shapers.rs", "../gfx-dsp/src/modal.rs"] {
        println!("cargo:rerun-if-changed={f}");
    }
    // ---- the dirt: make-up and small-signal tables
    let vowel = voice::vowel(voice::REF_F0, (0.25 * FS) as usize, voice::VOWEL_A, voice::REF_DB, FS);
    let from = (0.15 * FS) as usize;
    let vin = rms_db(&vowel, from);
    let pink = voice::pink((0.4 * FS) as usize, SMALL_DBFS, 0x9E37);
    let pfrom = (0.05 * FS) as usize;
    let pin = rms_db(&pink, pfrom);
    let mut makeup = Vec::new();
    let mut small = Vec::new();
    for ty in TYPES {
        let mut m = Vec::with_capacity(TABLE_POINTS);
        let mut sm = Vec::with_capacity(TABLE_POINTS);
        for k in 0..TABLE_POINTS {
            let d = k as f64;
            let mk = if ty.compensated() {
                // Rasp's make-up is its Tube's (the noise rides on top at −12 dB re the band)
                let measured = if ty == Type::Rasp { Type::Tube } else { ty };
                let y = render_type(&SlotParams::new(measured, d, 1.0, 60.0), &vowel, FS);
                let cap = if ty == Type::Sputter { SPUTTER_CAP_DB } else { MAKEUP_MAX_DB };
                (vin - rms_db(&y, from)).clamp(MAKEUP_MIN_DB, cap)
            } else {
                0.0
            };
            let y = render_type(&SlotParams::new(ty, d, 10f64.powf(mk / 20.0), 60.0), &pink, FS);
            let mut s = rms_db(&y, pfrom) - pin;
            if matches!(ty, Type::Decimate | Type::Ring) {
                // linear but time-varying: they move energy across the spectrum (images, sidebands), so
                // a band can gain while the whole loses. The guard's cap is the worst frequency's
                // (README §2.2): the largest third-octave gain
                s = s.max(band_max_gain(&pink, &y, pfrom));
            }
            m.push(mk as f32);
            sm.push(s.max(-60.0) as f32);
        }
        makeup.push((format!("{ty:?}"), m));
        small.push((format!("{ty:?}"), sm));
    }
    // ---- the objects
    let mut level = Vec::new();
    let mut peak = Vec::new();
    let mut vc = Vec::new();
    for (o, name) in objects::OBJECT_NAMES.iter().enumerate() {
        let (l, p) = objects::level_tables(o);
        let v = -objects::voice_level(o, &l);
        level.push((name.to_string(), l.iter().map(|&x| x as f32).collect::<Vec<f32>>()));
        peak.push((name.to_string(), p.iter().map(|&x| x as f32).collect::<Vec<f32>>()));
        vc.push(v as f32);
    }
    let mut s = String::new();
    writeln!(s, "// Generated by build.rs from src/types.rs, src/voice.rs and src/objects.rs. Do not edit.").unwrap();
    table(&mut s, "MAKEUP_DB", "f32", &makeup);
    table(&mut s, "SMALL_DB", "f32", &small);
    table(&mut s, "BOX_LEVEL_DB", "f32", &level);
    table(&mut s, "BOX_PEAK_DB", "f32", &peak);
    writeln!(s, "pub static BOX_VOICE_DB: [f32; {}] = {vc:?};", vc.len()).unwrap();
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("tables.rs");
    std::fs::write(out, s).unwrap();
}
