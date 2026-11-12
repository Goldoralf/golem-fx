// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-gapmeter — the probe of golem's `tools/check-scene-gap.py` (design: README.md).
//!
//! A measurement probe, not an effect. It replaces `golem_gapmeter.jsfx` on every engine.
//! - **Detector** (default): audio passes unchanged. A silence counts as a gap when the peak envelope
//!   (3 ms release, the max of the channels) stays under `thresh` for at least `min_gap`. Timing is
//!   sample-accurate.
//! - **Generator** (`tone` on): the output is a test tone; the input is discarded.
//!
//! Results leave by OSC over UDP to 127.0.0.1:`port`. The audio thread only publishes: atomics for the
//! latest clock, envelope and silence, and a lock-free ring for the gap events. No allocation, no I/O. A
//! background thread (the plugin wrapper's) sends:
//! - `/gfx/gap` `,iidd`: slot, gap index k, end time (s), duration (ms);
//! - `/gfx/gapmeter` `,idddid` every ~50 ms: slot, clock (s), envelope (dB), current silence (ms), gap
//!   count, longest gap (ms).
//!
//! If the audio thread stalls, its clock freezes while the heartbeats keep coming: that is how the tool
//! measures a stall.

#![allow(clippy::needless_range_loop)]

use gfx_dsp::{Effect, ParamInfo};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(feature = "plugin")]
mod plugin;

pub const RING: usize = 256;

pub static PARAMS: [ParamInfo; 7] = [
    ParamInfo::enumeration("slot", "Slot", 8, 0),
    ParamInfo::float("thresh", "Silence Threshold", -90.0, 0.0, -45.0),
    ParamInfo::float("min_gap", "Min Gap", 1.0, 500.0, 10.0),
    ParamInfo::float("port", "Port", 1024.0, 65535.0, 9071.0),
    ParamInfo::boolean("tone", "Test Tone", false),
    ParamInfo::float("tone_freq", "Tone Freq", 50.0, 2000.0, 196.0),
    ParamInfo::float("tone_level", "Tone Level", -60.0, 0.0, -20.0),
];

pub mod p {
    pub const SLOT: usize = 0;
    pub const THRESH: usize = 1;
    pub const MIN_GAP: usize = 2;
    pub const PORT: usize = 3;
    pub const TONE: usize = 4;
    pub const TONE_FREQ: usize = 5;
    pub const TONE_LEVEL: usize = 6;
    pub const N: usize = 7;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub v: [f32; p::N],
}

impl Default for Settings {
    fn default() -> Self {
        Self { v: core::array::from_fn(|i| PARAMS[i].default) }
    }
}

impl Settings {
    pub fn with(mut self, i: usize, value: f32) -> Self {
        self.v[i] = value;
        self
    }
}

/// A gap that ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gap {
    pub k: u64,
    pub end_s: f64,
    pub duration_ms: f64,
}

/// What the audio thread publishes and the sender reads: atomics only (single producer, single consumer).
pub struct Shared {
    pub active: AtomicBool,
    pub quit: AtomicBool,
    pub slot: AtomicU32,
    pub port: AtomicU32,
    clock: AtomicU64,
    env_db: AtomicU64,
    silence_ms: AtomicU64,
    count: AtomicU64,
    max_ms: AtomicU64,
    ring: [[AtomicU64; 3]; RING],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            quit: AtomicBool::new(false),
            slot: AtomicU32::new(0),
            port: AtomicU32::new(9071),
            clock: AtomicU64::new(0),
            env_db: AtomicU64::new((-120.0f64).to_bits()),
            silence_ms: AtomicU64::new(0),
            count: AtomicU64::new(0),
            max_ms: AtomicU64::new(0),
            ring: core::array::from_fn(|_| [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Producer side: queues a gap (dropped if the ring is full; `k` lets the reader see it).
    fn push(&self, g: Gap) {
        let h = self.head.load(Ordering::Relaxed);
        let t = self.tail.load(Ordering::Acquire);
        if h.wrapping_sub(t) >= RING {
            return;
        }
        let e = &self.ring[h % RING];
        e[0].store(g.k, Ordering::Relaxed);
        e[1].store(g.end_s.to_bits(), Ordering::Relaxed);
        e[2].store(g.duration_ms.to_bits(), Ordering::Relaxed);
        self.head.store(h.wrapping_add(1), Ordering::Release);
    }

    /// Consumer side: the next queued gap.
    pub fn pop(&self) -> Option<Gap> {
        let t = self.tail.load(Ordering::Relaxed);
        let h = self.head.load(Ordering::Acquire);
        if t == h {
            return None;
        }
        let e = &self.ring[t % RING];
        let g = Gap { k: e[0].load(Ordering::Relaxed), end_s: f64::from_bits(e[1].load(Ordering::Relaxed)), duration_ms: f64::from_bits(e[2].load(Ordering::Relaxed)) };
        self.tail.store(t.wrapping_add(1), Ordering::Release);
        Some(g)
    }

    /// The latest heartbeat values: (clock s, envelope dB, silence ms, gap count, longest ms).
    pub fn heartbeat(&self) -> (f64, f64, f64, u64, f64) {
        (
            f64::from_bits(self.clock.load(Ordering::Acquire)),
            f64::from_bits(self.env_db.load(Ordering::Relaxed)),
            f64::from_bits(self.silence_ms.load(Ordering::Relaxed)),
            self.count.load(Ordering::Relaxed),
            f64::from_bits(self.max_ms.load(Ordering::Relaxed)),
        )
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

// ---- OSC encoding (no allocation: into a caller's buffer) -------------------------------------------------

fn put_str(buf: &mut [u8], at: &mut usize, s: &str) {
    let b = s.as_bytes();
    buf[*at..*at + b.len()].copy_from_slice(b);
    *at += b.len();
    // a terminating zero, then padding to 4 bytes
    let pad = 4 - (b.len() % 4);
    for _ in 0..pad {
        buf[*at] = 0;
        *at += 1;
    }
}

fn put_i(buf: &mut [u8], at: &mut usize, v: i32) {
    buf[*at..*at + 4].copy_from_slice(&v.to_be_bytes());
    *at += 4;
}

fn put_d(buf: &mut [u8], at: &mut usize, v: f64) {
    buf[*at..*at + 8].copy_from_slice(&v.to_be_bytes());
    *at += 8;
}

/// `/gfx/gap ,iidd slot k end_s duration_ms` into `buf`; returns its length.
pub fn osc_gap(buf: &mut [u8; 128], slot: u32, g: Gap) -> usize {
    let mut at = 0;
    put_str(buf, &mut at, "/gfx/gap");
    put_str(buf, &mut at, ",iidd");
    put_i(buf, &mut at, slot as i32);
    put_i(buf, &mut at, g.k as i32);
    put_d(buf, &mut at, g.end_s);
    put_d(buf, &mut at, g.duration_ms);
    at
}

/// `/gfx/gapmeter ,idddid slot clock_s env_db silence_ms n max_ms` into `buf`; returns its length.
pub fn osc_heartbeat(buf: &mut [u8; 128], slot: u32, hb: (f64, f64, f64, u64, f64)) -> usize {
    let mut at = 0;
    put_str(buf, &mut at, "/gfx/gapmeter");
    put_str(buf, &mut at, ",idddid");
    put_i(buf, &mut at, slot as i32);
    put_d(buf, &mut at, hb.0);
    put_d(buf, &mut at, hb.1);
    put_d(buf, &mut at, hb.2);
    put_i(buf, &mut at, hb.3 as i32);
    put_d(buf, &mut at, hb.4);
    at
}

/// The sender loop (run on its own thread by the plugin wrapper): every 10 ms it drains the gap ring
/// and, every ~50 ms, sends a heartbeat, to 127.0.0.1:`port`. Nothing before activation; returns when
/// `quit` is set.
pub fn sender(shared: Arc<Shared>) {
    use std::net::UdpSocket;
    use std::time::{Duration, Instant};
    let Ok(sock) = UdpSocket::bind("127.0.0.1:0") else { return };
    let mut buf = [0u8; 128];
    let mut last_hb = Instant::now();
    while !shared.quit.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(10));
        if !shared.active.load(Ordering::Acquire) {
            continue;
        }
        let slot = shared.slot.load(Ordering::Relaxed);
        let addr = ("127.0.0.1", shared.port.load(Ordering::Relaxed) as u16);
        while let Some(g) = shared.pop() {
            let n = osc_gap(&mut buf, slot, g);
            let _ = sock.send_to(&buf[..n], addr);
        }
        if last_hb.elapsed() >= Duration::from_millis(50) {
            last_hb = Instant::now();
            let n = osc_heartbeat(&mut buf, slot, shared.heartbeat());
            let _ = sock.send_to(&buf[..n], addr);
        }
    }
}

pub struct GapMeter {
    s: Settings,
    sr: f32,
    pub shared: Arc<Shared>,
    env: f32,
    rel: f32,
    samples: u64,
    in_silence: bool,
    silence_start: u64,
    count: u64,
    max_ms: f64,
    tone_ph: f64,
    /// 0 = the input, 1 = the tone
    tone_m: f32,
}

impl GapMeter {
    pub fn new() -> Self {
        Self {
            s: Settings::default(),
            sr: 48000.0,
            shared: Arc::new(Shared::new()),
            env: 0.0,
            rel: 0.0,
            samples: 0,
            in_silence: false,
            silence_start: 0,
            count: 0,
            max_ms: 0.0,
            tone_ph: 0.0,
            tone_m: 0.0,
        }
    }

    pub fn settings(&self) -> Settings {
        self.s
    }

    pub fn set(&mut self, s: Settings) {
        self.shared.slot.store(s.v[p::SLOT].round().clamp(0.0, 7.0) as u32, Ordering::Relaxed);
        self.shared.port.store(s.v[p::PORT].round().clamp(1024.0, 65535.0) as u32, Ordering::Relaxed);
        self.s = s;
    }

    pub fn process_block(&mut self, buffers: &mut [&mut [f32]]) {
        let nch = buffers.len();
        let n = buffers.first().map_or(0, |b| b.len());
        let v = self.s.v;
        let thr = 10f32.powf(v[p::THRESH] / 20.0);
        let min_gap = (v[p::MIN_GAP] as f64 * 0.001 * self.sr as f64) as u64;
        let tone = v[p::TONE] >= 0.5;
        let tone_level = 10f32.powf(v[p::TONE_LEVEL] / 20.0);
        let inc = v[p::TONE_FREQ] as f64 / self.sr as f64;
        for i in 0..n {
            // generator: the tone replaces the input, crossfaded over ~5 ms both ways
            self.tone_m += ((if tone { 1.0 } else { 0.0 }) - self.tone_m) * 0.005;
            if self.tone_m > 1e-6 {
                self.tone_ph = (self.tone_ph + inc).fract();
                let y = tone_level * (std::f64::consts::TAU * self.tone_ph).sin() as f32;
                for c in 0..nch {
                    buffers[c][i] += (y - buffers[c][i]) * self.tone_m;
                }
            }
            // detector: peak envelope (3 ms release) of the channels' max
            let mut a = 0.0f32;
            for c in 0..nch {
                a = a.max(buffers[c][i].abs());
            }
            self.env = if a > self.env { a } else { a + (self.env - a) * self.rel };
            self.samples += 1;
            let silent = self.env < thr;
            if silent && !self.in_silence {
                self.in_silence = true;
                self.silence_start = self.samples;
            } else if !silent && self.in_silence {
                self.in_silence = false;
                let len = self.samples - self.silence_start;
                if len >= min_gap {
                    let ms = len as f64 * 1000.0 / self.sr as f64;
                    let g = Gap { k: self.count, end_s: self.samples as f64 / self.sr as f64, duration_ms: ms };
                    self.count += 1;
                    self.max_ms = self.max_ms.max(ms);
                    self.shared.push(g);
                }
            }
        }
        // publish the latest state (the clock last: a reader that sees it sees the rest)
        let sh = &self.shared;
        let silence = if self.in_silence { (self.samples - self.silence_start) as f64 * 1000.0 / self.sr as f64 } else { 0.0 };
        sh.env_db.store((20.0 * (self.env.max(1e-6) as f64).log10()).to_bits(), Ordering::Relaxed);
        sh.silence_ms.store(silence.to_bits(), Ordering::Relaxed);
        sh.count.store(self.count, Ordering::Relaxed);
        sh.max_ms.store(self.max_ms.to_bits(), Ordering::Relaxed);
        sh.clock.store((self.samples as f64 / self.sr as f64).to_bits(), Ordering::Release);
        sh.active.store(true, Ordering::Release);
    }
}

impl Default for GapMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for GapMeter {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sample_rate;
        self.rel = (-1.0 / (0.003 * sample_rate)).exp();
    }

    fn reset(&mut self) {
        self.env = 0.0;
        self.samples = 0;
        self.in_silence = false;
        self.silence_start = 0;
        self.count = 0;
        self.max_ms = 0.0;
        self.tone_ph = 0.0;
        self.tone_m = if self.s.v[p::TONE] >= 0.5 { 1.0 } else { 0.0 };
        while self.shared.pop().is_some() {}
        self.shared.active.store(false, Ordering::Release);
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(i) = PARAMS.iter().position(|pi| pi.id == id) {
            let s = self.s.with(i, value);
            self.set(s);
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        self.process_block(buffers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48000.0;

    fn gm(s: Settings) -> GapMeter {
        let mut g = GapMeter::new();
        g.prepare(SR, 512, 1);
        g.set(s);
        g.reset();
        g
    }

    /// A 196 Hz tone with a hole of `hole` samples at 0.5 s.
    fn holed(hole: usize) -> Vec<f32> {
        (0..48000)
            .map(|i| if (24000..24000 + hole).contains(&i) { 0.0 } else { 0.1 * (std::f32::consts::TAU * 196.0 * i as f32 / SR).sin() })
            .collect()
    }

    #[test]
    fn measures_a_gap_to_the_sample() {
        let mut g = gm(Settings::default());
        let mut x = holed(960); // 20 ms
        for c in x.chunks_mut(128) {
            g.process(&mut [c]);
        }
        let gap = g.shared.pop().expect("one gap");
        // the envelope's 3 ms release delays the start; the end is when the tone is back
        assert!((gap.end_s - 24960.0 / 48000.0).abs() < 2.0 / 48000.0, "end {}", gap.end_s);
        assert!(gap.duration_ms > 10.0 && gap.duration_ms <= 20.0, "{} ms", gap.duration_ms);
        assert_eq!(gap.k, 0);
        assert!(g.shared.pop().is_none(), "a 196 Hz tone must not read as gaps between its peaks");
    }

    #[test]
    fn short_holes_are_not_gaps() {
        let mut g = gm(Settings::default());
        let mut x = holed(240); // 5 ms < 10 ms
        for c in x.chunks_mut(128) {
            g.process(&mut [c]);
        }
        assert!(g.shared.pop().is_none());
    }

    #[test]
    fn the_tone_replaces_the_input_and_the_clock_runs() {
        let mut g = gm(Settings::default().with(p::TONE, 1.0));
        let mut x = vec![0.9f32; 4800];
        g.process(&mut [&mut x[..]]);
        let pk = x[2400..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((pk - 0.1).abs() < 0.005, "tone peak {pk}");
        let (clock, ..) = g.shared.heartbeat();
        assert!((clock - 0.1).abs() < 1e-9);
    }

    #[test]
    fn gaps_and_heartbeats_arrive_over_udp() {
        use std::net::UdpSocket;
        use std::time::Duration;
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
        let port = rx.local_addr().unwrap().port();
        let mut g = gm(Settings::default().with(p::PORT, port as f32).with(p::SLOT, 5.0));
        let shared = g.shared.clone();
        let t = std::thread::spawn(move || sender(shared));
        let mut x = holed(960);
        for c in x.chunks_mut(128) {
            g.process(&mut [c]);
        }
        let (mut got_gap, mut got_hb) = (false, false);
        let mut buf = [0u8; 256];
        for _ in 0..20 {
            if let Ok(n) = rx.recv(&mut buf) {
                if buf[..n].starts_with(b"/gfx/gap\0") {
                    got_gap = true;
                    assert_eq!(i32::from_be_bytes(buf[20..24].try_into().unwrap()), 5, "slot");
                } else if buf[..n].starts_with(b"/gfx/gapmeter") {
                    got_hb = true;
                }
            }
            if got_gap && got_hb {
                break;
            }
        }
        g.shared.quit.store(true, Ordering::Release);
        t.join().unwrap();
        assert!(got_gap && got_hb, "gap {got_gap}, heartbeat {got_hb}");
    }

    #[test]
    fn osc_messages_are_well_formed() {
        let mut buf = [0u8; 128];
        let n = osc_gap(&mut buf, 3, Gap { k: 7, end_s: 1.5, duration_ms: 20.0 });
        assert_eq!(&buf[..12], b"/gfx/gap\0\0\0\0");
        assert_eq!(&buf[12..20], b",iidd\0\0\0");
        assert_eq!(i32::from_be_bytes(buf[20..24].try_into().unwrap()), 3);
        assert_eq!(i32::from_be_bytes(buf[24..28].try_into().unwrap()), 7);
        assert_eq!(f64::from_be_bytes(buf[28..36].try_into().unwrap()), 1.5);
        assert_eq!(n, 44);
        assert_eq!(n % 4, 0);
        let m = osc_heartbeat(&mut buf, 0, (2.0, -30.0, 0.0, 4, 18.0));
        assert_eq!(m % 4, 0);
    }
}
