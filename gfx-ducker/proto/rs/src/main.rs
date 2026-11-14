use gfx_dsp::matched::{design, Analog};
use gfx_dsp::svf::{Svf, SvfMode};
use std::time::Instant;
use std::hint::black_box;
fn main() {
    let sr = 48000.0;
    // 1. design cost
    let n = 200_000;
    let t = Instant::now();
    let mut acc = 0.0;
    for i in 0..n {
        let g = -(i % 60) as f64 - 0.5;
        let f = 80.0 * 2f64.powf(((i % 16) as f64) / 2.0);
        let b = design(Analog::bell(2.87, black_box(g)), f, sr);
        acc += b.b0;
    }
    let per = t.elapsed().as_nanos() as f64 / n as f64;
    println!("design: {per:.0} ns each ({acc})");
    // design + set_biquad_ramped
    let mut svf = Svf::new();
    let t = Instant::now();
    for i in 0..n {
        let g = -(i % 60) as f64 - 0.5;
        let b = design(Analog::bell(2.87, black_box(g)), 1000.0, sr);
        svf.set_biquad_ramped(&b, 32);
    }
    println!("design+ramp set: {:.0} ns each", t.elapsed().as_nanos() as f64 / n as f64);
    // 2. 16 SVF in series, ramping always
    let mut bells: Vec<Svf> = (0..16).map(|_| Svf::new()).collect();
    for (k, s) in bells.iter_mut().enumerate() {
        s.set_biquad(&design(Analog::bell(2.87, -6.0), 80.0 * 2f64.powf(k as f64 / 2.0), sr));
    }
    let m = 4_800_000usize;
    let mut x = 0.3f32; let mut sum = 0.0f32;
    let t = Instant::now();
    for i in 0..m {
        if i % 32 == 0 {
            for (k, s) in bells.iter_mut().enumerate() {
                let g = -6.0 - 3.0 * ((i / 32 + k) % 5) as f64;
                // table-like: ramp only (design outside timing not possible, so measure separately below)
                let _ = g;
            }
        }
        x = (x * 1.7 + 0.1).sin() * 0.5;
        let mut y = x;
        for s in bells.iter_mut() { y = s.process(y); }
        sum += y;
    }
    println!("16 bells static: {:.1} ns/sample ({sum})", t.elapsed().as_nanos() as f64 / m as f64);
    // with design each 32
    let t = Instant::now();
    for i in 0..m {
        if i % 32 == 0 {
            for (k, s) in bells.iter_mut().enumerate() {
                let g = -6.0 - 3.0 * ((i / 32 + k) % 5) as f64;
                let b = design(Analog::bell(2.87, g), 80.0 * 2f64.powf(k as f64 / 2.0), sr);
                s.set_biquad_ramped(&b, 32);
            }
        }
        x = (x * 1.7 + 0.1).sin() * 0.5;
        let mut y = x;
        for s in bells.iter_mut() { y = s.process(y); }
        sum += y;
    }
    println!("16 bells + 16 designs/32: {:.1} ns/sample ({sum})", t.elapsed().as_nanos() as f64 / m as f64);
    // 16 band-pass analysis + followers
    let mut bps: Vec<Svf> = (0..16).map(|_| Svf::new()).collect();
    for (k, s) in bps.iter_mut().enumerate() { s.set(SvfMode::BandPass, 80.0 * 2f32.powf(k as f32 / 2.0), 2.87, 0.0, 48000.0); }
    let mut env = [0f32; 16];
    let t = Instant::now();
    for _ in 0..m {
        x = (x * 1.7 + 0.1).sin() * 0.5;
        for (k, s) in bps.iter_mut().enumerate() { let a = s.process(x).abs(); let c = if a > env[k] {0.99} else {0.9997}; env[k] = a + (env[k]-a)*c; }
    }
    println!("16 BP + env: {:.1} ns/sample ({})", t.elapsed().as_nanos() as f64 / m as f64, env[3]);
    // powf per sample
    let t = Instant::now();
    let mut g = 0f32;
    for i in 0..m { g += 10f32.powf(-black_box(i as f32 * 1e-6) * 0.05); }
    println!("powf gain: {:.1} ns/sample ({g})", t.elapsed().as_nanos() as f64 / m as f64);
}
