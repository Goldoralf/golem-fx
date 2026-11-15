// SPDX-License-Identifier: GPL-3.0-or-later
//! The built-in IR, slot 0 of `ir_a` (README §2.7): a generic closed-back 4×12 voicing of our own,
//! designed from gfx-dsp `matched` sections, not derived from any file. Rendered at the host's rate
//! into a 100 ms impulse response (the sections are minimum-phase), at the Loudness reference
//! (+14 dB), so slot 0 and a library cab play at the same level with `normalize` Loudness or Off.
//! The voicing is [I] until heard (the maintainer's ear).

use gfx_dsp::ir::{self, Source};
use gfx_dsp::matched::{design, Analog, Biquad};

/// The Loudness band and reference of gfx-cab (§2.2.7).
pub const BAND: (f64, f64) = (80.0, 6000.0);
pub const REF_DB: f64 = 14.0;
/// Rendered length (s).
pub const LEN_S: f64 = 0.1;

/// (section, centre Hz): the voicing.
fn sections() -> [(Analog, f64); 10] {
    [
        // the cab's low end: a 2nd-order high-pass under the resonance
        (Analog::high_pass(0.75), 75.0),
        // the closed-back resonance
        (Analog::bell(1.6, 6.0), 105.0),
        // low-mid body
        (Analog::bell(0.9, 2.0), 260.0),
        // the closed-back box's mid scoop
        (Analog::bell(1.1, -3.5), 550.0),
        // cone break-up dip
        (Analog::bell(2.2, -3.0), 1250.0),
        // presence
        (Analog::bell(1.3, 4.0), 2400.0),
        (Analog::bell(2.5, 2.5), 3600.0),
        // 4th-order roll-off from 5 kHz (Butterworth pair)
        (Analog::low_pass(0.541_196), 5200.0),
        (Analog::low_pass(1.306_563), 5200.0),
        // the cone's last peak before the fall
        (Analog::bell(3.0, -2.0), 7500.0),
    ]
}

fn run(b: &Biquad, z: &mut [f64; 2], x: f64) -> f64 {
    let y = b.b0 * x + z[0];
    z[0] = b.b1 * x - b.a1 * y + z[1];
    z[1] = b.b2 * x - b.a2 * y;
    y
}

/// The built-in IR at `sr`.
pub fn render(sr: f64) -> Source {
    let n = (LEN_S * sr).round() as usize;
    let bq: Vec<Biquad> = sections().iter().map(|(a, f)| design(*a, f.min(0.45 * sr), sr)).collect();
    let mut z = vec![[0.0f64; 2]; bq.len()];
    let mut h: Vec<f64> = (0..n)
        .map(|k| {
            let mut x = if k == 0 { 1.0 } else { 0.0 };
            for (b, s) in bq.iter().zip(z.iter_mut()) {
                x = run(b, s, x);
            }
            x
        })
        .collect();
    // a 5 ms half-Hann fade on the tail (it is far below −100 dB by then)
    let nf = (0.005 * sr) as usize;
    for i in 0..nf {
        let x = (i + 1) as f64 / nf as f64;
        h[n - nf + i] *= 0.5 + 0.5 * (std::f64::consts::PI * x).cos();
    }
    let g = ir::loudness_gain(std::slice::from_ref(&h), sr, BAND) * 10f64.powf(REF_DB / 20.0);
    h.iter_mut().for_each(|v| *v *= g);
    Source::mono(sr, h)
}
