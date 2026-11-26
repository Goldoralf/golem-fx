// SPDX-License-Identifier: GPL-3.0-or-later
//! The root sieve on the coded core (README §5, from "Inputs have a noise floor" to "Annotated DIs"):
//! the first organ task. Synthetic strings as the prototype's (`proto/organ.py` `string`, −100 dBFS of
//! noise from the first sample), golem's DIs, and annotated takes when they exist.
//!
//! Everything runs through the pedal itself at 48 kHz (detector A finds the picks, the core runs at
//! 24 kHz), voiced as plainly as possible: Custom, no Leslie, no scanner, no percussion, no click,
//! `touch` 100 %. **Ghost-root output** is measured as the prototype does: the same input rendered
//! twice, once with every root band that is not within 30 ¢ of a played (and already picked)
//! fundamental forced out of the roots; the difference is the ghosts' output, in dB re the organ's.
//!
//! `cargo test --release -p gfx-organ --no-default-features sieve_ -- --ignored --nocapture`
//!
//! **Annotated DIs** (README §5, "Annotated DIs"): a folder (`GFX_ORGAN_DI_DIR`, default
//! `gfx-organ/bench/annotated`) of `<take>.wav` (mono or stereo, any rate, PCM or float) with a
//! `<take>.notes` beside each: one line per pick, `time_s note [string]`, `#` comments (at a line's start or after a space); `note` in
//! scientific pitch (`E2`, `G#3`, `Bb3`) or a MIDI number; `string` an integer (a string picked again
//! ends its last note). See the README, "As coded".

use super::*;
use gfx_dsp::lfo::Rng;
use std::f64::consts::PI;

const SR: f32 = 48000.0;
pub(crate) const NOISE: f64 = 1e-5;

pub(crate) fn note_hz(name: &str) -> f64 {
    if let Ok(m) = name.parse::<f64>() {
        return 440.0 * ((m - 69.0) / 12.0).exp2();
    }
    let b = name.as_bytes();
    let pc = match b[0].to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => panic!("note {name}"),
    } as i32;
    let (acc, rest) = match b.get(1) {
        Some(b'#') => (1, &name[2..]),
        Some(b'b') => (-1, &name[2..]),
        _ => (0, &name[1..]),
    };
    let oct: i32 = rest.parse().expect("octave");
    let m = 12 * (oct + 1) + pc + acc;
    440.0 * ((m as f64 - 69.0) / 12.0).exp2()
}

pub(crate) fn gauss(r: &mut Rng) -> f64 {
    (0..4).map(|_| r.next_f64() * 2.0 - 1.0).sum::<f64>() * (3.0f64 / 4.0).sqrt()
}

/// The prototype's string (`organ.py` `string`): partials |sin(π k p)| / k, B = 10⁻⁴, T60 3 s falling
/// with k, a 4 ms pick noise, random phases, peak `level`.
pub(crate) fn string(f0: f64, dur: f64, sr: f64, r: &mut Rng, p: f64, t0: f64, level: f64) -> Vec<f64> {
    let n = (dur * sr) as usize;
    let mut y = vec![0.0; n];
    let i0 = (t0 * sr) as usize;
    let mut k = 1.0f64;
    loop {
        let fk = k * f0 * (1.0 + 1e-4 * k * k).sqrt();
        if fk > 7000.0f64.min(0.45 * sr) {
            break;
        }
        let a = (PI * k * p).sin().abs() / k;
        let t60k = 3.0 / (1.0 + 0.15 * (k - 1.0));
        let ph = r.next_f64() * TAU;
        let w = TAU * fk / sr;
        let dec = (-6.91 / (t60k * sr)).exp();
        let mut env = a;
        for (i, v) in y.iter_mut().enumerate().skip(i0) {
            *v += env * (w * (i - i0) as f64 + ph).sin();
            env *= dec;
        }
        k += 1.0;
    }
    let nn = (0.004 * sr) as usize;
    for i in 0..nn {
        if i0 + i < n {
            let h = 0.5 + 0.5 * (PI * i as f64 / nn as f64).cos();
            y[i0 + i] += 0.3 * gauss(r) * h;
        }
    }
    let pk = y.iter().fold(1e-9f64, |a, v| a.max(v.abs()));
    y.iter().map(|v| level * v / pk).collect()
}

/// A played event: (pick time s, note, string id).
pub(crate) type Ev = (f64, f64, usize);

/// The prototype's `play`: every event a string; a string picked again is damped (3 ms) and restarts;
/// −100 dBFS of noise from the first sample. Chords normalised to a 0.5 peak.
pub(crate) fn play(events: &[Ev], dur: f64, seed: u64, sr: f64, norm: bool) -> Vec<f32> {
    let mut r = Rng::new(seed);
    let n = (dur * sr) as usize;
    let mut x = vec![0.0; n];
    let mut ev = events.to_vec();
    ev.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (i, e) in ev.iter().enumerate() {
        let p = 0.1 + 0.1 * r.next_f64();
        let mut y = string(e.1, dur, sr, &mut r, p, e.0, 0.25);
        if let Some(nx) = ev[i + 1..].iter().find(|f| f.2 == e.2) {
            let i1 = (nx.0 * sr) as usize;
            let n3 = (0.003 * sr) as usize;
            for k in 0..n3.min(i1) {
                y[i1 - n3 + k] *= 1.0 - k as f64 / n3 as f64;
            }
            y[i1..].iter_mut().for_each(|v| *v = 0.0);
        }
        for (a, b) in x.iter_mut().zip(&y) {
            *a += b;
        }
    }
    if norm {
        let pk = x.iter().fold(1e-9f64, |a, v| a.max(v.abs()));
        x.iter_mut().for_each(|v| *v *= 0.5 / pk);
    }
    x.iter().map(|v| (v + NOISE * gauss(&mut r)) as f32).collect()
}

/// The plain voicing of the bench: Custom with `drawbars`, no Leslie, no scanner, no percussion, no
/// click, `touch` 100 %.
pub(crate) fn plain(drawbars: &str) -> Organ {
    let mut o = Organ::new();
    let set = |o: &mut Organ, id: &str, v: f32| o.set_param(id, v);
    set(&mut o, "register", 9.0);
    for (i, c) in drawbars.chars().filter(|c| c.is_ascii_digit()).enumerate() {
        set(&mut o, PARAMS[p::DB16 + i].id, c.to_digit(10).unwrap() as f32);
    }
    for (id, v) in [("speaker", 0.0), ("vibrato", 0.0), ("perc", 0.0), ("click", 0.0), ("touch", 100.0), ("dry", -60.0)] {
        set(&mut o, id, v);
    }
    o.prepare(SR, 64, 1);
    o.reset();
    o
}

/// Per control tick (1 ms): the time (s), and per clean band (f̂, root decision, candidate).
pub(crate) struct Log {
    pub t: Vec<f64>,
    pub f: Vec<Vec<f64>>,
    pub root: Vec<Vec<bool>>,
}

/// Renders `x` through `o`, sample-block by 48 samples, forcing out of the roots every band that
/// `keep(t, f̂)` refuses (None: no mask); returns the output and the log.
pub(crate) fn render(o: &mut Organ, x: &[f32], keep: Option<&dyn Fn(f64, f64) -> bool>) -> (Vec<f32>, Log) {
    let mut y = x.to_vec();
    let mut log = Log { t: vec![], f: vec![], root: vec![] };
    let blk = 48;
    let mut k = 0;
    while k < y.len() {
        let e = (k + blk).min(y.len());
        let t = k as f64 / SR as f64;
        {
            let core = o.core_mut();
            let n = core.a.clean.n;
            if let Some(kf) = keep {
                for b in 0..n {
                    let f = core.a.clean.fhat[b];
                    core.a.sieve.keep[b] = kf(t, f);
                }
            }
            log.t.push(t);
            log.f.push(core.a.clean.fhat.clone());
            log.root.push((0..n).map(|b| core.a.sieve.holds_root(b) && core.a.clean.sel[b] > 0.5).collect());
        }
        o.process(&mut [&mut y[k..e]]);
        k = e;
    }
    (y, log)
}

pub(crate) fn cents(a: f64, b: f64) -> f64 {
    1200.0 * (a / b).log2()
}

/// Ghost output (dB re the organ's) per window (s, absolute).
pub(crate) fn ghost_db(full: &[f32], kept: &[f32], wins: &[(f64, f64)]) -> Vec<f64> {
    wins.iter()
        .map(|&(a, b)| {
            let (i0, i1) = ((a * SR as f64) as usize, ((b * SR as f64) as usize).min(full.len()));
            let (mut g, mut t) = (0.0, 0.0);
            for i in i0..i1 {
                let d = (full[i] - kept[i]) as f64;
                g += d * d;
                t += (full[i] as f64).powi(2);
            }
            10.0 * (g / (t + 1e-20) + 1e-15).log10()
        })
        .collect()
}

/// The keep rule: a band within 30 ¢ of a note already picked.
pub(crate) fn keeper(events: &[Ev]) -> impl Fn(f64, f64) -> bool + '_ {
    move |t, f| events.iter().any(|e| t >= e.0 && cents(f, e.1).abs() < 30.0)
}

/// When note `f0` first becomes a root after `t0` (ms after it), from the log.
pub(crate) fn bloom_ms(log: &Log, f0: f64, t0: f64) -> Option<f64> {
    for (i, &t) in log.t.iter().enumerate() {
        if t > t0 && log.root[i].iter().zip(&log.f[i]).any(|(&r, &f)| r && cents(f, f0).abs() < 30.0) {
            return Some(1000.0 * (t - t0));
        }
    }
    None
}

pub(crate) fn max_roots(log: &Log) -> usize {
    log.root.iter().map(|r| r.iter().filter(|x| **x).count()).max().unwrap_or(0)
}

/// Share of ticks in (a, b) where `f0` is a root.
pub(crate) fn root_share(log: &Log, f0: f64, a: f64, b: f64) -> f64 {
    let (mut on, mut tot) = (0usize, 0usize);
    for (i, &t) in log.t.iter().enumerate() {
        if t > a && t < b {
            tot += 1;
            if log.root[i].iter().zip(&log.f[i]).any(|(&r, &f)| r && cents(f, f0).abs() < 30.0) {
                on += 1;
            }
        }
    }
    on as f64 / tot.max(1) as f64
}

pub(crate) struct Case {
    pub ghost: Vec<f64>,
    pub log: Log,
}

pub(crate) fn run_case(events: &[Ev], dur: f64, seed: u64, reg: &str, wins: &[(f64, f64)], norm: bool) -> Case {
    let x = play(events, dur, seed, SR as f64, norm);
    let mut a = plain(reg);
    let (full, log) = render(&mut a, &x, None);
    let mut b = plain(reg);
    let k = keeper(events);
    let (kept, _) = render(&mut b, &x, Some(&k));
    Case { ghost: ghost_db(&full, &kept, wins), log }
}

const SINGLES: [&str; 7] = ["E2", "A2", "D3", "G3", "B3", "E4", "A4"];
const WINS: [(f64, f64); 3] = [(0.07, 0.20), (0.20, 0.45), (0.45, 1.4)];

pub(crate) fn fmt_db(v: f64) -> String {
    if v < -140.0 {
        "none".into()
    } else {
        format!("{v:.1}")
    }
}

/// Single notes E2 … A4, four seeds: ghost output per window after the pick at 50 ms, when the
/// fundamental becomes a root, the most roots at once.
pub(crate) fn single_notes() -> Vec<(String, [f64; 3], (f64, f64), usize)> {
    let mut rows = vec![];
    for nm in SINGLES {
        let f0 = note_hz(nm);
        let (mut worst, mut bl, mut nr) = ([f64::MIN; 3], (f64::MAX, f64::MIN), 0);
        for seed in 1..=4u64 {
            let ev = [(0.05, f0, 0)];
            let wins: Vec<(f64, f64)> = WINS.iter().map(|w| (0.05 + w.0, 0.05 + w.1)).collect();
            let c = run_case(&ev, 1.5, seed, "888000000", &wins, false);
            for i in 0..3 {
                worst[i] = worst[i].max(c.ghost[i]);
            }
            if let Some(b) = bloom_ms(&c.log, f0, 0.05) {
                bl = (bl.0.min(b), bl.1.max(b));
            }
            nr = nr.max(max_roots(&c.log));
        }
        rows.push((nm.to_string(), worst, bl, nr));
    }
    rows
}

pub(crate) const CHORDS: [(&str, &[&str]); 6] = [
    ("E5 power", &["E2", "B2", "E3"]),
    ("open E", &["E2", "B2", "E3", "G#3", "B3", "E4"]),
    ("open C", &["C3", "E3", "G3", "C4", "E4"]),
    ("A minor barre", &["A2", "E3", "A3", "C4", "E4"]),
    ("E4 + G#4", &["E4", "G#4"]),
    ("E2 + G#2", &["E2", "G#2"]),
];

/// A note is covered when it is an integer multiple (within 15 ¢) of a lower note of the same strum.
pub(crate) fn covered(f: f64, others: &[f64]) -> bool {
    others.iter().any(|&g| g < f * 0.99 && {
        let k = (f / g).round();
        k >= 2.0 && cents(f, k * g).abs() < 15.0 + 866.0 * 1e-4 * k * k
    })
}

#[test]
#[ignore]
fn sieve_report() {
    println!("== Single notes (88 8000 000, −100 dBFS noise floor, 4 seeds): ghost-root output (dB re the organ, worst) at 70–200 / 200–450 / 450–1400 ms; root after the pick; max roots");
    for (nm, g, bl, nr) in single_notes() {
        println!("  {nm:4} {:>6} / {:>6} / {:>6} | root {:.0} – {:.0} ms | max roots {nr}", fmt_db(g[0]), fmt_db(g[1]), fmt_db(g[2]), bl.0, bl.1);
    }
    println!("== Chords (20 ms strum, 3 seeds): ghost worst per window; each note: root at (ms, worst seed) or covered / missed; max roots");
    for (name, notes) in CHORDS {
        let fs: Vec<f64> = notes.iter().map(|n| note_hz(n)).collect();
        let ev: Vec<Ev> = fs.iter().enumerate().map(|(i, &f)| (0.05 + 0.02 * i as f64 / (fs.len() - 1).max(1) as f64, f, i)).collect();
        let mut worst = [f64::MIN; 3];
        let mut bl: Vec<Option<f64>> = vec![Some(0.0); fs.len()];
        let mut nr = 0;
        for seed in 1..=3u64 {
            let wins: Vec<(f64, f64)> = WINS.iter().map(|w| (0.05 + w.0, 0.05 + w.1)).collect();
            let c = run_case(&ev, 1.5, seed, "888000000", &wins, true);
            for i in 0..3 {
                worst[i] = worst[i].max(c.ghost[i]);
            }
            for (i, &f) in fs.iter().enumerate() {
                bl[i] = match (bl[i], bloom_ms(&c.log, f, 0.05)) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    _ => None,
                };
            }
            nr = nr.max(max_roots(&c.log));
        }
        let notes_s: Vec<String> = notes
            .iter()
            .zip(&fs)
            .zip(&bl)
            .map(|((n, &f), b)| match b {
                Some(v) => format!("{n} {v:.0}"),
                None if covered(f, &fs) => format!("{n} covered"),
                None => format!("{n} MISSED"),
            })
            .collect();
        println!("  {name:14} {:>6} / {:>6} / {:>6} | {} | max roots {nr}", fmt_db(worst[0]), fmt_db(worst[1]), fmt_db(worst[2]), notes_s.join(", "));
    }
    sequences_report();
    di_report();
    annotated_report();
}

pub(crate) fn sequences() -> Vec<(&'static str, Vec<Ev>)> {
    let n = note_hz;
    let arp: Vec<Ev> = ["E2", "B2", "E3", "G#3", "B3", "E4"].iter().enumerate().map(|(i, s)| (0.05 + 0.3 * i as f64, n(s), i)).collect();
    let mut strum3 = vec![];
    for j in 0..3 {
        for (i, s) in ["E2", "B2", "E3", "G#3", "B3", "E4"].iter().enumerate() {
            strum3.push((0.05 + 0.5 * j as f64 + 0.004 * i as f64, n(s), i));
        }
    }
    vec![
        ("arpeggio open E, 0.3 s apart, ringing", arp),
        ("melody E3 E4 B3 over a ringing E2", vec![(0.05, n("E2"), 0), (0.5, n("E3"), 1), (0.9, n("E4"), 2), (1.3, n("B3"), 3)]),
        ("bass E2 enters under a held E4", vec![(0.05, n("E4"), 0), (0.6, n("E2"), 1)]),
        ("bass E2 x3 under an E4", vec![(0.05, n("E4"), 2), (0.5, n("E2"), 0), (0.95, n("E2"), 0), (1.4, n("E2"), 0)]),
        ("octave dyad E3 + E4 together", vec![(0.05, n("E3"), 0), (0.055, n("E4"), 1)]),
        ("E2 picked x5", (0..5).map(|i| (0.05 + 0.4 * i as f64, n("E2"), 0)).collect()),
        ("A2 picked x5", (0..5).map(|i| (0.05 + 0.4 * i as f64, n("A2"), 0)).collect()),
        ("open E strummed x3, 0.5 s apart", strum3),
    ]
}

fn sequences_report() {
    println!("== Sequences (4 seeds): share of seeds where each note is a root 160 – 280 ms after its pick; ghost from 70 ms (worst / median)");
    for (name, ev) in sequences() {
        let dur = ev.iter().map(|e| e.0).fold(0.0, f64::max) + 0.45;
        let mut hits = vec![0usize; ev.len()];
        let mut gh = vec![];
        for seed in 1..=4u64 {
            let c = run_case(&ev, dur, seed, "888000000", &[(0.12, dur)], false);
            for (i, e) in ev.iter().enumerate() {
                if root_share(&c.log, e.1, e.0 + 0.16, e.0 + 0.28) > 0.5 {
                    hits[i] += 1;
                }
            }
            gh.push(c.ghost[0]);
        }
        gh.sort_by(|a, b| a.total_cmp(b));
        let names: Vec<String> = ev.iter().zip(&hits).map(|(e, h)| format!("{:.0}Hz {}/4", e.1, h)).collect();
        println!("  {name:40} {} | ghost {} / {}", names.join(" "), fmt_db(*gh.last().unwrap()), fmt_db(0.5 * (gh[1] + gh[2])));
    }
}

// ---------------------------------------------------------------- DIs

/// Minimal WAV reader (gfx-bench's): PCM 16 / 24 / 32 and float 32, mixed to mono.
pub(crate) fn read_wav(path: &std::path::Path) -> Option<(Vec<f32>, f32)> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    let (mut fmt, mut ch, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
    let mut body: Option<&[u8]> = None;
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().ok()?) as usize;
        let b = &data[pos + 8..(pos + 8 + size).min(data.len())];
        if id == b"fmt " {
            fmt = u16::from_le_bytes([b[0], b[1]]);
            ch = u16::from_le_bytes([b[2], b[3]]);
            rate = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            bits = u16::from_le_bytes([b[14], b[15]]);
            if fmt == 0xFFFE && b.len() >= 26 {
                fmt = u16::from_le_bytes([b[24], b[25]]);
            }
        } else if id == b"data" {
            body = Some(b);
        }
        pos += 8 + size + (size & 1);
    }
    let body = body?;
    let ch = ch.max(1) as usize;
    let bps = (bits / 8) as usize;
    let frames = body.len() / (bps * ch);
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut acc = 0.0f32;
        for c in 0..ch {
            let o = (f * ch + c) * bps;
            let s = &body[o..o + bps];
            acc += match (fmt, bits) {
                (3, 32) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                (_, 16) => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
                (_, 24) => ((((s[2] as i32) << 24) | ((s[1] as i32) << 16) | ((s[0] as i32) << 8)) >> 8) as f32 / 8_388_608.0,
                (_, 32) => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
                _ => return None,
            };
        }
        out.push(acc / ch as f32);
    }
    Some((out, rate as f32))
}

/// Linear resampling to 48 kHz (enough for a sieve statistic).
pub(crate) fn to_48k(x: &[f32], sr: f32) -> Vec<f32> {
    if (sr - SR).abs() < 1.0 {
        return x.to_vec();
    }
    let n = (x.len() as f64 * SR as f64 / sr as f64) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 * sr as f64 / SR as f64;
            let k = t.floor() as usize;
            let f = (t - k as f64) as f32;
            let a = x[k.min(x.len() - 1)];
            let b = x[(k + 1).min(x.len() - 1)];
            a + (b - a) * f
        })
        .collect()
}

pub(crate) fn di_dir() -> std::path::PathBuf {
    std::env::var("GFX_DI_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("golem/library/di-refs/guitar"))
}

/// Roots on the 12-TET grid (tuning offset removed), roots per sounding frame, time over 6 roots.
pub(crate) fn di_stats(log: &Log) -> (f64, f64, [usize; 3], f64) {
    let mut devs = vec![];
    let mut counts = vec![];
    for (i, r) in log.root.iter().enumerate() {
        let c = r.iter().filter(|x| **x).count();
        if c > 0 {
            counts.push(c);
        }
        for (b, &on) in r.iter().enumerate() {
            if on {
                let m = 69.0 + 12.0 * (log.f[i][b] / 440.0).log2();
                devs.push(100.0 * (m - m.round()));
            }
        }
    }
    if devs.is_empty() {
        return (0.0, 0.0, [0; 3], 0.0);
    }
    let mut d = devs.clone();
    d.sort_by(|a, b| a.total_cmp(b));
    let off = d[d.len() / 2];
    let on = devs.iter().filter(|&&v| {
        let x = v - off;
        let x = x - 100.0 * (x / 100.0).round();
        x.abs() <= 30.0
    });
    let share = on.count() as f64 / devs.len() as f64;
    counts.sort();
    let q = |p: f64| counts[((counts.len() - 1) as f64 * p) as usize];
    let over6 = log.root.iter().filter(|r| r.iter().filter(|x| **x).count() > 6).count() as f64 / log.root.len().max(1) as f64;
    (share, off, [q(0.5), q(0.95), *counts.last().unwrap()], over6)
}

fn di_report() {
    println!("== golem's DIs (first 12 s; no ground truth): roots within ±30 ¢ of the grid (offset), roots per sounding frame (median / 95 % / max), time over 6 roots");
    for take in ["clean", "lead-clean", "funk"] {
        let p = di_dir().join(format!("{take}.wav"));
        let Some((x, sr)) = read_wav(&p) else {
            println!("  {take}: not found ({})", p.display());
            continue;
        };
        let x = to_48k(&x[..x.len().min((12.0 * sr) as usize)], sr);
        let mut o = plain("888000000");
        let (_, log) = render(&mut o, &x, None);
        let (share, off, q, over6) = di_stats(&log);
        println!("  {take:11} {:.1} % (offset {off:+.1} ¢) | {} / {} / {} | over 6: {:.2} %", 100.0 * share, q[0], q[1], q[2], 100.0 * over6);
    }
}

// ---------------------------------------------------------------- annotated DIs

pub(crate) fn annotated_dir() -> std::path::PathBuf {
    std::env::var("GFX_ORGAN_DI_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bench/annotated"))
}

/// A `.notes` file: `time_s note [string]` per line.
pub(crate) fn read_notes(text: &str) -> Vec<Ev> {
    let mut v = vec![];
    for (i, line) in text.lines().enumerate() {
        // a comment starts with `#` at the line's start or after a space (`G#3` is a note)
        let line = if line.trim_start().starts_with('#') { "" } else { line.split(" #").next().unwrap_or("").trim() };
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let t: f64 = f[0].parse().unwrap_or_else(|_| panic!("line {}: time", i + 1));
        let hz = note_hz(f.get(1).unwrap_or_else(|| panic!("line {}: note", i + 1)));
        let s: usize = f.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000 + i);
        v.push((t, hz, s));
    }
    v
}

/// Annotated takes: every played note a root (or covered by a note of its own strum, 30 ms) within
/// 200 ms of its pick and while it rings; roots that are no played note, and for how long.
pub(crate) fn annotated_check(ev: &[Ev], log: &Log) -> (usize, usize, usize, f64) {
    let (mut ok, mut cov, mut missed) = (0, 0, 0);
    for (i, e) in ev.iter().enumerate() {
        let end = ev.iter().skip(i + 1).find(|f| f.2 == e.2).map_or(e.0 + 1.0, |f| f.0).min(e.0 + 1.0);
        if root_share(log, e.1, e.0 + 0.2, end) > 0.5 {
            ok += 1;
        } else {
            let strum: Vec<f64> = ev.iter().filter(|f| (f.0 - e.0).abs() < 0.03).map(|f| f.1).collect();
            if covered(e.1, &strum) {
                cov += 1;
            } else {
                missed += 1;
            }
        }
    }
    // the longest run of a root that is no sounding played note (ms)
    let mut worst: f64 = 0.0;
    let nb = log.root.first().map_or(0, |r| r.len());
    let mut run = vec![0usize; nb];
    for (i, r) in log.root.iter().enumerate() {
        let t = log.t[i];
        for b in 0..nb {
            let f = log.f[i][b];
            let played = ev.iter().enumerate().any(|(j, e)| {
                let end = ev.iter().skip(j + 1).find(|g| g.2 == e.2).map_or(f64::INFINITY, |g| g.0);
                t >= e.0 && t < end + 0.1 && cents(f, e.1).abs() < 30.0
            });
            run[b] = if r[b] && !played { run[b] + 1 } else { 0 };
            worst = worst.max(run[b] as f64 * (log.t[1] - log.t[0]) * 1000.0);
        }
    }
    (ok, cov, missed, worst)
}

fn annotated_report() {
    let dir = annotated_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        println!("== Annotated DIs: none yet ({}: drop <take>.wav + <take>.notes there)", dir.display());
        return;
    };
    println!("== Annotated DIs ({}): notes rooted / covered / MISSED; longest root that is no played note", dir.display());
    let mut any = false;
    let mut names: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "notes")).collect();
    names.sort();
    for np in names {
        let wav = np.with_extension("wav");
        let (Some((x, sr)), Ok(text)) = (read_wav(&wav), std::fs::read_to_string(&np)) else {
            println!("  {}: no readable .wav beside it", np.display());
            continue;
        };
        any = true;
        let ev = read_notes(&text);
        let x = to_48k(&x, sr);
        let mut o = plain("888000000");
        let (_, log) = render(&mut o, &x, None);
        let (ok, cov, missed, worst) = annotated_check(&ev, &log);
        println!("  {:24} {ok} / {cov} / {missed} of {} | ghost root up to {worst:.0} ms (bound 20)", np.file_stem().unwrap().to_string_lossy(), ev.len());
    }
    if !any {
        println!("  (no take)");
    }
}

#[test]
fn notes_format() {
    let ev = read_notes("# a comment\n0.050 E2 6\n0.30 G#3 3 # third\n1.2 64\n");
    assert_eq!(ev.len(), 3);
    assert!((ev[0].1 - 82.407).abs() < 0.01 && ev[0].2 == 6);
    assert!((ev[1].1 - 207.65).abs() < 0.01);
    assert!((ev[2].1 - 329.63).abs() < 0.01);
    assert!((note_hz("Bb3") - 233.08).abs() < 0.01);
}

/// The sieve's entry exam on single notes (README §5 "No ghost notes on single notes"): ghost-root
/// output ≤ −40 dB from 70 ms on, one root, the fundamental a root within 200 ms.
#[test]
fn sieve_single_notes() {
    for (nm, g, bl, nr) in single_notes() {
        println!("{nm}: ghost {:?} dB, root {:.0} – {:.0} ms, max roots {nr}", g.map(|v| (v * 10.0).round() / 10.0), bl.0, bl.1);
        assert!(g.iter().all(|&v| v <= -40.0), "{nm}: ghost {g:?}");
        assert!(nr <= 1, "{nm}: {nr} roots");
        assert!(bl.1 <= 200.0, "{nm}: root at {bl:?} ms");
    }
}
