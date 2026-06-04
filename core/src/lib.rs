pub mod amp;
pub mod ir;

use amp::{AmpControls, VoxAmp, AMP_LATENCY};
use ir::{SpeakerStage, CONVOLUTION_LATENCY};
use nih_plug::prelude::*;
use std::sync::Arc;

pub struct VoxBox {
    params: Arc<VoxBoxParams>,
    channels: Vec<VoxAmp>,
    speakers: Vec<SpeakerStage>,
}

#[derive(Params)]
struct VoxBoxParams {
    #[id = "gain"]
    gain: FloatParam,
    #[id = "bass"]
    bass: FloatParam,
    #[id = "cut"]
    cut: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "master"]
    master: FloatParam,
    #[id = "speaker_ir"]
    speaker_ir: BoolParam,
}

impl Default for VoxBox {
    fn default() -> Self {
        Self {
            params: Arc::new(VoxBoxParams::default()),
            channels: Vec::new(),
            speakers: Vec::new(),
        }
    }
}

impl Default for VoxBoxParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Top Boost Volume",
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
            bass: FloatParam::new("Bass", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            cut: FloatParam::new("Cut", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            tone: FloatParam::new("Treble", 0.6, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            master: FloatParam::new(
                "Output Trim",
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
            speaker_ir: BoolParam::new("Speaker IR", false),
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
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let channels = audio_io_layout
            .main_output_channels
            .map(NonZeroU32::get)
            .unwrap_or(0) as usize;
        self.channels = (0..channels)
            .map(|_| VoxAmp::new(buffer_config.sample_rate))
            .collect();
        let sample_rate = buffer_config.sample_rate as u32;
        self.speakers = (0..channels)
            .map(|_| {
                SpeakerStage::from_embedded_ir(sample_rate)
                    .unwrap_or_else(|_| SpeakerStage::bypassed())
            })
            .collect();
        context.set_latency_samples((AMP_LATENCY + CONVOLUTION_LATENCY) as u32);
        true
    }

    fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.reset();
        }
        for speaker in &mut self.speakers {
            speaker.reset();
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
                volume: self.params.gain.smoothed.next(),
                bass: self.params.bass.smoothed.next(),
                cut: self.params.cut.smoothed.next(),
                treble: self.params.tone.smoothed.next(),
                output: self.params.master.smoothed.next(),
            };

            for (channel, sample) in channel_samples.iter_mut().enumerate() {
                let amp_output = self.channels[channel].process(*sample, controls);
                *sample =
                    self.speakers[channel].process(amp_output, self.params.speaker_ir.value());
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for VoxBox {
    const CLAP_ID: &'static str = "com.voxbox.graybox-amp";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("JMI AC30/6 Top Boost graybox amp");
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
