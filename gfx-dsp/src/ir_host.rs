// SPDX-License-Identifier: GPL-3.0-or-later
//! The plumbing of the IR plugins (feature `ir-host`; gfx-cab README §2.1): the golem-written index,
//! WAV decoding, the content hash, the process-wide cache of prepared kernels, the loader that turns a
//! slot request into kernels, and the lock-free hand-over to the audio thread.
//!
//! - **Index**: `$GOLEM_FX_IR_INDEX`, else `$XDG_DATA_HOME/golem-fx/ir-index.json`, else
//!   `~/.local/share/golem-fx/ir-index.json` (macOS `~/Library/Application Support/golem-fx/`, Windows
//!   `%APPDATA%\golem-fx\`). `{"format": 1, "slots": {"17": {"path": …, "sha256": …}}}`, read at every
//!   load (golem renames a whole new file into place).
//! - **Failures**: −1 the slot is not in the index (nor in the plugin's persisted fallback) or the file
//!   is missing; −2 the file is unreadable, not a WAV `hound` reads, or its hash does not match.
//! - **Cache**: one per process, keyed by (content, partner, rate, settings, role). Kernels in use are
//!   never evicted; unused ones are kept up to 64 MB, least recently used first. Decoded sources are kept
//!   under the same budget, so a settings change does not read the file again.
//! - **Hand-over**: [`Mailbox`] (background → audio) and [`ReturnQueue`] (audio → background), both
//!   `rtrb` rings of fixed size; nothing the audio thread receives is ever freed there.
//!
//! Everything here except the two queue ends held by the audio thread may block, allocate and do I/O:
//! call it from nih-plug's background task or from `initialize()`.

use crate::convolve::Kernel;
use crate::ir::{self, Source};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub use rtrb;

/// Meter codes.
pub const MISSING: f32 = -1.0;
pub const UNREADABLE: f32 = -2.0;
/// Unused prepared kernels and sources kept per process (bytes).
pub const CACHE_BYTES: usize = 64 << 20;

/// Why a slot did not load (its meter code).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    Missing,
    Unreadable,
}

impl LoadError {
    pub fn code(self) -> f32 {
        match self {
            LoadError::Missing => MISSING,
            LoadError::Unreadable => UNREADABLE,
        }
    }
}

/// An index entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub sha256: String,
}

/// The index path of the contract (environment first).
pub fn index_path() -> PathBuf {
    if let Some(p) = std::env::var_os("GOLEM_FX_IR_INDEX") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("golem-fx").join("ir-index.json");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(h) = std::env::var_os("HOME") {
            return PathBuf::from(h).join("Library/Application Support/golem-fx/ir-index.json");
        }
    }
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x).join("golem-fx").join("ir-index.json");
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".local/share/golem-fx/ir-index.json")
}

/// The index's entry for `slot` (None: no index, unreadable index, or no such slot).
pub fn lookup(index: &Path, slot: u32) -> Option<Entry> {
    let text = std::fs::read_to_string(index).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let e = v.get("slots")?.get(slot.to_string())?;
    let path = e.get("path")?.as_str()?;
    let sha = e.get("sha256")?.as_str()?;
    Some(Entry { path: PathBuf::from(path), sha256: sha.to_ascii_lowercase() })
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decodes a WAV (16 / 24 / 32-bit integer or 32-bit float, any channel count, extensible headers).
pub fn decode_wav(bytes: &[u8]) -> Result<Source, LoadError> {
    let mut r = hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|_| LoadError::Unreadable)?;
    let spec = r.spec();
    let nch = spec.channels.max(1) as usize;
    let mut chs: Vec<Vec<f64>> = vec![Vec::with_capacity(r.duration() as usize); nch];
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (i, s) in r.samples::<f32>().enumerate() {
                chs[i % nch].push(s.map_err(|_| LoadError::Unreadable)? as f64);
            }
        }
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f64;
            for (i, s) in r.samples::<i32>().enumerate() {
                chs[i % nch].push(s.map_err(|_| LoadError::Unreadable)? as f64 * scale);
            }
        }
    }
    if chs[0].is_empty() || spec.sample_rate == 0 {
        return Err(LoadError::Unreadable);
    }
    Ok(Source { rate: spec.sample_rate as f64, channels: chs })
}

/// Reads and checks a file: missing → −1, unreadable or hash mismatch → −2.
pub fn read_entry(e: &Entry) -> Result<Source, LoadError> {
    let bytes = match std::fs::read(&e.path) {
        Ok(b) => b,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Err(LoadError::Missing),
        Err(_) => return Err(LoadError::Unreadable),
    };
    if sha256_hex(&bytes) != e.sha256 {
        return Err(LoadError::Unreadable);
    }
    decode_wav(&bytes)
}

/// One output channel set of a voice: up to two kernels (L, R), shared `Arc`s.
pub type Kernels = [Option<Arc<Kernel>>; 2];

/// Builds the kernels of a prepared IR (one per output channel; equal channels share one kernel).
pub fn build_kernels(chs: &[Vec<f32>]) -> Kernels {
    let mut out: Kernels = [None, None];
    for (i, c) in chs.iter().take(2).enumerate() {
        if i == 1 && chs[0] == *c {
            out[1] = out[0].clone();
        } else {
            out[i] = Some(Arc::new(Kernel::new(c)));
        }
    }
    out
}

struct CacheEntry {
    kernels: Kernels,
    bytes: usize,
    used: u64,
}

struct SourceEntry {
    src: Arc<Source>,
    bytes: usize,
    used: u64,
}

/// The process-wide cache.
#[derive(Default)]
pub struct Cache {
    kernels: HashMap<String, CacheEntry>,
    sources: HashMap<String, SourceEntry>,
    clock: u64,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
static PREPARATIONS: AtomicUsize = AtomicUsize::new(0);
static READS: AtomicUsize = AtomicUsize::new(0);

pub fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

/// Kernel preparations done in this process (tests: "one preparation").
pub fn preparations() -> usize {
    PREPARATIONS.load(Ordering::Relaxed)
}

/// Files read in this process.
pub fn reads() -> usize {
    READS.load(Ordering::Relaxed)
}

fn in_use(k: &Kernels) -> bool {
    // the cache's own references: one per occurrence in the set
    k.iter().flatten().any(|a| {
        let own = k.iter().flatten().filter(|b| Arc::ptr_eq(a, b)).count();
        Arc::strong_count(a) > own
    })
}

impl Cache {
    pub fn get(&mut self, key: &str) -> Option<Kernels> {
        self.clock += 1;
        let c = self.clock;
        self.kernels.get_mut(key).map(|e| {
            e.used = c;
            e.kernels.clone()
        })
    }

    pub fn put(&mut self, key: String, kernels: Kernels) {
        self.clock += 1;
        let bytes = {
            let mut b = 0;
            for (i, k) in kernels.iter().enumerate() {
                if let Some(k) = k {
                    if i == 0 || !kernels[0].as_ref().is_some_and(|a| Arc::ptr_eq(a, k)) {
                        b += k.bytes();
                    }
                }
            }
            b
        };
        self.kernels.insert(key, CacheEntry { kernels, bytes, used: self.clock });
        self.evict();
    }

    pub fn source(&mut self, sha: &str) -> Option<Arc<Source>> {
        self.clock += 1;
        let c = self.clock;
        self.sources.get_mut(sha).map(|e| {
            e.used = c;
            e.src.clone()
        })
    }

    pub fn put_source(&mut self, sha: String, src: Arc<Source>) {
        self.clock += 1;
        let bytes = src.channels.iter().map(|c| c.len() * 8).sum();
        self.sources.insert(sha, SourceEntry { src, bytes, used: self.clock });
        self.evict();
    }

    /// Unused bytes held.
    pub fn unused_bytes(&self) -> usize {
        self.kernels.values().filter(|e| !in_use(&e.kernels)).map(|e| e.bytes).sum::<usize>() + self.sources.values().map(|e| e.bytes).sum::<usize>()
    }

    pub fn len(&self) -> usize {
        self.kernels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kernels.is_empty()
    }

    fn evict(&mut self) {
        while self.unused_bytes() > CACHE_BYTES {
            let k = self.kernels.iter().filter(|(_, e)| !in_use(&e.kernels)).map(|(k, e)| (e.used, k.clone())).min();
            let s = self.sources.iter().map(|(k, e)| (e.used, k.clone())).min();
            match (k, s) {
                (Some(k), Some(s)) if k.0 <= s.0 => {
                    self.kernels.remove(&k.1);
                }
                (Some(k), None) => {
                    self.kernels.remove(&k.1);
                }
                (_, Some(s)) => {
                    self.sources.remove(&s.1);
                }
                (None, None) => break,
            }
        }
    }
}

/// What a voice plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotSource {
    /// Slot 0 of a voice without a built-in: silence.
    None,
    /// The class's built-in IR (gfx-cab's slot 0 of A).
    Builtin,
    /// A slot of the index.
    Slot(u32),
}

/// A load request: what each voice should play, with which settings, at which rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Job {
    pub gen: u64,
    pub voices: [SlotSource; 2],
    pub nvoices: usize,
    pub settings: ir::Settings,
    pub sample_rate: f64,
}

/// A voice's result: kernels (all `None` for silence), or a failure code.
pub type VoiceResult = Result<Kernels, LoadError>;

/// The loader's answer, through the [`Mailbox`].
pub struct Done {
    pub gen: u64,
    pub voices: [SlotSource; 2],
    pub results: [VoiceResult; 2],
    pub settings: ir::Settings,
}

/// The persisted fallback `{slot: (path, sha256)}` (gfx-cab README §2.1 point 5).
pub type Persisted = Vec<(u32, String, String)>;

/// Turns jobs into kernels. Cheap to clone; shared by the background task and `initialize()`.
#[derive(Clone)]
pub struct Loader {
    /// The class's built-in IR at a rate (None: slot 0 is silence everywhere).
    pub builtin: Option<fn(f64) -> Source>,
    /// Fixed index path (tests); None = [`index_path`] at each load.
    pub index: Option<PathBuf>,
    pub persisted: Arc<std::sync::RwLock<Persisted>>,
}

impl Loader {
    pub fn new(builtin: Option<fn(f64) -> Source>) -> Self {
        Self { builtin, index: None, persisted: Arc::new(std::sync::RwLock::new(Vec::new())) }
    }

    fn entry(&self, slot: u32) -> Result<Entry, LoadError> {
        let idx = self.index.clone().unwrap_or_else(index_path);
        if let Some(e) = lookup(&idx, slot) {
            return Ok(e);
        }
        // the index wins; the persisted path only for a slot it does not hold
        let p = self.persisted.read().map(|p| p.iter().find(|(s, _, _)| *s == slot).cloned()).unwrap_or(None);
        match p {
            Some((_, path, sha)) => Ok(Entry { path: PathBuf::from(path), sha256: sha }),
            None => Err(LoadError::Missing),
        }
    }

    /// The decoded source of a slot and its content id.
    fn source(&self, s: SlotSource, sr: f64) -> Sourced {
        match s {
            SlotSource::None => Ok(None),
            SlotSource::Builtin => match self.builtin {
                Some(f) => {
                    let id = format!("builtin@{}", sr.to_bits());
                    let hit = cache().lock().map(|mut c| c.source(&id)).unwrap_or(None);
                    let src = match hit {
                        Some(s) => s,
                        None => {
                            let s = Arc::new(f(sr));
                            if let Ok(mut c) = cache().lock() {
                                c.put_source(id.clone(), s.clone());
                            }
                            s
                        }
                    };
                    Ok(Some((src, id)))
                }
                None => Ok(None),
            },
            SlotSource::Slot(slot) => {
                let e = self.entry(slot)?;
                if let Some(s) = cache().lock().map(|mut c| c.source(&e.sha256)).unwrap_or(None) {
                    // content known: still check the file is there (a slot of a deleted file fails)
                    if e.path.exists() {
                        return Ok(Some((s, e.sha256)));
                    }
                }
                READS.fetch_add(1, Ordering::Relaxed);
                let src = Arc::new(read_entry(&e)?);
                if let Ok(mut c) = cache().lock() {
                    c.put_source(e.sha256.clone(), src.clone());
                }
                if let Ok(mut p) = self.persisted.write() {
                    p.retain(|(s, _, _)| *s != slot);
                    p.push((slot, e.path.to_string_lossy().into_owned(), e.sha256.clone()));
                }
                Ok(Some((src, e.sha256)))
            }
        }
    }

    /// Runs a job (blocking).
    pub fn run(&self, job: &Job) -> Done {
        let mut srcs: [Sourced; 2] = [Ok(None), Ok(None)];
        for v in 0..job.nvoices.min(2) {
            srcs[v] = self.source(job.voices[v], job.sample_rate);
        }
        prepare_from(job, srcs, true)
    }
}

/// A source with its content id (the sha256, or `builtin@rate`).
pub type Sourced = Result<Option<(Arc<Source>, String)>, LoadError>;

/// Prepares a job's voices from their sources (`use_cache`: look up and store in the process
/// cache). Correlation with two IRs prepares the pair together; with one IR it is Onset.
pub fn prepare_from(job: &Job, srcs: [Sourced; 2], use_cache: bool) -> Done {
    let sr = job.sample_rate;
    let s = job.settings;
    let get = |k: &str| if use_cache { cache().lock().map(|mut c| c.get(k)).unwrap_or(None) } else { None };
    let put = |k: String, x: &Kernels| {
        if use_cache {
            if let Ok(mut c) = cache().lock() {
                c.put(k, x.clone());
            }
        }
    };
    let key = |role: &str, id: &str, partner: &str, st: &ir::Settings| format!("{role}/{id}/{partner}/{:x}/{}", sr.to_bits(), st.key());
    let mut results: [VoiceResult; 2] = [Ok([None, None]), Ok([None, None])];
    let pair = s.align == ir::Align::Correlation && job.nvoices == 2;
    if let (true, Ok(Some((a, ida))), Ok(Some((b, idb)))) = (pair, &srcs[0], &srcs[1]) {
        let (ka, kb) = (key("A", ida, idb, &s), key("B", ida, idb, &s));
        if let (Some(x), Some(y)) = (get(&ka), get(&kb)) {
            results = [Ok(x), Ok(y)];
        } else {
            let (pa, pb, _) = ir::prepare_pair(a, b, sr, &s);
            PREPARATIONS.fetch_add(1, Ordering::Relaxed);
            let (x, y) = (build_kernels(&pa), build_kernels(&pb));
            put(ka, &x);
            put(kb, &y);
            results = [Ok(x), Ok(y)];
        }
        return Done { gen: job.gen, voices: job.voices, results, settings: s };
    }
    let solo = if s.align == ir::Align::Correlation { ir::Settings { align: ir::Align::Onset, ..s } } else { s };
    for v in 0..job.nvoices.min(2) {
        results[v] = match &srcs[v] {
            Err(e) => Err(*e),
            Ok(None) => Ok([None, None]),
            Ok(Some((src, id))) => {
                let k = key("S", id, "", &solo);
                match get(&k) {
                    Some(x) => Ok(x),
                    None => {
                        let p = ir::prepare(src, sr, &solo);
                        PREPARATIONS.fetch_add(1, Ordering::Relaxed);
                        let x = build_kernels(&p);
                        put(k, &x);
                        Ok(x)
                    }
                }
            }
        };
    }
    Done { gen: job.gen, voices: job.voices, results, settings: s }
}

/// Background → audio: finished loads (a ring of 8; the audio thread only pops).
pub struct Mailbox;

impl Mailbox {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> (rtrb::Producer<Done>, rtrb::Consumer<Done>) {
        rtrb::RingBuffer::new(8)
    }
}

/// Audio → background: kernels to release (a ring of 256; the audio thread only pushes).
pub struct ReturnQueue;

impl ReturnQueue {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> (rtrb::Producer<Arc<Kernel>>, rtrb::Consumer<Arc<Kernel>>) {
        rtrb::RingBuffer::new(256)
    }
}

/// Writes a WAV (f32), for tests and tools.
pub fn write_wav(path: &Path, rate: u32, channels: &[Vec<f32>]) -> std::io::Result<()> {
    let spec = hound::WavSpec { channels: channels.len() as u16, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(path, spec).map_err(std::io::Error::other)?;
    let n = channels.iter().map(|c| c.len()).max().unwrap_or(0);
    for i in 0..n {
        for c in channels {
            w.write_sample(*c.get(i).unwrap_or(&0.0)).map_err(std::io::Error::other)?;
        }
    }
    w.finalize().map_err(std::io::Error::other)
}

/// Writes an index atomically (temporary file, then rename), for tests and tools.
pub fn write_index(path: &Path, slots: &[(u32, &Path)]) -> std::io::Result<()> {
    let mut m = serde_json::Map::new();
    for (slot, p) in slots {
        let bytes = std::fs::read(p)?;
        let mut e = serde_json::Map::new();
        e.insert("path".into(), serde_json::Value::String(p.to_string_lossy().into_owned()));
        e.insert("sha256".into(), serde_json::Value::String(sha256_hex(&bytes)));
        m.insert(slot.to_string(), serde_json::Value::Object(e));
    }
    let mut root = serde_json::Map::new();
    root.insert("format".into(), serde_json::Value::from(1));
    root.insert("slots".into(), serde_json::Value::Object(m));
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(&serde_json::Value::Object(root)).unwrap_or_default())?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Align, ChannelMap, Normalize, Settings};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gfx-ir-host-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn settings() -> Settings {
        Settings {
            align: Align::Onset,
            normalize: Normalize::Loudness,
            min_phase: false,
            reverse: false,
            length_s: 2.0,
            fade: 0.1,
            max_s: 2.0,
            band: (80.0, 6000.0),
            ref_db: 14.0,
            map: ChannelMap::Cab,
            out_channels: 1,
        }
    }

    fn ir(n: usize, seed: u32) -> Vec<f32> {
        (0..n).map(|k| ((k as f32 * 0.37 + seed as f32).sin()) * (-(k as f32) / 300.0).exp()).collect()
    }

    #[test]
    fn decodes_formats() {
        let d = dir("fmt");
        let h = ir(500, 1);
        for (bits, fmt) in [(16u16, hound::SampleFormat::Int), (24, hound::SampleFormat::Int), (32, hound::SampleFormat::Int), (32, hound::SampleFormat::Float)] {
            let p = d.join(format!("f{bits}{fmt:?}.wav"));
            let spec = hound::WavSpec { channels: 2, sample_rate: 44100, bits_per_sample: bits, sample_format: fmt };
            let mut w = hound::WavWriter::create(&p, spec).unwrap();
            for v in &h {
                for _ in 0..2 {
                    match fmt {
                        hound::SampleFormat::Float => w.write_sample(*v).unwrap(),
                        _ => w.write_sample((*v as f64 * ((1i64 << (bits - 1)) - 1) as f64) as i32).unwrap(),
                    }
                }
            }
            w.finalize().unwrap();
            let s = decode_wav(&std::fs::read(&p).unwrap()).unwrap();
            assert_eq!(s.rate, 44100.0);
            assert_eq!(s.channels.len(), 2);
            let err = s.channels[0].iter().zip(&h).fold(0.0f64, |m, (a, b)| m.max((a - *b as f64).abs()));
            assert!(err < 1e-4, "{bits} {fmt:?}: {err}");
        }
        assert_eq!(decode_wav(b"RIFF....junk"), Err(LoadError::Unreadable));
    }

    #[test]
    fn index_slots_failures_and_fallback() {
        let d = dir("idx");
        let a = d.join("a.wav");
        let b = d.join("b.wav");
        write_wav(&a, 48000, &[ir(800, 2)]).unwrap();
        write_wav(&b, 48000, &[ir(900, 3)]).unwrap();
        let idx = d.join("ir-index.json");
        write_index(&idx, &[(1, &a), (65535, &b)]).unwrap();
        let mut l = Loader::new(None);
        l.index = Some(idx.clone());
        let job = |v: [SlotSource; 2]| Job { gen: 1, voices: v, nvoices: 2, settings: settings(), sample_rate: 48000.0 };
        let r = l.run(&job([SlotSource::Slot(1), SlotSource::Slot(65535)]));
        assert!(r.results[0].as_ref().unwrap()[0].is_some());
        assert!(r.results[1].as_ref().unwrap()[0].is_some());
        // unknown slot: −1; hash mismatch: −2; missing file: −1
        let r = l.run(&job([SlotSource::Slot(7), SlotSource::None]));
        assert_eq!(r.results[0].as_ref().err(), Some(&LoadError::Missing));
        let c = d.join("c.wav");
        write_wav(&c, 48000, &[ir(100, 4)]).unwrap();
        write_index(&idx, &[(1, &a), (2, &c)]).unwrap();
        std::fs::write(&c, b"not the same bytes").unwrap();
        let r = l.run(&job([SlotSource::Slot(2), SlotSource::None]));
        assert_eq!(r.results[0].as_ref().err(), Some(&LoadError::Unreadable));
        std::fs::remove_file(&c).unwrap();
        let r = l.run(&job([SlotSource::Slot(2), SlotSource::None]));
        assert_eq!(r.results[0].as_ref().err(), Some(&LoadError::Missing));
        // the persisted path loads a slot the index lacks; the index wins when both hold it
        let p = l.persisted.read().unwrap().clone();
        assert!(p.iter().any(|(s, _, _)| *s == 65535));
        write_index(&idx, &[(1, &a)]).unwrap();
        let r = l.run(&job([SlotSource::Slot(65535), SlotSource::None]));
        assert!(r.results[0].is_ok());
        l.persisted.write().unwrap().push((1, b.to_string_lossy().into_owned(), "00".into()));
        let r = l.run(&job([SlotSource::Slot(1), SlotSource::None]));
        assert!(r.results[0].is_ok(), "the index wins over a wrong persisted entry");
    }

    #[test]
    fn cache_shares_and_keeps_kernels_in_use() {
        let d = dir("cache");
        let a = d.join("a.wav");
        write_wav(&a, 48000, &[ir(1200, 9)]).unwrap();
        let idx = d.join("ir-index.json");
        write_index(&idx, &[(3, &a)]).unwrap();
        let mut l = Loader::new(None);
        l.index = Some(idx);
        let mut s = settings();
        s.length_s = 1.234; // a key no other test uses
        let job = Job { gen: 1, voices: [SlotSource::Slot(3), SlotSource::None], nvoices: 1, settings: s, sample_rate: 48000.0 };
        let r1 = l.run(&job);
        let r2 = l.run(&job);
        let (k1, k2) = (r1.results[0].as_ref().unwrap()[0].clone().unwrap(), r2.results[0].as_ref().unwrap()[0].clone().unwrap());
        assert!(Arc::ptr_eq(&k1, &k2), "two instances on one IR share one kernel");
    }
}
