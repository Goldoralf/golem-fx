// CPU timing of gfx-spectralcomp's per-hop work (README §2.9): forward rFFT, power, band sums by prefix
// sum (f64), dB follower, static curve (log10 / exp via std), gains, inverse rFFT, window and OLA.
// Live's ACGE cascade is gfx-geq's measured cost, scaled (not timed here).
use realfft::RealFftPlanner;
use std::time::Instant;

fn main() {
    for &n in &[512usize, 1024, 2048, 4096, 8192] {
        for &(hop_div, inverse) in &[(4usize, false), (8, true)] {
            let h = n / hop_div;
            let mut pl = RealFftPlanner::<f32>::new();
            let fwd = pl.plan_fft_forward(n);
            let inv = pl.plan_fft_inverse(n);
            let w: Vec<f32> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos()).collect();
            let nb = n / 2 + 1;
            let mut x = vec![0f32; n];
            let mut spec = fwd.make_output_vec();
            let mut sfw = fwd.make_scratch_vec();
            let mut siv = inv.make_scratch_vec();
            let mut p = vec![0f32; nb];
            let mut s = vec![0f64; nb + 1];
            let mut ls = vec![-60f32; nb];
            let lo: Vec<f32> = (0..nb).map(|k| (k as f32 - (k as f32 * 0.056).max(2.0)).max(0.0)).collect();
            let hi: Vec<f32> = (0..nb).map(|k| (k as f32 + (k as f32 * 0.059).max(2.0)).min(nb as f32 - 1.0)).collect();
            let thr: Vec<f32> = (0..nb).map(|k| -36.0 + 0.0 * k as f32).collect();
            let mut ola = vec![0f32; n];
            let mut seed = 1u32;
            let iters = (2_000_000 / n).max(200);
            let t0 = Instant::now();
            let mut acc = 0f32;
            for it in 0..iters {
                for i in 0..n {
                    seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5;
                    x[i] = ((seed >> 8) as f32 / 16777216.0 - 0.5) * w[i];
                }
                let t1 = Instant::now();
                fwd.process_with_scratch(&mut x, &mut spec, &mut sfw).unwrap();
                for k in 0..nb { p[k] = spec[k].norm_sqr(); }
                s[0] = 0.0;
                for k in 0..nb { s[k + 1] = s[k] + p[k] as f64; }
                for k in 0..nb {
                    let c = |y: f32| { let y = y + 0.5; let i = (y as usize).min(nb - 1); s[i] + (y - i as f32) as f64 * p[i] as f64 };
                    let e = (c(hi[k]) - c(lo[k])) as f32;
                    let l = 10.0 * (e.max(1e-14) * 2.0).log10();
                    let d = l - ls[k];
                    ls[k] += if d > 0.0 { 0.3 } else { 0.05 } * d;
                    let over = ls[k] - thr[k];
                    let gd = -(0.75) * over.max(0.0);
                    let g = 10f32.powf(gd.max(-24.0) / 20.0);
                    spec[k] = spec[k] * g;
                }
                if inverse {
                    spec[0].im = 0.0; spec[nb - 1].im = 0.0;
                    inv.process_with_scratch(&mut spec, &mut x, &mut siv).unwrap();
                    for i in 0..n { ola[i] += x[i] * w[i]; }
                }
                acc += ls[it % nb];
                let _ = t1;
            }
            let dt = t0.elapsed().as_secs_f64();
            // subtract the noise generation (timed separately)
            let t2 = Instant::now();
            for _ in 0..iters { for i in 0..n { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; x[i] = ((seed >> 8) as f32 / 16777216.0 - 0.5) * w[i]; } }
            let gen = t2.elapsed().as_secs_f64();
            let per_hop = (dt - gen) / iters as f64;
            println!("N {:5} hop N/{} {}: {:6.2} us per hop, {:5.1} ns per sample at the hop rate (acc {:.1})",
                n, hop_div, if inverse { "studio (fwd+gains+inv+OLA)" } else { "live detector (fwd+gains)  " }, per_hop * 1e6, per_hop / h as f64 * 1e9, acc + ola[0] + x[0]);
        }
    }
}
