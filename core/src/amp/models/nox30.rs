use super::AmpModel;
use crate::amp::components::{TopBoostToneStack, WdfHighpass};
use crate::amp::{AmpControls, NeuralCellMode, Nox30OperatingPoint};
use crate::circuit::passive::{
    BrightVolumeInputParams, BrightVolumeInputStage, CutPresenceParams, CutPresenceStage,
};
use crate::circuit::power::{
    OutputTransformerParams, OutputTransformerStage, PushPullEl84Params, PushPullEl84Stage,
    SupplyNetwork, SupplyNetworkParams,
};
use crate::circuit::triode::{
    CathodeFollowerParams, CathodeFollowerStage, CommonCathodeParams, CommonCathodeStage,
    LongTailPairParams, LongTailPairStage, TriodeParams,
};
use crate::neural_cell::{
    CommonCathodeNeuralAdapter, CommonCathodeNeuralAdapterParams, ExperimentalNeuralCell,
};
use std::env;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
struct FirstStageNeuralConfig {
    descriptor_path: PathBuf,
    mode: NeuralCellMode,
}

struct FirstStageNeural {
    adapter: CommonCathodeNeuralAdapter,
    mode: NeuralCellMode,
}

static FIRST_STAGE_NEURAL_CONFIG: OnceLock<Mutex<Option<FirstStageNeuralConfig>>> = OnceLock::new();

pub(in crate::amp) struct Nox30 {
    sample_rate: f32,
    input_volume: BrightVolumeInputStage,
    first_stage: CommonCathodeStage,
    first_stage_neural: Option<FirstStageNeural>,
    input_volume_output_v: f32,
    first_stage_output_v: f32,
    follower_output_v: f32,
    tone_stack_output_v: f32,
    preamp_send_v: f32,
    phase_inverter_input_v: f32,
    phase_inverter_output_v: f32,
    power_stage_output_v: f32,
    output_transformer_output_v: f32,
    first_stage_shadow_output_v: Option<f32>,
    first_stage_shadow_error_v: Option<f32>,
    follower: CathodeFollowerStage,
    drive_stage: CommonCathodeStage,
    recovery_stage: CommonCathodeStage,
    tone_stack: TopBoostToneStack,
    phase_inverter_coupling: WdfHighpass,
    phase_inverter: LongTailPairStage,
    cut_presence: CutPresenceStage,
    power_stage: PushPullEl84Stage,
    output_transformer: OutputTransformerStage,
    supply: SupplyNetwork,
}

pub(in crate::amp) struct Nox30PreampOutput {
    pub send_voltage: f32,
    first_stage_current: f32,
    follower_current: f32,
    drive_current: f32,
    recovery_current: f32,
}

impl Nox30 {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            input_volume: BrightVolumeInputStage::new(input_volume_params(sample_rate)),
            first_stage: CommonCathodeStage::new(first_stage_params(sample_rate)),
            first_stage_neural: first_stage_neural(sample_rate),
            input_volume_output_v: 0.0,
            first_stage_output_v: 0.0,
            follower_output_v: 0.0,
            tone_stack_output_v: 0.0,
            preamp_send_v: 0.0,
            phase_inverter_input_v: 0.0,
            phase_inverter_output_v: 0.0,
            power_stage_output_v: 0.0,
            output_transformer_output_v: 0.0,
            first_stage_shadow_output_v: None,
            first_stage_shadow_error_v: None,
            follower: CathodeFollowerStage::new(follower_params(sample_rate)),
            drive_stage: CommonCathodeStage::new(drive_stage_params(sample_rate)),
            recovery_stage: CommonCathodeStage::new(recovery_stage_params(sample_rate)),
            tone_stack: TopBoostToneStack::new(sample_rate),
            phase_inverter_coupling: WdfHighpass::from_rc(sample_rate, 1_000_000.0, 47e-9),
            phase_inverter: LongTailPairStage::new(phase_inverter_params(sample_rate)),
            cut_presence: CutPresenceStage::new(cut_presence_params(sample_rate)),
            power_stage: PushPullEl84Stage::new(power_stage_params(sample_rate)),
            output_transformer: OutputTransformerStage::new(output_transformer_params(sample_rate)),
            supply: SupplyNetwork::new(supply_network_params(sample_rate)),
        }
    }

    pub(super) fn operating_point(&self) -> Nox30OperatingPoint {
        let rails = self.supply.operating_point();
        let first = self.first_stage.operating_point();
        let follower = self.follower.operating_point();
        let drive = self.drive_stage.operating_point();
        let recovery = self.recovery_stage.operating_point();
        let phase_inverter = self.phase_inverter.operating_point();
        let power = self.power_stage.operating_point();
        let transformer = self.output_transformer.operating_point();

        Nox30OperatingPoint {
            input_volume_output_v: self.input_volume_output_v,
            first_stage_output_v: self.first_stage_output_v,
            follower_output_v: self.follower_output_v,
            tone_stack_output_v: self.tone_stack_output_v,
            preamp_send_v: self.preamp_send_v,
            phase_inverter_input_v: self.phase_inverter_input_v,
            phase_inverter_output_v: self.phase_inverter_output_v,
            power_stage_output_v: self.power_stage_output_v,
            output_transformer_output_v: self.output_transformer_output_v,
            preamp_voltage: rails.preamp_voltage,
            phase_inverter_voltage: rails.phase_inverter_voltage,
            power_voltage: rails.power_voltage,
            first_stage_plate_current: first.plate_current,
            first_stage_cathode_voltage: first.cathode_voltage,
            follower_plate_current: follower.plate_current,
            follower_cathode_voltage: follower.cathode_voltage,
            drive_stage_plate_current: drive.plate_current,
            recovery_stage_plate_current: recovery.plate_current,
            first_stage_shadow_output_v: self.first_stage_shadow_output_v,
            first_stage_shadow_error_v: self.first_stage_shadow_error_v,
            phase_inverter_plate_a_current: phase_inverter.plate_a_current,
            phase_inverter_plate_b_current: phase_inverter.plate_b_current,
            phase_inverter_cathode_voltage: phase_inverter.cathode_voltage,
            power_positive_current: power.positive_current,
            power_negative_current: power.negative_current,
            power_positive_screen_current: power.positive_screen_current,
            power_negative_screen_current: power.negative_screen_current,
            power_screen_voltage: power.screen_voltage,
            power_cathode_bias_voltage: power.cathode_bias_voltage,
            power_attack_current: power.attack_current,
            transformer_core_flux: transformer.core_flux,
        }
    }

    #[inline]
    pub(super) fn process_preamp(
        &mut self,
        input: f32,
        controls: AmpControls,
    ) -> Nox30PreampOutput {
        let rails = self.supply.operating_point();
        let preamp_voltage = rails.preamp_voltage / 280.0;
        let phase_inverter_voltage = rails.phase_inverter_voltage / 300.0;
        let volume_output = self.input_volume.process(input, controls.volume);
        self.input_volume_output_v = volume_output;

        let analytic_first_stage = self.first_stage.process(volume_output);
        let mut first_stage = analytic_first_stage;
        if let Some(neural) = &mut self.first_stage_neural {
            let neural_output = neural.adapter.process_sample(volume_output);
            self.first_stage_shadow_output_v = Some(neural_output);
            self.first_stage_shadow_error_v = Some(neural_output - analytic_first_stage);
            if neural.mode == NeuralCellMode::Replace {
                first_stage = neural_output;
            }
        } else {
            self.first_stage_shadow_output_v = None;
            self.first_stage_shadow_error_v = None;
        }
        self.first_stage_output_v = first_stage;
        let first_stage_current = self.first_stage.operating_point().plate_current;

        let follower_drive = self.follower.process(first_stage * preamp_voltage);
        self.follower_output_v = follower_drive;
        let follower_current = self.follower.operating_point().plate_current;
        let tone_stack_output = self
            .tone_stack
            .process(follower_drive, controls.bass, controls.treble);
        self.tone_stack_output_v = tone_stack_output;
        let toned = tone_stack_output;
        let nox30_drive = controls.drive.clamp(0.0, 1.0);
        let (driven_tone, drive_current, recovery_current) = if nox30_drive > 0.0 {
            let hot_stage = self
                .drive_stage
                .process(toned * (0.35 + nox30_drive * 1.85));
            let drive_current = self.drive_stage.operating_point().plate_current;
            let recovered = self
                .recovery_stage
                .process(hot_stage * (0.45 + nox30_drive * 1.35));
            let recovery_current = self.recovery_stage.operating_point().plate_current;
            (
                toned * (1.0 - nox30_drive * 0.45) + recovered * nox30_drive * 1.15,
                drive_current,
                recovery_current,
            )
        } else {
            (toned, 0.0, 0.0)
        };

        let send_voltage = driven_tone * 5.0 * preamp_voltage * phase_inverter_voltage;
        self.preamp_send_v = send_voltage;

        Nox30PreampOutput {
            send_voltage,
            first_stage_current,
            follower_current,
            drive_current,
            recovery_current,
        }
    }

    #[inline]
    pub(super) fn process_power_amp(
        &mut self,
        return_voltage: f32,
        preamp: Nox30PreampOutput,
        controls: AmpControls,
    ) -> f32 {
        let rails = self.supply.operating_point();
        let power_voltage = rails.power_voltage / 320.0;
        let pi_input = self.phase_inverter_coupling.process(return_voltage);
        self.phase_inverter_input_v = pi_input;
        let differential = self.phase_inverter.process(pi_input);
        self.phase_inverter_output_v = differential;
        let phase_inverter_op = self.phase_inverter.operating_point();
        let phase_inverter_current =
            phase_inverter_op.plate_a_current + phase_inverter_op.plate_b_current;
        let voiced_output =
            self.cut_presence
                .process(differential, controls.cut, controls.presence);

        let power_output = self
            .power_stage
            .process(voiced_output * power_voltage, controls.sag);
        self.power_stage_output_v = power_output;
        let power_current = {
            let operating_point = self.power_stage.operating_point();
            operating_point.positive_current
                + operating_point.negative_current
                + operating_point.positive_screen_current
                + operating_point.negative_screen_current
                + operating_point.attack_current * 0.65
        };
        let preamp_current = preamp.first_stage_current
            + preamp.follower_current
            + preamp.drive_current
            + preamp.recovery_current;
        self.supply.process(
            preamp_current,
            phase_inverter_current,
            power_current,
            controls.sag,
        );

        let transformer_output = self.output_transformer.process(power_output);
        self.output_transformer_output_v = transformer_output;
        transformer_output * controls.output
    }
}

impl AmpModel for Nox30 {
    fn reset(&mut self) {
        *self = Self::new(self.sample_rate);
    }

    #[inline]
    fn process(&mut self, input: f32, controls: AmpControls) -> f32 {
        let preamp = self.process_preamp(input, controls);
        let return_voltage = preamp.send_voltage;
        self.process_power_amp(return_voltage, preamp, controls)
    }
}

fn input_volume_params(sample_rate: f32) -> BrightVolumeInputParams {
    BrightVolumeInputParams {
        sample_rate,
        input_resistance: 1_000_000.0,
        input_coupling_capacitance: 47e-9,
        bright_cutoff_hz: 2_900.0,
        bright_bypass_gain: 0.18,
    }
}

fn cut_presence_params(sample_rate: f32) -> CutPresenceParams {
    CutPresenceParams {
        sample_rate,
        min_cutoff_hz: 1_150.0,
        max_cutoff_hz: 18_000.0,
        presence_gain: 1.05,
    }
}

fn first_stage_params(sample_rate: f32) -> CommonCathodeParams {
    CommonCathodeParams {
        sample_rate,
        grid_leak_resistance: 1_000_000.0,
        input_coupling_capacitance: 22e-9,
        plate_resistance: 100_000.0,
        cathode_resistance: 1_500.0,
        cathode_bypass_capacitance: Some(25e-6),
        supply_resistance: 10_000.0,
        supply_capacitance: 22e-6,
        nominal_supply_voltage: 280.0,
        input_gain: 3.2,
        output_scale: 0.16,
        triode: TriodeParams::ECC83,
    }
}

pub(super) fn configure_first_stage_neural(descriptor_path: Option<PathBuf>, mode: NeuralCellMode) {
    let slot = FIRST_STAGE_NEURAL_CONFIG.get_or_init(|| Mutex::new(None));
    *slot.lock().expect("nox30 neural config mutex poisoned") =
        descriptor_path.map(|descriptor_path| FirstStageNeuralConfig {
            descriptor_path,
            mode,
        });
}

fn first_stage_neural(sample_rate: f32) -> Option<FirstStageNeural> {
    let config = configured_first_stage_neural()
        .or_else(|| {
            env_first_stage_neural(
                "GREYBOUND_NOX30_FIRST_STAGE_SHADOW_DESCRIPTOR",
                NeuralCellMode::Shadow,
            )
        })
        .or_else(|| {
            env_first_stage_neural(
                "GREYBOUND_NOX30_FIRST_STAGE_REPLACE_DESCRIPTOR",
                NeuralCellMode::Replace,
            )
        })?;
    let cell = ExperimentalNeuralCell::from_descriptor_path(&config.descriptor_path).ok()?;
    let params = first_stage_params(sample_rate);
    let output_bias = common_cathode_silence_output(params);
    Some(FirstStageNeural {
        adapter: CommonCathodeNeuralAdapter::from_cell(
            cell,
            CommonCathodeNeuralAdapterParams {
                input_gain: params.input_gain,
                output_scale: params.output_scale,
                output_bias,
            },
        ),
        mode: config.mode,
    })
}

fn common_cathode_silence_output(params: CommonCathodeParams) -> f32 {
    let mut stage = CommonCathodeStage::new(params);
    let warmup_samples = (params.sample_rate as usize / 20).max(1);
    let mut output = 0.0;
    for _ in 0..warmup_samples {
        output = stage.process(0.0);
    }
    output
}

fn configured_first_stage_neural() -> Option<FirstStageNeuralConfig> {
    FIRST_STAGE_NEURAL_CONFIG
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("nox30 neural config mutex poisoned")
        .clone()
}

fn env_first_stage_neural(name: &str, mode: NeuralCellMode) -> Option<FirstStageNeuralConfig> {
    env::var_os(name).map(|descriptor_path| FirstStageNeuralConfig {
        descriptor_path: PathBuf::from(descriptor_path),
        mode,
    })
}

fn follower_params(sample_rate: f32) -> CathodeFollowerParams {
    CathodeFollowerParams {
        sample_rate,
        grid_leak_resistance: 1_000_000.0,
        input_coupling_capacitance: 47e-9,
        cathode_resistance: 100_000.0,
        nominal_supply_voltage: 280.0,
        input_gain: 1.0,
        output_scale: 0.85,
        triode: TriodeParams::ECC83,
    }
}

fn drive_stage_params(sample_rate: f32) -> CommonCathodeParams {
    CommonCathodeParams {
        sample_rate,
        grid_leak_resistance: 470_000.0,
        input_coupling_capacitance: 4.7e-9,
        plate_resistance: 100_000.0,
        cathode_resistance: 10_000.0,
        cathode_bypass_capacitance: None,
        supply_resistance: 18_000.0,
        supply_capacitance: 10e-6,
        nominal_supply_voltage: 280.0,
        input_gain: 1.0,
        output_scale: 0.20,
        triode: TriodeParams::ECC83,
    }
}

fn recovery_stage_params(sample_rate: f32) -> CommonCathodeParams {
    CommonCathodeParams {
        sample_rate,
        grid_leak_resistance: 470_000.0,
        input_coupling_capacitance: 22e-9,
        plate_resistance: 100_000.0,
        cathode_resistance: 2_200.0,
        cathode_bypass_capacitance: Some(1e-6),
        supply_resistance: 12_000.0,
        supply_capacitance: 22e-6,
        nominal_supply_voltage: 280.0,
        input_gain: 1.0,
        output_scale: 0.12,
        triode: TriodeParams::ECC83,
    }
}

fn phase_inverter_params(sample_rate: f32) -> LongTailPairParams {
    LongTailPairParams {
        sample_rate,
        grid_leak_resistance: 1_000_000.0,
        input_coupling_capacitance: 47e-9,
        plate_a_resistance: 100_000.0,
        plate_b_resistance: 82_000.0,
        tail_resistance: 10_000.0,
        nominal_supply_voltage: 300.0,
        input_gain: 1.0,
        output_scale: 0.018,
        triode: TriodeParams::ECC83,
    }
}

fn power_stage_params(sample_rate: f32) -> PushPullEl84Params {
    PushPullEl84Params {
        sample_rate,
        nominal_supply_voltage: 320.0,
        screen_voltage: 300.0,
        screen_resistance: 1_800.0,
        screen_capacitance: 10e-6,
        primary_half_resistance: 3_200.0,
        supply_resistance: 520.0,
        supply_capacitance: 16e-6,
        cathode_resistance: 130.0,
        cathode_capacitance: 18e-6,
        idle_current: 0.040,
        drive_gain: 34.0,
        current_gain: 0.0048,
        load_current_coupling: 1.35,
        attack_current_coupling: 0.65,
        compression: 0.38,
        output_scale: 0.010,
    }
}

fn output_transformer_params(sample_rate: f32) -> OutputTransformerParams {
    OutputTransformerParams {
        sample_rate,
        primary_resistance: 8_000.0,
        primary_inductance: 47.0,
        leakage_cutoff_hz: 32_000.0,
        core_saturation: 6_500.0,
        secondary_saturation_voltage: 0.52,
        output_scale: 0.90,
    }
}

fn supply_network_params(sample_rate: f32) -> SupplyNetworkParams {
    SupplyNetworkParams {
        sample_rate,
        rectifier_voltage: 340.0,
        power_nominal_voltage: 320.0,
        phase_inverter_nominal_voltage: 300.0,
        preamp_nominal_voltage: 280.0,
        rectifier_resistance: 420.0,
        phase_inverter_resistance: 10_000.0,
        preamp_resistance: 12_000.0,
        reservoir_capacitance: 32e-6,
        phase_inverter_capacitance: 22e-6,
        preamp_capacitance: 22e-6,
    }
}
