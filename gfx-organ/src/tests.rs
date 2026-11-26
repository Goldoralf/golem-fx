// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour tests (README §5) on synthetic strings (the prototype's model, with a −100 dBFS noise
//! floor) and the pedal's own paths.

use super::*;
use crate::sieve_bench::{note_hz, play, plain, render};

const SR: f32 = 48000.0;

#[test]
fn latency_dry_at_sample_zero() {
    let mut o = Organ::new();
    o.set_param("dry", 0.0);
    o.prepare(SR, 256, 1);
    o.reset();
    assert_eq!(o.latency(), 0);
    let mut x = vec![0.0f32; 512];
    x[0] = 0.5;
    o.process(&mut [&mut x]);
    assert!((x[0] - 0.5).abs() < 1e-6, "{}", x[0]);
}

#[test]
fn smoke_single_note() {
    let x = play(&[(0.05, note_hz("A2"), 0)], 1.0, 1, SR as f64, false);
    let mut o = plain("888000000");
    let (y, _) = render(&mut o, &x, None);
    let e: f64 = y[24000..].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / (y.len() - 24000) as f64;
    println!("A2 organ RMS {:.1} dBFS", 10.0 * e.log10());
    assert!(y.iter().all(|v| v.is_finite()));
    assert!(e > 1e-6);
}
