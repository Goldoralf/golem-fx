// SPDX-License-Identifier: GPL-3.0-or-later
//! The nih-plug wrapper (VST3). It only maps parameters onto the processor and calls it.

use crate::{Boost, GAIN_MAX_DB, GAIN_MIN_DB};
use gfx_dsp::Effect;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Params)]
struct BoostParams {
    /// Catalogue id `gain` (blocks `volume` and `clean-boost`), plain dB, linear range: golem
    /// writes the value as is.
    #[id = "gain"]
    gain: FloatParam,
    #[id = "polarity"]
    polarity: BoolParam,
}

impl Default for BoostParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Gain",
                0.0,
                FloatRange::Linear { min: GAIN_MIN_DB, max: GAIN_MAX_DB },
            )
            .with_unit(" dB")
            .with_step_size(0.01)
            .with_value_to_string(Arc::new(|v| {
                if v <= GAIN_MIN_DB {
                    "-inf".to_string()
                } else {
                    format!("{v:.2}")
                }
            })),
            polarity: BoolParam::new("Polarity", false).with_value_to_string(Arc::new(|v| {
                if v { "Inverted" } else { "Normal" }.to_string()
            })),
        }
    }
}

pub struct GfxBoost {
    params: Arc<BoostParams>,
    dsp: Boost,
    last_gain: f32,
    last_polarity: bool,
}

impl Default for GfxBoost {
    fn default() -> Self {
        Self {
            params: Arc::new(BoostParams::default()),
            dsp: Boost::new(),
            last_gain: f32::NAN,
            last_polarity: false,
        }
    }
}

impl GfxBoost {
    fn sync_params(&mut self, force: bool) {
        let g = self.params.gain.value();
        if force || g != self.last_gain {
            self.last_gain = g;
            self.dsp.set_gain_db(g);
        }
        let p = self.params.polarity.value();
        if force || p != self.last_polarity {
            self.last_polarity = p;
            self.dsp.set_polarity(p);
        }
    }
}

impl Plugin for GfxBoost {
    const NAME: &'static str = "GoleM FX Boost";
    const VENDOR: &'static str = "GoleM";
    const URL: &'static str = "https://golem.fm";
    const EMAIL: &'static str = "contact@golem.fm";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Mono first: a gain before the amp must not turn a golem lane stereo.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    #[cfg(feature = "gui")]
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-boost"))
    }

    fn initialize(
        &mut self,
        layout: &AudioIOLayout,
        config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let channels = layout.main_output_channels.map_or(1, |c| c.get() as usize);
        self.dsp.prepare(config.sample_rate, config.max_buffer_size as usize, channels);
        true
    }

    fn reset(&mut self) {
        self.sync_params(true);
        self.dsp.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params(false);
        self.dsp.process(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for GfxBoost {
    // Permanent: golem's sessions and engine table store it.
    const VST3_CLASS_ID: [u8; 16] = *b"GoleMFxBoost0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_vst3!(GfxBoost);
