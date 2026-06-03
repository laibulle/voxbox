mod amp;

use amp::{AmpControls, VoxAmp};
use nih_plug::prelude::*;
use std::sync::Arc;

pub struct VoxBox {
    params: Arc<VoxBoxParams>,
    channels: Vec<VoxAmp>,
}

#[derive(Params)]
struct VoxBoxParams {
    #[id = "gain"]
    gain: FloatParam,
    #[id = "cut"]
    cut: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "master"]
    master: FloatParam,
}

impl Default for VoxBox {
    fn default() -> Self {
        Self {
            params: Arc::new(VoxBoxParams::default()),
            channels: Vec::new(),
        }
    }
}

impl Default for VoxBoxParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Gain",
                0.55,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 1.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            cut: FloatParam::new("Cut", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            tone: FloatParam::new("Tone", 0.6, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            master: FloatParam::new(
                "Master",
                util::db_to_gain(-9.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-36.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Plugin for VoxBox {
    const NAME: &'static str = "VoxBox";
    const VENDOR: &'static str = "VoxBox";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "dev@localhost";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let channels = audio_io_layout
            .main_output_channels
            .map(NonZeroU32::get)
            .unwrap_or(0) as usize;
        self.channels = (0..channels)
            .map(|_| VoxAmp::new(buffer_config.sample_rate))
            .collect();
        true
    }

    fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for mut channel_samples in buffer.iter_samples() {
            let controls = AmpControls {
                gain: self.params.gain.smoothed.next(),
                cut: self.params.cut.smoothed.next(),
                tone: self.params.tone.smoothed.next(),
                master: self.params.master.smoothed.next(),
            };

            for (channel, sample) in channel_samples.iter_mut().enumerate() {
                *sample = self.channels[channel].process(*sample, controls);
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for VoxBox {
    const CLAP_ID: &'static str = "com.voxbox.graybox-amp";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Graybox British chime amp");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for VoxBox {
    const VST3_CLASS_ID: [u8; 16] = *b"VoxBoxGrayAmp001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(VoxBox);
nih_export_vst3!(VoxBox);
