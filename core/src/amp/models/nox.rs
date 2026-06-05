use super::AmpModel;
use crate::amp::components::{TopBoostToneStack, WdfHighpass};
use crate::amp::AmpControls;
use crate::circuit::passive::{
    BrightVolumeInputParams, BrightVolumeInputStage, CutPresenceParams, CutPresenceStage,
};
use crate::circuit::power::{
    OutputTransformerParams, OutputTransformerStage, PushPullEl84Params, PushPullEl84Stage,
};
use crate::circuit::triode::{
    CathodeFollowerParams, CathodeFollowerStage, CommonCathodeParams, CommonCathodeStage,
    LongTailPairParams, LongTailPairStage, TriodeParams,
};

pub(in crate::amp) struct Nox {
    sample_rate: f32,
    input_volume: BrightVolumeInputStage,
    first_stage: CommonCathodeStage,
    follower: CathodeFollowerStage,
    drive_stage: CommonCathodeStage,
    recovery_stage: CommonCathodeStage,
    tone_stack: TopBoostToneStack,
    phase_inverter_coupling: WdfHighpass,
    phase_inverter: LongTailPairStage,
    cut_presence: CutPresenceStage,
    power_stage: PushPullEl84Stage,
    output_transformer: OutputTransformerStage,
}

impl Nox {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            input_volume: BrightVolumeInputStage::new(input_volume_params(sample_rate)),
            first_stage: CommonCathodeStage::new(first_stage_params(sample_rate)),
            follower: CathodeFollowerStage::new(follower_params(sample_rate)),
            drive_stage: CommonCathodeStage::new(drive_stage_params(sample_rate)),
            recovery_stage: CommonCathodeStage::new(recovery_stage_params(sample_rate)),
            tone_stack: TopBoostToneStack::new(sample_rate),
            phase_inverter_coupling: WdfHighpass::from_rc(sample_rate, 1_000_000.0, 47e-9),
            phase_inverter: LongTailPairStage::new(phase_inverter_params(sample_rate)),
            cut_presence: CutPresenceStage::new(cut_presence_params(sample_rate)),
            power_stage: PushPullEl84Stage::new(power_stage_params(sample_rate)),
            output_transformer: OutputTransformerStage::new(output_transformer_params(sample_rate)),
        }
    }
}

impl AmpModel for Nox {
    fn reset(&mut self) {
        *self = Self::new(self.sample_rate);
    }

    #[inline]
    fn process(&mut self, input: f32, controls: AmpControls) -> f32 {
        let volume_output = self.input_volume.process(input, controls.volume);

        let first_stage = self.first_stage.process(volume_output);
        let preamp_voltage = self.first_stage.operating_point().supply_voltage / 280.0;

        let follower_drive = self.follower.process(first_stage * preamp_voltage);
        let toned = self
            .tone_stack
            .process(follower_drive, controls.bass, controls.treble);
        let nox_drive = controls.drive.clamp(0.0, 1.0);
        let driven_tone = if nox_drive > 0.0 {
            let hot_stage = self.drive_stage.process(toned * (0.35 + nox_drive * 1.85));
            let recovered = self
                .recovery_stage
                .process(hot_stage * (0.45 + nox_drive * 1.35));
            toned * (1.0 - nox_drive * 0.45) + recovered * nox_drive * 1.15
        } else {
            toned
        };

        let pi_input = self
            .phase_inverter_coupling
            .process(driven_tone * 2.8 * preamp_voltage);
        let differential = self.phase_inverter.process(pi_input);
        let voiced_output =
            self.cut_presence
                .process(differential, controls.cut, controls.presence);

        let power_output = self.power_stage.process(voiced_output, controls.sag);
        self.output_transformer.process(power_output) * controls.output
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
        max_cutoff_hz: 13_500.0,
        presence_gain: 0.35,
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
        primary_half_resistance: 3_200.0,
        supply_resistance: 360.0,
        supply_capacitance: 32e-6,
        cathode_resistance: 130.0,
        cathode_capacitance: 50e-6,
        idle_current: 0.040,
        drive_gain: 18.0,
        current_gain: 0.0048,
        compression: 0.22,
        output_scale: 0.020,
    }
}

fn output_transformer_params(sample_rate: f32) -> OutputTransformerParams {
    OutputTransformerParams {
        sample_rate,
        primary_resistance: 100_000.0,
        primary_inductance: 47.0,
        leakage_cutoff_hz: 13_000.0,
        core_saturation: 1_400.0,
        output_scale: 1.0,
    }
}
