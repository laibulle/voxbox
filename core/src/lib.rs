pub mod amp;
pub mod chain;
pub mod circuit;
pub mod ir;
pub mod neural_cell;
pub mod pedal;
pub mod rig;

pub use amp::{
    configure_nox30_first_stage_graybox, configure_nox30_first_stage_neural, AmpControls,
    ComponentBoundary, NeuralCellMode, Nox30OperatingPoint, NOX30_COMPONENT_BOUNDARIES,
};
pub use chain::{
    amp_model_descriptor, AmpModelDescriptor, ControlDescriptor, ControlKind, DeviceConfig,
    DeviceControls, DeviceModelDescriptor, DeviceSlotConfig, DeviceSlotControls,
    DeviceVisualDescriptor, SignalChain, SignalChainConfig, SignalChainControls,
};
pub use pedal::{
    configure_minotaur_clip_neural, configure_minotaur_tone_neural, Brigade, BrigadeControls,
    Celeste, CelesteControls, ConnectionState, Dartford, DartfordControls, DartfordWave,
    ElectricalSignal, GodessOne, GodessOneControls, GodessOneMode, Jetstream, JetstreamControls,
    Load, Lumen, LumenControls, Minotaur, MinotaurControls, Monarch, MonarchControls, Muffin,
    MuffinControls, Muon, MuonControls, Springfield, SpringfieldControls, Tron, TronControls,
};
pub use rig::RigConfig;

#[cfg(feature = "plugin")]
use amp::AMP_LATENCY;
#[cfg(feature = "plugin")]
use ir::{SpeakerStage, CONVOLUTION_LATENCY};
#[cfg(feature = "plugin")]
use nih_plug::prelude::*;
#[cfg(feature = "plugin")]
use std::sync::Arc;

#[cfg(feature = "plugin")]
pub struct Greybound {
    params: Arc<GreyboundParams>,
    channels: Vec<SignalChain>,
    speakers: Vec<SpeakerStage>,
    ui_controls: Option<AmpControls>,
    chain_config: SignalChainConfig,
    sample_rate: Option<f32>,
}

#[cfg(feature = "plugin")]
#[derive(Params)]
struct GreyboundParams {
    #[id = "gain"]
    gain: FloatParam,
    #[id = "bass"]
    bass: FloatParam,
    #[id = "dumbler"]
    dumbler: BoolParam,
    #[id = "cut"]
    cut: FloatParam,
    #[id = "tone"]
    tone: FloatParam,
    #[id = "master"]
    master: FloatParam,
    #[id = "speaker_ir"]
    speaker_ir: BoolParam,
    #[id = "fuzz"]
    fuzz: BoolParam,
    #[id = "fuzz_sustain"]
    fuzz_sustain: FloatParam,
    #[id = "fuzz_tone"]
    fuzz_tone: FloatParam,
    #[id = "fuzz_level"]
    fuzz_level: FloatParam,
    #[id = "overdrive"]
    overdrive: BoolParam,
    #[id = "overdrive_gain"]
    overdrive_gain: FloatParam,
    #[id = "overdrive_treble"]
    overdrive_treble: FloatParam,
    #[id = "overdrive_output"]
    overdrive_output: FloatParam,
}

#[cfg(feature = "plugin")]
impl Default for Greybound {
    fn default() -> Self {
        Self {
            params: Arc::new(GreyboundParams::default()),
            channels: Vec::new(),
            speakers: Vec::new(),
            ui_controls: None,
            chain_config: SignalChainConfig::amp_only("nox30"),
            sample_rate: None,
        }
    }
}

#[cfg(feature = "plugin")]
impl Default for GreyboundParams {
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
            dumbler: BoolParam::new("Dumbler Model", false),
            fuzz: BoolParam::new("Muffin Fuzz", false),
            fuzz_sustain: FloatParam::new(
                "Fuzz Sustain",
                MuffinControls::default().sustain,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            fuzz_tone: FloatParam::new(
                "Fuzz Tone",
                MuffinControls::default().tone,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            fuzz_level: FloatParam::new(
                "Fuzz Level",
                MuffinControls::default().level,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            overdrive: BoolParam::new("Minotaur Overdrive", false),
            overdrive_gain: FloatParam::new(
                "Minotaur Gain",
                MinotaurControls::default().gain,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            overdrive_treble: FloatParam::new(
                "Minotaur Treble",
                MinotaurControls::default().treble,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            overdrive_output: FloatParam::new(
                "Minotaur Output",
                MinotaurControls::default().output,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

#[cfg(feature = "plugin")]
impl Plugin for Greybound {
    const NAME: &'static str = "Greybound";
    const VENDOR: &'static str = "Greybound";
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
        let mut chain_config = self.chain_config.clone();
        if self.params.dumbler.value() && chain_config.amp_model == "nox30" {
            chain_config.amp_model = "dumbler".to_string();
        }
        if chain_config.pre_amp.is_empty() {
            chain_config
                .pre_amp
                .push(DeviceSlotConfig::active(DeviceConfig::Minotaur));
            chain_config
                .pre_amp
                .push(DeviceSlotConfig::active(DeviceConfig::Muffin));
        }
        self.sample_rate = Some(buffer_config.sample_rate);
        self.channels = build_signal_chains(buffer_config.sample_rate, channels, &chain_config);
        let sample_rate = buffer_config.sample_rate as u32;
        self.speakers = (0..channels)
            .map(|_| {
                SpeakerStage::from_embedded_ir(sample_rate)
                    .unwrap_or_else(|_| SpeakerStage::bypassed())
            })
            .collect();
        _context.set_latency_samples((AMP_LATENCY + CONVOLUTION_LATENCY) as u32);
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
            let controls = if let Some(ui_controls) = self.ui_controls {
                ui_controls
            } else {
                AmpControls {
                    volume: self.params.gain.smoothed.next(),
                    bass: self.params.bass.smoothed.next(),
                    cut: self.params.cut.smoothed.next(),
                    treble: self.params.tone.smoothed.next(),
                    output: self.params.master.smoothed.next(),
                    drive: 0.0,
                    presence: 0.0,
                    sag: 0.0,
                }
            };
            let fuzz_controls = MuffinControls {
                sustain: self.params.fuzz_sustain.smoothed.next(),
                tone: self.params.fuzz_tone.smoothed.next(),
                level: self.params.fuzz_level.smoothed.next(),
            };
            let overdrive_controls = MinotaurControls {
                gain: self.params.overdrive_gain.smoothed.next(),
                treble: self.params.overdrive_treble.smoothed.next(),
                output: self.params.overdrive_output.smoothed.next(),
            };
            let device_controls = [
                DeviceSlotControls {
                    bypassed: !self.params.overdrive.value(),
                    controls: DeviceControls::Minotaur(overdrive_controls),
                },
                DeviceSlotControls {
                    bypassed: !self.params.fuzz.value(),
                    controls: DeviceControls::Muffin(fuzz_controls),
                },
            ];
            let chain_controls = SignalChainControls {
                amp: controls,
                devices: &device_controls,
            };

            for (channel, sample) in channel_samples.iter_mut().enumerate() {
                let amp_output = self.channels[channel].process(*sample, chain_controls);
                *sample =
                    self.speakers[channel].process(amp_output, self.params.speaker_ir.value());
            }
        }

        ProcessStatus::Normal
    }
}

#[cfg(feature = "plugin")]
impl Greybound {
    /// Set a float parameter by id (0.0..=1.0 for floats).
    pub fn set_param_value(&mut self, id: &str, _value: f32) {
        match id {
            "gain" => {
                // Not implemented: direct parameter mutation through nih_plug API.
            }
            "bass" => {}
            "cut" => {}
            "tone" => {}
            "master" => {}
            _ => {}
        }
    }

    /// Set a boolean parameter by id.
    pub fn set_bool_param(&mut self, id: &str, _value: bool) {
        match id {
            "dumbler" => {}
            "speaker_ir" => {}
            _ => {}
        }
    }

    /// Replace the configured chain topology. Existing channel DSP is rebuilt
    /// when the plugin has already been initialized.
    pub fn set_signal_chain_config(&mut self, config: SignalChainConfig) {
        self.chain_config = config;
        if let Some(sample_rate) = self.sample_rate {
            let channel_count = self.channels.len();
            self.channels = build_signal_chains(sample_rate, channel_count, &self.chain_config);
        }
    }

    /// Set UI-driven controls that override param-sourced controls.
    pub fn set_ui_controls(&mut self, controls: AmpControls) {
        self.ui_controls = Some(controls);
    }

    /// Clear UI override so parameter smoothing is used again.
    pub fn clear_ui_controls(&mut self) {
        self.ui_controls = None;
    }
}

#[cfg(feature = "plugin")]
fn build_signal_chains(
    sample_rate: f32,
    channels: usize,
    config: &SignalChainConfig,
) -> Vec<SignalChain> {
    (0..channels)
        .map(|_| SignalChain::new(sample_rate, config.clone()))
        .collect()
}

#[cfg(feature = "plugin")]
impl ClapPlugin for Greybound {
    const CLAP_ID: &'static str = "com.greybound.graybox-amp";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Nox30 circuit-informed guitar amp");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}

#[cfg(feature = "plugin")]
impl Vst3Plugin for Greybound {
    const VST3_CLASS_ID: [u8; 16] = *b"GreyboundGrayAmp";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

#[cfg(feature = "plugin")]
nih_export_clap!(Greybound);
#[cfg(feature = "plugin")]
nih_export_vst3!(Greybound);
