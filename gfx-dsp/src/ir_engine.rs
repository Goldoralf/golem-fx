// SPDX-License-Identifier: GPL-3.0-or-later
//! The IR engine of gfx-cab and gfx-irfx (feature `ir-host`; gfx-cab README §2.1, §2.4): the
//! instance side ([`IrCore`]) and the plugin side of the slot plumbing ([`Host`], [`Worker`]).
//!
//! **IrCore**, an instance's engine: one [`Convolver`] per channel with one voice per IR, the slots
//! in place, their meters, and the hand-over of kernels.
//!
//! - **Plugin mode** ([`IrCore::set_async`]): the wrapper's [`Host`] turns
//!   [`IrCore::want`]s into background jobs and hands the results to [`IrCore::apply`]; nothing here
//!   does I/O or allocates.
//! - **Sync mode** (the default: tests, gfx-bench): a changed request is prepared at once from the
//!   sources registered with [`IrCore::load_ir`] (and the built-in), in `set_param`, never in `process`.
//! - Kernels in place are kept across `prepare` at the same rate and across `reset`: no I/O, no FFT.
//! - Kernels the engine drops go to a fixed-size list the wrapper drains ([`IrCore::take_retired`]),
//!   so the last reference is never released on the audio thread.
//!
//! **Host and Worker**, the plugin side of the slot plumbing, shared by both wrappers, with no
//! nih-plug in it so it can be tested alone.
//!
//! - **Audio side** ([`Host::tick`], once per block): takes finished loads from the mailbox (a stale
//!   generation is released, never dropped here), compares the parameters' request with what is in
//!   place and posts a job when they differ — at once for a slot or a switch, after 100 ms of rest for
//!   `length` and `fade`; a request equal to what plays restores the meters with no load; a slot whose
//!   meter reads −1 is asked again every 0.5 s. Kernels the engine drops go to the return queue.
//! - **Background side** ([`Worker::run`]): releases returned kernels, runs the job through the
//!   [`Loader`] (cache, index, file, preparation) and posts the result.
//!
//! Nothing on the audio side allocates, locks or does I/O.

use crate::convolve::{Convolver, Kernel};
use crate::delay::{DelayLine, Interp};
use crate::ir::{Settings, Source};
use crate::ir_host::{self, Done, Job, Kernels, Loader, LoadError, Mailbox, ReturnQueue, SlotSource, Sourced, MISSING};
use crate::ir_host::rtrb::{Consumer, Producer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Samples per inner segment (scratch size).
pub const SEG: usize = 64;
const RETIRED: usize = 64;

/// What the voices should play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Want {
    pub voices: [SlotSource; 2],
    pub settings: Settings,
}

/// The engine of one instance.
pub struct IrCore {
    nvoices: usize,
    channels: usize,
    sr: f32,
    max_s: f64,
    builtin: Option<fn(f64) -> Source>,
    sources: HashMap<u32, Arc<Source>>,
    sync: bool,
    conv: Vec<Convolver>,
    /// per channel, per voice: the last segment's output
    pub out: Vec<[Vec<f32>; 2]>,
    installed: [Option<(SlotSource, Settings)>; 2],
    kern: [Kernels; 2],
    kern_rate: f32,
    loaded: [f32; 2],
    retired: Vec<Arc<Kernel>>,
    want: Option<Want>,
}

fn slot_value(s: SlotSource) -> f32 {
    match s {
        SlotSource::None | SlotSource::Builtin => 0.0,
        SlotSource::Slot(n) => n as f32,
    }
}

impl IrCore {
    pub fn new(nvoices: usize, max_s: f64, builtin: Option<fn(f64) -> Source>) -> Self {
        Self {
            nvoices,
            channels: 0,
            sr: 0.0,
            max_s,
            builtin,
            sources: HashMap::new(),
            sync: true,
            conv: Vec::new(),
            out: Vec::new(),
            installed: [None, None],
            kern: [[None, None], [None, None]],
            kern_rate: 0.0,
            loaded: [0.0; 2],
            retired: Vec::with_capacity(RETIRED),
            want: None,
        }
    }

    /// Plugin mode: requests are served by the wrapper's background task.
    pub fn set_async(&mut self) {
        self.sync = false;
    }

    pub fn is_sync(&self) -> bool {
        self.sync
    }

    pub fn nvoices(&self) -> usize {
        self.nvoices
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    pub fn builtin(&self) -> Option<fn(f64) -> Source> {
        self.builtin
    }

    /// Meter of voice `v`: the slot installed, or a failure code.
    pub fn loaded(&self, v: usize) -> f32 {
        self.loaded[v]
    }

    /// True when a voice plays an IR (B present).
    pub fn has_kernel(&self, v: usize) -> bool {
        self.kern[v][0].is_some()
    }

    /// What is in place equals `w` (per voice: slot and settings).
    pub fn installed_matches(&self, w: &Want) -> bool {
        (0..self.nvoices).all(|v| self.installed[v] == Some((w.voices[v], w.settings)))
    }

    /// Voice `v` is in place as `w` asks.
    pub fn voice_matches(&self, v: usize, w: &Want) -> bool {
        self.installed[v] == Some((w.voices[v], w.settings))
    }

    /// Meters back to the slots in place (a request equal to what plays: no load).
    pub fn restore_meters(&mut self) {
        for v in 0..self.nvoices {
            if let Some((s, _)) = self.installed[v] {
                self.loaded[v] = slot_value(s);
            }
        }
    }

    /// Sizes the convolvers (class limit `max_s`). Kernels in place at the same rate and layout are
    /// kept (no I/O, no preparation); the caller then states its request ([`IrCore::set_want`]).
    /// Allocates.
    pub fn prepare(&mut self, sample_rate: f32, channels: usize) {
        let channels = channels.clamp(1, 2);
        let max_len = (self.max_s * sample_rate as f64).round() as usize;
        let same = self.kern_rate == sample_rate && self.channels == channels;
        self.sr = sample_rate;
        self.channels = channels;
        // the old convolvers' kernels are all in `kern` or retired: drop the engines off-line here
        let old: Vec<Convolver> = std::mem::take(&mut self.conv);
        drop(old);
        self.conv = (0..channels).map(|_| Convolver::new(self.nvoices, max_len, sample_rate)).collect();
        self.out = (0..channels).map(|_| [vec![0.0; SEG], vec![0.0; SEG]]).collect();
        if same {
            for v in 0..self.nvoices {
                let ks = self.kern[v].clone();
                self.place_now(v, &ks);
            }
        } else {
            self.installed = [None, None];
            self.kern = [[None, None], [None, None]];
            self.loaded = [0.0; 2];
            self.kern_rate = sample_rate;
        }
        self.retired.clear();
    }

    pub fn reset(&mut self) {
        for c in self.conv.iter_mut() {
            c.reset();
        }
    }

    /// The request of the parameters. Sync mode prepares a change at once (with the crossfade).
    pub fn set_want(&mut self, w: Want) {
        self.want = Some(w);
        if self.sync && !self.conv.is_empty() {
            if self.installed_matches(&w) {
                self.restore_meters();
            } else {
                self.load_sync(&w, false);
            }
        }
    }

    /// The kernel voice `v` plays on channel 0 (tests: sharing and reuse).
    pub fn kernel(&self, v: usize) -> Option<&Arc<Kernel>> {
        self.kern[v][0].as_ref()
    }

    pub fn want(&self) -> Option<Want> {
        self.want
    }

    /// Registers an IR under a slot for sync mode (gfx-bench, tests): prepared synchronously when a
    /// voice asks for that slot. Allocates.
    pub fn load_ir(&mut self, slot: u32, channels: &[Vec<f32>], rate: f32) {
        let src = Source { rate: rate as f64, channels: channels.iter().map(|c| c.iter().map(|&v| v as f64).collect()).collect() };
        self.sources.insert(slot, Arc::new(src));
        // a request waiting for this slot loads now
        if let Some(w) = self.want {
            let waiting = (0..self.nvoices).any(|v| w.voices[v] == SlotSource::Slot(slot));
            if self.sync && !self.conv.is_empty() && waiting {
                for v in 0..self.nvoices {
                    if w.voices[v] == SlotSource::Slot(slot) {
                        self.installed[v] = None;
                    }
                }
                self.load_sync(&w, false);
            }
        }
    }

    /// The job of a request.
    pub fn job(&self, w: &Want, gen: u64) -> Job {
        Job { gen, voices: w.voices, nvoices: self.nvoices, settings: w.settings, sample_rate: self.sr as f64 }
    }

    fn load_sync(&mut self, w: &Want, now: bool) {
        let job = self.job(w, 0);
        let mut srcs: [Sourced; 2] = [Ok(None), Ok(None)];
        for v in 0..self.nvoices {
            srcs[v] = match w.voices[v] {
                SlotSource::None => Ok(None),
                SlotSource::Builtin => Ok(self.builtin.map(|f| (Arc::new(f(self.sr as f64)), "builtin".to_string()))),
                SlotSource::Slot(n) => match self.sources.get(&n) {
                    Some(s) => Ok(Some((s.clone(), format!("mem{n}")))),
                    None => Err(LoadError::Missing),
                },
            };
        }
        let done = ir_host::prepare_from(&job, srcs, false);
        self.apply(done, now);
        // sync mode has no background thread: release here (never in `process`)
        self.retired.clear();
        for c in self.conv.iter_mut() {
            while c.take_retired().is_some() {}
        }
    }

    /// Installs a finished load: each voice's kernels (crossfaded, or at once when `now`), or its
    /// failure code (the previous IR keeps playing).
    pub fn apply(&mut self, done: Done, now: bool) {
        let Done { voices, results, settings, .. } = done;
        for (v, r) in results.into_iter().enumerate() {
            if v >= self.nvoices {
                if let Ok(ks) = r {
                    self.retire_set(ks);
                }
                continue;
            }
            match r {
                Ok(ks) => {
                    if now {
                        self.place_now(v, &ks);
                    } else {
                        self.place(v, &ks);
                    }
                    let old = std::mem::replace(&mut self.kern[v], ks);
                    self.retire_set(old);
                    self.installed[v] = Some((voices[v], settings));
                    self.loaded[v] = slot_value(voices[v]);
                }
                Err(e) => self.loaded[v] = e.code(),
            }
        }
    }

    /// Releases a result that is not used (stale generation).
    pub fn discard(&mut self, done: Done) {
        for ks in done.results.into_iter().flatten() {
            self.retire_set(ks);
        }
    }

    fn kernel_for(&self, ks: &Kernels, ch: usize) -> Option<Arc<Kernel>> {
        if self.channels == 2 {
            ks[ch].clone().or_else(|| ks[0].clone())
        } else {
            ks[0].clone()
        }
    }

    fn place(&mut self, v: usize, ks: &Kernels) {
        for ch in 0..self.channels {
            let k = self.kernel_for(ks, ch);
            let same = match (self.conv[ch].kernel(v), &k) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same {
                self.conv[ch].set_kernel(v, k);
            }
        }
    }

    fn place_now(&mut self, v: usize, ks: &Kernels) {
        for ch in 0..self.channels {
            let k = self.kernel_for(ks, ch);
            self.conv[ch].set_now(v, k);
        }
    }

    fn retire_set(&mut self, ks: Kernels) {
        for k in ks.into_iter().flatten() {
            if self.retired.len() < RETIRED {
                self.retired.push(k);
            } else {
                std::mem::forget(k);
            }
        }
    }

    /// A kernel to release off the audio thread.
    pub fn take_retired(&mut self) -> Option<Arc<Kernel>> {
        if let Some(k) = self.retired.pop() {
            return Some(k);
        }
        for c in self.conv.iter_mut() {
            if let Some(k) = c.take_retired() {
                return Some(k);
            }
        }
        None
    }

    /// Convolves `input` (≤ SEG samples) of channel `ch`; the voices' outputs land in `self.out[ch]`.
    pub fn convolve(&mut self, ch: usize, input: &[f32]) {
        let n = input.len();
        let [a, b] = &mut self.out[ch];
        if self.nvoices == 2 {
            self.conv[ch].process(input, &mut [&mut a[..n], &mut b[..n]]);
        } else {
            self.conv[ch].process(input, &mut [&mut a[..n]]);
        }
    }

    /// Times a level was late (the schedule's proof: always 0).
    pub fn late_count(&self) -> u64 {
        self.conv.iter().map(|c| c.late_count()).sum()
    }
}

/// A fractional read (Lagrange 3). Under one sample the centred nodes would need a future sample:
/// the cubic through taps 0 – 3 instead, which meets the bypass (tap 0) at 0 and the centred read at 1.
#[inline]
pub fn read_frac(line: &DelayLine, d: f64) -> f32 {
    let r = d.round();
    if (d - r).abs() < 1e-9 {
        return line.tap(r.max(0.0) as usize);
    }
    if d >= 1.0 {
        return line.read(d, Interp::Lagrange3);
    }
    let d = d.max(0.0) as f32;
    let (a, b, c) = (d - 1.0, d - 2.0, d - 3.0);
    let l0 = -a * b * c / 6.0;
    let l1 = d * b * c / 2.0;
    let l2 = -d * a * c / 2.0;
    let l3 = d * a * b / 6.0;
    l0 * line.tap(0) + l1 * line.tap(1) + l2 * line.tap(2) + l3 * line.tap(3)
}

// ---- the plugin side of the slot plumbing ----

/// Debounce of `length` / `fade` (s) and the retry period of a missing slot (s).
pub const DEBOUNCE_S: f64 = 0.1;
pub const RETRY_S: f64 = 0.5;

/// The background half: the loader and the queue ends it owns.
pub struct Worker {
    pub loader: Loader,
    tx: Mutex<Producer<Done>>,
    back: Mutex<Consumer<Arc<Kernel>>>,
}

impl Worker {
    /// Releases what the audio thread handed back.
    pub fn collect(&self) {
        if let Ok(mut b) = self.back.lock() {
            while let Ok(k) = b.pop() {
                drop(k);
            }
        }
    }

    /// Runs a job and posts its result (blocking: the background thread, or `initialize()`).
    pub fn run(&self, job: &Job) {
        self.collect();
        let done = self.loader.run(job);
        if let Ok(mut tx) = self.tx.lock() {
            // a full mailbox drops the result here, on this thread; the audio side asks again
            let _ = tx.push(done);
        }
    }

    /// Runs a job and returns its result directly (`initialize()`).
    pub fn run_now(&self, job: &Job) -> Done {
        self.collect();
        self.loader.run(job)
    }
}

/// The audio half.
pub struct Host {
    rx: Consumer<Done>,
    back: Producer<Arc<Kernel>>,
    gen: u64,
    requested: Option<Want>,
    last_post: u64,
    shape_at: u64,
    shape: (f64, f64),
}

/// Both halves of a new plumbing.
pub fn pair(loader: Loader) -> (Host, Arc<Worker>) {
    let (tx, rx) = Mailbox::new();
    let (bp, bc) = ReturnQueue::new();
    let w = Arc::new(Worker { loader, tx: Mutex::new(tx), back: Mutex::new(bc) });
    (Host { rx, back: bp, gen: 0, requested: None, last_post: 0, shape_at: 0, shape: (f64::NAN, f64::NAN) }, w)
}

fn same_but_shape(a: &Want, b: &Want) -> bool {
    let strip = |s: Settings| Settings { length_s: 0.0, fade: 0.0, ..s };
    a.voices == b.voices && strip(a.settings) == strip(b.settings)
}

impl Host {
    /// The generation of the latest request.
    pub fn gen(&self) -> u64 {
        self.gen
    }

    /// `initialize()`: what `core` now plays is the request (no job in flight counts any more).
    pub fn settle(&mut self, want: Want) {
        self.gen += 1;
        self.requested = Some(want);
        self.shape = (want.settings.length_s, want.settings.fade);
    }

    /// One block: results in, requests out (`post` = nih-plug's `execute_background`), retired
    /// kernels back. `now` = the effect's clock (samples). True when kernels were handed back (the
    /// wrapper then posts a collect task).
    pub fn tick(&mut self, core: &mut IrCore, now: u64, mut post: impl FnMut(Job)) -> bool {
        while let Ok(done) = self.rx.pop() {
            if done.gen == self.gen {
                core.apply(done, false);
            } else {
                core.discard(done);
            }
        }
        if let Some(want) = core.want() {
            let sr = core.sample_rate() as f64;
            let shape = (want.settings.length_s, want.settings.fade);
            if shape != self.shape {
                self.shape = shape;
                self.shape_at = now;
            }
            if Some(want) != self.requested {
                if core.installed_matches(&want) {
                    // the previous slot written again: meters back at once, no load
                    self.gen += 1;
                    self.requested = Some(want);
                    core.restore_meters();
                } else {
                    let debounce = self.requested.is_some_and(|r| same_but_shape(&r, &want)) && ((now - self.shape_at) as f64) < DEBOUNCE_S * sr;
                    if !debounce {
                        self.gen += 1;
                        self.requested = Some(want);
                        self.last_post = now;
                        post(core.job(&want, self.gen));
                    }
                }
            } else if (0..core.nvoices()).any(|v| core.loaded(v) == MISSING && !core.voice_matches(v, &want)) && ((now - self.last_post) as f64) >= RETRY_S * sr {
                // a slot written before its index entry: ask again
                self.gen += 1;
                self.last_post = now;
                post(core.job(&want, self.gen));
            }
        }
        let mut returned = false;
        while let Some(k) = core.take_retired() {
            returned = true;
            if let Err(e) = self.back.push(k) {
                // never release on the audio thread
                std::mem::forget(e);
            }
        }
        returned
    }
}
