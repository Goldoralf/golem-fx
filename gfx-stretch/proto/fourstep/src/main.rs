use realfft::RealFftPlanner;
use rustfft::{num_complex::Complex, FftPlanner};
use std::time::Instant;
type C = Complex<f32>;
type D = Complex<f64>;
fn lcg(s: &mut u64) -> f32 { *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); ((*s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0 }
/// four-step complex FFT of n = n1*n2. Returns (out in natural order, per-unit max ns, total ns, units)
fn four_step(x: &[C], n1: usize, n2: usize, tw: &[C]) -> (Vec<C>, u128, u128, usize) {
    let n = n1 * n2;
    let mut pl = FftPlanner::<f32>::new();
    let f1 = pl.plan_fft_forward(n1);
    let f2 = pl.plan_fft_forward(n2);
    let mut a = x.to_vec(); // a[i1*n2 + i2]
    let mut col = vec![C::default(); n1];
    let mut sc = vec![C::default(); f1.get_inplace_scratch_len().max(f2.get_inplace_scratch_len())];
    let (mut mx, mut tot, mut units) = (0u128, 0u128, 0usize);
    let mut unit = |t: Instant, mx: &mut u128, tot: &mut u128, units: &mut usize| { let d = t.elapsed().as_nanos(); if d > *mx { *mx = d; } *tot += d; *units += 1; };
    // 1: n2 column FFTs of n1 (gather / FFT / twiddle / scatter): one unit each
    for i2 in 0..n2 {
        let t = Instant::now();
        for i1 in 0..n1 { col[i1] = a[i1 * n2 + i2]; }
        f1.process_with_scratch(&mut col, &mut sc);
        for k1 in 0..n1 { a[k1 * n2 + i2] = col[k1] * tw[(i2 * k1) % n]; }
        unit(t, &mut mx, &mut tot, &mut units);
    }
    // 2: n1 row FFTs of n2: one unit each (grouped so a unit is >= ~512 points)
    let grp = (512 / n2).max(1);
    let mut k1 = 0;
    while k1 < n1 {
        let t = Instant::now();
        for r in k1..(k1 + grp).min(n1) { f2.process_with_scratch(&mut a[r * n2..(r + 1) * n2], &mut sc); }
        unit(t, &mut mx, &mut tot, &mut units);
        k1 += grp;
    }
    // natural order: X[k1 + k2*n1] = a[k1*n2 + k2] (transpose, chunked 4096)
    let mut out = vec![C::default(); n];
    let mut k = 0;
    while k < n {
        let t = Instant::now();
        for kk in k..(k + 4096).min(n) { let (k1, k2) = (kk % n1, kk / n1); out[kk] = a[k1 * n2 + k2]; }
        unit(t, &mut mx, &mut tot, &mut units);
        k += 4096;
    }
    (out, mx, tot, units)
}
fn main() {
    let mut s = 12345u64;
    println!("--- one-piece real FFT (realfft f32), best / median of 200, incl. nothing else");
    for &n in &[4096usize, 32768, 65536, 98304, 131072, 196608, 262144] {
        let mut pl = RealFftPlanner::<f32>::new();
        let f = pl.plan_fft_forward(n);
        let mut x: Vec<f32> = (0..n).map(|_| lcg(&mut s)).collect();
        let x0 = x.clone();
        let mut y = f.make_output_vec();
        let mut sc = f.make_scratch_vec();
        let mut ts = vec![];
        for _ in 0..200 { x.copy_from_slice(&x0); let t = Instant::now(); f.process_with_scratch(&mut x, &mut y, &mut sc).unwrap(); ts.push(t.elapsed().as_nanos()); }
        ts.sort();
        println!("N {:>6}: best {:>7.1} us  median {:>7.1} us  worst {:>7.1} us   ({:.1} % / {:.1} % of a 64-sample block at 48 kHz)", n, ts[0] as f64 / 1e3, ts[100] as f64 / 1e3, ts[199] as f64 / 1e3, ts[100] as f64 / 1e3 / 1333.3 * 100.0, ts[199] as f64 / 1e3 / 1333.3 * 100.0);
    }
    println!("--- four-step complex FFT (N/2 points), units vs direct rustfft; error vs f64 direct");
    for &(n1, n2) in &[(128usize, 128usize), (256, 256), (384, 256), (512, 256), (768, 256), (512, 512), (1024, 128)] {
        let n = n1 * n2;
        let x: Vec<C> = (0..n).map(|_| C::new(lcg(&mut s), lcg(&mut s))).collect();
        let tw: Vec<C> = (0..n).map(|m| { let a = -2.0 * std::f64::consts::PI * m as f64 / n as f64; C::new(a.cos() as f32, a.sin() as f32) }).collect();
        // f64 reference
        let mut pd = FftPlanner::<f64>::new();
        let fd = pd.plan_fft_forward(n);
        let mut xd: Vec<D> = x.iter().map(|c| D::new(c.re as f64, c.im as f64)).collect();
        fd.process(&mut xd);
        // f32 direct
        let mut pf = FftPlanner::<f32>::new();
        let ff = pf.plan_fft_forward(n);
        let mut xf = x.clone();
        let mut scf = vec![C::default(); ff.get_inplace_scratch_len()];
        let mut td = vec![];
        for _ in 0..50 { xf.copy_from_slice(&x); let t = Instant::now(); ff.process_with_scratch(&mut xf, &mut scf); td.push(t.elapsed().as_nanos()); }
        td.sort();
        let err = |y: &[C]| { let (mut e, mut p) = (0.0f64, 0.0f64); for (a, b) in y.iter().zip(&xd) { e += (a.re as f64 - b.re).powi(2) + (a.im as f64 - b.im).powi(2); p += b.norm_sqr(); } (e / p).sqrt() };
        let mut best = (u128::MAX, 0u128, 0usize); let mut worst_unit = 0u128; let mut e4 = 0.0;
        for _ in 0..50 { let (y, mx, tot, u) = four_step(&x, n1, n2, &tw); if tot < best.0 { best = (tot, mx, u); } worst_unit = worst_unit.max(mx); e4 = err(&y); }
        let (mut dd, mut pp) = (0.0f64, 0.0f64);
        let (y, _, _, _) = four_step(&x, n1, n2, &tw);
        for (a, b) in y.iter().zip(&xf) { dd += ((a.re - b.re) as f64).powi(2) + ((a.im - b.im) as f64).powi(2); pp += (b.norm_sqr()) as f64; }
        println!("N/2 {:>6} = {:>4} x {:>3}: direct {:>6.1} us (err {:.1e}) | four-step total {:>6.1} us in {} units, unit max in best run {:.2} us, worst unit of 50 runs {:.1} us (err {:.1e}); four-step vs direct f32 {:.1e}", n, n1, n2, td[25] as f64 / 1e3, err(&xf), best.0 as f64 / 1e3, best.2, best.1 as f64 / 1e3, worst_unit as f64 / 1e3, e4, (dd / pp).sqrt());
    }
}
