// SPDX-License-Identifier: GPL-3.0-or-later
//! The plugin's parameters by id, read and written from the GUI thread.

use nih_plug::prelude::{GuiContext, ParamFlags, ParamPtr, Params};
use std::collections::HashMap;
use std::sync::Arc;

/// Every parameter of the plugin by its `#[id]`. Keeps the `Params` object alive: the pointers are
/// valid as long as this is.
pub struct Binding {
    _owner: Arc<dyn Params>,
    map: HashMap<String, ParamPtr>,
}

impl Binding {
    pub fn new(params: Arc<dyn Params>) -> Self {
        let map = params.param_map().into_iter().map(|(id, ptr, _)| (id, ptr)).collect();
        Self { _owner: params, map }
    }

    pub fn get(&self, id: &str) -> Option<Param> {
        self.map.get(id).map(|p| Param(*p))
    }

    /// A value that changes when any parameter does (to redo what depends on all of them).
    pub fn stamp(&self, ids: &[String]) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for id in ids {
            if let Some(p) = self.get(id) {
                h = (h ^ p.normalized().to_bits() as u64).wrapping_mul(0x100000001b3);
            }
        }
        h
    }
}

/// One parameter. Reading is safe from any thread (atomics); the pointer lives as long as its `Binding`.
#[derive(Clone, Copy, PartialEq)]
pub struct Param(ParamPtr);

impl Param {
    pub fn normalized(self) -> f32 {
        unsafe { self.0.unmodulated_normalized_value() }
    }
    pub fn plain(self) -> f32 {
        unsafe { self.0.unmodulated_plain_value() }
    }
    pub fn default_normalized(self) -> f32 {
        unsafe { self.0.default_normalized_value() }
    }
    pub fn to_normalized(self, plain: f32) -> f32 {
        unsafe { self.0.preview_normalized(plain) }
    }
    /// The value as the plugin prints it, with its unit.
    pub fn text(self, normalized: f32) -> String {
        unsafe { self.0.normalized_value_to_string(normalized, true) }
    }
    /// The same without its unit (what a typed value starts from).
    pub fn bare_text(self, normalized: f32) -> String {
        unsafe { self.0.normalized_value_to_string(normalized, false) }
    }
    /// A typed value, normalised, if the plugin reads it.
    pub fn parse(self, text: &str) -> Option<f32> {
        unsafe { self.0.string_to_normalized_value(text.trim()) }
    }
    /// The host's bypass: the presets leave it alone.
    pub fn is_bypass(self) -> bool {
        unsafe { self.0.flags() }.contains(ParamFlags::BYPASS)
    }
}

/// Writes parameters the way a host expects: a gesture opens, values follow, the gesture closes.
#[derive(Clone, Copy)]
pub struct Setter<'a>(pub &'a dyn GuiContext);

impl Setter<'_> {
    pub fn begin(&self, p: Param) {
        unsafe { self.0.raw_begin_set_parameter(p.0) }
    }
    pub fn set(&self, p: Param, normalized: f32) {
        unsafe { self.0.raw_set_parameter_normalized(p.0, normalized.clamp(0.0, 1.0)) }
    }
    pub fn end(&self, p: Param) {
        unsafe { self.0.raw_end_set_parameter(p.0) }
    }
    /// One value as a whole gesture (a click, a wheel notch, a preset).
    pub fn once(&self, p: Param, normalized: f32) {
        self.begin(p);
        self.set(p, normalized);
        self.end(p);
    }
}

/// The parameter the pointer holds: its gesture is open, and `at` is where the pointer put it, 0 to 1
/// along the parameter's taper. The editor closes the gesture when no button is down any more, so a
/// drag that ends anywhere, or with the window, is closed.
pub struct Held {
    pub param: Param,
    pub at: f32,
}

impl Held {
    pub fn grab(slot: &mut Option<Held>, setter: Setter, param: Param, at: f32) {
        Self::release(slot, setter);
        setter.begin(param);
        *slot = Some(Held { param, at });
    }

    pub fn release(slot: &mut Option<Held>, setter: Setter) {
        if let Some(h) = slot.take() {
            setter.end(h.param);
        }
    }
}
