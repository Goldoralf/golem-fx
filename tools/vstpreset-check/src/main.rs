// SPDX-License-Identifier: GPL-3.0-or-later
//! Loads the .vstpreset files of tools/export-vstpresets.py into the real plugins, as a VST3 host does,
//! and checks that every parameter ends up at the preset's value.
//!
//!   cargo build --release -p gfx-chorus -p gfx-delay        # (repo root) the plugins' cdylibs
//!   python3 tools/export-vstpresets.py
//!   cargo run --release --manifest-path tools/vstpreset-check/Cargo.toml -- gfx-chorus gfx-delay
//!
//! Per crate it dlopens target/release/lib<crate>.so (`--lib-dir` to change; Linux), takes the factory
//! through GetPluginFactory and creates one instance (IComponent + IEditController). Then for each of the
//! crate's presets, in one row on that same instance (so a value left over from the previous preset is
//! caught):
//!   - the header: 'VST3', version 1, a class id string equal to the factory's class id as the VST3
//!     SDK's FUID::toString prints it, a chunk list whose 'Comp' entry lies inside the file;
//!   - the 'Comp' chunk parses as nih-plug's own `PluginState`;
//!   - IComponent::setState(Comp) returns kResultOk (nih-plug's setState: deserialize_json then
//!     set_state_inner, the code path of a host loading a preset);
//!   - IComponent::getState, parsed as `PluginState`, holds every parameter of the descriptor at the value
//!     the preset asks for (presets/<crate>.json, the descriptor's default otherwise), resolved here
//!     independently of the Python exporter; enum parameters are compared through the controller's own
//!     display string (IEditController::getParamStringByValue), which is the option label.

use libloading::{Library, Symbol};
use nih_plug::prelude::PluginState;
use nih_plug::wrapper::state::ParamValue;
use nih_plug::wrapper::vst3::vst3_sys;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use vst3_sys as vst3_com; // the co_class macro refers to `vst3_com`
use vst3_sys::base::{kIBSeekCur, kIBSeekEnd, kIBSeekSet, kResultFalse, kResultOk, tresult};
use vst3_sys::base::{IBStream, IPluginBase, IPluginFactory, PClassInfo};
use vst3_sys::utils::SharedVstPtr;
use vst3_sys::vst::{IComponent, IEditController};
use vst3_sys::{ComInterface, VstPtr, IID, VST3};

#[VST3(implements(IBStream))]
struct MemStream {
    data: RefCell<Vec<u8>>,
    pos: Cell<usize>,
}

impl MemStream {
    fn new(data: Vec<u8>) -> Box<Self> {
        Self::allocate(RefCell::new(data), Cell::new(0))
    }
    /// The object as the `IBStream*` a host passes (single-interface class: the object pointer).
    fn shared(&self) -> SharedVstPtr<dyn IBStream> {
        let raw = self as *const Self as *mut c_void;
        unsafe { std::mem::transmute::<*mut c_void, SharedVstPtr<dyn IBStream>>(raw) }
    }
}

impl IBStream for MemStream {
    unsafe fn read(&self, buffer: *mut c_void, num_bytes: i32, num_bytes_read: *mut i32) -> tresult {
        let data = self.data.borrow();
        let pos = self.pos.get().min(data.len());
        let n = (num_bytes.max(0) as usize).min(data.len() - pos);
        std::ptr::copy_nonoverlapping(data[pos..].as_ptr(), buffer as *mut u8, n);
        self.pos.set(pos + n);
        if !num_bytes_read.is_null() {
            *num_bytes_read = n as i32;
        }
        kResultOk
    }
    unsafe fn write(&self, buffer: *const c_void, num_bytes: i32, num_bytes_written: *mut i32) -> tresult {
        let mut data = self.data.borrow_mut();
        let n = num_bytes.max(0) as usize;
        let pos = self.pos.get();
        if data.len() < pos + n {
            data.resize(pos + n, 0);
        }
        std::ptr::copy_nonoverlapping(buffer as *const u8, data[pos..].as_mut_ptr(), n);
        self.pos.set(pos + n);
        if !num_bytes_written.is_null() {
            *num_bytes_written = n as i32;
        }
        kResultOk
    }
    unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> tresult {
        let base = match mode {
            m if m == kIBSeekSet => 0,
            m if m == kIBSeekCur => self.pos.get() as i64,
            m if m == kIBSeekEnd => self.data.borrow().len() as i64,
            _ => return kResultFalse,
        };
        let p = base + pos;
        if p < 0 {
            return kResultFalse;
        }
        self.pos.set(p as usize);
        if !result.is_null() {
            *result = p;
        }
        kResultOk
    }
    unsafe fn tell(&self, pos: *mut i64) -> tresult {
        if !pos.is_null() {
            *pos = self.pos.get() as i64;
        }
        kResultOk
    }
}

/// nih-plug's `hash_param_id` (src/wrapper/util.rs): the VST3 ParamID of a string id.
fn hash_param_id(id: &str) -> u32 {
    let mut hash: u32 = 0;
    for c in id.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(c as u32);
    }
    hash & !(1 << 31)
}

/// VST3 SDK FUID::toString: COM-compatible platforms print the GUID fields (Data1..3 as integers of the
/// in-memory bytes), the others the 16 bytes in order.
fn fuid_string(cid: &[u8; 16]) -> String {
    if cfg!(target_os = "windows") {
        let d1 = u32::from_le_bytes([cid[0], cid[1], cid[2], cid[3]]);
        let d2 = u16::from_le_bytes([cid[4], cid[5]]);
        let d3 = u16::from_le_bytes([cid[6], cid[7]]);
        let rest: String = cid[8..].iter().map(|b| format!("{b:02X}")).collect();
        format!("{d1:08X}{d2:04X}{d3:04X}{rest}")
    } else {
        cid.iter().map(|b| format!("{b:02X}")).collect()
    }
}

struct Preset {
    class_id: String,
    chunks: BTreeMap<String, Vec<u8>>,
}

fn parse_vstpreset(bytes: &[u8]) -> Result<Preset, String> {
    let i32_at = |o: usize| bytes.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let i64_at = |o: usize| bytes.get(o..o + 8).map(|b| i64::from_le_bytes(b.try_into().unwrap()));
    if bytes.get(0..4) != Some(b"VST3") {
        return Err("no 'VST3' magic".into());
    }
    if i32_at(4) != Some(1) {
        return Err(format!("version {:?}", i32_at(4)));
    }
    let class_id = std::str::from_utf8(bytes.get(8..40).ok_or("short header")?).map_err(|e| e.to_string())?.to_string();
    let list = i64_at(40).ok_or("short header")? as usize;
    if bytes.get(list..list + 4) != Some(b"List") {
        return Err("no 'List' at the list offset".into());
    }
    let count = i32_at(list + 4).ok_or("short list")?;
    let mut chunks = BTreeMap::new();
    for k in 0..count as usize {
        let e = list + 8 + k * 20;
        let id = String::from_utf8_lossy(bytes.get(e..e + 4).ok_or("short list")?).to_string();
        let off = i64_at(e + 4).ok_or("short list")? as usize;
        let size = i64_at(e + 12).ok_or("short list")? as usize;
        if off < 48 || off + size > list {
            return Err(format!("chunk {id} at {off}+{size} outside the data area"));
        }
        chunks.insert(id, bytes[off..off + size].to_vec());
    }
    if list + 8 + count as usize * 20 != bytes.len() {
        return Err("bytes after the chunk list".into());
    }
    Ok(Preset { class_id, chunks })
}

/// What a preset asks of one parameter.
enum Want {
    F32(f64),
    I32(i64),
    Bool(bool),
    Label(String),
}

/// presets/<crate>.json + descriptors.json + manifest.json -> every parameter's expected value.
fn expected(desc: &Value, kinds: &BTreeMap<String, String>, preset: &Value) -> BTreeMap<String, Want> {
    let given = &preset["params"];
    let mut out = BTreeMap::new();
    for (pid, q) in desc["params"].as_object().unwrap() {
        let v = given.get(pid);
        let lo = q["min"].as_f64().unwrap();
        let def = q["default"].as_f64().unwrap();
        let want = match q["kind"].as_str().unwrap() {
            "enum" => {
                let opts: Vec<String> = match q["options"].as_array() {
                    Some(o) => o.iter().map(|x| x.as_str().unwrap().to_string()).collect(),
                    None => (lo as i64..=q["max"].as_f64().unwrap() as i64).map(|x| x.to_string()).collect(),
                };
                Want::Label(v.map(|x| x.as_str().unwrap().to_string()).unwrap_or_else(|| opts[def as usize].clone()))
            }
            "bool" => Want::Bool(v.map(|x| x.as_bool().unwrap()).unwrap_or(def != 0.0)),
            _ => {
                let x = v.map(|x| x.as_f64().unwrap()).unwrap_or(def);
                if kinds.get(pid).map(String::as_str) == Some("int") {
                    Want::I32(x as i64)
                } else {
                    Want::F32(x)
                }
            }
        };
        out.insert(pid.clone(), want);
    }
    out
}

fn tchar_string(s: &[i16]) -> String {
    let units: Vec<u16> = s.iter().take_while(|c| **c != 0).map(|c| *c as u16).collect();
    String::from_utf16_lossy(&units)
}

fn check_crate(root: &Path, lib_dir: &Path, crate_name: &str, descs: &Value, manifest: &Value) -> Result<(usize, usize), String> {
    let desc = &descs[crate_name];
    let plugin_name = desc["name"].as_str().ok_or("not in descriptors.json")?;
    let kinds: BTreeMap<String, String> = manifest[crate_name]
        .as_array()
        .ok_or("not in manifest.json")?
        .iter()
        .map(|p| (p["id"].as_str().unwrap().to_string(), p["kind"].as_str().unwrap().to_string()))
        .collect();
    let presets_json: Value = serde_json::from_slice(&std::fs::read(root.join("presets").join(format!("{crate_name}.json"))).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let dir = root.join("dist/vstpresets/GoleM").join(plugin_name);
    let lib_path = lib_dir.join(format!("lib{}.so", crate_name.replace('-', "_")));

    unsafe {
        let lib = Library::new(&lib_path).map_err(|e| format!("{}: {e}", lib_path.display()))?;
        if let Ok(entry) = lib.get::<extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry") {
            entry(std::ptr::null_mut());
        }
        let get_factory: Symbol<extern "system" fn() -> *mut c_void> = lib.get(b"GetPluginFactory").map_err(|e| e.to_string())?;
        let factory = VstPtr::<dyn IPluginFactory>::owned(get_factory() as *mut _).ok_or("null factory")?;
        let mut info: PClassInfo = std::mem::zeroed();
        if factory.get_class_info(0, &mut info) != kResultOk {
            return Err("getClassInfo failed".into());
        }
        let class_name = std::ffi::CStr::from_ptr(info.name.as_ptr()).to_string_lossy().to_string();
        if class_name != plugin_name {
            return Err(format!("factory class name {class_name:?} != descriptor name {plugin_name:?}"));
        }
        let want_class = fuid_string(&info.cid.data);
        let ascii_class: String = desc["class_id"].as_str().unwrap().bytes().map(|b| format!("{b:02X}")).collect();
        if want_class != ascii_class {
            return Err(format!("factory FUID {want_class} != hex(descriptor class_id) {ascii_class}"));
        }
        let mut obj: *mut c_void = std::ptr::null_mut();
        let iid: IID = <dyn IComponent as ComInterface>::IID;
        if factory.create_instance(&info.cid, &iid, &mut obj) != kResultOk || obj.is_null() {
            return Err("createInstance failed".into());
        }
        let comp = VstPtr::<dyn IComponent>::owned(obj as *mut _).ok_or("null component")?;
        comp.initialize(std::ptr::null_mut());
        let ctrl = comp.cast::<dyn IEditController>().ok_or("no IEditController")?;

        let mut files = 0;
        let mut values = 0;
        for preset in presets_json["presets"].as_array().unwrap() {
            let name = preset["name"].as_str().unwrap();
            let file = std::fs::read_dir(&dir)
                .map_err(|e| format!("{}: {e}", dir.display()))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .find(|p| {
                    let stem = p.file_stem().unwrap().to_string_lossy().to_string();
                    let clean: String = name.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
                    stem == clean.trim().trim_end_matches('.')
                })
                .ok_or(format!("{name}: no .vstpreset"))?;
            let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
            let p = parse_vstpreset(&bytes).map_err(|e| format!("{name}: {e}"))?;
            if p.class_id != want_class {
                return Err(format!("{name}: header class id {} != factory {want_class}", p.class_id));
            }
            let comp_bytes = p.chunks.get("Comp").ok_or(format!("{name}: no Comp chunk"))?;
            let parsed: PluginState = serde_json::from_slice(comp_bytes).map_err(|e| format!("{name}: Comp is not a PluginState: {e}"))?;
            let info_xml = p.chunks.get("Info").map(|b| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
            if !info_xml.starts_with("<?xml") || !info_xml.contains("<MetaInfo>") {
                return Err(format!("{name}: no Info meta information"));
            }

            let input = MemStream::new(comp_bytes.clone());
            if comp.set_state(input.shared()) != kResultOk {
                return Err(format!("{name}: IComponent::setState refused the Comp chunk"));
            }
            let output = MemStream::new(Vec::new());
            if comp.get_state(output.shared()) != kResultOk {
                return Err(format!("{name}: getState failed"));
            }
            let saved: PluginState = serde_json::from_slice(&output.data.borrow()).map_err(|e| e.to_string())?;

            let want = expected(desc, &kinds, preset);
            let meters: Vec<&str> = desc["meters"].as_array().map(|m| m.iter().filter_map(|x| x["id"].as_str()).collect()).unwrap_or_default();
            for pid in saved.params.keys() {
                // the read-only meter outputs are parameters too (golem's fork), never part of a preset
                if !want.contains_key(pid) && !meters.contains(&pid.as_str()) {
                    return Err(format!("{name}: the plugin has {pid}, the descriptor does not"));
                }
            }
            if parsed.params.len() != want.len() {
                return Err(format!("{name}: Comp holds {} params, the plugin {}", parsed.params.len(), want.len()));
            }
            for (pid, w) in &want {
                let got = saved.params.get(pid).ok_or(format!("{name}: plugin lacks {pid}"))?;
                let ok = match (w, got) {
                    (Want::F32(x), ParamValue::F32(g)) => ((*g as f64) - x).abs() <= 1e-5 * x.abs().max(1.0),
                    (Want::I32(x), ParamValue::I32(g)) => *g as i64 == *x,
                    (Want::Bool(x), ParamValue::Bool(g)) => g == x,
                    (Want::Label(x), _) => {
                        let id = hash_param_id(pid);
                        let mut s = [0i16; 128];
                        let norm = ctrl.get_param_normalized(id);
                        ctrl.get_param_string_by_value(id, norm, s.as_mut_ptr()) == kResultOk && tchar_string(&s) == *x
                    }
                    _ => false,
                };
                if !ok {
                    let shown = match w {
                        Want::F32(x) => format!("{x}"),
                        Want::I32(x) => format!("{x}"),
                        Want::Bool(x) => format!("{x}"),
                        Want::Label(x) => x.clone(),
                    };
                    return Err(format!("{name}: {pid} = {got:?} after loading, the preset says {shown}"));
                }
                values += 1;
            }
            files += 1;
        }
        drop(ctrl);
        comp.terminate();
        drop(comp);
        drop(factory);
        std::mem::forget(lib); // the plugin's statics may outlive the instance (logger)
        Ok((files, values))
    }
}

fn main() -> std::process::ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let mut lib_dir = root.join("target/release");
    let mut crates = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--lib-dir" {
            lib_dir = PathBuf::from(args.next().expect("--lib-dir DIR"));
        } else {
            crates.push(a);
        }
    }
    let read = |p: &str| -> Value { serde_json::from_slice(&std::fs::read(root.join(p)).unwrap()).unwrap() };
    let descs = read("visuals/data/descriptors.json")["descriptors"].clone();
    let manifest = read("visuals/data/manifest.json")["effects"].clone();
    if crates.is_empty() {
        crates = ["gfx-chorus", "gfx-morph", "gfx-formant", "gfx-delay"].map(String::from).to_vec();
    }
    let mut failed = false;
    for c in &crates {
        match check_crate(&root, &lib_dir, c, &descs, &manifest) {
            Ok((files, values)) => println!("{c}: {files} presets loaded, {values} parameter values checked: ok"),
            Err(e) => {
                println!("{c}: FAIL {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
