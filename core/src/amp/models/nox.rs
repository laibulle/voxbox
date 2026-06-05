use super::AmpModel;
use crate::amp::components::{
    el84_bank, triode_stage, OnePoleLowpass, SupplyNode, TopBoostToneStack, WdfHighpass,
};
use crate::amp::AmpControls;
use crate::circuit::triode::{
    CathodeFollowerParams, CathodeFollowerStage, CommonCathodeParams, CommonCathodeStage,
    TriodeParams,
};

pub(in crate::amp) struct Nox {
    sample_rate: f32,
    input_coupling: WdfHighpass,
    bright_filter: OnePoleLowpass,
    first_stage: CommonCathodeStage,
    follower: CathodeFollowerStage,
    drive_stage: CommonCathodeStage,
    recovery_stage: CommonCathodeStage,
    tone_stack: TopBoostToneStack,
    phase_inverter_coupling: WdfHighpass,
    cut_filter: OnePoleLowpass,
    transformer_highpass: WdfHighpass,
    transformer_lowpass: OnePoleLowpass,
    power_supply: SupplyNode,
}

impl Nox {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            input_coupling: WdfHighpass::from_rc(sample_rate, 1_000_000.0, 47e-9),
            bright_filter: OnePoleLowpass::new(sample_rate, 2_900.0),
            first_stage: CommonCathodeStage::new(first_stage_params(sample_rate)),
            follower: CathodeFollowerStage::new(follower_params(sample_rate)),
            drive_stage: CommonCathodeStage::new(drive_stage_params(sample_rate)),
            recovery_stage: CommonCathodeStage::new(recovery_stage_params(sample_rate)),
            tone_stack: TopBoostToneStack::new(sample_rate),
            phase_inverter_coupling: WdfHighpass::from_rc(sample_rate, 1_000_000.0, 47e-9),
            cut_filter: OnePoleLowpass::new(sample_rate, 12_000.0),
            transformer_highpass: WdfHighpass::from_rc(sample_rate, 100_000.0, 47e-9),
            transformer_lowpass: OnePoleLowpass::new(sample_rate, 13_000.0),
            power_supply: SupplyNode::new(sample_rate, 320.0, 360.0, 32e-6),
        }
    }
}

impl AmpModel for Nox {
    fn reset(&mut self) {
        *self = Self::new(self.sample_rate);
    }

    #[inline]
    fn process(&mut self, input: f32, controls: AmpControls) -> f32 {
        let input = self.input_coupling.process(input);

        let volume = controls.volume * controls.volume;
        let high = input - self.bright_filter.process(input);
        let volume_output = input * volume + high * (1.0 - volume) * 0.18;

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
            .process(driven_tone * 4.6 * preamp_voltage);
        let phase_a = triode_stage(pi_input * 1.34, 0.040 * preamp_voltage);
        let phase_b = triode_stage(-pi_input * 1.30, -0.032 * preamp_voltage);
        let differential = (phase_a - phase_b) * 0.5;

        let cut_hz = 13_500.0 * (1.0 - controls.cut).powi(2) + 1_150.0;
        self.cut_filter.set_cutoff(self.sample_rate, cut_hz);
        let cut_output = self.cut_filter.process(differential);
        let presence = controls.presence.clamp(0.0, 1.0);
        let voiced_output = cut_output + (differential - cut_output) * presence * 0.35;

        let power_voltage = self.power_supply.normalized();
        let power_drive = voiced_output * 1.58 * power_voltage;
        let positive_bank = el84_bank(power_drive);
        let negative_bank = el84_bank(-power_drive);
        let push_pull_current =
            (positive_bank.abs() + negative_bank.abs()) * (0.020 + controls.sag * 0.700);
        let updated_power_voltage = self.power_supply.process(push_pull_current) / 320.0;
        let power_output = (positive_bank - negative_bank) * 0.72 * updated_power_voltage;

        let mut transformer = self.transformer_highpass.process(power_output);
        transformer = self.transformer_lowpass.process(transformer);
        transformer * controls.output
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
