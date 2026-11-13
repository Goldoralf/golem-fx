// SPDX-License-Identifier: GPL-3.0-or-later
//! Small linear circuits, simulated from their netlist: resistors, capacitors and ideal op-amps, driven
//! by one voltage source. Used where an effect's response is a real circuit's network (gfx-shem's
//! boost network, gfx-emet's 808 tone stage) rather than a fitted filter.
//!
//! - **Simulation**: modified nodal analysis (MNA), each capacitor replaced by its trapezoidal companion
//!   model (a conductance 2C / T and a history current). The matrix depends only on the component values
//!   and the sample rate: its inverse is computed once, and a sample costs one sparse matrix-vector
//!   product. The states are the capacitors' voltages and currents, so changing a resistor (a pot)
//!   rebuilds the matrix without touching the state: no jump, no zipper.
//! - **Analysis**: [`Netlist::response`] solves the same netlist in the Laplace domain at s = jω, the
//!   analog reference the tests compare the simulation with.
//!
//! Real-time: [`Circuit::new`] allocates; [`Circuit::set_resistor`] and [`Circuit::process`] do not.

/// Node 0 is ground (every reference voltage — Vref, the supplies — is AC ground).
pub type Node = usize;

#[derive(Clone, Copy, Debug)]
pub enum Element {
    /// A resistor between two nodes, in ohms.
    R(Node, Node, f64),
    /// A capacitor between two nodes, in farads.
    C(Node, Node, f64),
    /// An ideal op-amp: V(plus) = V(minus), any output current. (plus, minus, out).
    OpAmp(Node, Node, Node),
}

/// A circuit: its elements, the node driven by the input source, and the output node.
#[derive(Clone, Debug)]
pub struct Netlist {
    pub nodes: usize, // node count, ground included
    pub elements: Vec<Element>,
    pub input: Node,
    pub output: Node,
}

impl Netlist {
    fn opamps(&self) -> usize {
        self.elements.iter().filter(|e| matches!(e, Element::OpAmp(..))).count()
    }
    /// Unknowns: node voltages 1..nodes, the input source's current, one output current per op-amp.
    fn size(&self) -> usize {
        self.nodes - 1 + 1 + self.opamps()
    }

    /// The analog transfer function V(output) / V(input) at `freq` Hz, as (re, im).
    pub fn response(&self, freq: f64) -> (f64, f64) {
        let n = self.size();
        let w = 2.0 * std::f64::consts::PI * freq;
        let mut a = vec![(0.0, 0.0); n * n];
        let mut b = vec![(0.0, 0.0); n];
        let add = |a: &mut Vec<(f64, f64)>, r: usize, c: usize, v: (f64, f64)| {
            a[r * n + c].0 += v.0;
            a[r * n + c].1 += v.1;
        };
        let stamp = |a: &mut Vec<(f64, f64)>, p: Node, q: Node, y: (f64, f64)| {
            let neg = (-y.0, -y.1);
            if p > 0 {
                add(a, p - 1, p - 1, y);
            }
            if q > 0 {
                add(a, q - 1, q - 1, y);
            }
            if p > 0 && q > 0 {
                add(a, p - 1, q - 1, neg);
                add(a, q - 1, p - 1, neg);
            }
        };
        self.stamp_common(&mut |r, c, v| add(&mut a, r, c, (v, 0.0)));
        for e in &self.elements {
            match *e {
                Element::R(p, q, r) => stamp(&mut a, p, q, (1.0 / r, 0.0)),
                Element::C(p, q, c) => stamp(&mut a, p, q, (0.0, w * c)),
                Element::OpAmp(..) => {}
            }
        }
        b[self.nodes - 1] = (1.0, 0.0);
        let x = solve_complex(&mut a, &mut b, n);
        x[self.output - 1]
    }

    /// The source and op-amp rows and columns (real, frequency-independent).
    fn stamp_common(&self, put: &mut dyn FnMut(usize, usize, f64)) {
        let src = self.nodes - 1;
        // input source: V(input) = u, its current enters KCL at the input node
        put(self.input - 1, src, 1.0);
        put(src, self.input - 1, 1.0);
        let mut k = src + 1;
        for e in &self.elements {
            if let Element::OpAmp(p, m, o) = *e {
                // output current into the output node; constraint V(p) − V(m) = 0
                put(o - 1, k, 1.0);
                if p > 0 {
                    put(k, p - 1, 1.0);
                }
                if m > 0 {
                    put(k, m - 1, -1.0);
                }
                k += 1;
            }
        }
    }
}

/// A netlist simulated at one sample rate.
#[derive(Clone, Debug)]
pub struct Circuit {
    net: Netlist,
    n: usize,
    t: f64,
    m: Vec<f64>,     // the MNA matrix
    minv: Vec<f64>,  // its inverse
    work: Vec<f64>,  // inversion workspace (n × 2n)
    caps: Vec<(Node, Node, f64)>, // (p, q, G = 2C / T)
    state: Vec<(f64, f64)>,       // per capacitor: (v, i) at the previous sample
    rhs: Vec<f64>,
    x: Vec<f64>,
}

impl Circuit {
    pub fn new(net: Netlist, sample_rate: f64) -> Self {
        let n = net.size();
        let t = 1.0 / sample_rate;
        let caps: Vec<_> = net
            .elements
            .iter()
            .filter_map(|e| if let Element::C(p, q, c) = *e { Some((p, q, 2.0 * c / t)) } else { None })
            .collect();
        let ncaps = caps.len();
        let mut c = Self {
            net,
            n,
            t,
            m: vec![0.0; n * n],
            minv: vec![0.0; n * n],
            work: vec![0.0; n * 2 * n],
            caps,
            state: vec![(0.0, 0.0); ncaps],
            rhs: vec![0.0; n],
            x: vec![0.0; n],
        };
        c.rebuild();
        c
    }

    pub fn reset(&mut self) {
        for s in &mut self.state {
            *s = (0.0, 0.0);
        }
    }

    pub fn netlist(&self) -> &Netlist {
        &self.net
    }

    /// Changes the value of element `index` (which must be a resistor) and rebuilds the matrix.
    /// No allocation; the capacitors keep their state.
    pub fn set_resistor(&mut self, index: usize, ohms: f64) {
        if let Element::R(p, q, _) = self.net.elements[index] {
            self.net.elements[index] = Element::R(p, q, ohms.max(1e-3));
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        let n = self.n;
        for v in &mut self.m {
            *v = 0.0;
        }
        let m = &mut self.m;
        let mut stamp = |p: Node, q: Node, g: f64| {
            if p > 0 {
                m[(p - 1) * n + p - 1] += g;
            }
            if q > 0 {
                m[(q - 1) * n + q - 1] += g;
            }
            if p > 0 && q > 0 {
                m[(p - 1) * n + q - 1] -= g;
                m[(q - 1) * n + p - 1] -= g;
            }
        };
        for e in &self.net.elements {
            match *e {
                Element::R(p, q, r) => stamp(p, q, 1.0 / r),
                Element::C(p, q, c) => stamp(p, q, 2.0 * c / self.t),
                Element::OpAmp(..) => {}
            }
        }
        let m = &mut self.m;
        self.net.stamp_common(&mut |r, c, v| m[r * n + c] += v);
        invert(&self.m, &mut self.minv, &mut self.work, n);
    }

    /// One sample through the circuit.
    #[inline]
    pub fn process(&mut self, u: f64) -> f64 {
        let n = self.n;
        for v in &mut self.rhs {
            *v = 0.0;
        }
        // trapezoidal companion: i_n = G v_n − J, J = G v_{n−1} + i_{n−1}, injected p ← q
        for (k, &(p, q, g)) in self.caps.iter().enumerate() {
            let (v, i) = self.state[k];
            let j = g * v + i;
            if p > 0 {
                self.rhs[p - 1] += j;
            }
            if q > 0 {
                self.rhs[q - 1] -= j;
            }
        }
        self.rhs[self.net.nodes - 1] = u;
        for r in 0..n {
            let row = &self.minv[r * n..(r + 1) * n];
            let mut acc = 0.0;
            for c in 0..n {
                acc += row[c] * self.rhs[c];
            }
            self.x[r] = acc;
        }
        let volt = |x: &Vec<f64>, a: Node| if a == 0 { 0.0 } else { x[a - 1] };
        for (k, &(p, q, g)) in self.caps.iter().enumerate() {
            let (v0, i0) = self.state[k];
            let v = volt(&self.x, p) - volt(&self.x, q);
            let i = g * v - (g * v0 + i0);
            self.state[k] = (v, i);
        }
        volt(&self.x, self.net.output)
    }
}

/// Gauss-Jordan inverse with partial pivoting, in the caller's buffers (no allocation).
fn invert(m: &[f64], inv: &mut [f64], work: &mut [f64], n: usize) {
    let w = 2 * n;
    for r in 0..n {
        for c in 0..n {
            work[r * w + c] = m[r * n + c];
            work[r * w + n + c] = if r == c { 1.0 } else { 0.0 };
        }
    }
    for col in 0..n {
        let mut piv = col;
        for r in col + 1..n {
            if work[r * w + col].abs() > work[piv * w + col].abs() {
                piv = r;
            }
        }
        if piv != col {
            for c in 0..w {
                work.swap(col * w + c, piv * w + c);
            }
        }
        let d = work[col * w + col];
        let d = if d.abs() < 1e-300 { 1e-300 } else { d };
        for c in 0..w {
            work[col * w + c] /= d;
        }
        for r in 0..n {
            if r != col {
                let f = work[r * w + col];
                if f != 0.0 {
                    for c in 0..w {
                        work[r * w + c] -= f * work[col * w + c];
                    }
                }
            }
        }
    }
    for r in 0..n {
        for c in 0..n {
            inv[r * n + c] = work[r * w + n + c];
        }
    }
}

fn solve_complex(a: &mut [(f64, f64)], b: &mut [(f64, f64)], n: usize) -> Vec<(f64, f64)> {
    let mul = |x: (f64, f64), y: (f64, f64)| (x.0 * y.0 - x.1 * y.1, x.0 * y.1 + x.1 * y.0);
    let div = |x: (f64, f64), y: (f64, f64)| {
        let d = y.0 * y.0 + y.1 * y.1;
        ((x.0 * y.0 + x.1 * y.1) / d, (x.1 * y.0 - x.0 * y.1) / d)
    };
    let mag = |x: (f64, f64)| x.0.hypot(x.1);
    for col in 0..n {
        let mut piv = col;
        for r in col + 1..n {
            if mag(a[r * n + col]) > mag(a[piv * n + col]) {
                piv = r;
            }
        }
        if piv != col {
            for c in 0..n {
                a.swap(col * n + c, piv * n + c);
            }
            b.swap(col, piv);
        }
        let d = a[col * n + col];
        for r in col + 1..n {
            let f = div(a[r * n + col], d);
            for c in col..n {
                let t = mul(f, a[col * n + c]);
                a[r * n + c].0 -= t.0;
                a[r * n + c].1 -= t.1;
            }
            let t = mul(f, b[col]);
            b[r].0 -= t.0;
            b[r].1 -= t.1;
        }
    }
    let mut x = vec![(0.0, 0.0); n];
    for r in (0..n).rev() {
        let mut acc = b[r];
        for c in r + 1..n {
            let t = mul(a[r * n + c], x[c]);
            acc.0 -= t.0;
            acc.1 -= t.1;
        }
        x[r] = div(acc, a[r * n + r]);
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use Element::*;

    fn db(z: (f64, f64)) -> f64 {
        20.0 * z.0.hypot(z.1).log10()
    }

    /// RC low-pass: 1 kΩ, 159.15 nF → 1 kHz.
    fn rc() -> Netlist {
        Netlist { nodes: 3, elements: vec![R(1, 2, 1000.0), C(2, 0, 159.155e-9)], input: 1, output: 2 }
    }

    #[test]
    fn rc_response_is_analog() {
        let g = db(rc().response(1000.0));
        assert!((g + 3.0103).abs() < 0.01, "{g}");
    }

    #[test]
    fn non_inverting_gain() {
        // + = input, − tapped from 9k / 1k: gain 10
        let net = Netlist {
            nodes: 4,
            elements: vec![OpAmp(1, 2, 3), R(3, 2, 9000.0), R(2, 0, 1000.0)],
            input: 1,
            output: 3,
        };
        assert!((db(net.response(100.0)) - 20.0).abs() < 1e-9);
        let mut c = Circuit::new(net, 48000.0);
        let y = (0..10).map(|_| c.process(0.1)).last().unwrap();
        assert!((y - 1.0).abs() < 1e-9, "{y}");
    }

    #[test]
    fn simulation_matches_analysis() {
        let sr = 48000.0;
        for f in [100.0, 1000.0, 5000.0] {
            let mut c = Circuit::new(rc(), sr);
            let w = 2.0 * std::f64::consts::PI * f / sr;
            let (mut ein, mut eout) = (0.0, 0.0);
            for i in 0..96000 {
                let x = (w * i as f64).sin();
                let y = c.process(x);
                if i > 48000 {
                    ein += x * x;
                    eout += y * y;
                }
            }
            let sim = 10.0 * (eout / ein).log10();
            // the trapezoidal rule is the bilinear transform: the digital response at f is the
            // analog one at the pre-warped frequency (sr / π) · tan(π f / sr), exactly
            let fw = sr / std::f64::consts::PI * (std::f64::consts::PI * f / sr).tan();
            let ana = db(rc().response(fw));
            assert!((sim - ana).abs() < 0.01, "{f} Hz: {sim} vs {ana}");
        }
    }

    #[test]
    fn changing_a_resistor_keeps_the_state() {
        let mut c = Circuit::new(rc(), 48000.0);
        for _ in 0..1000 {
            c.process(1.0);
        }
        let before = c.process(1.0);
        c.set_resistor(0, 2000.0);
        let after = c.process(1.0);
        assert!((before - after).abs() < 1e-3, "{before} → {after}");
    }
}
