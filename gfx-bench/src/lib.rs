// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-bench — offline render and measurement of golem-fx effects (design: README.md).
//!
//! It drives each effect's host-independent processor ([`gfx_dsp::Effect`]); golem's own tools test
//! the VST3 in REAPER and the Golem Engine.

pub mod checks;
pub mod selftest;
pub mod signals;
pub mod wav;

use gfx_dsp::Effect;

/// Every effect the bench knows, by crate name.
pub const EFFECTS: &[&str] = &["gfx-boost", "gfx-gate", "gfx-comp", "gfx-limiter", "gfx-eq", "gfx-deesser", "gfx-wah", "gfx-autowah", "gfx-tremolo", "gfx-vibrato", "gfx-chorus", "gfx-flanger", "gfx-phaser", "gfx-rotary", "gfx-drive", "gfx-delay", "gfx-reverb", "gfx-shimmer", "gfx-killswitch", "gfx-octaver", "gfx-pitch", "gfx-pitch-poly", "gfx-pitch-classic", "gfx-detune", "gfx-gapmeter", "gfx-met", "gfx-emet", "gfx-shem", "gfx-golem", "gfx-glitch", "gfx-geq", "gfx-freqshift", "gfx-swell", "gfx-crusher", "gfx-ringmod", "gfx-bassdi", "gfx-tape", "gfx-grit", "gfx-dist", "gfx-transient", "gfx-mbcomp", "gfx-widener", "gfx-ducker", "gfx-fuzz", "gfx-octafuzz", "gfx-tuner", "gfx-cab", "gfx-irfx", "gfx-correct", "gfx-vocoder", "gfx-acoustic", "gfx-radio", "gfx-stretch", "gfx-grains", "gfx-sustainer", "gfx-polysynth", "gfx-spectralcomp", "gfx-morph", "gfx-formant"];

pub fn make(name: &str) -> Option<Box<dyn Effect>> {
    match name {
        "gfx-boost" | "boost" => Some(Box::new(gfx_boost::Boost::new())),
        "gfx-gate" | "gate" => Some(Box::new(gfx_gate::Gate::new())),
        "gfx-comp" | "comp" => Some(Box::new(gfx_comp::Comp::new())),
        "gfx-limiter" | "limiter" => Some(Box::new(gfx_limiter::Limiter::new())),
        "gfx-eq" | "eq" => Some(Box::new(gfx_eq::Eq::new())),
        "gfx-deesser" | "deesser" => Some(Box::new(gfx_deesser::Deesser::new())),
        "gfx-wah" | "wah" => Some(Box::new(gfx_wah::Wah::new())),
        "gfx-autowah" | "autowah" => Some(Box::new(gfx_autowah::Autowah::new())),
        "gfx-tremolo" | "tremolo" => Some(Box::new(gfx_tremolo::Tremolo::new())),
        "gfx-vibrato" | "vibrato" => Some(Box::new(gfx_vibrato::Vibrato::new())),
        "gfx-chorus" | "chorus" => Some(Box::new(gfx_chorus::Chorus::new())),
        "gfx-flanger" | "flanger" => Some(Box::new(gfx_flanger::Flanger::new())),
        "gfx-phaser" | "phaser" => Some(Box::new(gfx_phaser::Phaser::new())),
        "gfx-rotary" | "rotary" => Some(Box::new(gfx_rotary::Rotary::new())),
        "gfx-drive" | "drive" => Some(Box::new(gfx_drive::Drive::new())),
        "gfx-delay" | "delay" => Some(Box::new(gfx_delay::Delay::new())),
        "gfx-reverb" | "reverb" => Some(Box::new(gfx_reverb::Reverb::new())),
        "gfx-shimmer" | "shimmer" => Some(Box::new(gfx_shimmer::Shimmer::new())),
        "gfx-killswitch" | "killswitch" => Some(Box::new(gfx_killswitch::Killswitch::new())),
        "gfx-octaver" | "octaver" => Some(Box::new(gfx_octaver::Octaver::new())),
        "gfx-pitch" | "pitch" => Some(Box::new(gfx_pitch::Pitch::new())),
        // gfx-pitch with its Poly engine (the phase vocoder) chosen: the same plugin, checked on both engines
        "gfx-pitch-poly" | "pitch-poly" => {
            let mut p = gfx_pitch::Pitch::new();
            p.set_param("engine", 1.0);
            Some(Box::new(p))
        }
        "gfx-pitch-classic" | "pitch-classic" => Some(Box::new(gfx_pitch_classic::PitchClassic::new())),
        "gfx-detune" | "detune" => Some(Box::new(gfx_detune::Detune::new())),
        "gfx-gapmeter" | "gapmeter" => Some(Box::new(gfx_gapmeter::GapMeter::new())),
        "gfx-met" | "met" => Some(Box::new(gfx_met::Met::new())),
        "gfx-emet" | "emet" => Some(Box::new(gfx_emet::Emet::new())),
        "gfx-shem" | "shem" => Some(Box::new(gfx_shem::Shem::new())),
        "gfx-golem" | "golem" => Some(Box::new(gfx_golem::Golem::new())),
        "gfx-glitch" | "glitch" => Some(Box::new(gfx_glitch::Glitch::new())),
        "gfx-geq" | "geq" => Some(Box::new(gfx_geq::Geq::new())),
        "gfx-freqshift" | "freqshift" => Some(Box::new(gfx_freqshift::FreqShift::new())),
        "gfx-swell" | "swell" => Some(Box::new(gfx_swell::Swell::new())),
        "gfx-crusher" | "crusher" => Some(Box::new(gfx_crusher::Crusher::new())),
        "gfx-ringmod" | "ringmod" => Some(Box::new(gfx_ringmod::RingMod::new())),
        "gfx-bassdi" | "bassdi" => Some(Box::new(gfx_bassdi::BassDi::new())),
        "gfx-tape" | "tape" => Some(Box::new(gfx_tape::Tape::new())),
        "gfx-grit" | "grit" => Some(Box::new(gfx_grit::Grit::new())),
        "gfx-dist" | "dist" => Some(Box::new(gfx_dist::Dist::new())),
        "gfx-transient" | "transient" => Some(Box::new(gfx_transient::Transient::new())),
        "gfx-mbcomp" | "mbcomp" => Some(Box::new(gfx_mbcomp::MbComp::new())),
        "gfx-widener" | "widener" => Some(Box::new(gfx_widener::Widener::new())),
        "gfx-ducker" | "ducker" => Some(Box::new(gfx_ducker::Ducker::new())),
        "gfx-fuzz" | "fuzz" => Some(Box::new(gfx_fuzz::Fuzz::new())),
        "gfx-octafuzz" | "octafuzz" => Some(Box::new(gfx_octafuzz::Octafuzz::new())),
        "gfx-tuner" | "tuner" => Some(Box::new(gfx_tuner::Tuner::new())),
        "gfx-cab" | "cab" => Some(Box::new(gfx_cab::Cab::new())),
        "gfx-irfx" | "irfx" => Some(Box::new(gfx_irfx::IrFx::new())),
        "gfx-correct" | "correct" => Some(Box::new(gfx_correct::Correct::new())),
        "gfx-vocoder" | "vocoder" => Some(Box::new(gfx_vocoder::Vocoder::new())),
        "gfx-acoustic" | "acoustic" => Some(Box::new(gfx_acoustic::Acoustic::new())),
        "gfx-radio" | "radio" => Some(Box::new(gfx_radio::Radio::new())),
        "gfx-stretch" | "stretch" => Some(Box::new(gfx_stretch::Stretch::new())),
        "gfx-grains" | "grains" => Some(Box::new(gfx_grains::Grains::new())),
        "gfx-sustainer" | "sustainer" => Some(Box::new(gfx_sustainer::Sustainer::new())),
        "gfx-polysynth" | "polysynth" => Some(Box::new(gfx_polysynth::Polysynth::new())),
        "gfx-spectralcomp" | "spectralcomp" => Some(Box::new(gfx_spectralcomp::SpectralComp::new())),
        "gfx-morph" | "morph" => Some(Box::new(gfx_morph::Morph::new())),
        "gfx-formant" | "formant" => Some(Box::new(gfx_formant::Formant::new())),
        // the bench's own faulty effects (selftest.rs)
        "_hard-gain" => Some(Box::new(selftest::HardGain::default())),
        "_sloppy" => Some(Box::new(selftest::Sloppy::default())),
        _ => None,
    }
}

/// Every effect's parameters (PARAMS order) as JSON, for `visuals/`: one entry per plugin crate
/// (gfx-pitch-poly is gfx-pitch with another engine, so it is left out).
pub fn manifest_json() -> String {
    use gfx_dsp::ParamKind;
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let num = |v: f32| if v.is_finite() { format!("{v}") } else { "null".into() };
    let mut out = String::from("{\n  \"_comment\": \"Generated by `gfx-bench manifest` from each effect's PARAMS. Do not edit.\",\n  \"effects\": {\n");
    let names: Vec<&str> = EFFECTS.iter().copied().filter(|n| *n != "gfx-pitch-poly").collect();
    for (k, name) in names.iter().enumerate() {
        let e = make(name).expect("every EFFECTS entry is known");
        out += &format!("    \"{name}\": [\n");
        let ps = e.params();
        for (i, p) in ps.iter().enumerate() {
            let (kind, extra) = match p.kind {
                ParamKind::Float => ("float", String::new()),
                ParamKind::Bool => ("bool", String::new()),
                ParamKind::Enum(n) => ("enum", format!(", \"options\": {n}")),
                ParamKind::Int => ("int", String::new()),
            };
            out += &format!(
                "      {{\"id\": \"{}\", \"name\": \"{}\", \"kind\": \"{kind}\", \"min\": {}, \"max\": {}, \"default\": {}{extra}{}}}{}\n",
                esc(p.id),
                esc(p.name),
                num(p.min),
                num(p.max),
                num(p.default),
                if p.exp { ", \"exp\": true" } else { "" },
                if i + 1 < ps.len() { "," } else { "" }
            );
        }
        out += &format!("    ]{}\n", if k + 1 < names.len() { "," } else { "" });
    }
    out += "  }\n}\n";
    out
}

/// The common checks' options for an effect (bounds its design documents).
pub fn options(name: &str) -> checks::Options {
    let mut o = checks::Options::default();
    if matches!(name, "gfx-chorus" | "chorus" | "gfx-flanger" | "flanger" | "gfx-phaser" | "phaser" | "gfx-rotary" | "rotary") {
        // the profiles spread the voices left / right: stereo by nature
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-shimmer" | "shimmer") {
        // a 4 s tail regenerated through the shifters is still at −55 dBFS three seconds after the
        // burst: the pad itself, not a residue
        o.silence_floor_db = -40.0;
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-glitch" | "glitch") {
        // Ring Hold at 40 % (the default) lets the loudest partials ring for ~2.5 s (T60): 3 s after the
        // burst they are at −90 – −110 dBFS, the effect itself; the channels' randoms differ by `width`
        o.silence_floor_db = -80.0;
        // trim +24 → −24 dB drives the wet's soft limiter, then the held partials (Ring Hold) fall back
        // through it as they decay: moving distortion for a moment (−42 dB); the Ringing window has hard
        // frame edges by design and its engine fades in over 50 ms (−45 dB at 192 kHz)
        o.click_bound_db = -40.0;
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-widener" | "widener") {
        // a side generated from the mid: L ≠ R on equal inputs by design (README §5); the one-channel
        // checks render the fold, which is the dry (exact law)
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-cab" | "cab") {
        // the built-in IR is minimum-phase with a few samples of group delay: the noise correlation
        // peak is not at 0. The true 0 is checked by the Dirac test (README §5, the gfx-drive
        // precedent)
        o.skip_latency = true;
    }
    if matches!(name, "gfx-irfx" | "irfx") {
        // a stereo effect IR plays L and R through different channels: stereo by nature; its tail is
        // the effect (README §5, the gfx-delay precedent)
        o.stereo_by_nature = true;
        o.silence_floor_db = -60.0;
    }
    if matches!(name, "gfx-freqshift" | "freqshift") {
        // the 2 / 2 layout comes first, but L = R on equal inputs (the right locks onto the left): the
        // mono / stereo check runs. `fbk` 110 % at `time` 0 self-oscillates on purpose (README §5: the
        // silence floor is exempt there): its fastest loop modes are the high ones, a burst of
        // −10 dB above 10 kHz at 192 kHz while the runaway starts. That onset is the effect, not a click
        o.step_exempt = vec![("fbk", 110.0)];
    }
    if matches!(name, "gfx-swell" | "swell") {
        // `lookahead` changes the reported latency (setup only, README §2.6); `fire` is a trigger, not a
        // level change (README §5)
        o.skip_steps = vec!["lookahead", "fire"];
    }
    if matches!(name, "gfx-correct" | "correct") {
        // `fastmode` and `register` change the reported latency: setup parameters (fade out, engines
        // reset, new latency, fade in; README §2.7 and §3.1)
        o.skip_steps = vec!["fastmode", "register"];
    }
    if matches!(name, "gfx-transient" | "transient") {
        // `lookahead` changes the reported latency (setup only, README §2.6 and §5)
        o.skip_steps = vec!["lookahead"];
    }
    if matches!(name, "gfx-ducker" | "ducker") {
        // without a key a ducker is the identity and every check would pass trivially (README §2.10):
        // the common checks run with a key that ducks and releases all the time
        o.key = Some(signals::key_bursts);
        // `lookahead` changes the reported latency (setup only, README §2.9 and §2.10)
        o.skip_steps = vec!["lookahead"];
    }
    if matches!(name, "gfx-ringmod" | "ringmod") {
        // a ring modulator's wet does not correlate with its input; the minimum-phase oversamplers add a
        // few samples of group delay to wet and dry alike, not reported (README §2, the gfx-drive
        // precedent)
        o.skip_latency = true;
        // `freq` or `pitch` from end to end moves the carrier 10 octaves (16 kHz ↔ 0, 14 kHz ↔ 14 Hz)
        // through its 20 ms log smoothing: the products sweep through the band, a chirp, not a click
        // (−45 dB above 10 kHz at 44.1 kHz)
        o.click_bound_db = -40.0;
    }
    if matches!(name, "gfx-reverb" | "reverb") {
        // a 1.3 s tail is still at −138 dBFS three seconds after the burst: its own decay, not a residue
        o.silence_floor_db = -120.0;
        // decorrelated outputs: stereo by nature
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-delay" | "delay") {
        // 3 s after the burst the repeats are still decaying at the feedback rate (35 %, 375 ms:
        // ≈ −75 dBFS): that is the trail itself, not a residue
        o.silence_floor_db = -60.0;
    }
    if matches!(name, "gfx-pitch-classic" | "pitch-classic") {
        // superpitch reports 0 and so do we: the mean delay (about window / 2) is the splicer's own
        o.skip_latency = true;
    }
    if matches!(name, "gfx-detune" | "detune") {
        // two voices: up left, down right
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-octaver" | "octaver") {
        // the dry runs through two all-passes that align it with the sub's smoothing filter: a phase
        // rotation (~0.5 ms of group delay at the bottom), no delay line, not reported (README §3)
        o.skip_latency = true;
    }
    if matches!(name, "gfx-emet" | "emet") {
        // 8× minimum-phase oversampling: a few samples of group delay, no delay line, not reported
        // (README §2, the gfx-drive precedent)
        o.skip_latency = true;
        // a headroom step glides the knee through the whole clipping range (−18 → +6 dB): real
        // harmonics on the way, as gfx-drive's ceiling; a full tone sweep in 30 ms rings the tone
        // network as a pot turned that fast would (−46 dB)
        o.click_bound_db = -35.0;
    }
    if matches!(name, "gfx-crusher" | "crusher") {
        // minimum-phase kernels (sampling and rendering) and the hold's T / 2: a few samples of group
        // delay, no delay line, not reported; the Off setting measures exactly 0 (README §5)
        o.skip_latency = true;
        // a `quant` step 16 → 1 bit or a `rate` jump creates real high-frequency content: the effect,
        // not a click (README §5). Measured: a `rate` / `rate_shift` glide 50 ↔ 48,000 Hz passes
        // the 1 kHz tone through every rate, whose Raw images and steps stand −4 dB re peak above
        // 10 kHz (96 kHz), `quant` 16 → 1 −11 dB, `input` ±24 dB −26 dB; the README's −30 dB cannot
        // hold. Every other parameter stays below −35 dB (gfx-crusher's own parameter-step test).
        o.click_bound_db = -3.0;
    }
    if matches!(name, "gfx-bassdi" | "bassdi") {
        // 8× minimum-phase oversampling (5.5 samples of group delay) and the twins' dispersion, no
        // delay line, not reported (README §5, the gfx-drive / gfx-emet precedent)
        o.skip_latency = true;
        // README §5 allowed −30 dB for a `gain` jump; measured −78 dB worst: the default bound holds
    }
    if matches!(name, "gfx-tape" | "tape") {
        // the wet carries the transport's D0 (≈ 1 ms at the defaults, moving with the settings) and the
        // half-bands' minimum-phase group delay: not reported (README §2.8, the gfx-vibrato precedent)
        o.skip_latency = true;
        // dropouts, snags, crinkle and stop create real high-frequency changes that are the effect
        // (README §5)
        o.click_bound_db = -40.0;
    }
    if matches!(name, "gfx-grit" | "grit") {
        // minimum-phase half-bands, one inner sample of averaging and the crossovers' all-pass
        // dispersion (4.9 ms at DC, LR4 defaults): no delay line, not reported (README §2.7, the
        // gfx-drive precedent)
        o.skip_latency = true;
        // a module change at high Amount creates real high-frequency content (README §5, gfx-crusher's
        // bound)
        o.click_bound_db = -30.0;
    }
    if matches!(name, "gfx-dist" | "dist") {
        // 8× / 16× minimum-phase oversampling: a few samples of group delay, no delay line, not
        // reported (README §2.9, the gfx-drive / gfx-emet precedent)
        o.skip_latency = true;
        // a high-gain clipper puts real energy above 10 kHz on any step (README §5: −35 dB, the
        // gfx-drive precedent). Measured: `tight` 1000 → 20 Hz glides the drive up 3 dB and moves
        // the clip's phase over 30 ms; the square-ish output's own HF changes by −32 dB re peak
        // between the glide and the steady state (−59 dB for 1000 → 100 Hz): the effect, not a click
        // (gfx-dist README, As coded). Every other step stays below −36 dB.
        o.click_bound_db = -30.0;
    }
    if matches!(name, "gfx-mbcomp" | "mbcomp") {
        // Minimum phase: the crossovers' all-pass dispersion (4.0 ms at 20 Hz, LR4 3-band defaults),
        // no delay line, reported 0 (README §5, gfx-grit's precedent); Linear is checked by its own
        // test (latency L per rate family, impulse peak at L)
        o.skip_latency = true;
        // `phase` changes the reported latency: a set-up switch (fade out, switch, fade in; README §2.3)
        o.skip_steps = vec!["phase"];
    }
    if matches!(name, "gfx-fuzz" | "fuzz") {
        // 4× – 16× minimum-phase oversampling: a few samples of group delay, no delay line, not
        // reported (README §2.8, the gfx-drive / gfx-emet precedent)
        o.skip_latency = true;
        // README §5 asked −35 dB for the structural changes (a model, buffer or bias change moves
        // the operating point); with the incoming circuit settling 60 ms unheard before its fade they
        // stay below −35 dB. What does not is a drive step: `input` ±24 dB (−21 dB at 96 kHz, −26 dB
        // at 48 kHz) and `guitar_vol` 0 → 100 % (−31 dB at 192 kHz) move 24 dB of signal into or out
        // of the face, whose coupling capacitors charge or discharge and move its bias for ~100 ms:
        // the output blocks (−20 dB for 50 ms) then recovers through a louder, brighter bias
        // (blocking distortion), the circuit's own transient as on the real pedal (gfx-fuzz README,
        // As coded)
        o.click_bound_db = -20.0;
    }
    if matches!(name, "gfx-octafuzz" | "octafuzz") {
        // gfx-fuzz's engine at 8×: minimum-phase oversampling, not reported (gfx-fuzz README §2.8)
        o.skip_latency = true;
        // drive steps move the coupling capacitors and the input clamp, and with them the octave
        // itself (H2 − H1 swings by ≈ 30 dB between 30 mV and 0.3 V): real HF content that changes,
        // the circuit's own transient. Measured: `input` ±24 dB −20 / −20 / −16 / −15 dB re peak at
        // 44.1 / 48 / 96 / 192 kHz, `fuzz` 0 ↔ 100 % −18 dB at 96 kHz (gfx-octafuzz README, As coded)
        o.click_bound_db = -14.0;
    }
    if matches!(name, "gfx-vocoder" | "vocoder") {
        // the wet is the carrier (the pedal's own oscillators at the defaults) shaped by the input's
        // envelopes: it does not correlate with the input, and the dry is not delayed. Zero latency by
        // construction (IIR banks, no lookahead); Self's identity is checked by its own test (README §5)
        o.skip_latency = true;
    }
    if matches!(name, "gfx-acoustic" | "acoustic") {
        // a body has no single correlation peak: no delay line, reported 0, its own onset test
        // (README §5: first non-zero output at sample 0, gfx-acoustic's tests)
        o.skip_latency = true;
        // 2 / 2 is the two-microphone pair: at the default `width` 40 % the channels hear two mic
        // positions (README §2.9); `width` 0 is L = R = the 1 / 1 output, checked by its own test
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-radio" | "radio") {
        // the half-bands and the IIR filters' minimum phase: group delay, no delay line, not reported
        // (README §5, the gfx-drive precedent)
        o.skip_latency = true;
        // key-up clicks, squelch tails, PLC repeats and crackles are content (README §5, gfx-crusher's
        // precedent); the behaviour tests check the transitions themselves
        o.click_bound_db = -30.0;
        // Hall's 2.2 s tail is about −82 dB three seconds after the burst; the channel noise has
        // stopped by then (README §5)
        o.silence_floor_db = -70.0;
        // 2 / 2 runs the room's two decorrelated outputs (README §2.9): L ≠ R on equal inputs
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-stretch" | "stretch") {
        // decorrelated random-phase channels from a mono DI (`width` 100 %): stereo by nature
        // (README §5); the dry sits at sample 0 and the wet is late by design (reported 0, §2.9)
        o.stereo_by_nature = true;
        // Rolling ×4 plays the 0.5 s burst back four times slower: its head leaves the burst at
        // 4 × (0.5 s + N) ≈ 5 s and the last frame sounds until ≈ 5.9 s (−3 dBFS at 3 s, measured).
        // The floor is read after 7 s of silence instead of 3 (README As coded: §5's "the default
        // holds" was wrong); the bound stays −140 dBFS
        o.silence_tail_s = 7.0;
    }
    if matches!(name, "gfx-grains" | "grains") {
        // grains spread over the stereo field (`spread` 50 % by default): stereo by nature (README §5).
        // The silence floor stays −140 dBFS at the defaults (no feedback: the wet has nothing left to
        // read 0.72 s after the burst), the latency check runs (the unity dry dominates lag 0)
        o.stereo_by_nature = true;
    }
    if matches!(name, "gfx-sustainer" | "sustainer") {
        // README §5: defaults. `feed`, `trigger` and `carry` are gestures and rules, not levels
        o.skip_steps = vec!["feed", "trigger", "carry"];
    }
    if matches!(name, "gfx-polysynth" | "polysynth") {
        // README §5: default step-click bound; silence floor −90 dBFS (the release tails of up to 4 s
        // at `release` maximum are the effect itself, as gfx-delay's)
        o.silence_floor_db = -90.0;
        // the wet is the pedal's own waveforms (one per partial, or the voice's oscillators) and the
        // dry is off at the defaults: nothing correlates with the input at a lag. README §5's latency
        // row (0 reported, the dry's impulse at sample 0 at every `dry`) is the crate's own test; IIR
        // banks and half-bands, no delay line
        o.skip_latency = true;
        // Steps that are the effect, not clicks (README As coded): `drive` 0 ↔ 100 % lifts the resonant
        // synth over −6 dBFS on the way (the tanh's small-signal gain k_d / √k_d, gfx-autowah's law,
        // README §2.5) and the ceiling (§2.1) soft-clips it, −34 dB re peak (−63 dB at `reso` 0);
        // `cutoff` 40 Hz ↔ 12 kHz at 48 kHz sweeps the resonance across the synth's own top harmonics
        // (9 – 10 kHz, under fcap 10.08 kHz, at the check's 10 kHz corner: −23 dB re peak when steady),
        // −36 dB (−141 dB at `reso` 0, −90 dB at 44.1 kHz, ≤ −54 dB at 96 – 192 kHz). Every other step
        // ≤ −50 dB
        o.step_exempt = vec![("drive", 0.0), ("drive", 100.0), ("cutoff", 12000.0), ("cutoff", 40.0)];
    }
    if matches!(name, "gfx-spectralcomp" | "spectralcomp") {
        // README §5: the common checks run with a key (and without: `source` Main, the default, never
        // reads it), so `source` Key is exercised by the extreme-value and parameter-step checks. No
        // exemption: Live reports 0 and Studio N − 1 (its own latency test); `mode` and `window` are
        // set-up switches whose fades stay under the click bound
        o.key = Some(signals::key_bursts);
    }
    if matches!(name, "gfx-morph" | "morph") {
        // README §5: minimum-phase half-bands and crossovers, ADAA, IIR banks: group delay and
        // dispersion, no delay line, reported 0 (gfx-grit's precedent); the first output sample of an
        // impulse is sample 0, checked by the crate's own test
        o.skip_latency = true;
        // a type change at high drive makes real high-frequency content (gfx-grit's bound)
        o.click_bound_db = -30.0;
    }
    if matches!(name, "gfx-formant" | "formant") {
        // README §5: the latency check runs on the identity rows only (the crate's own test: Live 0 or
        // `lookahead`, Grain D, the impulse at the reported sample); at the default `formant` −3 st the
        // minimum-phase envelope filter adds the group delay of the resonances it moves (gfx-crusher's
        // precedent). `engine`, `register` and `lookahead` change the reported latency: setup
        // parameters (fade out, engines reset, new latency, fade in; README §2.5). Step-click bound:
        // default
        o.skip_latency = true;
        o.skip_steps = vec!["engine", "register", "lookahead"];
    }
    if matches!(name, "gfx-drive" | "drive") {
        // 4× minimum-phase oversampling: ~6 samples of group delay, no delay line, not reported (README §3)
        o.skip_latency = true;
        // a ceiling step glides through heavier clipping at a still-high level on its way (0 → −40 dB):
        // real harmonics above 10 kHz, not a click
        o.click_bound_db = -25.0;
    }
    if matches!(name, "gfx-vibrato" | "vibrato" | "gfx-chorus" | "chorus" | "gfx-rotary" | "rotary") {
        // the mean delay of a pitch vibrato moves with the speed: by design it is not reported (README §2)
        o.skip_latency = true;
    }
    if matches!(name, "gfx-wah" | "wah" | "gfx-autowah" | "autowah") {
        o.skip_latency = true;
        // a +20 dB resonance swept heel to toe in 15 ms is a chirp, and `drive` clips that resonance
        // on purpose: both put real energy above 10 kHz that is not a click
        o.click_bound_db = -35.0;
    }
    o
}

/// A fresh, prepared and reset instance.
pub fn fresh(name: &str, sample_rate: f32, channels: usize, max_block: usize) -> Box<dyn Effect> {
    let mut e = make(name).unwrap_or_else(|| panic!("unknown effect {name}"));
    e.prepare(sample_rate, max_block, channels);
    e.reset();
    e
}

/// Sets parameters, settles them (by processing silence is NOT done: settings are applied before
/// reset so they start settled), and renders `input` in blocks of `block`.
pub fn render(effect: &mut dyn Effect, input: &[Vec<f32>], block: usize) -> Vec<Vec<f32>> {
    let mut out: Vec<Vec<f32>> = input.to_vec();
    let n = out.first().map_or(0, |c| c.len());
    let mut start = 0;
    while start < n {
        let end = (start + block).min(n);
        let mut slices: Vec<&mut [f32]> = out.iter_mut().map(|c| &mut c[start..end]).collect();
        effect.process(&mut slices);
        start = end;
    }
    out
}

/// Same, with a key (sidechain) signal: `key` holds the auxiliary bus's channels, as long as `input`.
pub fn render_with_key(effect: &mut dyn Effect, input: &[Vec<f32>], key: Option<&[Vec<f32>]>, block: usize) -> Vec<Vec<f32>> {
    let Some(key) = key else { return render(effect, input, block) };
    let mut out: Vec<Vec<f32>> = input.to_vec();
    let n = out.first().map_or(0, |c| c.len());
    let mut start = 0;
    while start < n {
        let end = (start + block).min(n);
        let mut slices: Vec<&mut [f32]> = out.iter_mut().map(|c| &mut c[start..end]).collect();
        let k: Vec<&[f32]> = key.iter().map(|c| &c[start..end]).collect();
        effect.process_with_key(&mut slices, Some(&k));
        start = end;
    }
    out
}

/// Applies settings and lets them settle: set, then `reset()` (smoothers settle on their targets).
pub fn apply_settings(effect: &mut dyn Effect, settings: &[(String, f32)]) {
    for (id, v) in settings {
        effect.set_param(id, *v);
    }
    effect.reset();
}
