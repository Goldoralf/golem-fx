// SPDX-License-Identifier: GPL-3.0-or-later
//! Host tempo and position → note values and LFO phase.
//!
//! The golem contract: tempo comes from the host transport, never from a parameter; no tempo → 120 BPM,
//! never silence. Synced phases come from the host's **position** (so loops and seeks stay aligned),
//! computed per sample in f64.

pub const FALLBACK_BPM: f64 = 120.0;

/// The contract's shared `sync` list (docs/golem-contract.md §Parameter conventions), in enum order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sync {
    Off,
    W4,
    W2,
    W1,
    D2,
    D2Dot,
    D2T,
    D4,
    D4Dot,
    D4T,
    D8,
    D8Dot,
    D8T,
    D16,
    D16Dot,
    D16T,
    D32,
}

impl Sync {
    pub const ALL: [Sync; 17] = [
        Sync::Off,
        Sync::W4,
        Sync::W2,
        Sync::W1,
        Sync::D2,
        Sync::D2Dot,
        Sync::D2T,
        Sync::D4,
        Sync::D4Dot,
        Sync::D4T,
        Sync::D8,
        Sync::D8Dot,
        Sync::D8T,
        Sync::D16,
        Sync::D16Dot,
        Sync::D16T,
        Sync::D32,
    ];

    pub fn from_index(i: usize) -> Sync {
        *Self::ALL.get(i).unwrap_or(&Sync::Off)
    }

    /// Length in quarter notes (beats); `None` for Off.
    pub fn beats(self) -> Option<f64> {
        Some(match self {
            Sync::Off => return None,
            Sync::W4 => 16.0,
            Sync::W2 => 8.0,
            Sync::W1 => 4.0,
            Sync::D2 => 2.0,
            Sync::D2Dot => 3.0,
            Sync::D2T => 4.0 / 3.0,
            Sync::D4 => 1.0,
            Sync::D4Dot => 1.5,
            Sync::D4T => 2.0 / 3.0,
            Sync::D8 => 0.5,
            Sync::D8Dot => 0.75,
            Sync::D8T => 1.0 / 3.0,
            Sync::D16 => 0.25,
            Sync::D16Dot => 0.375,
            Sync::D16T => 1.0 / 6.0,
            Sync::D32 => 0.125,
        })
    }
}

/// What an effect knows about the host transport for the current block.
#[derive(Clone, Copy, Debug)]
#[derive(Default)]
pub struct TransportInfo {
    /// Host tempo, if any.
    pub tempo: Option<f64>,
    /// Position of the block's first sample in quarter notes, if the host gives it.
    pub pos_beats: Option<f64>,
    pub playing: bool,
}


impl TransportInfo {
    pub fn bpm(&self) -> f64 {
        match self.tempo {
            Some(t) if t.is_finite() && t > 1.0 => t,
            _ => FALLBACK_BPM,
        }
    }

    /// Samples per beat at the current tempo.
    pub fn samples_per_beat(&self, sample_rate: f32) -> f64 {
        sample_rate as f64 * 60.0 / self.bpm()
    }

    /// A note value in samples.
    pub fn note_samples(&self, beats: f64, sample_rate: f32) -> f64 {
        beats * self.samples_per_beat(sample_rate)
    }

    /// Phase (0 – 1) of a cycle `period_beats` long, at sample `n` of the block, when the host is playing
    /// and gives its position. `None` otherwise (the effect then runs a free accumulator at the host tempo).
    pub fn phase_at(&self, n: usize, period_beats: f64, offset: f64, sample_rate: f32) -> Option<f64> {
        if !self.playing {
            return None;
        }
        let pos = self.pos_beats?;
        let beats = pos + n as f64 / self.samples_per_beat(sample_rate);
        Some((beats / period_beats + offset).rem_euclid(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_tempo() {
        let t = TransportInfo::default();
        assert_eq!(t.bpm(), 120.0);
        assert_eq!(t.note_samples(Sync::D8Dot.beats().unwrap(), 48000.0), 18000.0);
    }

    #[test]
    fn phase_follows_position() {
        let t = TransportInfo { tempo: Some(120.0), pos_beats: Some(3.5), playing: true };
        // one-bar cycle (4 beats): beat 3.5 is phase 0.875
        let p = t.phase_at(0, 4.0, 0.0, 48000.0).unwrap();
        assert!((p - 0.875).abs() < 1e-12);
        // half a beat later (12000 samples at 120 BPM) → phase 0
        let p = t.phase_at(12000, 4.0, 0.0, 48000.0).unwrap();
        assert!(p < 1e-9 || p > 1.0 - 1e-9);
    }
}

/// Declares the contract's shared `sync` enum (`SyncParam`) for a nih-plug wrapper, in [`Sync::ALL`]'s
/// order. Expanded where `nih_plug::prelude::*` is in scope.
#[macro_export]
macro_rules! sync_param_enum {
    () => {
        #[derive(Enum, Debug, PartialEq, Clone, Copy)]
        enum SyncParam {
            #[id = "off"]
            Off,
            #[id = "4/1"]
            #[name = "4/1"]
            W4,
            #[id = "2/1"]
            #[name = "2/1"]
            W2,
            #[id = "1/1"]
            #[name = "1/1"]
            W1,
            #[id = "1/2"]
            #[name = "1/2"]
            D2,
            #[id = "1/2d"]
            #[name = "1/2•"]
            D2Dot,
            #[id = "1/2t"]
            #[name = "1/2T"]
            D2T,
            #[id = "1/4"]
            #[name = "1/4"]
            D4,
            #[id = "1/4d"]
            #[name = "1/4•"]
            D4Dot,
            #[id = "1/4t"]
            #[name = "1/4T"]
            D4T,
            #[id = "1/8"]
            #[name = "1/8"]
            D8,
            #[id = "1/8d"]
            #[name = "1/8•"]
            D8Dot,
            #[id = "1/8t"]
            #[name = "1/8T"]
            D8T,
            #[id = "1/16"]
            #[name = "1/16"]
            D16,
            #[id = "1/16d"]
            #[name = "1/16•"]
            D16Dot,
            #[id = "1/16t"]
            #[name = "1/16T"]
            D16T,
            #[id = "1/32"]
            #[name = "1/32"]
            D32,
        }
    };
}
