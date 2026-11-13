// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §6).

use crate::Glitch;
use gfx_dsp::Effect;

fn render(e: &mut Glitch, x: &[f32], block: usize) -> Vec<f32> {
    let mut y = x.to_vec();
    for chunk in y.chunks_mut(block) {
        e.process(&mut [chunk]);
    }
    y
}

fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut r = gfx_dsp::lfo::Rng::new(seed);
    (0..n).map(|_| 0.3 * r.bipolar()).collect()
}

fn neutral(e: &mut Glitch) {
    e.set_param("fx_a", 0.0);
    e.set_param("mix", 100.0);
    e.set_param("hpf", 20.0);
    e.set_param("lpf", 20000.0);
    e.set_param("damp", 0.0);
}

/// Every stage neutral, each size and window: the engine's output is the input delayed by size + hop,
/// to −90 dB.
#[test]
fn identity_every_size_and_window() {
    use crate::engine::{Engine, FrameSet, SIZES};
    let sr = 48000.0;
    for size in 0..7 {
        for window in 0..3 {
            let mut e = Engine::new();
            e.size_idx = size;
            e.window = window;
            e.prepare(sr, 2);
            let lat = SIZES[size] * 5 / 4;
            assert_eq!(e.latency(), lat);
            let n = lat + 3 * SIZES[size] + 4096;
            let x = noise(n, 3);
            let x2 = noise(n, 4);
            let mut worst = 0.0f32;
            for i in 0..n {
                let mut y = [0.0; 2];
                e.tick(&[x[i], x2[i]], &mut y);
                if e.want_latch {
                    e.set = FrameSet::default();
                    e.want_latch = false;
                }
                if i >= lat + SIZES[size] {
                    worst = worst.max((y[0] - x[i - lat]).abs()).max((y[1] - x2[i - lat]).abs());
                }
            }
            let db = 20.0 * (worst / 0.3).max(1e-12).log10();
            assert!(db < -90.0, "size {size} window {window}: residual {db:.1} dB");
        }
    }
}

fn sine(n: usize, f: f32, sr: f32) -> Vec<f32> {
    (0..n).map(|i| 0.25 * (2.0 * std::f32::consts::PI * f * i as f32 / sr).sin()).collect()
}

/// Magnitude spectrum (Hann) of `x[from..from + 65536]`, bin = sr / 65536.
fn spectrum(x: &[f32], from: usize) -> Vec<f32> {
    use realfft::RealFftPlanner;
    let n = 65536;
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
    let mut buf: Vec<f32> = (0..n).map(|i| x[from + i] * (0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut buf, &mut out).unwrap();
    out.iter().map(|z| z.norm()).collect()
}

fn peak_hz(sp: &[f32], sr: f32) -> f32 {
    let k = (1..sp.len()).max_by(|&a, &b| sp[a].partial_cmp(&sp[b]).unwrap()).unwrap();
    k as f32 * sr / 65536.0
}

fn wet_only(e: &mut Glitch) {
    neutral(e);
}

#[test]
fn drift_moves_up_and_pitch_by_octave() {
    let sr = 48000.0;
    let n = 2 * 48000 + 65536;
    let x = sine(n, 1000.0, sr);
    let mut e = Glitch::new();
    e.prepare(sr, 512, 1);
    wet_only(&mut e);
    e.set_param("drift", 200.0);
    e.reset();
    let y = render(&mut e, &x, 256);
    let f = peak_hz(&spectrum(&y, n - 65536), sr);
    assert!((f - 1200.0).abs() < 3.0, "drift +200 Hz: peak at {f}");
    // image rejected
    let sp = spectrum(&y, n - 65536);
    let at = |hz: f32| sp[(hz * 65536.0 / sr).round() as usize];
    let img = 20.0 * (at(800.0) / at(1200.0)).log10();
    assert!(img < -35.0, "image {img:.1} dB");

    let x = sine(n, 440.0, sr);
    let mut e = Glitch::new();
    e.prepare(sr, 512, 1);
    wet_only(&mut e);
    e.set_param("pitch", 12.0);
    e.set_param("size", 3.0);
    e.reset();
    let y = render(&mut e, &x, 256);
    let f = peak_hz(&spectrum(&y, n - 65536), sr);
    assert!((f - 880.0).abs() < 3.0, "pitch +12: peak at {f}");
}

/// Each effect at 0 % is neutral; at 100 % finite and bounded, on noise and on a chord.
#[test]
fn effects_bounded() {
    let sr = 48000.0;
    let n = 48000;
    let mut x = noise(n, 5);
    for (i, v) in x.iter_mut().enumerate() {
        let t = i as f32 / sr;
        *v = 0.5 * *v + 0.2 * ((2.0 * std::f32::consts::PI * 196.0 * t).sin() + (2.0 * std::f32::consts::PI * 247.0 * t).sin() + (2.0 * std::f32::consts::PI * 294.0 * t).sin());
    }
    let reference = {
        let mut e = Glitch::new();
        e.prepare(sr, 512, 1);
        wet_only(&mut e);
        e.reset();
        render(&mut e, &x, 512)
    };
    for fx in 1..30 {
        for amt in [0.0f32, 100.0] {
            let mut e = Glitch::new();
            e.prepare(sr, 512, 1);
            wet_only(&mut e);
            e.set_param("fx_a", fx as f32);
            e.set_param("amt_a", amt);
            e.set_param("lo_a", 3.0);
            e.set_param("hi_a", 9.0);
            e.reset();
            let y = render(&mut e, &x, 512);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 1.0), "fx {fx} at {amt}: not bounded");
            if amt == 0.0 {
                let d = y.iter().zip(&reference).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
                assert!(d < 1e-6, "fx {fx} at 0 %: not neutral ({d})");
            }
        }
    }
}

/// Band Cut at 100 % on 1 – 2 kHz: pink-ish noise down by 40 dB there.
#[test]
fn band_cut_depth() {
    let sr = 48000.0;
    let n = 2 * 48000 + 65536;
    let x = noise(n, 9);
    let mut e = Glitch::new();
    e.prepare(sr, 512, 1);
    wet_only(&mut e);
    e.set_param("fx_a", 4.0);
    e.set_param("amt_a", 100.0);
    e.set_param("lo_a", (1000.0f32 / 20.0).log2());
    e.set_param("hi_a", (2000.0f32 / 20.0).log2());
    e.reset();
    let y = render(&mut e, &x, 512);
    let sx = spectrum(&x, n - 65536 - 2560);
    let sy = spectrum(&y, n - 65536);
    let band = |s: &[f32], a: f32, b: f32| -> f32 {
        let (ka, kb) = ((a * 65536.0 / sr) as usize, (b * 65536.0 / sr) as usize);
        s[ka..kb].iter().map(|v| v * v).sum::<f32>()
    };
    let d = 10.0 * (band(&sy, 1150.0, 1850.0) / band(&sx, 1150.0, 1850.0)).log10();
    let keep = 10.0 * (band(&sy, 3000.0, 6000.0) / band(&sx, 3000.0, 6000.0)).log10();
    assert!(d < -40.0, "band cut only {d:.1} dB");
    assert!(keep.abs() < 1.0, "outside the band {keep:.1} dB");
}

/// Sustain is a T60 in time: the same decay at 1,024 and 8,192 points (±15 %).
#[test]
fn sustain_is_size_independent() {
    let sr = 48000.0;
    let mut t60 = Vec::new();
    for size in [1.0f32, 4.0] {
        let n = 5 * 48000;
        let mut x = sine(n, 440.0, sr);
        for v in x.iter_mut().skip(48000) {
            *v = 0.0;
        }
        let mut e = Glitch::new();
        e.prepare(sr, 512, 1);
        wet_only(&mut e);
        e.set_param("sustain", 2000.0);
        e.set_param("size", size);
        e.reset();
        let y = render(&mut e, &x, 512);
        // RMS in 50 ms windows; time from −10 dB to −40 dB re the held level, × 2
        let rms = |a: usize| (y[a..a + 2400].iter().map(|v| v * v).sum::<f32>() / 2400.0).sqrt();
        let held = rms(40000);
        let mut t10 = 0;
        let mut t40 = 0;
        for w in (48000..n - 2400).step_by(240) {
            let l = 20.0 * (rms(w) / held).max(1e-9).log10();
            if t10 == 0 && l < -10.0 {
                t10 = w;
            }
            if t40 == 0 && l < -40.0 {
                t40 = w;
                break;
            }
        }
        t60.push(2.0 * (t40 - t10) as f32 / sr);
    }
    assert!((t60[0] / t60[1] - 1.0).abs() < 0.15, "T60 at 1,024 {:.2} s, at 8,192 {:.2} s", t60[0], t60[1]);
    assert!((t60[0] - 2.0).abs() < 0.4, "T60 {:.2} s for 2 s", t60[0]);
}

#[test]
#[ignore]
fn probe_trim_step() {
    let sr = 48000.0;
    let n = 48000;
    let x = sine(n, 1000.0, sr);
    for fx in [0.0f32, 16.0] {
        let mut e = Glitch::new();
        e.prepare(sr, 512, 1);
        e.set_param("fx_a", fx);
        e.set_param("trim", 24.0);
        e.reset();
        let mut y = x.clone();
        let (p, q) = y.split_at_mut(n / 2);
        e.process(&mut [p]);
        e.set_param("trim", -24.0);
        e.process(&mut [q]);
        let mut line = String::new();
        for w in (n / 2 - 2400..n / 2 + 9600).step_by(480) {
            let mut s = 0.0f32;
            for i in w..w + 480 {
                s = s.max((y[i + 1] - 2.0 * y[i] + y[i - 1]).abs());
            }
            line += &format!("{:.0} ", 20.0 * s.max(1e-9).log10());
        }
        println!("fx {fx}: {line}");
    }
}

/// Fable's metric: a 220 Hz sine through each effect at 50 % (size 2,048): the strongest component at
/// 220 ± k · fs / hop (k = 1 … 4) re the carrier.
#[test]
#[ignore]
fn probe_hop_spurs() {
    let sr = 48000.0;
    let n = 2 * 48000 + 65536;
    let x = sine(n, 220.0, sr);
    let names = crate::slots::EFFECTS;
    for fx in 0..30 {
        let mut e = Glitch::new();
        e.prepare(sr, 512, 1);
        neutral(&mut e);
        e.set_param("fx_a", fx as f32);
        e.set_param("amt_a", if fx == 0 { 0.0 } else { 50.0 });
        e.reset();
        let y = render(&mut e, &x, 512);
        let sp = spectrum(&y, n - 65536);
        let bin = |hz: f32| (hz * 65536.0 / sr).round() as usize;
        let around = |hz: f32| (bin(hz) - 3..=bin(hz) + 3).map(|k| sp[k]).fold(0.0f32, f32::max);
        let car = around(220.0);
        let mut worst = -200.0f32;
        for k in 1..=4 {
            let f = 220.0 + k as f32 * sr / 512.0;
            worst = worst.max(20.0 * (around(f) / car).max(1e-12).log10());
            let f = k as f32 * sr / 512.0 - 220.0;
            worst = worst.max(20.0 * (around(f) / car).max(1e-12).log10());
        }
        println!("{:>2} {:<15} carrier {:>6.1} dB, hop spurs {:>6.1} dB", fx, names[fx], 20.0 * (car / around(220.0).max(1e-12)).log10() + 20.0 * (car / 1.0).log10(), worst);
    }
}

/// Worst 64-sample block and mean, every size, stereo, three slots + density + sustain + pitch active.
#[test]
#[ignore]
fn probe_cpu_worst_block() {
    let sr = 48000.0;
    let n = 4 * 48000;
    let x = noise(n, 1);
    for size in 0..7 {
        let mut e = Glitch::new();
        e.prepare(sr, 64, 2);
        for (id, v) in [("size", size as f32), ("fx_a", 16.0), ("fx_b", 21.0), ("amt_b", 40.0), ("fx_c", 22.0), ("amt_c", 30.0), ("density", 40.0), ("sustain", 2000.0), ("pitch", 7.0), ("cloud", 50.0)] {
            e.set_param(id, v);
        }
        e.reset();
        let mut l = x.clone();
        let mut r = x.clone();
        let mut times = Vec::new();
        let mut total = 0.0f64;
        for (a, b) in l.chunks_mut(64).zip(r.chunks_mut(64)) {
            let t = std::time::Instant::now();
            e.process(&mut [a, b]);
            let dt = t.elapsed().as_secs_f64();
            times.push(dt);
            total += dt;
        }
        let mut sorted = times.clone();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let worst = sorted[0];
        let top: Vec<String> = sorted[..4].iter().map(|t| format!("{:.0}", 100.0 * t / (64.0 / sr as f64))).collect();
        let first = times.iter().position(|&t| t == worst).unwrap();
        println!("  top blocks % {:?}, worst at block {first}", top);
        let block = 64.0 / sr as f64;
        println!("size {:>5}: worst block {:>5.1} % of its duration, mean {:.2} % of a core", crate::engine::SIZES[size], 100.0 * worst / block, 100.0 * total / (n as f64 / sr as f64));
    }
}

/// A size change during a chord: the longest stretch of near-silence in the wet.
#[test]
#[ignore]
fn probe_size_change_gap() {
    let sr = 48000.0;
    let n = 4 * 48000;
    let x: Vec<f32> = (0..n).map(|i| { let t = i as f32 / sr; 0.2 * ((std::f32::consts::TAU * 196.0 * t).sin() + (std::f32::consts::TAU * 247.0 * t).sin()) }).collect();
    for (a, b) in [(1.0f32, 4.0f32), (4.0, 1.0), (0.0, 6.0)] {
        let mut e = Glitch::new();
        e.prepare(sr, 512, 1);
        neutral(&mut e);
        e.set_param("size", a);
        e.reset();
        let mut y = x.clone();
        let (p, q) = y.split_at_mut(n / 2);
        e.process(&mut [p]);
        e.set_param("size", b);
        for c in q.chunks_mut(512) { e.process(&mut [c]); }
        let mut run = 0usize; let mut longest = 0usize;
        let mut dip = 0.0f32;
        for w in (n / 2..n - 480).step_by(48) {
            let rms = (y[w..w + 480].iter().map(|v| v * v).sum::<f32>() / 480.0).sqrt();
            let l = 20.0 * (rms / 0.2).max(1e-9).log10();
            dip = dip.min(l);
            if l < -20.0 { run += 48; longest = longest.max(run); } else { run = 0; }
        }
        println!("size {a} -> {b}: longest gap {:.1} ms, deepest 10 ms dip {:.1} dB", longest as f32 / sr * 1000.0, dip);
    }
}
