// SPDX-License-Identifier: GPL-3.0-or-later
//! CPU timing of gfx-morph's signal path (README §2.10), plain std, no gfx-dsp dependency: every kernel
//! is re-typed in the form gfx-dsp uses (HIIR half-bands in f64, ADAA1 with ln cosh, TPT SVFs in f64,
//! an f64 modal bank). It runs the whole chain per configuration on 10 s of a synthetic signal at
//! 48 kHz, mono, and prints ns per sample (median of 7 runs).
use std::hint::black_box;
use std::time::Instant;

const FS: f64 = 48000.0;

// ---------------------------------------------------------------- half-band (8 coefs) -----------------
const HB: [f64; 8] = [0.040633, 0.150505, 0.300757, 0.460775, 0.609524, 0.738504, 0.849224, 0.949743];

#[derive(Clone, Default)]
struct Chains { x: [f64; 8], y: [f64; 8] }
impl Chains {
    #[inline(always)]
    fn process(&mut self, mut a: f64, mut b: f64) -> (f64, f64) {
        let mut i = 0;
        while i < 8 {
            let t0 = self.x[i];
            let t1 = self.x[i + 1];
            self.x[i] = a;
            self.x[i + 1] = b;
            a = (a - self.y[i]) * HB[i] + t0;
            b = (b - self.y[i + 1]) * HB[i + 1] + t1;
            self.y[i] = a;
            self.y[i + 1] = b;
            i += 2;
        }
        (a, b)
    }
}
#[derive(Clone, Default)]
struct Os2 { up: Chains, down: Chains }
impl Os2 {
    #[inline(always)]
    fn up(&mut self, x: f64) -> [f64; 2] { let (a, b) = self.up.process(x, x); [a, b] }
    #[inline(always)]
    fn down(&mut self, x: [f64; 2]) -> f64 { let (a, b) = self.down.process(x[1], x[0]); 0.5 * (a + b) }
}

// ---------------------------------------------------------------- TPT SVF -------------------------------
#[derive(Clone, Default)]
struct Svf { a1: f64, a2: f64, a3: f64, k: f64, ic1: f64, ic2: f64 }
impl Svf {
    fn set(&mut self, f: f64, q: f64, fs: f64) {
        let g = (std::f64::consts::PI * f / fs).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }
    #[inline(always)]
    fn tick(&mut self, x: f64) -> (f64, f64) {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v1, v2) // band, low
    }
}

// LR4 split: low = LP2(LP2(x)), twin = low + high computed as in gfx-dsp (one tick per section)
#[derive(Clone, Default)]
struct Lr4 { s1: Svf, s2: Svf, ap: Svf }
impl Lr4 {
    fn set(&mut self, f: f64, fs: f64) {
        self.s1.set(f, std::f64::consts::FRAC_1_SQRT_2, fs);
        self.s2.set(f, std::f64::consts::FRAC_1_SQRT_2, fs);
        self.ap.set(f, std::f64::consts::FRAC_1_SQRT_2, fs);
    }
    #[inline(always)]
    fn split(&mut self, x: f64) -> (f64, f64) {
        let (_, l1) = self.s1.tick(x);
        let (_, low) = self.s2.tick(l1);
        let (b, l) = self.ap.tick(x);
        let twin = x - 2.0 * self.ap.k * b; // 2nd-order all-pass from the same SVF
        let _ = l;
        (low, twin - low)
    }
}

// ---------------------------------------------------------------- shapers ------------------------------
#[inline(always)]
fn ln_cosh(x: f64) -> f64 { let a = x.abs(); a + (-2.0 * a).exp().ln_1p() - std::f64::consts::LN_2 }
#[derive(Clone, Default)]
struct Adaa { xp: f64, fp: f64 }
impl Adaa {
    #[inline(always)]
    fn tanh(&mut self, x: f64) -> f64 {
        let f = ln_cosh(x);
        let d = x - self.xp;
        let y = if d.abs() < 1e-7 { (0.5 * (x + self.xp)).tanh() } else { (f - self.fp) / d };
        self.xp = x;
        self.fp = f;
        y
    }
    #[inline(always)]
    fn hard(&mut self, x: f64) -> f64 {
        let a = x.abs();
        let f = if a <= 1.0 { 0.5 * x * x } else { a - 0.5 };
        let d = x - self.xp;
        let y = if d.abs() < 1e-7 { (0.5 * (x + self.xp)).clamp(-1.0, 1.0) } else { (f - self.fp) / d };
        self.xp = x;
        self.fp = f;
        y
    }
}

// ---------------------------------------------------------------- modal bank ---------------------------
const MODES: usize = 48;
struct Bank { a1: [f64; MODES], a2: [f64; MODES], a3: [f64; MODES], kg: [f64; MODES], ic1: [f64; MODES], ic2: [f64; MODES],
              f: [f64; MODES], q: [f64; MODES], g: [f64; MODES] }
impl Bank {
    fn new() -> Self {
        let mut b = Bank { a1: [0.0; MODES], a2: [0.0; MODES], a3: [0.0; MODES], kg: [0.0; MODES], ic1: [0.0; MODES],
                           ic2: [0.0; MODES], f: [0.0; MODES], q: [0.0; MODES], g: [0.0; MODES] };
        for m in 0..MODES {
            b.f[m] = 120.0 * 1.09f64.powi(m as i32);
            b.q[m] = 30.0;
            b.g[m] = 1.0 / (1.0 + m as f64);
        }
        b.coefs(1.0);
        b
    }
    fn coefs(&mut self, scale: f64) {
        for m in 0..MODES {
            let g = (std::f64::consts::PI * self.f[m] * scale / FS).tan();
            let k = 1.0 / self.q[m];
            self.a1[m] = 1.0 / (1.0 + g * (g + k));
            self.a2[m] = g * self.a1[m];
            self.a3[m] = g * self.a2[m];
            self.kg[m] = k * self.g[m];
        }
    }
    #[inline(always)]
    fn tick(&mut self, x: f64) -> f64 {
        let mut y = 0.0;
        for m in 0..MODES {
            let v3 = x - self.ic2[m];
            let v1 = self.a1[m] * self.ic1[m] + self.a2[m] * v3;
            let v2 = self.ic2[m] + self.a2[m] * self.ic1[m] + self.a3[m] * v3;
            self.ic1[m] = 2.0 * v1 - self.ic1[m];
            self.ic2[m] = 2.0 * v2 - self.ic2[m];
            y += self.kg[m] * v1;
        }
        y
    }
}

// ---------------------------------------------------------------- the chain ----------------------------
#[derive(Clone, Copy)]
struct Cfg { name: &'static str, bands: usize, factor: usize, shapers_per_band: usize, box_on: bool, box_moving: bool,
             comp: bool, echo: bool }

fn run(cfg: Cfg, x: &[f64]) -> f64 {
    let mut pre = [Svf::default(), Svf::default(), Svf::default()];
    pre[0].set(80.0, 0.707, FS);
    pre[1].set(12000.0, 0.707, FS);
    pre[2].set(1200.0, 1.0, FS);
    let mut post = [Svf::default(), Svf::default()];
    post[0].set(100.0, 0.707, FS);
    post[1].set(9000.0, 0.707, FS);
    let mut defizz = Lr4::default();
    defizz.set(4500.0, FS);
    let mut det_hp = Svf::default();
    det_hp.set(4500.0, 0.707, FS);
    let mut os = [Os2::default(), Os2::default()];
    let fi = FS * cfg.factor as f64;
    let mut x1 = Lr4::default();
    x1.set(250.0, fi);
    let mut x2 = Lr4::default();
    x2.set(3000.0, fi);
    let mut tw = Lr4::default();
    tw.set(3000.0, fi);
    let mut sh: Vec<Adaa> = vec![Adaa::default(); 6];
    let mut bank = Bank::new();
    let mut skirt = [Svf::default(), Svf::default()];
    skirt[0].set(300.0, 0.707, FS);
    skirt[1].set(5000.0, 0.707, FS);
    let mut echo = vec![0.0f64; 32768];
    let mut wp = 0usize;
    let (mut p_full, mut p_hf, mut g_def, mut e_guard, mut e_comp, mut g_comp) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut out = 0.0f64;
    let mut size: f64;
    let t0 = Instant::now();
    for (n, &s) in x.iter().enumerate() {
        // guard detector + expander gain (RMS one-pole, dB law)
        e_guard = 0.995 * e_guard + 0.005 * s * s;
        let gdb = (10.0 * (e_guard + 1e-20).log10() + 45.0).clamp(-6.0, 0.0) * 3.5;
        let mut v = s * (gdb * 0.115_129_25).exp();
        // pre filters
        v = v - pre[0].tick(v).1; // HP from LP
        v = pre[1].tick(v).1;
        let (bp, _) = pre[2].tick(v);
        v += 0.5 * bp;
        // de-fizz: HF share detector, band gain, drive duck
        let (dl, dh) = defizz.split(v);
        let hf = { let (b, l) = det_hp.tick(v); v - l - 0.0 * b };
        p_full = 0.99 * p_full + 0.01 * v * v;
        p_hf = 0.99 * p_hf + 0.01 * hf * hf;
        let share = 10.0 * ((p_hf + 1e-20) / (p_full + 1e-20)).log10();
        let target = -9.0 * ((share + 14.0) / 8.0).clamp(0.0, 1.0);
        g_def += (target - g_def) * if target < g_def { 0.04 } else { 0.0007 };
        v = dl + dh * (g_def * 0.115_129_25).exp();
        let drive = (24.0 + g_def) * 0.115_129_25;
        let gd = drive.exp();
        // dirt
        let mut acc = 0.0;
        let up = if cfg.factor == 2 { let u = os[0].up(v); [u[0], u[1], 0.0, 0.0] } else {
            let u = os[0].up(v); let a = os[1].up(u[0]); let b = os[1].up(u[1]); [a[0], a[1], b[0], b[1]] };
        let mut y_in = [0.0f64; 4];
        for k in 0..cfg.factor {
            let u = up[k];
            let bands: [f64; 3] = match cfg.bands {
                1 => [u, 0.0, 0.0],
                2 => { let (l, h) = x1.split(u); [l, h, 0.0] }
                _ => { let (l, r) = x1.split(u); let (lc, _) = tw.split(l); let (m, h) = x2.split(r); [lc, m, h] }
            };
            let mut sum = 0.0;
            for b in 0..cfg.bands {
                let z = bands[b] * gd;
                let ya = sh[2 * b].tanh(z);
                let yb = if cfg.shapers_per_band > 1 { sh[2 * b + 1].hard(z) } else { 0.0 };
                sum += 0.6 * ya + 0.4 * yb;
            }
            y_in[k] = sum * 0.14;
        }
        let y = if cfg.factor == 2 { os[0].down([y_in[0], y_in[1]]) } else {
            let a = os[1].down([y_in[0], y_in[1]]); let b = os[1].down([y_in[2], y_in[3]]); os[0].down([a, b]) };
        acc += y;
        // box
        if cfg.box_on {
            if cfg.box_moving && n % 32 == 0 {
                size = 1.0 + 0.5 * ((n as f64) * 1e-5).sin();
                bank.coefs(size);
            }
            let b = bank.tick(acc);
            let lo0 = skirt[0].tick(acc).1;
            let sk = skirt[1].tick(acc - lo0).1;
            acc = 0.3 * acc + 0.7 * (b + 0.2 * sk);
        }
        // comp (one-knob, feed-forward, dB domain)
        if cfg.comp {
            e_comp = 0.999 * e_comp + 0.001 * acc.abs();
            let l = 20.0 * (e_comp + 1e-12).log10();
            let t = -((l + 24.0).max(0.0)) * 0.66;
            g_comp += (t - g_comp) * 0.002;
            acc *= (g_comp * 0.115_129_25).exp();
        }
        // post
        acc = acc - post[0].tick(acc).1;
        acc = post[1].tick(acc).1;
        // echo (linear interpolation read, feedback)
        if cfg.echo {
            let d = 5760.5;
            let rp = (wp as f64 + 32768.0 - d) % 32768.0;
            let i = rp as usize;
            let fr = rp - i as f64;
            let e = echo[i] * (1.0 - fr) + echo[(i + 1) & 32767] * fr;
            echo[wp] = acc + 0.35 * e;
            wp = (wp + 1) & 32767;
            acc += 0.3 * e;
        }
        out += black_box(acc);
    }
    let dt = t0.elapsed().as_secs_f64();
    black_box(out);
    dt / x.len() as f64 * 1e9
}

struct Bank32 { a1: [f32; MODES], a2: [f32; MODES], a3: [f32; MODES], kg: [f32; MODES], ic1: [f32; MODES], ic2: [f32; MODES] }
impl Bank32 {
    fn from(b: &Bank) -> Self {
        let c = |a: &[f64; MODES]| { let mut o = [0.0f32; MODES]; for i in 0..MODES { o[i] = a[i] as f32; } o };
        Bank32 { a1: c(&b.a1), a2: c(&b.a2), a3: c(&b.a3), kg: c(&b.kg), ic1: [0.0; MODES], ic2: [0.0; MODES] }
    }
    #[inline(always)]
    fn tick(&mut self, x: f32) -> f32 {
        let mut acc = [0.0f32; 8];
        for c in 0..MODES / 8 {
            for l in 0..8 {
                let m = c * 8 + l;
                let v3 = x - self.ic2[m];
                let v1 = self.a1[m] * self.ic1[m] + self.a2[m] * v3;
                let v2 = self.ic2[m] + self.a2[m] * self.ic1[m] + self.a3[m] * v3;
                self.ic1[m] = 2.0 * v1 - self.ic1[m];
                self.ic2[m] = 2.0 * v2 - self.ic2[m];
                acc[l] += self.kg[m] * v1;
            }
        }
        acc.iter().sum()
    }
}

// review (2026-11-15): the bank in f64 with 4 accumulator lanes (gfx-dsp's numerics rule: recursive states in f64)
struct Bank64L { a1: [f64; MODES], a2: [f64; MODES], a3: [f64; MODES], kg: [f64; MODES], ic1: [f64; MODES], ic2: [f64; MODES] }
impl Bank64L {
    fn from(b: &Bank) -> Self { Bank64L { a1: b.a1, a2: b.a2, a3: b.a3, kg: b.kg, ic1: [0.0; MODES], ic2: [0.0; MODES] } }
    #[inline(always)]
    fn tick(&mut self, x: f64) -> f64 {
        // states in one plain loop over the modes (it vectorises: no reduction inside), then the sum in 4 lanes
        let mut out = [0.0f64; MODES];
        for m in 0..MODES {
            let v3 = x - self.ic2[m];
            let v1 = self.a1[m] * self.ic1[m] + self.a2[m] * v3;
            let v2 = self.ic2[m] + self.a2[m] * self.ic1[m] + self.a3[m] * v3;
            self.ic1[m] = 2.0 * v1 - self.ic1[m];
            self.ic2[m] = 2.0 * v2 - self.ic2[m];
            out[m] = self.kg[m] * v1;
        }
        let mut acc = [0.0f64; 4];
        for c in out.chunks_exact(4) {
            for l in 0..4 { acc[l] += c[l]; }
        }
        (acc[0] + acc[1]) + (acc[2] + acc[3])
    }
}

fn parts(x: &[f64]) {
    let reps = 7;
    let time = |f: &mut dyn FnMut() -> f64| { let mut v: Vec<f64> = (0..reps).map(|_| f()).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[reps / 2] };
    let t = time(&mut || { let mut b = Bank::new(); let t0 = Instant::now(); let mut o = 0.0; for &s in x { o += b.tick(s); } black_box(o); t0.elapsed().as_secs_f64() / x.len() as f64 * 1e9 });
    println!("{:52} {:7.1} ns / sample", "part: modal bank 48 modes, f64 scalar", t);
    let t = time(&mut || { let b0 = Bank::new(); let mut b = Bank32::from(&b0); let t0 = Instant::now(); let mut o = 0.0f32; for &s in x { o += b.tick(s as f32); } black_box(o); t0.elapsed().as_secs_f64() / x.len() as f64 * 1e9 });
    println!("{:52} {:7.1} ns / sample", "part: modal bank 48 modes, f32 8 lanes", t);
    let t = time(&mut || { let b0 = Bank::new(); let mut b = Bank64L::from(&b0); let t0 = Instant::now(); let mut o = 0.0f64; for &s in x { o += b.tick(s); } black_box(o); t0.elapsed().as_secs_f64() / x.len() as f64 * 1e9 });
    println!("{:52} {:7.1} ns / sample", "part: modal bank 48 modes, f64 4 lanes (review)", t);
    let t = time(&mut || { let mut b = Bank::new(); let t0 = Instant::now(); for _ in 0..x.len() / 32 { b.coefs(1.01); } black_box(b.a1[0]); t0.elapsed().as_secs_f64() / x.len() as f64 * 1e9 });
    println!("{:52} {:7.1} ns / sample", "part: 48 tan() coefficient updates every 32 samples", t);
    let t = time(&mut || { let mut o2 = Os2::default(); let mut a = Adaa::default(); let t0 = Instant::now(); let mut o = 0.0; for &s in x { let u = o2.up(s * 16.0); let y = [a.tanh(u[0]), a.tanh(u[1])]; o += o2.down(y); } black_box(o); t0.elapsed().as_secs_f64() / x.len() as f64 * 1e9 });
    println!("{:52} {:7.1} ns / sample", "part: 2x pair + 2 ADAA tanh (one shaper)", t);
    let t = time(&mut || { let mut p = 0.0f64; let t0 = Instant::now(); let mut o = 0.0; for &s in x { p = 0.99 * p + 0.01 * s * s; o += (10.0 * (p + 1e-20).log10()).exp(); } black_box(o); t0.elapsed().as_secs_f64() / x.len() as f64 * 1e9 });
    println!("{:52} {:7.1} ns / sample", "part: one log10 + exp per sample (a detector's dB law)", t);
}

fn main() {
    let n = (10.0 * FS) as usize;
    let mut seed = 12345u64;
    let x: Vec<f64> = (0..n).map(|i| {
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        let r = (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
        0.2 * (2.0 * std::f64::consts::PI * 187.3 * i as f64 / FS).sin() + 0.02 * r
    }).collect();
    let cfgs = [
        Cfg { name: "dirt only: Single band, 1 shaper, 2x", bands: 1, factor: 2, shapers_per_band: 1, box_on: false, box_moving: false, comp: false, echo: false },
        Cfg { name: "Single band, A+B shapers, 2x (dirt only)", bands: 1, factor: 2, shapers_per_band: 2, box_on: false, box_moving: false, comp: false, echo: false },
        Cfg { name: "DEFAULT-like: Single, A+B, 2x, box, comp, echo", bands: 1, factor: 2, shapers_per_band: 2, box_on: true, box_moving: false, comp: true, echo: true },
        Cfg { name: "Three bands, A+B, 2x (dirt only)", bands: 3, factor: 2, shapers_per_band: 2, box_on: false, box_moving: false, comp: false, echo: false },
        Cfg { name: "Three bands, A+B, 2x, box, comp, echo", bands: 3, factor: 2, shapers_per_band: 2, box_on: true, box_moving: false, comp: true, echo: true },
        Cfg { name: "Three bands, A+B, 4x, box, comp, echo", bands: 3, factor: 4, shapers_per_band: 2, box_on: true, box_moving: false, comp: true, echo: true },
        Cfg { name: "WORST: Three, A+B, 4x, box moving, comp, echo", bands: 3, factor: 4, shapers_per_band: 2, box_on: true, box_moving: true, comp: true, echo: true },
        Cfg { name: "box alone (48 modes + skirt), static", bands: 1, factor: 2, shapers_per_band: 1, box_on: true, box_moving: false, comp: false, echo: false },
    ];
    let mut base = 0.0;
    for (i, c) in cfgs.iter().enumerate() {
        let mut v: Vec<f64> = (0..7).map(|_| run(*c, &x)).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if i == 0 { base = v[3]; }
        println!("{:52} {:7.1} ns / sample (min {:6.1}, max {:6.1})", c.name, v[3], v[0], v[6]);
    }
    let _ = base;
    parts(&x);
}
