// SPDX-License-Identifier: GPL-3.0-or-later
//! The host-independent face of an effect: what gfx-bench drives, and what each nih-plug wrapper
//! forwards to. An effect crate implements [`Effect`] on its processor; the wrapper only maps nih-plug
//! parameters onto `set_param` (or the processor's typed setters) and calls `process`.

/// What kind of value a parameter carries. Values are always passed as plain `f32`
/// (an enum's index, a bool's 0 / 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Float,
    Bool,
    /// An enum with this many options; the value is the index.
    Enum(u32),
    /// An integer parameter (patterns, counts).
    Int,
}

/// A parameter as the bench sees it. `id` is the permanent id (the golem catalogue's when the
/// effect stands for a golem block).
#[derive(Clone, Copy, Debug)]
pub struct ParamInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub kind: ParamKind,
    /// Whether golem may drive it from the expression pedal.
    pub exp: bool,
}

impl ParamInfo {
    pub const fn float(id: &'static str, name: &'static str, min: f32, max: f32, default: f32) -> Self {
        Self { id, name, min, max, default, kind: ParamKind::Float, exp: false }
    }
    pub const fn boolean(id: &'static str, name: &'static str, default: bool) -> Self {
        Self {
            id,
            name,
            min: 0.0,
            max: 1.0,
            default: if default { 1.0 } else { 0.0 },
            kind: ParamKind::Bool,
            exp: false,
        }
    }
    pub const fn enumeration(id: &'static str, name: &'static str, options: u32, default: u32) -> Self {
        Self {
            id,
            name,
            min: 0.0,
            max: (options - 1) as f32,
            default: default as f32,
            kind: ParamKind::Enum(options),
            exp: false,
        }
    }
    pub const fn with_exp(mut self) -> Self {
        self.exp = true;
        self
    }
}

/// A host-independent effect processor.
pub trait Effect: Send {
    /// Every parameter, in a stable order.
    fn params(&self) -> &'static [ParamInfo];

    /// Sizes all memory. Called before any processing and whenever the sample rate, the largest
    /// block or the channel count changes. May allocate; nothing else may.
    fn prepare(&mut self, sample_rate: f32, max_block: usize, channels: usize);

    /// Back to the initial state: silence in every buffer, LFOs at phase 0, seeds restored,
    /// smoothers settled on their current targets.
    fn reset(&mut self);

    /// Sets a parameter by id to a plain value; it takes effect from the next processed sample
    /// (through the parameter's own smoothing). Unknown ids are ignored.
    fn set_param(&mut self, id: &str, value: f32);

    /// The host transport for the coming block (tempo, position). Effects that sync to the tempo use
    /// it; the default ignores it. Called before `process` when the host gives it.
    fn set_transport(&mut self, _transport: crate::transport::TransportInfo) {}

    /// A read-only meter (the plugin's output parameter) by id, as last published; None for an
    /// unknown id. Effects without meters keep the default.
    fn meter(&self, _id: &str) -> Option<f32> {
        None
    }

    /// Reported latency in samples.
    fn latency(&self) -> usize {
        0
    }

    /// Processes in place. `buffers.len()` is the channel count given to `prepare`; every slice has
    /// the same length, at most `max_block`.
    fn process(&mut self, buffers: &mut [&mut [f32]]);

    /// Processes in place with a key (sidechain) signal: the auxiliary bus's channels (1 or 2), each
    /// as long as the main buffers; `None` when no key is connected. Effects without a key input
    /// ignore it: the default is `process`, so nothing changes for them.
    fn process_with_key(&mut self, buffers: &mut [&mut [f32]], key: Option<&[&[f32]]>) {
        let _ = key;
        self.process(buffers);
    }
}
