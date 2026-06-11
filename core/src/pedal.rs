use crate::amp::NeuralCellMode;
use crate::ir::SpeakerStage;
use crate::neural_cell::{ExperimentalNeuralCell, NeuralCellRuntime};
use std::env;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const SPRINGFIELD_TANK_IR_BYTES: &[u8] =
    include_bytes!("../../lab/references/spring-irs/smac2023/fig7a-full-modal-model.wav");

pub const GUITAR_SOURCE_IMPEDANCE_OHMS: f32 = 10_000.0;
pub const AMP_INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;

#[derive(Clone)]
struct MinotaurClipNeuralConfig {
    descriptor_path: PathBuf,
    mode: NeuralCellMode,
}

#[derive(Clone)]
enum MinotaurClipNeuralSelection {
    Default,
    Disabled,
    Configured(MinotaurClipNeuralConfig),
}

struct MinotaurClipNeural {
    runtime: NeuralCellRuntime,
    mode: NeuralCellMode,
    buffer_history: [f32; 4],
    last_clip_ac_v: f32,
}

struct MinotaurToneNeural {
    runtime: NeuralCellRuntime,
    mode: NeuralCellMode,
    buffer_history: [f32; 4],
    last_tone_ac_v: f32,
}

static MINOTAUR_CLIP_NEURAL_CONFIG: OnceLock<Mutex<MinotaurClipNeuralSelection>> = OnceLock::new();
static MINOTAUR_TONE_NEURAL_CONFIG: OnceLock<Mutex<MinotaurClipNeuralSelection>> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub struct ElectricalSignal {
    pub voltage: f32,
    pub source_impedance_ohms: f32,
}

impl ElectricalSignal {
    pub fn new(voltage: f32, source_impedance_ohms: f32) -> Self {
        Self {
            voltage,
            source_impedance_ohms: source_impedance_ohms.max(1.0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Load {
    pub impedance_ohms: f32,
}

impl Load {
    pub fn new(impedance_ohms: f32) -> Self {
        Self {
            impedance_ohms: impedance_ohms.max(1.0),
        }
    }
}

/// Stateful electrical connection between two devices.
///
/// This models the part of a rig that belongs to neither endpoint: source/load
/// voltage division plus cable capacitance. Pedals and amps keep their own DSP
/// state, while this object carries the shared physical boundary state.
#[derive(Debug)]
pub struct ConnectionState {
    sample_rate: f32,
    cable_capacitance_farads: f32,
    voltage_state: f32,
}

impl ConnectionState {
    pub fn new(sample_rate: f32, cable_capacitance_farads: f32) -> Self {
        Self {
            sample_rate,
            cable_capacitance_farads: cable_capacitance_farads.max(0.0),
            voltage_state: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.voltage_state = 0.0;
    }

    pub fn drive_load(&mut self, source: ElectricalSignal, load: Load) -> f32 {
        let source_impedance = source.source_impedance_ohms.max(1.0);
        let load_impedance = load.impedance_ohms.max(1.0);
        let divided = source.voltage * load_impedance / (source_impedance + load_impedance);

        if self.cable_capacitance_farads <= 0.0 {
            self.voltage_state = divided;
            return divided;
        }

        let parallel_resistance = 1.0 / (1.0 / source_impedance + 1.0 / load_impedance);
        let time_constant = parallel_resistance * self.cable_capacitance_farads;
        if time_constant <= f32::EPSILON {
            self.voltage_state = divided;
            return divided;
        }

        let coefficient = 1.0 - (-1.0 / (self.sample_rate * time_constant)).exp();
        self.voltage_state += coefficient.clamp(0.0, 1.0) * (divided - self.voltage_state);
        self.voltage_state
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MuffinControls {
    pub sustain: f32,
    pub tone: f32,
    pub level: f32,
}

impl Default for MuffinControls {
    fn default() -> Self {
        Self {
            sustain: 0.55,
            tone: 0.50,
            level: 0.70,
        }
    }
}

pub struct Muffin {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    stage_filters: [OnePoleLowpass; 4],
    tone_lowpass: OnePoleLowpass,
    tone_highpass: OnePoleHighpass,
    output_coupling: OnePoleHighpass,
}

#[derive(Clone, Copy, Debug)]
pub struct MinotaurControls {
    pub gain: f32,
    pub treble: f32,
    pub output: f32,
}

impl Default for MinotaurControls {
    fn default() -> Self {
        Self {
            gain: 0.42,
            treble: 0.70,
            output: 0.42,
        }
    }
}

pub struct Minotaur {
    input_connection: ConnectionState,
    input_highpass: OnePoleHighpass,
    drive_input_highpass: OnePoleHighpass,
    drive_feedback_lowpass: OnePoleLowpass,
    clip_coupling_highpass: OnePoleHighpass,
    clean_feed_highpass: OnePoleHighpass,
    summing_lowpass: OnePoleLowpass,
    treble_lowpass: OnePoleLowpass,
    treble_highpass: OnePoleHighpass,
    treble_mid_highpass: OnePoleHighpass,
    treble_mid_lowpass: OnePoleLowpass,
    transient_fast_envelope: OnePoleLowpass,
    transient_slow_envelope: OnePoleLowpass,
    level_highpass: OnePoleHighpass,
    output_lowpass: OnePoleLowpass,
    clip_neural: Option<MinotaurClipNeural>,
    tone_neural: Option<MinotaurToneNeural>,
}

#[derive(Clone, Copy, Debug)]
pub struct MonarchControls {
    pub gain: f32,
    pub tone: f32,
    pub output: f32,
}

impl Default for MonarchControls {
    fn default() -> Self {
        Self {
            gain: 0.45,
            tone: 0.52,
            output: 0.58,
        }
    }
}

pub struct Monarch {
    input_connection: ConnectionState,
    input_highpass: OnePoleHighpass,
    preclip_lowpass: OnePoleLowpass,
    first_stage_lowpass: OnePoleLowpass,
    second_stage_lowpass: OnePoleLowpass,
    tone_lowpass: OnePoleLowpass,
    tone_highpass: OnePoleHighpass,
    output_lowpass: OnePoleLowpass,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GodessOneMode {
    #[default]
    Standard,
    Custom,
}

#[derive(Clone, Copy, Debug)]
pub struct GodessOneControls {
    pub distortion: f32,
    pub tone: f32,
    pub level: f32,
    pub mode: GodessOneMode,
}

impl Default for GodessOneControls {
    fn default() -> Self {
        Self {
            distortion: 0.55,
            tone: 0.52,
            level: 0.58,
            mode: GodessOneMode::Standard,
        }
    }
}

pub struct GodessOne {
    input_connection: ConnectionState,
    input_highpass: OnePoleHighpass,
    pre_emphasis: OnePoleHighpass,
    preclip_lowpass: OnePoleLowpass,
    postclip_lowpass: OnePoleLowpass,
    body_lowpass: OnePoleLowpass,
    edge_highpass: OnePoleHighpass,
    output_lowpass: OnePoleLowpass,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DartfordWave {
    #[default]
    Sine,
    Triangle,
    Square,
}

#[derive(Clone, Copy, Debug)]
pub struct DartfordControls {
    pub rate_hz: f32,
    pub depth: f32,
    pub level: f32,
    pub wave: DartfordWave,
}

impl Default for DartfordControls {
    fn default() -> Self {
        Self {
            rate_hz: 4.5,
            depth: 0.55,
            level: 1.0,
            wave: DartfordWave::Sine,
        }
    }
}

pub struct Dartford {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    low_band: OnePoleLowpass,
    high_band: OnePoleHighpass,
    lfo_smoother: OnePoleLowpass,
    bias_memory: OnePoleLowpass,
    output_lowpass: OnePoleLowpass,
    sample_rate: f32,
    phase: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct TronControls {
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for TronControls {
    fn default() -> Self {
        Self {
            rate_hz: 0.65,
            depth: 0.68,
            feedback: 0.34,
            mix: 0.50,
        }
    }
}

pub struct Tron {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    lamp_smoother: OnePoleLowpass,
    output_lowpass: OnePoleLowpass,
    stages: [AllPassStage; 6],
    sample_rate: f32,
    phase: f32,
    feedback_state: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct JetstreamControls {
    pub manual: f32,
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for JetstreamControls {
    fn default() -> Self {
        Self {
            manual: 0.42,
            rate_hz: 0.28,
            depth: 0.68,
            feedback: 0.46,
            mix: 0.56,
        }
    }
}

pub struct Jetstream {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    pre_delay_lowpass: OnePoleLowpass,
    post_delay_lowpass: OnePoleLowpass,
    delay: FractionalDelayLine,
    sample_rate: f32,
    phase: f32,
    feedback_state: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct CelesteControls {
    pub rate_hz: f32,
    pub depth: f32,
    pub tone: f32,
    pub mix: f32,
}

impl Default for CelesteControls {
    fn default() -> Self {
        Self {
            rate_hz: 0.62,
            depth: 0.58,
            tone: 0.55,
            mix: 0.42,
        }
    }
}

pub struct Celeste {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    pre_delay_lowpass: OnePoleLowpass,
    wet_lowpass: OnePoleLowpass,
    output_lowpass: OnePoleLowpass,
    delay_a: FractionalDelayLine,
    delay_b: FractionalDelayLine,
    sample_rate: f32,
    phase: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BrigadeControls {
    pub time_ms: f32,
    pub repeats: f32,
    pub tone: f32,
    pub mix: f32,
}

impl Default for BrigadeControls {
    fn default() -> Self {
        Self {
            time_ms: 320.0,
            repeats: 0.38,
            tone: 0.42,
            mix: 0.30,
        }
    }
}

pub struct Brigade {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    pre_delay_lowpass: OnePoleLowpass,
    repeat_lowpass: OnePoleLowpass,
    output_lowpass: OnePoleLowpass,
    delay: FractionalDelayLine,
    sample_rate: f32,
    feedback_state: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LumenControls {
    pub peak_reduction: f32,
    pub gain: f32,
    pub emphasis: f32,
    pub mix: f32,
}

impl Default for LumenControls {
    fn default() -> Self {
        Self {
            peak_reduction: 0.42,
            gain: 0.50,
            emphasis: 0.44,
            mix: 0.82,
        }
    }
}

pub struct Lumen {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    sidechain_highpass: OnePoleHighpass,
    tube_lowpass: OnePoleLowpass,
    output_lowpass: OnePoleLowpass,
    sample_rate: f32,
    gain_reduction_db: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MuonControls {
    pub sensitivity: f32,
    pub range: f32,
    pub resonance: f32,
    pub mix: f32,
}

impl Default for MuonControls {
    fn default() -> Self {
        Self {
            sensitivity: 0.58,
            range: 0.62,
            resonance: 0.46,
            mix: 0.82,
        }
    }
}

pub struct Muon {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    sidechain_lowpass: OnePoleLowpass,
    output_lowpass: OnePoleLowpass,
    filter: StateVariableFilter,
    sample_rate: f32,
    envelope: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SpringfieldControls {
    pub dwell: f32,
    pub tone: f32,
    pub mix: f32,
}

impl Default for SpringfieldControls {
    fn default() -> Self {
        Self {
            dwell: 0.42,
            tone: 0.54,
            mix: 0.24,
        }
    }
}

pub struct Springfield {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    pre_emphasis: OnePoleHighpass,
    tank_lowpass: OnePoleLowpass,
    bright_highpass: OnePoleHighpass,
    output_lowpass: OnePoleLowpass,
    delays: [SpringDelay; 4],
    tank_ir: Option<SpeakerStage>,
    feedback: f32,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StudioVerbAlgorithm {
    #[default]
    Room,
    Plate,
}

#[derive(Clone, Copy, Debug)]
pub struct StudioVerbControls {
    pub algorithm: StudioVerbAlgorithm,
    pub decay: f32,
    pub size: f32,
    pub pre_delay_ms: f32,
    pub diffusion: f32,
    pub tone: f32,
    pub low_cut: f32,
    pub mod_depth: f32,
    pub mix: f32,
}

impl Default for StudioVerbControls {
    fn default() -> Self {
        Self {
            algorithm: StudioVerbAlgorithm::Room,
            decay: 0.42,
            size: 0.46,
            pre_delay_ms: 12.0,
            diffusion: 0.64,
            tone: 0.54,
            low_cut: 0.36,
            mod_depth: 0.18,
            mix: 0.24,
        }
    }
}

pub struct StudioVerb {
    input_connection: ConnectionState,
    input_coupling: OnePoleHighpass,
    low_cut_highpass: OnePoleHighpass,
    pre_delay: FractionalDelayLine,
    early_delays: [SpringDelay; 4],
    fdn_delays: [FractionalDelayLine; 8],
    fdn_damping: [OnePoleLowpass; 8],
    tone_lowpass: OnePoleLowpass,
    tone_highpass: OnePoleHighpass,
    output_lowpass: OnePoleLowpass,
    feedback_inputs: [f32; 8],
    modulation_phases: [f32; 8],
    sample_rate: f32,
}

impl Springfield {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        let tank_ir = SpeakerStage::from_wav_bytes(
            SPRINGFIELD_TANK_IR_BYTES,
            sample_rate.round().max(1.0) as u32,
        )
        .ok();
        Self {
            input_connection: ConnectionState::new(sample_rate, 150e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 55.0),
            pre_emphasis: OnePoleHighpass::new(sample_rate, 1_100.0),
            tank_lowpass: OnePoleLowpass::new(sample_rate, 4_200.0),
            bright_highpass: OnePoleHighpass::new(sample_rate, 1_850.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 9_500.0),
            delays: [
                SpringDelay::new(sample_rate, 0.029),
                SpringDelay::new(sample_rate, 0.037),
                SpringDelay::new(sample_rate, 0.053),
                SpringDelay::new(sample_rate, 0.071),
            ],
            tank_ir,
            feedback: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.pre_emphasis.reset();
        self.tank_lowpass.reset();
        self.bright_highpass.reset();
        self.output_lowpass.reset();
        for delay in &mut self.delays {
            delay.reset();
        }
        if let Some(tank_ir) = &mut self.tank_ir {
            tank_ir.reset();
        }
        self.feedback = 0.0;
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: SpringfieldControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: SpringfieldControls,
    ) -> ElectricalSignal {
        let dwell = controls.dwell.clamp(0.0, 1.0);
        let tone = controls.tone.clamp(0.0, 1.0);
        let mix = controls.mix.clamp(0.0, 1.0);

        let dry = loaded_input;
        let coupled = self.input_coupling.process(loaded_input);
        let excited = coupled + self.pre_emphasis.process(coupled) * (0.12 + tone * 0.34);
        let driver_gain = 0.30 + dwell * 0.95;
        let driver_feedback = self.feedback * (0.04 + dwell * 0.12);
        let tank_drive = (excited * driver_gain + driver_feedback).tanh();

        let a = self.delays[0].process(tank_drive + self.feedback * 0.06);
        let b = self.delays[1].process(-tank_drive * 0.74 + a * 0.38);
        let c = self.delays[2].process(tank_drive * 0.52 - b * 0.31);
        let d = self.delays[3].process(-tank_drive * 0.46 + c * 0.27);
        let splash = a * 0.42 - b * 0.36 + c * 0.32 - d * 0.28;
        self.feedback = (splash * (0.18 + dwell * 0.14)).clamp(-0.65, 0.65);

        let ir_drive = tank_drive * (0.012 + dwell * 0.020);
        let ir_tank = self
            .tank_ir
            .as_mut()
            .map_or(splash, |tank_ir| tank_ir.process(ir_drive, true));
        let tank = ir_tank + splash * (0.18 + tone * 0.08);

        let dark = self.tank_lowpass.process(tank);
        let bright = self.bright_highpass.process(tank);
        let voiced = dark * (1.18 - tone * 0.42) + bright * (0.10 + tone * 0.74);
        let wet = self
            .output_lowpass
            .process(voiced * (0.28 + dwell * 0.18))
            .clamp(-1.0, 1.0);
        let output = dry + wet * mix * 1.8;

        ElectricalSignal::new(output.clamp(-32.0, 32.0), Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl StudioVerb {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 120e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 22.0),
            low_cut_highpass: OnePoleHighpass::new(sample_rate, 135.0),
            pre_delay: FractionalDelayLine::new(sample_rate, 0.14),
            early_delays: [
                SpringDelay::new(sample_rate, 0.006),
                SpringDelay::new(sample_rate, 0.011),
                SpringDelay::new(sample_rate, 0.017),
                SpringDelay::new(sample_rate, 0.023),
            ],
            fdn_delays: std::array::from_fn(|_| FractionalDelayLine::new(sample_rate, 0.25)),
            fdn_damping: std::array::from_fn(|_| OnePoleLowpass::new(sample_rate, 6_200.0)),
            tone_lowpass: OnePoleLowpass::new(sample_rate, 5_800.0),
            tone_highpass: OnePoleHighpass::new(sample_rate, 1_900.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 14_000.0),
            feedback_inputs: [0.0; 8],
            modulation_phases: [0.0, 0.19, 0.37, 0.53, 0.71, 0.83, 0.91, 0.97],
            sample_rate,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.low_cut_highpass.reset();
        self.pre_delay.reset();
        for delay in &mut self.early_delays {
            delay.reset();
        }
        for delay in &mut self.fdn_delays {
            delay.reset();
        }
        for damping in &mut self.fdn_damping {
            damping.reset();
        }
        self.tone_lowpass.reset();
        self.tone_highpass.reset();
        self.output_lowpass.reset();
        self.feedback_inputs = [0.0; 8];
        self.modulation_phases = [0.0, 0.19, 0.37, 0.53, 0.71, 0.83, 0.91, 0.97];
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: StudioVerbControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: StudioVerbControls,
    ) -> ElectricalSignal {
        let decay = controls.decay.clamp(0.0, 1.0);
        let size = controls.size.clamp(0.0, 1.0);
        let diffusion = controls.diffusion.clamp(0.0, 1.0);
        let tone = controls.tone.clamp(0.0, 1.0);
        let low_cut = controls.low_cut.clamp(0.0, 1.0);
        let mod_depth = controls.mod_depth.clamp(0.0, 1.0);
        let mix = controls.mix.clamp(0.0, 1.0);
        let pre_delay_ms = controls.pre_delay_ms.clamp(0.0, 120.0);

        let dry = loaded_input;
        let coupled = self.input_coupling.process(loaded_input);
        let low_cut_input =
            coupled * (1.0 - low_cut * 0.55) + self.low_cut_highpass.process(coupled) * low_cut;
        let predelayed = self
            .pre_delay
            .process(low_cut_input, pre_delay_ms * 0.001 * self.sample_rate);

        let algo = match controls.algorithm {
            StudioVerbAlgorithm::Room => StudioVerbTuning {
                delay_scale: 0.64 + size * 0.55,
                feedback_gain: 0.42 + decay * 0.32,
                input_gain: 0.20 + diffusion * 0.14,
                early_gain: 0.24 + diffusion * 0.24,
                output_gain: 0.74,
                modulation_hz: 0.19,
            },
            StudioVerbAlgorithm::Plate => StudioVerbTuning {
                delay_scale: 0.88 + size * 0.82,
                feedback_gain: 0.56 + decay * 0.35,
                input_gain: 0.16 + diffusion * 0.12,
                early_gain: 0.10 + diffusion * 0.14,
                output_gain: 0.82,
                modulation_hz: 0.12,
            },
        };

        let e0 = self.early_delays[0].process(predelayed * 0.58);
        let e1 = self.early_delays[1].process(-predelayed * 0.44 + e0 * 0.18);
        let e2 = self.early_delays[2].process(predelayed * 0.36 - e1 * 0.14);
        let e3 = self.early_delays[3].process(-predelayed * 0.30 + e2 * 0.12);
        let early = e0 * 0.36 - e1 * 0.28 + e2 * 0.22 - e3 * 0.18;

        let base_delays = [
            0.0297, 0.0371, 0.0419, 0.0533, 0.0617, 0.0719, 0.0839, 0.0973,
        ];
        let mut outs = [0.0; 8];
        for idx in 0..8 {
            self.modulation_phases[idx] = (self.modulation_phases[idx]
                + algo.modulation_hz * (0.71 + idx as f32 * 0.07) / self.sample_rate)
                .fract();
            let lfo = (std::f32::consts::TAU * self.modulation_phases[idx]).sin();
            let mod_samples = lfo * mod_depth * (1.0 + size * 3.5);
            let delay_samples =
                base_delays[idx] * algo.delay_scale * self.sample_rate + mod_samples;
            outs[idx] = self.fdn_delays[idx].process(
                predelayed * algo.input_gain + self.feedback_inputs[idx],
                delay_samples,
            );
        }

        let damped: [f32; 8] = std::array::from_fn(|idx| {
            let damp = self.fdn_damping[idx].process(outs[idx]);
            damp * (0.62 + tone * 0.34) + outs[idx] * (0.14 + tone * 0.24)
        });
        let mixed = hadamard8(damped);
        let feedback_gain = algo.feedback_gain.clamp(0.0, 0.93);
        for idx in 0..8 {
            self.feedback_inputs[idx] = (mixed[idx] * feedback_gain).clamp(-4.0, 4.0);
        }

        let tank_sum = outs[0] * 0.18 - outs[1] * 0.16 + outs[2] * 0.15 - outs[3] * 0.13
            + outs[4] * 0.12
            - outs[5] * 0.11
            + outs[6] * 0.10
            - outs[7] * 0.09;
        let raw_wet = early * algo.early_gain + tank_sum * algo.output_gain;
        let dark = self.tone_lowpass.process(raw_wet);
        let bright = self.tone_highpass.process(raw_wet);
        let voiced = dark * (1.12 - tone * 0.54) + bright * (0.12 + tone * 0.58);
        let wet = self.output_lowpass.process(voiced).clamp(-4.0, 4.0);
        let wet_gain = match controls.algorithm {
            StudioVerbAlgorithm::Room => 1.65,
            StudioVerbAlgorithm::Plate => 1.45,
        };
        let output = dry * (1.0 - mix * 0.12) + wet * mix * wet_gain;

        ElectricalSignal::new(output.clamp(-32.0, 32.0), Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

#[derive(Clone, Copy)]
struct StudioVerbTuning {
    delay_scale: f32,
    feedback_gain: f32,
    input_gain: f32,
    early_gain: f32,
    output_gain: f32,
    modulation_hz: f32,
}

fn hadamard8(input: [f32; 8]) -> [f32; 8] {
    const SCALE: f32 = 0.353_553_38;
    [
        (input[0] + input[1] + input[2] + input[3] + input[4] + input[5] + input[6] + input[7])
            * SCALE,
        (input[0] - input[1] + input[2] - input[3] + input[4] - input[5] + input[6] - input[7])
            * SCALE,
        (input[0] + input[1] - input[2] - input[3] + input[4] + input[5] - input[6] - input[7])
            * SCALE,
        (input[0] - input[1] - input[2] + input[3] + input[4] - input[5] - input[6] + input[7])
            * SCALE,
        (input[0] + input[1] + input[2] + input[3] - input[4] - input[5] - input[6] - input[7])
            * SCALE,
        (input[0] - input[1] + input[2] - input[3] - input[4] + input[5] - input[6] + input[7])
            * SCALE,
        (input[0] + input[1] - input[2] - input[3] - input[4] - input[5] + input[6] + input[7])
            * SCALE,
        (input[0] - input[1] - input[2] + input[3] - input[4] + input[5] + input[6] - input[7])
            * SCALE,
    ]
}

impl Minotaur {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 560.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 220e-12),
            input_highpass: OnePoleHighpass::new(sample_rate, 1.6),
            drive_input_highpass: OnePoleHighpass::new(sample_rate, 26.0),
            drive_feedback_lowpass: OnePoleLowpass::new(sample_rate, 900.0),
            clip_coupling_highpass: OnePoleHighpass::new(sample_rate, 3.4),
            clean_feed_highpass: OnePoleHighpass::new(sample_rate, 106.0),
            summing_lowpass: OnePoleLowpass::new(sample_rate, 4_900.0),
            treble_lowpass: OnePoleLowpass::new(sample_rate, 2_100.0),
            treble_highpass: OnePoleHighpass::new(sample_rate, 1_650.0),
            treble_mid_highpass: OnePoleHighpass::new(sample_rate, 850.0),
            treble_mid_lowpass: OnePoleLowpass::new(sample_rate, 3_600.0),
            transient_fast_envelope: OnePoleLowpass::new(sample_rate, 120.0),
            transient_slow_envelope: OnePoleLowpass::new(sample_rate, 12.0),
            level_highpass: OnePoleHighpass::new(sample_rate, 0.34),
            output_lowpass: OnePoleLowpass::new(sample_rate, 18_000.0),
            clip_neural: minotaur_clip_neural(),
            tone_neural: minotaur_tone_neural(),
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_highpass.reset();
        self.drive_input_highpass.reset();
        self.drive_feedback_lowpass.reset();
        self.clip_coupling_highpass.reset();
        self.clean_feed_highpass.reset();
        self.summing_lowpass.reset();
        self.treble_lowpass.reset();
        self.treble_highpass.reset();
        self.treble_mid_highpass.reset();
        self.treble_mid_lowpass.reset();
        self.transient_fast_envelope.reset();
        self.transient_slow_envelope.reset();
        self.level_highpass.reset();
        self.output_lowpass.reset();
        if let Some(neural) = &mut self.clip_neural {
            neural.reset();
        }
        if let Some(neural) = &mut self.tone_neural {
            neural.reset();
        }
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: MinotaurControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: MinotaurControls,
    ) -> ElectricalSignal {
        let gain = controls.gain.clamp(0.0, 1.0);
        let treble = controls.treble.clamp(0.0, 1.0);
        let output = controls.output.clamp(0.0, 1.0);

        let buffered = self.input_highpass.process(loaded_input);

        // Klon-style dual-gang gain: the clean feed stays present while the
        // non-inverting gain stage and summing contribution rise together.
        let gain_pot = gain.powf(1.15);
        let drive_input = self.drive_input_highpass.process(buffered);
        let feedback_gain = 1.45 + gain_pot * 13.5;
        let drive_stage = self
            .drive_feedback_lowpass
            .process(drive_input * feedback_gain);
        let clip_input = self.clip_coupling_highpass.process(drive_stage);
        let analytic_clipped = diode_pair_clip(clip_input, 0.42);
        let clipped = if let Some(neural) = &mut self.clip_neural {
            neural.process(buffered, gain, treble, output, analytic_clipped)
        } else {
            analytic_clipped
        };

        let clean_feed =
            self.clean_feed_highpass.process(buffered) * (0.13 - gain * 0.06).max(0.035);
        let drive_feed = clipped * (2.35 + gain_pot * 2.2);
        let sum_node = self
            .summing_lowpass
            .process(drive_feed + clean_feed - drive_input * 0.08);

        let low = self.treble_lowpass.process(sum_node);
        let high = self.treble_highpass.process(sum_node);
        let mid = self
            .treble_mid_lowpass
            .process(self.treble_mid_highpass.process(sum_node));
        let tone_gain = 0.78 + treble * 0.44;
        let analytic_voiced =
            low * (0.88 - treble * 0.36) + high * (0.17 + treble * 1.00) + mid * 0.28;
        let voiced = if let Some(neural) = &mut self.tone_neural {
            neural.process(buffered, gain, treble, output, analytic_voiced)
        } else {
            analytic_voiced
        };
        let envelope_input = voiced.abs();
        let fast_envelope = self.transient_fast_envelope.process(envelope_input);
        let slow_envelope = self.transient_slow_envelope.process(envelope_input);
        let transient_lift =
            ((fast_envelope - slow_envelope).max(0.0) / (slow_envelope + 1.0e-5)).clamp(0.0, 1.0);
        let dynamic_gain = 1.0 + transient_lift * 0.055;
        let level = (0.22 + output * 1.95) * 0.04;
        let final_output = self
            .output_lowpass
            .process(
                self.level_highpass
                    .process(voiced * tone_gain * dynamic_gain * level),
            )
            .clamp(-4.5, 4.5);

        ElectricalSignal::new(final_output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

pub fn configure_minotaur_clip_neural(descriptor_path: Option<PathBuf>, mode: NeuralCellMode) {
    let slot = MINOTAUR_CLIP_NEURAL_CONFIG
        .get_or_init(|| Mutex::new(MinotaurClipNeuralSelection::Default));
    *slot.lock().expect("minotaur neural config mutex poisoned") =
        if let Some(descriptor_path) = descriptor_path {
            MinotaurClipNeuralSelection::Configured(MinotaurClipNeuralConfig {
                descriptor_path,
                mode,
            })
        } else {
            MinotaurClipNeuralSelection::Disabled
        };
}

pub fn configure_minotaur_tone_neural(descriptor_path: Option<PathBuf>, mode: NeuralCellMode) {
    let slot = MINOTAUR_TONE_NEURAL_CONFIG
        .get_or_init(|| Mutex::new(MinotaurClipNeuralSelection::Default));
    *slot.lock().expect("minotaur neural config mutex poisoned") =
        if let Some(descriptor_path) = descriptor_path {
            MinotaurClipNeuralSelection::Configured(MinotaurClipNeuralConfig {
                descriptor_path,
                mode,
            })
        } else {
            MinotaurClipNeuralSelection::Disabled
        };
}

fn minotaur_clip_neural() -> Option<MinotaurClipNeural> {
    let config = match minotaur_clip_neural_selection() {
        MinotaurClipNeuralSelection::Disabled => None,
        MinotaurClipNeuralSelection::Configured(config) => Some(config),
        MinotaurClipNeuralSelection::Default => env_minotaur_clip_neural(
            "GREYBOUND_MINOTAUR_CLIP_REPLACE_DESCRIPTOR",
            NeuralCellMode::Replace,
        )
        .or_else(|| {
            env_minotaur_clip_neural(
                "GREYBOUND_MINOTAUR_CLIP_SHADOW_DESCRIPTOR",
                NeuralCellMode::Shadow,
            )
        }),
    }?;
    let cell = ExperimentalNeuralCell::from_descriptor_path(&config.descriptor_path).ok()?;
    Some(MinotaurClipNeural {
        runtime: cell.into_runtime(),
        mode: config.mode,
        buffer_history: [0.0; 4],
        last_clip_ac_v: 0.0,
    })
}

fn minotaur_tone_neural() -> Option<MinotaurToneNeural> {
    let config = match minotaur_tone_neural_selection() {
        MinotaurClipNeuralSelection::Disabled => None,
        MinotaurClipNeuralSelection::Configured(config) => Some(config),
        MinotaurClipNeuralSelection::Default => env_minotaur_clip_neural(
            "GREYBOUND_MINOTAUR_TONE_REPLACE_DESCRIPTOR",
            NeuralCellMode::Replace,
        )
        .or_else(|| {
            env_minotaur_clip_neural(
                "GREYBOUND_MINOTAUR_TONE_SHADOW_DESCRIPTOR",
                NeuralCellMode::Shadow,
            )
        }),
    }?;
    let cell = ExperimentalNeuralCell::from_descriptor_path(&config.descriptor_path).ok()?;
    Some(MinotaurToneNeural {
        runtime: cell.into_runtime(),
        mode: config.mode,
        buffer_history: [0.0; 4],
        last_tone_ac_v: 0.0,
    })
}

fn minotaur_clip_neural_selection() -> MinotaurClipNeuralSelection {
    MINOTAUR_CLIP_NEURAL_CONFIG
        .get_or_init(|| Mutex::new(MinotaurClipNeuralSelection::Default))
        .lock()
        .expect("minotaur neural config mutex poisoned")
        .clone()
}

fn minotaur_tone_neural_selection() -> MinotaurClipNeuralSelection {
    MINOTAUR_TONE_NEURAL_CONFIG
        .get_or_init(|| Mutex::new(MinotaurClipNeuralSelection::Default))
        .lock()
        .expect("minotaur neural config mutex poisoned")
        .clone()
}

fn env_minotaur_clip_neural(name: &str, mode: NeuralCellMode) -> Option<MinotaurClipNeuralConfig> {
    env::var_os(name).map(|descriptor_path| MinotaurClipNeuralConfig {
        descriptor_path: PathBuf::from(descriptor_path),
        mode,
    })
}

impl MinotaurClipNeural {
    fn reset(&mut self) {
        self.buffer_history = [0.0; 4];
        self.last_clip_ac_v = 0.0;
    }

    fn process(
        &mut self,
        buffered: f32,
        gain: f32,
        treble: f32,
        _output: f32,
        analytic_clipped: f32,
    ) -> f32 {
        self.buffer_history.copy_within(0..3, 1);
        self.buffer_history[0] = buffered;
        let mut features = vec![0.0; self.runtime.input_features()];
        let history_len = features.len().min(4);
        features[..history_len].copy_from_slice(&self.buffer_history[..history_len]);
        if features.len() > 4 {
            features[4] = gain;
        }
        if features.len() > 5 {
            features[5] = treble;
        }
        if features.len() > 6 {
            features[6] = 0.70;
        }
        let neural = self
            .runtime
            .process_features(&features)
            .unwrap_or(analytic_clipped)
            .clamp(-1.5, 1.5);
        self.last_clip_ac_v = neural;
        if self.mode == NeuralCellMode::Replace {
            neural
        } else {
            analytic_clipped
        }
    }
}

impl MinotaurToneNeural {
    fn reset(&mut self) {
        self.buffer_history = [0.0; 4];
        self.last_tone_ac_v = 0.0;
    }

    fn process(
        &mut self,
        buffered: f32,
        gain: f32,
        treble: f32,
        _output: f32,
        analytic_tone: f32,
    ) -> f32 {
        self.buffer_history.copy_within(0..3, 1);
        self.buffer_history[0] = buffered;
        let mut features = vec![0.0; self.runtime.input_features()];
        let history_len = features.len().min(4);
        features[..history_len].copy_from_slice(&self.buffer_history[..history_len]);
        if features.len() > 4 {
            features[4] = gain;
        }
        if features.len() > 5 {
            features[5] = treble;
        }
        let neural = self
            .runtime
            .process_features(&features)
            .unwrap_or(analytic_tone)
            .clamp(-4.5, 4.5);
        self.last_tone_ac_v = neural;
        if self.mode == NeuralCellMode::Replace {
            neural
        } else {
            analytic_tone
        }
    }
}

impl Monarch {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 4_700.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 330e-12),
            input_highpass: OnePoleHighpass::new(sample_rate, 28.0),
            preclip_lowpass: OnePoleLowpass::new(sample_rate, 8_200.0),
            first_stage_lowpass: OnePoleLowpass::new(sample_rate, 4_500.0),
            second_stage_lowpass: OnePoleLowpass::new(sample_rate, 5_800.0),
            tone_lowpass: OnePoleLowpass::new(sample_rate, 680.0),
            tone_highpass: OnePoleHighpass::new(sample_rate, 1_450.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 13_500.0),
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_highpass.reset();
        self.preclip_lowpass.reset();
        self.first_stage_lowpass.reset();
        self.second_stage_lowpass.reset();
        self.tone_lowpass.reset();
        self.tone_highpass.reset();
        self.output_lowpass.reset();
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: MonarchControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: MonarchControls,
    ) -> ElectricalSignal {
        let gain = controls.gain.clamp(0.0, 1.0);
        let tone = controls.tone.clamp(0.0, 1.0);
        let output = controls.output.clamp(0.0, 1.0);

        let input = self.input_highpass.process(loaded_input);
        let filtered = self.preclip_lowpass.process(input);
        let drive = 1.4 + gain * 22.0;
        let first_clip = asymmetric_diode_clip(filtered * drive, 0.50, 0.74);
        let first_stage = self.first_stage_lowpass.process(first_clip) * (0.32 + gain * 0.82);
        let second_drive = first_stage * (1.15 + gain * 7.5);
        let second_clip = asymmetric_diode_clip(second_drive, 0.56, 0.86);
        let second_stage = self.second_stage_lowpass.process(second_clip);

        let clean_blend = input * (0.38 - gain * 0.22).max(0.06);
        let mixed = clean_blend + second_stage * (0.58 + gain * 0.72);
        let low = self.tone_lowpass.process(mixed);
        let high = self.tone_highpass.process(mixed);
        let voiced = low * (1.15 - tone * 0.55) + high * (0.18 + tone * 1.05);
        let level = 0.20 + output * 2.25;
        let final_output = self.output_lowpass.process(voiced * level).clamp(-3.5, 3.5);

        ElectricalSignal::new(final_output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl Dartford {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 120e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 16.0),
            low_band: OnePoleLowpass::new(sample_rate, 690.0),
            high_band: OnePoleHighpass::new(sample_rate, 690.0),
            lfo_smoother: OnePoleLowpass::new(sample_rate, 18.0),
            bias_memory: OnePoleLowpass::new(sample_rate, 8.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 16_500.0),
            sample_rate,
            phase: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.low_band.reset();
        self.high_band.reset();
        self.lfo_smoother.reset();
        self.bias_memory.reset();
        self.output_lowpass.reset();
        self.phase = 0.0;
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: DartfordControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: DartfordControls,
    ) -> ElectricalSignal {
        let rate_hz = controls.rate_hz.clamp(0.05, 20.0);
        let depth = controls.depth.clamp(0.0, 1.0);
        let intensity = depth.powf(0.75);
        let level = controls.level.clamp(0.0, 2.0);

        let phase_radians = self.phase * std::f32::consts::TAU;
        let raw_lfo = match controls.wave {
            DartfordWave::Sine => phase_radians.sin(),
            DartfordWave::Triangle => {
                if self.phase < 0.5 {
                    self.phase * 4.0 - 1.0
                } else {
                    3.0 - self.phase * 4.0
                }
            }
            DartfordWave::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        };
        let quadrature_lfo = (phase_radians + std::f32::consts::FRAC_PI_2).sin();
        self.phase = (self.phase + rate_hz / self.sample_rate).fract();

        let asymmetric_lfo = (raw_lfo + 0.20 * raw_lfo * raw_lfo - 0.10).clamp(-1.0, 1.0);
        let smoothed_lfo = self.lfo_smoother.process(asymmetric_lfo);
        let pulse = ((smoothed_lfo + 1.0) * 0.5).clamp(0.0, 1.0);
        let tremolo_gain = 1.0 - intensity * 0.72 * (1.0 - pulse).powf(1.25);
        let makeup_gain = 1.0 + intensity * 0.03;

        let coupled_input = self.input_coupling.process(loaded_input);
        let low = self.low_band.process(coupled_input);
        let high = self.high_band.process(coupled_input);
        let low_motion = 1.0 - intensity * 0.035 * quadrature_lfo;
        let high_motion = 1.0 + intensity * 0.05 * quadrature_lfo;
        let voiced = low * low_motion + high * high_motion;

        let bias_memory = self
            .bias_memory
            .process((1.0 - tremolo_gain) * voiced.abs());
        let bias_compression = 1.0 / (1.0 + bias_memory * 0.35);
        let output = self
            .output_lowpass
            .process(voiced * tremolo_gain * makeup_gain * bias_compression * level)
            .clamp(-4.0, 4.0);
        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl Tron {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 470_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 180e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 18.0),
            lamp_smoother: OnePoleLowpass::new(sample_rate, 5.5),
            output_lowpass: OnePoleLowpass::new(sample_rate, 15_000.0),
            stages: [AllPassStage::default(); 6],
            sample_rate,
            phase: 0.0,
            feedback_state: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.lamp_smoother.reset();
        self.output_lowpass.reset();
        for stage in &mut self.stages {
            stage.reset();
        }
        self.phase = 0.0;
        self.feedback_state = 0.0;
    }

    pub fn process(&mut self, input: ElectricalSignal, controls: TronControls) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: TronControls,
    ) -> ElectricalSignal {
        let rate_hz = controls.rate_hz.clamp(0.03, 12.0);
        let depth = controls.depth.clamp(0.0, 1.0);
        let feedback = controls.feedback.clamp(0.0, 0.92);
        let mix = controls.mix.clamp(0.0, 1.0);

        let phase_radians = self.phase * std::f32::consts::TAU;
        let lamp_drive = (phase_radians.sin() * 0.5 + 0.5).powf(1.35);
        self.phase = (self.phase + rate_hz / self.sample_rate).fract();

        let lamp = self.lamp_smoother.process(lamp_drive);
        let sweep = (1.0 - depth) * 0.38 + depth * lamp;
        let center_hz = 70.0 * (6_200.0_f32 / 70.0).powf(sweep.clamp(0.0, 1.0));
        let stage_spreads = [0.56, 0.74, 0.98, 1.28, 1.70, 2.25];

        let input = self.input_coupling.process(loaded_input);
        let mut shifted = input + self.feedback_state * feedback * 0.42;
        for (stage, spread) in self.stages.iter_mut().zip(stage_spreads) {
            shifted = stage.process(
                shifted,
                allpass_coefficient(center_hz * spread, self.sample_rate),
            );
        }
        self.feedback_state = shifted.clamp(-8.0, 8.0);

        let phase_mix = mix * (0.72 + depth * 0.28);
        let notched =
            input * (1.0 - phase_mix * 0.48) + shifted * phase_mix * (0.72 + feedback * 0.18);
        let level = 1.14 + feedback * 0.08;
        let output = self
            .output_lowpass
            .process(notched * level)
            .clamp(-32.0, 32.0);

        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl Jetstream {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 160e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 24.0),
            pre_delay_lowpass: OnePoleLowpass::new(sample_rate, 7_200.0),
            post_delay_lowpass: OnePoleLowpass::new(sample_rate, 6_600.0),
            delay: FractionalDelayLine::new(sample_rate, 0.018),
            sample_rate,
            phase: 0.0,
            feedback_state: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.pre_delay_lowpass.reset();
        self.post_delay_lowpass.reset();
        self.delay.reset();
        self.phase = 0.0;
        self.feedback_state = 0.0;
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: JetstreamControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: JetstreamControls,
    ) -> ElectricalSignal {
        let manual = controls.manual.clamp(0.0, 1.0);
        let rate_hz = controls.rate_hz.clamp(0.02, 8.0);
        let depth = controls.depth.clamp(0.0, 1.0);
        let feedback = controls.feedback.clamp(0.0, 0.94);
        let mix = controls.mix.clamp(0.0, 1.0);

        let phase_radians = self.phase * std::f32::consts::TAU;
        let lfo = phase_radians.sin() * 0.5 + 0.5;
        self.phase = (self.phase + rate_hz / self.sample_rate).fract();

        let base_ms = 0.55 + manual * 5.2;
        let sweep_ms = depth * (0.35 + manual * 3.8);
        let delay_ms = (base_ms + (lfo - 0.5) * sweep_ms * 2.0).clamp(0.25, 9.5);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;

        let input = self.input_coupling.process(loaded_input);
        let bbd_input = self
            .pre_delay_lowpass
            .process((input + self.feedback_state * feedback * 0.72).clamp(-18.0, 18.0));
        let delayed = self.delay.process(bbd_input, delay_samples);
        let wet = self.post_delay_lowpass.process(delayed);
        self.feedback_state = wet.clamp(-12.0, 12.0);

        let comb = input * (1.0 - mix * 0.20) + wet * mix * (0.82 + feedback * 0.18);
        let output = comb.clamp(-32.0, 32.0);

        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl Celeste {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 150e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 28.0),
            pre_delay_lowpass: OnePoleLowpass::new(sample_rate, 7_800.0),
            wet_lowpass: OnePoleLowpass::new(sample_rate, 3_600.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 12_000.0),
            delay_a: FractionalDelayLine::new(sample_rate, 0.035),
            delay_b: FractionalDelayLine::new(sample_rate, 0.035),
            sample_rate,
            phase: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.pre_delay_lowpass.reset();
        self.wet_lowpass.reset();
        self.output_lowpass.reset();
        self.delay_a.reset();
        self.delay_b.reset();
        self.phase = 0.0;
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: CelesteControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: CelesteControls,
    ) -> ElectricalSignal {
        let rate_hz = controls.rate_hz.clamp(0.05, 6.0);
        let depth = controls.depth.clamp(0.0, 1.0);
        let tone = controls.tone.clamp(0.0, 1.0);
        let mix = controls.mix.clamp(0.0, 1.0);

        let phase_radians = self.phase * std::f32::consts::TAU;
        let lfo_a = phase_radians.sin();
        let lfo_b = (phase_radians + std::f32::consts::TAU * 0.37).sin();
        self.phase = (self.phase + rate_hz / self.sample_rate).fract();

        let base_ms = 14.0;
        let sweep_ms = 1.4 + depth * 7.8;
        let delay_a_ms = (base_ms + lfo_a * sweep_ms).clamp(5.0, 27.0);
        let delay_b_ms = (base_ms * 1.17 + lfo_b * sweep_ms * 0.82).clamp(6.0, 31.0);

        let input = self.input_coupling.process(loaded_input);
        let bbd_input = self.pre_delay_lowpass.process(input).clamp(-18.0, 18.0);
        let voice_a = self
            .delay_a
            .process(bbd_input, delay_a_ms * 0.001 * self.sample_rate);
        let voice_b = self
            .delay_b
            .process(bbd_input, delay_b_ms * 0.001 * self.sample_rate);
        let wet_raw = (voice_a + voice_b) * 0.5;
        let wet_dark = self.wet_lowpass.process(wet_raw);
        let wet = wet_dark * (1.0 - tone * 0.55) + wet_raw * tone * 0.55;

        let dry_gain = 1.0 - mix * 0.28;
        let wet_gain = mix * (0.78 + depth * 0.18);
        let output = self
            .output_lowpass
            .process(input * dry_gain + wet * wet_gain)
            .clamp(-32.0, 32.0);

        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl Brigade {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 170e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 35.0),
            pre_delay_lowpass: OnePoleLowpass::new(sample_rate, 5_800.0),
            repeat_lowpass: OnePoleLowpass::new(sample_rate, 2_900.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 10_500.0),
            delay: FractionalDelayLine::new(sample_rate, 0.85),
            sample_rate,
            feedback_state: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.pre_delay_lowpass.reset();
        self.repeat_lowpass.reset();
        self.output_lowpass.reset();
        self.delay.reset();
        self.feedback_state = 0.0;
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: BrigadeControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: BrigadeControls,
    ) -> ElectricalSignal {
        let time_ms = controls.time_ms.clamp(60.0, 700.0);
        let repeats = controls.repeats.clamp(0.0, 0.92);
        let tone = controls.tone.clamp(0.0, 1.0);
        let mix = controls.mix.clamp(0.0, 1.0);

        let input = self.input_coupling.process(loaded_input);
        let delay_samples = time_ms * 0.001 * self.sample_rate;
        let feedback_gain = repeats * 0.86;
        let drive = (input + self.feedback_state * feedback_gain).clamp(-18.0, 18.0);
        let bbd_input = self.pre_delay_lowpass.process(drive);
        let delayed = self.delay.process(bbd_input, delay_samples);

        let compressed = (delayed * (1.0 + repeats * 0.45)).tanh();
        let dark_repeat = self.repeat_lowpass.process(compressed);
        let wet = dark_repeat * (1.0 - tone * 0.50) + compressed * tone * 0.50;
        self.feedback_state = wet.clamp(-10.0, 10.0);

        let dry_gain = 1.0 - mix * 0.18;
        let wet_gain = mix * (0.82 + repeats * 0.16);
        let output = self
            .output_lowpass
            .process(input * dry_gain + wet * wet_gain)
            .clamp(-32.0, 32.0);

        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl Lumen {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_200.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 140e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 18.0),
            sidechain_highpass: OnePoleHighpass::new(sample_rate, 115.0),
            tube_lowpass: OnePoleLowpass::new(sample_rate, 18_000.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 16_000.0),
            sample_rate,
            gain_reduction_db: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.sidechain_highpass.reset();
        self.tube_lowpass.reset();
        self.output_lowpass.reset();
        self.gain_reduction_db = 0.0;
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: LumenControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: LumenControls,
    ) -> ElectricalSignal {
        let peak_reduction = controls.peak_reduction.clamp(0.0, 1.0);
        let makeup = controls.gain.clamp(0.0, 1.0);
        let emphasis = controls.emphasis.clamp(0.0, 1.0);
        let mix = controls.mix.clamp(0.0, 1.0);

        let input = self.input_coupling.process(loaded_input);
        let sidechain_hp = self.sidechain_highpass.process(input);
        let detector = (input.abs() * (1.0 - emphasis * 0.36)
            + sidechain_hp.abs() * emphasis * 0.92)
            .max(1e-6);
        let threshold = 0.065 * 10.0_f32.powf(-peak_reduction * 0.95);
        let over_db = (20.0 * (detector / threshold).log10()).max(0.0);
        let knee = (over_db * over_db) / (over_db + 7.0);
        let target_reduction_db = (knee * (0.42 + peak_reduction * 0.44)).min(24.0);

        let attack_ms = 3.5 + (1.0 - peak_reduction) * 14.0;
        let release_ms = if target_reduction_db > self.gain_reduction_db {
            attack_ms
        } else {
            95.0 + self.gain_reduction_db * 34.0
        };
        let coeff = time_coefficient(self.sample_rate, release_ms);
        self.gain_reduction_db += coeff * (target_reduction_db - self.gain_reduction_db);

        let gain_reduction = 10.0_f32.powf(-self.gain_reduction_db / 20.0);
        let makeup_gain = 10.0_f32.powf((-1.0 + makeup * 17.0) / 20.0);
        let compressed = input * gain_reduction * makeup_gain;
        let tube_drive = 1.04 + peak_reduction * 0.22;
        let tube = (compressed * tube_drive).tanh() / tube_drive;
        let warm = self.tube_lowpass.process(tube);
        let blended = input * (1.0 - mix) + warm * mix;
        let output = self.output_lowpass.process(blended).clamp(-32.0, 32.0);

        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl Muon {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 180e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 24.0),
            sidechain_lowpass: OnePoleLowpass::new(sample_rate, 18.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 14_500.0),
            filter: StateVariableFilter::default(),
            sample_rate,
            envelope: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        self.sidechain_lowpass.reset();
        self.output_lowpass.reset();
        self.filter.reset();
        self.envelope = 0.0;
    }

    pub fn process(&mut self, input: ElectricalSignal, controls: MuonControls) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: MuonControls,
    ) -> ElectricalSignal {
        let sensitivity = controls.sensitivity.clamp(0.0, 1.0);
        let range = controls.range.clamp(0.0, 1.0);
        let resonance = controls.resonance.clamp(0.0, 1.0);
        let mix = controls.mix.clamp(0.0, 1.0);

        let input = self.input_coupling.process(loaded_input);
        let rectified = self.sidechain_lowpass.process(input.abs());
        let target = (rectified * (5.5 + sensitivity * 48.0)).clamp(0.0, 1.0);
        let coefficient = if target > self.envelope {
            time_coefficient(self.sample_rate, 4.5)
        } else {
            time_coefficient(self.sample_rate, 135.0 + range * 120.0)
        };
        self.envelope += (target - self.envelope) * coefficient;

        let sweep = self.envelope.powf(0.56 + (1.0 - range) * 0.42);
        let base_hz = 180.0 + range * 240.0;
        let top_hz = 1_450.0 + range * 2_850.0;
        let center_hz = (base_hz * (top_hz / base_hz).powf(sweep)).clamp(80.0, 6_000.0);
        let q = 0.58 + resonance * 7.2;
        let band = self
            .filter
            .process_bandpass(input, center_hz, q, self.sample_rate);

        let body = input * (1.0 - mix * 0.42);
        let quack = band * mix * (1.85 + resonance * 1.20);
        let output = self
            .output_lowpass
            .process((body + quack).tanh() * (1.02 + sensitivity * 0.08))
            .clamp(-32.0, 32.0);

        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl GodessOne {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 1_000_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 1_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 220e-12),
            input_highpass: OnePoleHighpass::new(sample_rate, 35.0),
            pre_emphasis: OnePoleHighpass::new(sample_rate, 720.0),
            preclip_lowpass: OnePoleLowpass::new(sample_rate, 6_800.0),
            postclip_lowpass: OnePoleLowpass::new(sample_rate, 4_800.0),
            body_lowpass: OnePoleLowpass::new(sample_rate, 620.0),
            edge_highpass: OnePoleHighpass::new(sample_rate, 1_150.0),
            output_lowpass: OnePoleLowpass::new(sample_rate, 14_000.0),
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_highpass.reset();
        self.pre_emphasis.reset();
        self.preclip_lowpass.reset();
        self.postclip_lowpass.reset();
        self.body_lowpass.reset();
        self.edge_highpass.reset();
        self.output_lowpass.reset();
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: GodessOneControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: GodessOneControls,
    ) -> ElectricalSignal {
        let distortion = controls.distortion.clamp(0.0, 1.0);
        let tone = controls.tone.clamp(0.0, 1.0);
        let level = controls.level.clamp(0.0, 1.0);
        let custom = matches!(controls.mode, GodessOneMode::Custom);

        let input = self.input_highpass.process(loaded_input);
        let edge = self.pre_emphasis.process(input) * if custom { 0.46 } else { 0.64 };
        let body = input * if custom { 0.78 } else { 0.50 };
        let voiced_input = body + edge;
        let filtered = self.preclip_lowpass.process(voiced_input);

        let drive_gain = if custom {
            4.0 + distortion * 58.0
        } else {
            3.2 + distortion * 44.0
        };
        let knee = if custom { 0.58 } else { 0.48 };
        let clipped = diode_pair_clip(filtered * drive_gain, knee);
        let clipped = clipped + hard_clip(filtered * drive_gain * 0.42, knee * 1.08) * 0.32;
        let clipped = self.postclip_lowpass.process(clipped);

        let low = self.body_lowpass.process(clipped);
        let high = self.edge_highpass.process(clipped);
        let mid_fill = clipped * if custom { 0.34 } else { 0.12 };
        let low_weight = if custom {
            1.22 - tone * 0.46
        } else {
            0.92 - tone * 0.58
        };
        let high_weight = if custom {
            0.22 + tone * 0.92
        } else {
            0.42 + tone * 1.42
        };
        let voiced = low * low_weight + high * high_weight + mid_fill;
        let output_gain = if custom {
            0.16 + level * 2.05
        } else {
            0.13 + level * 1.82
        };
        let output = self
            .output_lowpass
            .process(voiced * output_gain)
            .clamp(-3.8, 3.8);

        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }
}

impl Muffin {
    pub const INPUT_IMPEDANCE_OHMS: f32 = 130_000.0;
    pub const OUTPUT_IMPEDANCE_OHMS: f32 = 10_000.0;

    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_connection: ConnectionState::new(sample_rate, 470e-12),
            input_coupling: OnePoleHighpass::new(sample_rate, 7.0),
            stage_filters: [
                OnePoleLowpass::new(sample_rate, 7_500.0),
                OnePoleLowpass::new(sample_rate, 6_500.0),
                OnePoleLowpass::new(sample_rate, 6_500.0),
                OnePoleLowpass::new(sample_rate, 8_500.0),
            ],
            tone_lowpass: OnePoleLowpass::new(sample_rate, 720.0),
            tone_highpass: OnePoleHighpass::new(sample_rate, 1_250.0),
            output_coupling: OnePoleHighpass::new(sample_rate, 16.0),
        }
    }

    pub fn reset(&mut self) {
        self.input_connection.reset();
        self.input_coupling.reset();
        for filter in &mut self.stage_filters {
            filter.reset();
        }
        self.tone_lowpass.reset();
        self.tone_highpass.reset();
        self.output_coupling.reset();
    }

    pub fn process(
        &mut self,
        input: ElectricalSignal,
        controls: MuffinControls,
    ) -> ElectricalSignal {
        let loaded_input = self
            .input_connection
            .drive_load(input, Load::new(Self::INPUT_IMPEDANCE_OHMS));
        self.process_loaded_voltage(loaded_input, controls)
    }

    pub fn process_loaded_voltage(
        &mut self,
        loaded_input: f32,
        controls: MuffinControls,
    ) -> ElectricalSignal {
        let sustain = controls.sustain.clamp(0.0, 1.0);
        let tone = controls.tone.clamp(0.0, 1.0);
        let level = controls.level.clamp(0.0, 1.0);

        let mut x = self.input_coupling.process(loaded_input);

        x = self.common_emitter_stage(x, 0, 7.5 + sustain * 8.0, false);
        x = self.common_emitter_stage(x, 1, 9.0 + sustain * 30.0, true);
        x = self.common_emitter_stage(x, 2, 9.0 + sustain * 34.0, true);
        x = self.common_emitter_stage(x, 3, 5.0 + level * 11.0, false);

        let low = self.tone_lowpass.process(x);
        let high = self.tone_highpass.process(x);
        let scooped = low * (1.0 - tone) + high * tone;
        let volume = 0.08 + level * 1.65;
        let output = self
            .output_coupling
            .process(scooped * volume)
            .clamp(-4.5, 4.5);

        ElectricalSignal::new(output, Self::OUTPUT_IMPEDANCE_OHMS)
    }

    fn common_emitter_stage(
        &mut self,
        input: f32,
        stage_index: usize,
        gain: f32,
        diode_clip: bool,
    ) -> f32 {
        let filtered = self.stage_filters[stage_index].process(input);
        let amplified = -filtered * gain;
        if diode_clip {
            diode_pair_clip(amplified, 0.42)
        } else {
            transistor_limit(amplified)
        }
    }
}

fn transistor_limit(input: f32) -> f32 {
    1.8 * (input / 1.8).tanh()
}

fn diode_pair_clip(input: f32, knee_voltage: f32) -> f32 {
    knee_voltage * (input / knee_voltage).tanh()
}

fn hard_clip(input: f32, limit: f32) -> f32 {
    input.clamp(-limit, limit)
}

fn asymmetric_diode_clip(input: f32, negative_knee: f32, positive_knee: f32) -> f32 {
    if input >= 0.0 {
        positive_knee * (input / positive_knee).tanh()
    } else {
        negative_knee * (input / negative_knee).tanh()
    }
}

#[derive(Clone, Copy, Default)]
struct AllPassStage {
    previous_input: f32,
    previous_output: f32,
}

impl AllPassStage {
    fn reset(&mut self) {
        self.previous_input = 0.0;
        self.previous_output = 0.0;
    }

    fn process(&mut self, input: f32, coefficient: f32) -> f32 {
        let output = coefficient * (input - self.previous_output) + self.previous_input;
        self.previous_input = input;
        self.previous_output = output;
        output
    }
}

fn allpass_coefficient(frequency_hz: f32, sample_rate: f32) -> f32 {
    let tangent =
        (std::f32::consts::PI * frequency_hz.clamp(20.0, sample_rate * 0.42) / sample_rate).tan();
    ((tangent - 1.0) / (tangent + 1.0)).clamp(-0.98, 0.98)
}

fn time_coefficient(sample_rate: f32, time_ms: f32) -> f32 {
    let samples = (time_ms.max(0.1) * 0.001 * sample_rate).max(1.0);
    (1.0 - (-1.0 / samples).exp()).clamp(0.0, 1.0)
}

struct FractionalDelayLine {
    buffer: Vec<f32>,
    write_index: usize,
}

impl FractionalDelayLine {
    fn new(sample_rate: f32, max_seconds: f32) -> Self {
        let len = (sample_rate * max_seconds).ceil().max(2.0) as usize + 2;
        Self {
            buffer: vec![0.0; len],
            write_index: 0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_index = 0;
    }

    fn process(&mut self, input: f32, delay_samples: f32) -> f32 {
        let len = self.buffer.len();
        let delay = delay_samples.clamp(1.0, (len - 2) as f32);
        self.buffer[self.write_index] = input;

        let read_position = (self.write_index as f32 - delay).rem_euclid(len as f32);
        let index_floor = read_position.floor();
        let index_a = index_floor as usize % len;
        let fraction = read_position - index_floor;
        let index_b = (index_a + 1) % len;
        let output = self.buffer[index_a] * (1.0 - fraction) + self.buffer[index_b] * fraction;

        self.write_index = (self.write_index + 1) % len;
        output
    }
}

struct SpringDelay {
    buffer: Vec<f32>,
    index: usize,
}

impl SpringDelay {
    fn new(sample_rate: f32, seconds: f32) -> Self {
        let len = (sample_rate * seconds).round().max(1.0) as usize;
        Self {
            buffer: vec![0.0; len],
            index: 0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.buffer[self.index] = input;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

#[derive(Clone, Copy)]
struct OnePoleLowpass {
    coefficient: f32,
    state: f32,
}

impl OnePoleLowpass {
    fn new(sample_rate: f32, cutoff_hz: f32) -> Self {
        let coefficient = 1.0 - (-std::f32::consts::TAU * cutoff_hz / sample_rate).exp();
        Self {
            coefficient: coefficient.clamp(0.0, 1.0),
            state: 0.0,
        }
    }

    fn reset(&mut self) {
        self.state = 0.0;
    }

    fn process(&mut self, input: f32) -> f32 {
        self.state += self.coefficient * (input - self.state);
        self.state
    }
}

struct OnePoleHighpass {
    lowpass: OnePoleLowpass,
}

impl OnePoleHighpass {
    fn new(sample_rate: f32, cutoff_hz: f32) -> Self {
        Self {
            lowpass: OnePoleLowpass::new(sample_rate, cutoff_hz),
        }
    }

    fn reset(&mut self) {
        self.lowpass.reset();
    }

    fn process(&mut self, input: f32) -> f32 {
        input - self.lowpass.process(input)
    }
}

#[derive(Default)]
struct StateVariableFilter {
    low: f32,
    band: f32,
}

impl StateVariableFilter {
    fn reset(&mut self) {
        self.low = 0.0;
        self.band = 0.0;
    }

    fn process_bandpass(
        &mut self,
        input: f32,
        center_hz: f32,
        resonance: f32,
        sample_rate: f32,
    ) -> f32 {
        let cutoff = center_hz.clamp(20.0, sample_rate * 0.42);
        let f = (2.0 * (std::f32::consts::PI * cutoff / sample_rate).sin()).clamp(0.0, 1.92);
        let damping = (1.0 / resonance.max(0.35)).clamp(0.08, 1.8);
        let high = input - self.low - damping * self.band;
        self.band = (self.band + f * high).clamp(-32.0, 32.0);
        self.low = (self.low + f * self.band).clamp(-32.0, 32.0);
        self.band
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_applies_voltage_divider() {
        let mut connection = ConnectionState::new(48_000.0, 0.0);
        let output =
            connection.drive_load(ElectricalSignal::new(1.0, 100_000.0), Load::new(100_000.0));

        assert!((output - 0.5).abs() < 1e-6);
    }

    #[test]
    fn muffin_exposes_low_output_impedance() {
        let mut pedal = Muffin::new(48_000.0);
        let output = pedal.process(
            ElectricalSignal::new(0.1, GUITAR_SOURCE_IMPEDANCE_OHMS),
            MuffinControls::default(),
        );

        assert_eq!(output.source_impedance_ohms, Muffin::OUTPUT_IMPEDANCE_OHMS);
        assert!(output.voltage.is_finite());
    }

    #[test]
    fn muffin_sustain_changes_transfer_curve() {
        let mut quiet = Muffin::new(48_000.0);
        let mut driven = Muffin::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..9_600 {
            let input = (std::f32::consts::TAU * 110.0 * sample_idx as f32 / 48_000.0).sin() * 0.08;
            let quiet_output = quiet.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MuffinControls {
                    sustain: 0.1,
                    tone: 0.5,
                    level: 0.7,
                },
            );
            let driven_output = driven.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MuffinControls {
                    sustain: 0.9,
                    tone: 0.5,
                    level: 0.7,
                },
            );
            if sample_idx >= 4_800 {
                difference_sum += (driven_output.voltage - quiet_output.voltage).abs();
            }
        }

        assert!(difference_sum > 20.0);
    }

    #[test]
    fn minotaur_exposes_buffered_output_impedance() {
        let mut pedal = Minotaur::new(48_000.0);
        let output = pedal.process(
            ElectricalSignal::new(0.1, GUITAR_SOURCE_IMPEDANCE_OHMS),
            MinotaurControls::default(),
        );

        assert_eq!(
            output.source_impedance_ohms,
            Minotaur::OUTPUT_IMPEDANCE_OHMS
        );
        assert!(output.voltage.is_finite());
    }

    #[test]
    fn minotaur_gain_changes_clean_drive_blend() {
        let mut low_gain = Minotaur::new(48_000.0);
        let mut high_gain = Minotaur::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..9_600 {
            let input = (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 48_000.0).sin() * 0.12;
            let low_output = low_gain.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MinotaurControls {
                    gain: 0.05,
                    treble: 0.5,
                    output: 0.5,
                },
            );
            let high_output = high_gain.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MinotaurControls {
                    gain: 0.9,
                    treble: 0.5,
                    output: 0.5,
                },
            );
            if sample_idx >= 4_800 {
                difference_sum += (high_output.voltage - low_output.voltage).abs();
            }
        }

        assert!(difference_sum > 10.0);
    }

    #[test]
    fn minotaur_treble_changes_presence_band() {
        let mut dark = Minotaur::new(48_000.0);
        let mut bright = Minotaur::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..9_600 {
            let input =
                (std::f32::consts::TAU * 2_000.0 * sample_idx as f32 / 48_000.0).sin() * 0.04;
            let dark_output = dark.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MinotaurControls {
                    gain: 0.35,
                    treble: 0.05,
                    output: 0.5,
                },
            );
            let bright_output = bright.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MinotaurControls {
                    gain: 0.35,
                    treble: 0.95,
                    output: 0.5,
                },
            );
            if sample_idx >= 4_800 {
                difference_sum += (bright_output.voltage - dark_output.voltage).abs();
            }
        }

        assert!(difference_sum > 2.0);
    }

    #[test]
    fn minotaur_reference_setting_has_nam_anchored_makeup_gain() {
        let mut pedal = Minotaur::new(48_000.0);
        let mut input_sum = 0.0;
        let mut output_sum = 0.0;
        let mut count = 0.0;

        for sample_idx in 0..12_000 {
            let input =
                (std::f32::consts::TAU * 1_000.0 * sample_idx as f32 / 48_000.0).sin() * 0.12;
            let output = pedal.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MinotaurControls {
                    gain: 0.55,
                    treble: 0.60,
                    output: 0.70,
                },
            );
            if sample_idx >= 6_000 {
                input_sum += input * input;
                output_sum += output.voltage * output.voltage;
                count += 1.0;
            }
        }

        let input_rms = (input_sum / count).sqrt();
        let output_rms = (output_sum / count).sqrt();
        let gain = output_rms / input_rms;

        assert!(
            (0.25..0.80).contains(&gain),
            "input_rms={input_rms}, output_rms={output_rms}, gain={gain}"
        );
    }

    #[test]
    fn minotaur_germanium_clip_knee_stays_below_silicon_range() {
        let clipped = diode_pair_clip(1.2, 0.42);

        assert!((0.40..0.43).contains(&clipped), "clipped={clipped}");
    }

    #[test]
    fn monarch_exposes_buffered_output_impedance() {
        let mut pedal = Monarch::new(48_000.0);
        let output = pedal.process(
            ElectricalSignal::new(0.1, GUITAR_SOURCE_IMPEDANCE_OHMS),
            MonarchControls::default(),
        );

        assert_eq!(output.source_impedance_ohms, Monarch::OUTPUT_IMPEDANCE_OHMS);
        assert!(output.voltage.is_finite());
    }

    #[test]
    fn monarch_gain_changes_dual_clip_drive() {
        let mut low_gain = Monarch::new(48_000.0);
        let mut high_gain = Monarch::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..9_600 {
            let input = (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 48_000.0).sin() * 0.10;
            let low_output = low_gain.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MonarchControls {
                    gain: 0.05,
                    tone: 0.5,
                    output: 0.55,
                },
            );
            let high_output = high_gain.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MonarchControls {
                    gain: 0.92,
                    tone: 0.5,
                    output: 0.55,
                },
            );
            if sample_idx >= 4_800 {
                difference_sum += (high_output.voltage - low_output.voltage).abs();
            }
        }

        assert!(difference_sum > 8.0);
    }

    #[test]
    fn monarch_tone_changes_high_band() {
        let mut dark = Monarch::new(48_000.0);
        let mut bright = Monarch::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..9_600 {
            let input =
                (std::f32::consts::TAU * 1_600.0 * sample_idx as f32 / 48_000.0).sin() * 0.05;
            let dark_output = dark.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MonarchControls {
                    gain: 0.45,
                    tone: 0.05,
                    output: 0.55,
                },
            );
            let bright_output = bright.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MonarchControls {
                    gain: 0.45,
                    tone: 0.95,
                    output: 0.55,
                },
            );
            if sample_idx >= 4_800 {
                difference_sum += (bright_output.voltage - dark_output.voltage).abs();
            }
        }

        assert!(difference_sum > 2.0);
    }

    #[test]
    fn godess_one_exposes_boss_style_buffered_output_impedance() {
        let mut pedal = GodessOne::new(48_000.0);
        let output = pedal.process(
            ElectricalSignal::new(0.1, GUITAR_SOURCE_IMPEDANCE_OHMS),
            GodessOneControls::default(),
        );

        assert_eq!(
            output.source_impedance_ohms,
            GodessOne::OUTPUT_IMPEDANCE_OHMS
        );
        assert!(output.voltage.is_finite());
    }

    #[test]
    fn godess_one_distortion_changes_hard_clip_drive() {
        let mut low_distortion = GodessOne::new(48_000.0);
        let mut high_distortion = GodessOne::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..9_600 {
            let input = (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 48_000.0).sin() * 0.10;
            let low_output = low_distortion.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                GodessOneControls {
                    distortion: 0.08,
                    tone: 0.5,
                    level: 0.55,
                    mode: GodessOneMode::Standard,
                },
            );
            let high_output = high_distortion.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                GodessOneControls {
                    distortion: 0.95,
                    tone: 0.5,
                    level: 0.55,
                    mode: GodessOneMode::Standard,
                },
            );
            if sample_idx >= 4_800 {
                difference_sum += (high_output.voltage - low_output.voltage).abs();
            }
        }

        assert!(difference_sum > 8.0);
    }

    #[test]
    fn godess_one_tone_changes_bright_edge() {
        let mut dark = GodessOne::new(48_000.0);
        let mut bright = GodessOne::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..9_600 {
            let input =
                (std::f32::consts::TAU * 1_800.0 * sample_idx as f32 / 48_000.0).sin() * 0.05;
            let dark_output = dark.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                GodessOneControls {
                    distortion: 0.55,
                    tone: 0.05,
                    level: 0.55,
                    mode: GodessOneMode::Standard,
                },
            );
            let bright_output = bright.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                GodessOneControls {
                    distortion: 0.55,
                    tone: 0.95,
                    level: 0.55,
                    mode: GodessOneMode::Standard,
                },
            );
            if sample_idx >= 4_800 {
                difference_sum += (bright_output.voltage - dark_output.voltage).abs();
            }
        }

        assert!(difference_sum > 2.0);
    }

    #[test]
    fn godess_one_custom_mode_changes_voice() {
        let mut standard = GodessOne::new(48_000.0);
        let mut custom = GodessOne::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..9_600 {
            let input = (std::f32::consts::TAU * 165.0 * sample_idx as f32 / 48_000.0).sin() * 0.11;
            let standard_output = standard.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                GodessOneControls {
                    distortion: 0.62,
                    tone: 0.48,
                    level: 0.55,
                    mode: GodessOneMode::Standard,
                },
            );
            let custom_output = custom.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                GodessOneControls {
                    distortion: 0.62,
                    tone: 0.48,
                    level: 0.55,
                    mode: GodessOneMode::Custom,
                },
            );
            if sample_idx >= 4_800 {
                difference_sum += (custom_output.voltage - standard_output.voltage).abs();
            }
        }

        assert!(difference_sum > 4.0);
    }

    #[test]
    fn dartford_depth_modulates_level() {
        let mut dry = Dartford::new(48_000.0);
        let mut wet = Dartford::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..48_000 {
            let input = (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 48_000.0).sin() * 0.2;
            let dry_output = dry.process(
                ElectricalSignal::new(input, 1_000.0),
                DartfordControls {
                    rate_hz: 5.0,
                    depth: 0.0,
                    level: 1.0,
                    wave: DartfordWave::Sine,
                },
            );
            let wet_output = wet.process(
                ElectricalSignal::new(input, 1_000.0),
                DartfordControls {
                    rate_hz: 5.0,
                    depth: 0.85,
                    level: 1.0,
                    wave: DartfordWave::Sine,
                },
            );
            if sample_idx >= 24_000 {
                difference_sum += (wet_output.voltage - dry_output.voltage).abs();
            }
        }

        assert!(difference_sum > 500.0);
    }

    #[test]
    fn tron_depth_moves_phase_notches() {
        let mut dry = Tron::new(48_000.0);
        let mut wet = Tron::new(48_000.0);
        let mut difference_sum = 0.0;
        let mut wet_sum = 0.0;

        for sample_idx in 0..48_000 {
            let input = (std::f32::consts::TAU * 330.0 * sample_idx as f32 / 48_000.0).sin() * 0.14
                + (std::f32::consts::TAU * 880.0 * sample_idx as f32 / 48_000.0).sin() * 0.08;
            let dry_output = dry.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                TronControls {
                    rate_hz: 0.8,
                    depth: 0.0,
                    feedback: 0.2,
                    mix: 0.5,
                },
            );
            let wet_output = wet.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                TronControls {
                    rate_hz: 0.8,
                    depth: 0.85,
                    feedback: 0.45,
                    mix: 0.7,
                },
            );
            if sample_idx >= 24_000 {
                difference_sum += (wet_output.voltage - dry_output.voltage).abs();
                wet_sum += wet_output.voltage.abs();
            }
        }

        assert!(difference_sum > 100.0, "difference_sum={difference_sum}");
        assert!(wet_sum > 50.0, "wet_sum={wet_sum}");
    }

    #[test]
    fn jetstream_depth_sweeps_short_delay_comb() {
        let mut shallow = Jetstream::new(48_000.0);
        let mut deep = Jetstream::new(48_000.0);
        let mut difference_sum = 0.0;
        let mut deep_sum = 0.0;

        for sample_idx in 0..48_000 {
            let input = (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 48_000.0).sin() * 0.10
                + (std::f32::consts::TAU * 880.0 * sample_idx as f32 / 48_000.0).sin() * 0.08;
            let shallow_output = shallow.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                JetstreamControls {
                    manual: 0.42,
                    rate_hz: 0.35,
                    depth: 0.05,
                    feedback: 0.18,
                    mix: 0.56,
                },
            );
            let deep_output = deep.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                JetstreamControls {
                    manual: 0.42,
                    rate_hz: 0.35,
                    depth: 0.85,
                    feedback: 0.56,
                    mix: 0.64,
                },
            );
            if sample_idx >= 24_000 {
                difference_sum += (deep_output.voltage - shallow_output.voltage).abs();
                deep_sum += deep_output.voltage.abs();
            }
        }

        assert!(difference_sum > 150.0, "difference_sum={difference_sum}");
        assert!(deep_sum > 50.0, "deep_sum={deep_sum}");
    }

    #[test]
    fn celeste_depth_adds_modulated_bbd_chorus() {
        let mut shallow = Celeste::new(48_000.0);
        let mut deep = Celeste::new(48_000.0);
        let mut difference_sum = 0.0;
        let mut deep_sum = 0.0;

        for sample_idx in 0..48_000 {
            let input = (std::f32::consts::TAU * 247.0 * sample_idx as f32 / 48_000.0).sin() * 0.10
                + (std::f32::consts::TAU * 741.0 * sample_idx as f32 / 48_000.0).sin() * 0.06;
            let shallow_output = shallow.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                CelesteControls {
                    rate_hz: 0.62,
                    depth: 0.05,
                    tone: 0.55,
                    mix: 0.42,
                },
            );
            let deep_output = deep.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                CelesteControls {
                    rate_hz: 0.62,
                    depth: 0.82,
                    tone: 0.62,
                    mix: 0.50,
                },
            );
            if sample_idx >= 24_000 {
                difference_sum += (deep_output.voltage - shallow_output.voltage).abs();
                deep_sum += deep_output.voltage.abs();
            }
        }

        assert!(difference_sum > 80.0, "difference_sum={difference_sum}");
        assert!(deep_sum > 40.0, "deep_sum={deep_sum}");
    }

    #[test]
    fn brigade_repeats_create_dark_delay_tail() {
        let mut dry = Brigade::new(48_000.0);
        let mut echo = Brigade::new(48_000.0);
        let mut dry_tail_sum = 0.0;
        let mut echo_tail_sum = 0.0;

        for sample_idx in 0..36_000 {
            let input = if sample_idx < 400 {
                (std::f32::consts::TAU * 180.0 * sample_idx as f32 / 48_000.0).sin() * 0.2
            } else {
                0.0
            };
            let dry_output = dry.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                BrigadeControls {
                    time_ms: 160.0,
                    repeats: 0.0,
                    tone: 0.45,
                    mix: 0.0,
                },
            );
            let echo_output = echo.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                BrigadeControls {
                    time_ms: 160.0,
                    repeats: 0.56,
                    tone: 0.36,
                    mix: 0.45,
                },
            );
            if sample_idx >= 7_680 {
                dry_tail_sum += dry_output.voltage.abs();
                echo_tail_sum += echo_output.voltage.abs();
            }
        }

        assert!(
            echo_tail_sum > dry_tail_sum + 10.0,
            "dry_tail_sum={dry_tail_sum}, echo_tail_sum={echo_tail_sum}"
        );
    }

    #[test]
    fn lumen_peak_reduction_levels_loud_guitar_segments() {
        let mut open = Lumen::new(48_000.0);
        let mut compressed = Lumen::new(48_000.0);
        let mut open_quiet_sum = 0.0;
        let mut open_loud_sum = 0.0;
        let mut compressed_quiet_sum = 0.0;
        let mut compressed_loud_sum = 0.0;

        for sample_idx in 0..96_000 {
            let loud = sample_idx >= 48_000;
            let amplitude = if loud { 0.22 } else { 0.035 };
            let input =
                (std::f32::consts::TAU * 196.0 * sample_idx as f32 / 48_000.0).sin() * amplitude;
            let open_output = open.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                LumenControls {
                    peak_reduction: 0.0,
                    gain: 0.5,
                    emphasis: 0.44,
                    mix: 0.0,
                },
            );
            let compressed_output = compressed.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                LumenControls {
                    peak_reduction: 0.74,
                    gain: 0.52,
                    emphasis: 0.48,
                    mix: 1.0,
                },
            );
            if (36_000..48_000).contains(&sample_idx) {
                open_quiet_sum += open_output.voltage.abs();
                compressed_quiet_sum += compressed_output.voltage.abs();
            } else if sample_idx >= 84_000 {
                open_loud_sum += open_output.voltage.abs();
                compressed_loud_sum += compressed_output.voltage.abs();
            }
        }

        let open_ratio = open_loud_sum / open_quiet_sum.max(1e-6);
        let compressed_ratio = compressed_loud_sum / compressed_quiet_sum.max(1e-6);
        assert!(
            compressed_ratio < open_ratio * 0.72,
            "open_ratio={open_ratio}, compressed_ratio={compressed_ratio}"
        );
        assert!(
            compressed_loud_sum > 40.0,
            "compressed_loud_sum={compressed_loud_sum}"
        );
    }

    #[test]
    fn muon_envelope_opens_filter_on_guitar_attacks() {
        let mut subtle = Muon::new(48_000.0);
        let mut open = Muon::new(48_000.0);
        let mut difference_sum = 0.0;
        let mut open_sum = 0.0;

        for sample_idx in 0..48_000 {
            let burst = if sample_idx % 12_000 < 2_200 {
                1.0
            } else {
                0.18
            };
            let decay = (1.0 - (sample_idx % 12_000) as f32 / 12_000.0).clamp(0.0, 1.0);
            let amplitude = 0.035 + burst * decay * 0.12;
            let input = (std::f32::consts::TAU * 164.0 * sample_idx as f32 / 48_000.0).sin()
                * amplitude
                + (std::f32::consts::TAU * 492.0 * sample_idx as f32 / 48_000.0).sin()
                    * amplitude
                    * 0.55;
            let subtle_output = subtle.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MuonControls {
                    sensitivity: 0.18,
                    range: 0.26,
                    resonance: 0.16,
                    mix: 0.25,
                },
            );
            let open_output = open.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                MuonControls {
                    sensitivity: 0.72,
                    range: 0.78,
                    resonance: 0.60,
                    mix: 0.90,
                },
            );
            if sample_idx >= 12_000 {
                difference_sum += (open_output.voltage - subtle_output.voltage).abs();
                open_sum += open_output.voltage.abs();
            }
        }

        assert!(difference_sum > 60.0, "difference_sum={difference_sum}");
        assert!(open_sum > 20.0, "open_sum={open_sum}");
    }

    #[test]
    fn springfield_mix_adds_spring_tail() {
        let mut dry = Springfield::new(48_000.0);
        let mut wet = Springfield::new(48_000.0);
        let mut dry_sum = 0.0;
        let mut wet_tail_sum = 0.0;

        for sample_idx in 0..12_000 {
            let input = if sample_idx == 0 { 0.8 } else { 0.0 };
            let dry_output = dry.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                SpringfieldControls {
                    dwell: 0.45,
                    tone: 0.5,
                    mix: 0.0,
                },
            );
            let wet_output = wet.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                SpringfieldControls {
                    dwell: 0.65,
                    tone: 0.58,
                    mix: 0.55,
                },
            );
            if sample_idx < 512 {
                dry_sum += dry_output.voltage.abs();
            }
            if sample_idx > 4_000 {
                wet_tail_sum += wet_output.voltage.abs();
            }
        }

        assert!(dry_sum > 0.1, "dry_sum={dry_sum}");
        assert!(wet_tail_sum > 0.05, "wet_tail_sum={wet_tail_sum}");
    }

    #[test]
    fn springfield_tone_changes_tail_color() {
        let mut dark = Springfield::new(48_000.0);
        let mut bright = Springfield::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..16_000 {
            let input = if sample_idx % 997 == 0 { 0.35 } else { 0.0 };
            let dark_output = dark.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                SpringfieldControls {
                    dwell: 0.55,
                    tone: 0.15,
                    mix: 0.45,
                },
            );
            let bright_output = bright.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                SpringfieldControls {
                    dwell: 0.55,
                    tone: 0.9,
                    mix: 0.45,
                },
            );
            if sample_idx > 4_000 {
                difference_sum += (bright_output.voltage - dark_output.voltage).abs();
            }
        }

        assert!(difference_sum > 0.2, "difference_sum={difference_sum}");
    }

    #[test]
    fn studioverb_mix_adds_room_tail() {
        let mut dry = StudioVerb::new(48_000.0);
        let mut wet = StudioVerb::new(48_000.0);
        let mut dry_tail_sum = 0.0;
        let mut wet_tail_sum = 0.0;
        let mut wet_peak = 0.0_f32;

        for sample_idx in 0..18_000 {
            let input = if sample_idx == 0 { 0.8 } else { 0.0 };
            let dry_output = dry.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                StudioVerbControls {
                    algorithm: StudioVerbAlgorithm::Room,
                    decay: 0.55,
                    size: 0.5,
                    pre_delay_ms: 10.0,
                    diffusion: 0.7,
                    tone: 0.55,
                    low_cut: 0.45,
                    mod_depth: 0.15,
                    mix: 0.0,
                },
            );
            let wet_output = wet.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                StudioVerbControls {
                    algorithm: StudioVerbAlgorithm::Room,
                    decay: 0.55,
                    size: 0.5,
                    pre_delay_ms: 10.0,
                    diffusion: 0.7,
                    tone: 0.55,
                    low_cut: 0.45,
                    mod_depth: 0.15,
                    mix: 0.45,
                },
            );
            if sample_idx > 6_000 {
                dry_tail_sum += dry_output.voltage.abs();
                wet_tail_sum += wet_output.voltage.abs();
                wet_peak = wet_peak.max(wet_output.voltage.abs());
            }
        }

        assert!(
            wet_tail_sum > dry_tail_sum + 0.12,
            "dry_tail_sum={dry_tail_sum}, wet_tail_sum={wet_tail_sum}"
        );
        assert!(wet_peak < 1.0, "wet_peak={wet_peak}");
    }

    #[test]
    fn studioverb_plate_and_room_have_distinct_tails() {
        let mut room = StudioVerb::new(48_000.0);
        let mut plate = StudioVerb::new(48_000.0);
        let mut difference_sum = 0.0;

        for sample_idx in 0..22_000 {
            let input = if sample_idx % 1_337 == 0 { 0.45 } else { 0.0 };
            let room_output = room.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                StudioVerbControls {
                    algorithm: StudioVerbAlgorithm::Room,
                    decay: 0.58,
                    size: 0.54,
                    pre_delay_ms: 12.0,
                    diffusion: 0.72,
                    tone: 0.55,
                    low_cut: 0.4,
                    mod_depth: 0.16,
                    mix: 0.42,
                },
            );
            let plate_output = plate.process(
                ElectricalSignal::new(input, GUITAR_SOURCE_IMPEDANCE_OHMS),
                StudioVerbControls {
                    algorithm: StudioVerbAlgorithm::Plate,
                    decay: 0.58,
                    size: 0.54,
                    pre_delay_ms: 12.0,
                    diffusion: 0.72,
                    tone: 0.55,
                    low_cut: 0.4,
                    mod_depth: 0.16,
                    mix: 0.42,
                },
            );
            if sample_idx > 5_000 {
                difference_sum += (plate_output.voltage - room_output.voltage).abs();
            }
        }

        assert!(difference_sum > 0.4, "difference_sum={difference_sum}");
    }
}
