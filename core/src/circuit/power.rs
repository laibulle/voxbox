#[derive(Clone, Copy)]
pub struct PushPullEl84Params {
    pub sample_rate: f32,
    pub nominal_supply_voltage: f32,
    pub screen_voltage: f32,
    pub primary_half_resistance: f32,
    pub supply_resistance: f32,
    pub supply_capacitance: f32,
    pub cathode_resistance: f32,
    pub cathode_capacitance: f32,
    pub idle_current: f32,
    pub drive_gain: f32,
    pub current_gain: f32,
    pub compression: f32,
    pub output_scale: f32,
}

#[derive(Clone, Copy)]
pub struct OutputTransformerParams {
    pub sample_rate: f32,
    pub primary_resistance: f32,
    pub primary_inductance: f32,
    pub leakage_cutoff_hz: f32,
    pub core_saturation: f32,
    pub output_scale: f32,
}

#[derive(Clone, Copy)]
pub struct SupplyNetworkParams {
    pub sample_rate: f32,
    pub rectifier_voltage: f32,
    pub power_nominal_voltage: f32,
    pub phase_inverter_nominal_voltage: f32,
    pub preamp_nominal_voltage: f32,
    pub rectifier_resistance: f32,
    pub phase_inverter_resistance: f32,
    pub preamp_resistance: f32,
    pub reservoir_capacitance: f32,
    pub phase_inverter_capacitance: f32,
    pub preamp_capacitance: f32,
}

pub struct PushPullEl84Stage {
    params: PushPullEl84Params,
    supply_voltage: f32,
    cathode_bias_voltage: f32,
    plate_a_voltage: f32,
    plate_b_voltage: f32,
    reference_plate_a_voltage: f32,
    reference_plate_b_voltage: f32,
    positive_current: f32,
    negative_current: f32,
}

pub struct OutputTransformerStage {
    params: OutputTransformerParams,
    primary_lowpass: OnePole,
    leakage_lowpass: OnePole,
    core_flux: f32,
}

pub struct SupplyNetwork {
    params: SupplyNetworkParams,
    power_voltage: f32,
    phase_inverter_voltage: f32,
    preamp_voltage: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PushPullEl84OperatingPoint {
    pub supply_voltage: f32,
    pub plate_a_voltage: f32,
    pub plate_b_voltage: f32,
    pub cathode_bias_voltage: f32,
    pub positive_current: f32,
    pub negative_current: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct OutputTransformerOperatingPoint {
    pub core_flux: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SupplyNetworkOperatingPoint {
    pub power_voltage: f32,
    pub phase_inverter_voltage: f32,
    pub preamp_voltage: f32,
}

#[derive(Clone, Copy)]
struct PentodePoint {
    current: f32,
    d_current_d_plate: f32,
}

impl OutputTransformerStage {
    pub fn new(params: OutputTransformerParams) -> Self {
        let primary_cutoff_hz = params.primary_resistance
            / (std::f32::consts::TAU * params.primary_inductance.max(1e-6));
        Self {
            params,
            primary_lowpass: OnePole::new(params.sample_rate, primary_cutoff_hz),
            leakage_lowpass: OnePole::new(params.sample_rate, params.leakage_cutoff_hz),
            core_flux: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.primary_lowpass.reset();
        self.leakage_lowpass.reset();
        self.core_flux = 0.0;
    }

    pub fn operating_point(&self) -> OutputTransformerOperatingPoint {
        OutputTransformerOperatingPoint {
            core_flux: self.core_flux,
        }
    }

    pub fn process(&mut self, primary_voltage: f32) -> f32 {
        let primary_highpass = primary_voltage - self.primary_lowpass.process(primary_voltage);
        let flux_coefficient = 1.0 / self.params.sample_rate;
        self.core_flux += flux_coefficient * primary_highpass;
        let saturation = 1.0 / (1.0 + (self.core_flux.abs() * self.params.core_saturation).powi(2));
        self.core_flux *= 0.9995;

        self.leakage_lowpass
            .process(primary_highpass * saturation * self.params.output_scale)
    }
}

impl SupplyNetwork {
    pub fn new(params: SupplyNetworkParams) -> Self {
        Self {
            params,
            power_voltage: params.power_nominal_voltage,
            phase_inverter_voltage: params.phase_inverter_nominal_voltage,
            preamp_voltage: params.preamp_nominal_voltage,
        }
    }

    pub fn reset(&mut self) {
        self.power_voltage = self.params.power_nominal_voltage;
        self.phase_inverter_voltage = self.params.phase_inverter_nominal_voltage;
        self.preamp_voltage = self.params.preamp_nominal_voltage;
    }

    pub fn operating_point(&self) -> SupplyNetworkOperatingPoint {
        SupplyNetworkOperatingPoint {
            power_voltage: self.power_voltage,
            phase_inverter_voltage: self.phase_inverter_voltage,
            preamp_voltage: self.preamp_voltage,
        }
    }

    pub fn process(
        &mut self,
        preamp_current: f32,
        phase_inverter_current: f32,
        power_current: f32,
        sag: f32,
    ) -> SupplyNetworkOperatingPoint {
        let sag = sag.clamp(0.0, 1.0);
        let power_current = power_current.max(0.0);
        let phase_inverter_current = phase_inverter_current.max(0.0);
        let preamp_current = preamp_current.max(0.0);
        let power_target = self.params.rectifier_voltage
            - power_current * self.params.rectifier_resistance * (0.30 + sag);
        let phase_inverter_target = power_target
            - (phase_inverter_current + preamp_current) * self.params.phase_inverter_resistance;
        let preamp_target = phase_inverter_target - preamp_current * self.params.preamp_resistance;

        self.power_voltage = smooth_voltage(
            self.power_voltage,
            power_target,
            self.params.sample_rate,
            self.params.rectifier_resistance,
            self.params.reservoir_capacitance,
        )
        .clamp(
            self.params.power_nominal_voltage * 0.55,
            self.params.rectifier_voltage,
        );
        self.phase_inverter_voltage = smooth_voltage(
            self.phase_inverter_voltage,
            phase_inverter_target,
            self.params.sample_rate,
            self.params.phase_inverter_resistance,
            self.params.phase_inverter_capacitance,
        )
        .clamp(
            self.params.phase_inverter_nominal_voltage * 0.55,
            self.power_voltage,
        );
        self.preamp_voltage = smooth_voltage(
            self.preamp_voltage,
            preamp_target,
            self.params.sample_rate,
            self.params.preamp_resistance,
            self.params.preamp_capacitance,
        )
        .clamp(
            self.params.preamp_nominal_voltage * 0.55,
            self.phase_inverter_voltage,
        );

        self.operating_point()
    }
}

impl PushPullEl84Stage {
    pub fn new(params: PushPullEl84Params) -> Self {
        let idle_cathode = params.idle_current * params.cathode_resistance;
        let idle_plate_drop = params.idle_current * 0.5 * params.primary_half_resistance;
        let idle_plate = params.nominal_supply_voltage - idle_plate_drop;
        let mut stage = Self {
            params,
            supply_voltage: params.nominal_supply_voltage,
            cathode_bias_voltage: idle_cathode,
            plate_a_voltage: idle_plate,
            plate_b_voltage: idle_plate,
            reference_plate_a_voltage: idle_plate,
            reference_plate_b_voltage: idle_plate,
            positive_current: params.idle_current * 0.5,
            negative_current: params.idle_current * 0.5,
        };
        for _ in 0..512 {
            stage.process(0.0, 0.0);
        }
        stage.reference_plate_a_voltage = stage.plate_a_voltage;
        stage.reference_plate_b_voltage = stage.plate_b_voltage;
        stage
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.params);
    }

    pub fn operating_point(&self) -> PushPullEl84OperatingPoint {
        PushPullEl84OperatingPoint {
            supply_voltage: self.supply_voltage,
            plate_a_voltage: self.plate_a_voltage,
            plate_b_voltage: self.plate_b_voltage,
            cathode_bias_voltage: self.cathode_bias_voltage,
            positive_current: self.positive_current,
            negative_current: self.negative_current,
        }
    }

    pub fn process(&mut self, drive: f32, sag: f32) -> f32 {
        let supply_ratio = self.supply_ratio();
        let drive_voltage = drive * self.params.drive_gain * supply_ratio;
        let idle_bias = self.params.idle_current * self.params.cathode_resistance;
        let bias_offset = (self.cathode_bias_voltage - idle_bias) * 0.030;

        let (plate_a, positive_current) =
            self.solve_plate(self.plate_a_voltage, drive_voltage - bias_offset);
        let (plate_b, negative_current) =
            self.solve_plate(self.plate_b_voltage, -drive_voltage - bias_offset);
        let total_current = positive_current + negative_current;

        self.plate_a_voltage = plate_a;
        self.plate_b_voltage = plate_b;
        self.positive_current = positive_current;
        self.negative_current = negative_current;
        self.update_cathode_bias(total_current);
        self.update_supply(total_current, sag);

        let plate_a_signal = self.plate_a_voltage - self.reference_plate_a_voltage;
        let plate_b_signal = self.plate_b_voltage - self.reference_plate_b_voltage;
        (plate_b_signal - plate_a_signal) * self.params.output_scale * self.supply_ratio()
    }

    fn solve_plate(&self, previous_plate_voltage: f32, grid_drive: f32) -> (f32, f32) {
        let mut plate_voltage = previous_plate_voltage.clamp(1.0, self.supply_voltage);
        let pentode = self.pentode_point(plate_voltage, grid_drive);
        let residual = (self.supply_voltage - plate_voltage) / self.params.primary_half_resistance
            - pentode.current;
        let derivative = -1.0 / self.params.primary_half_resistance - pentode.d_current_d_plate;
        if derivative.abs() > 1e-12 {
            plate_voltage = (plate_voltage - residual / derivative).clamp(1.0, self.supply_voltage);
        }

        let current = self.pentode_point(plate_voltage, grid_drive).current;
        (plate_voltage, current)
    }

    fn pentode_point(&self, plate_voltage: f32, grid_drive: f32) -> PentodePoint {
        let plate_to_cathode = (plate_voltage - self.cathode_bias_voltage).max(0.0);
        let screen_to_cathode = (self.params.screen_voltage.min(self.supply_voltage)
            - self.cathode_bias_voltage)
            .max(0.0);
        let grid_to_cathode = grid_drive - self.cathode_bias_voltage;
        let control = softplus(grid_to_cathode + screen_to_cathode / 42.0, 0.65);
        let saturation = 1.0 - (-plate_to_cathode / 42.0).exp();
        let d_saturation_d_plate = (-plate_to_cathode / 42.0).exp() / 42.0;
        let screen_factor =
            (screen_to_cathode / self.params.screen_voltage.max(1.0)).clamp(0.0, 1.2);
        let shaped = self.params.current_gain * control.powf(1.32) * screen_factor
            / (1.0 + control * self.params.compression);

        PentodePoint {
            current: (shaped * saturation).clamp(0.0, 0.090),
            d_current_d_plate: (shaped * d_saturation_d_plate).max(0.0),
        }
    }

    fn update_supply(&mut self, total_current: f32, sag: f32) {
        let effective_current = total_current * (0.18 + sag.clamp(0.0, 1.0) * 1.35);
        let target =
            self.params.nominal_supply_voltage - effective_current * self.params.supply_resistance;
        let coefficient = 1.0
            - (-1.0
                / (self.params.sample_rate
                    * self.params.supply_resistance
                    * self.params.supply_capacitance))
                .exp();
        self.supply_voltage += coefficient * (target - self.supply_voltage);
        self.supply_voltage = self.supply_voltage.clamp(
            self.params.nominal_supply_voltage * 0.45,
            self.params.nominal_supply_voltage,
        );
    }

    fn update_cathode_bias(&mut self, total_current: f32) {
        let target = total_current * self.params.cathode_resistance;
        let coefficient = 1.0
            - (-1.0
                / (self.params.sample_rate
                    * self.params.cathode_resistance
                    * self.params.cathode_capacitance))
                .exp();
        self.cathode_bias_voltage += coefficient * (target - self.cathode_bias_voltage);
    }

    fn supply_ratio(&self) -> f32 {
        (self.supply_voltage / self.params.nominal_supply_voltage).clamp(0.45, 1.05)
    }
}

fn smooth_voltage(
    previous: f32,
    target: f32,
    sample_rate: f32,
    resistance: f32,
    capacitance: f32,
) -> f32 {
    let coefficient = 1.0 - (-1.0 / (sample_rate * resistance * capacitance)).exp();
    previous + coefficient * (target - previous)
}

struct OnePole {
    coefficient: f32,
    state: f32,
}

impl OnePole {
    fn new(sample_rate: f32, cutoff_hz: f32) -> Self {
        Self {
            coefficient: 1.0 - (-std::f32::consts::TAU * cutoff_hz / sample_rate).exp(),
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

fn softplus(value: f32, scale: f32) -> f32 {
    let normalized = value / scale;
    if normalized > 20.0 {
        value
    } else if normalized < -20.0 {
        0.0
    } else {
        scale * normalized.exp().ln_1p()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn stage() -> PushPullEl84Stage {
        PushPullEl84Stage::new(PushPullEl84Params {
            sample_rate: 48_000.0,
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
        })
    }

    fn transformer() -> OutputTransformerStage {
        OutputTransformerStage::new(OutputTransformerParams {
            sample_rate: 48_000.0,
            primary_resistance: 100_000.0,
            primary_inductance: 47.0,
            leakage_cutoff_hz: 13_000.0,
            core_saturation: 1_400.0,
            output_scale: 1.0,
        })
    }

    fn supply() -> SupplyNetwork {
        SupplyNetwork::new(SupplyNetworkParams {
            sample_rate: 48_000.0,
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
        })
    }

    #[test]
    fn silence_stays_centered_and_finite() {
        let mut stage = stage();
        for _ in 0..2048 {
            let output = stage.process(0.0, 0.5);
            assert!(output.is_finite());
            assert!(output.abs() < 1e-5, "output={output}");
        }
    }

    #[test]
    fn output_is_odd_symmetric_for_small_signal() {
        let mut positive = stage();
        let mut negative = stage();
        for _ in 0..1024 {
            positive.process(0.0, 0.0);
            negative.process(0.0, 0.0);
        }

        let up = positive.process(0.05, 0.0);
        let down = negative.process(-0.05, 0.0);

        assert!((up + down).abs() < up.abs() * 0.12, "up={up}, down={down}");
    }

    #[test]
    fn sustained_drive_drops_supply_voltage() {
        let mut stage = stage();
        let idle_supply = stage.operating_point().supply_voltage;
        for sample_idx in 0..48_000 {
            let input = (std::f32::consts::TAU * 110.0 * sample_idx as f32 / 48_000.0).sin() * 0.7;
            stage.process(input, 1.0);
        }

        let driven_supply = stage.operating_point().supply_voltage;
        assert!(
            driven_supply < idle_supply - 1.0,
            "idle_supply={idle_supply}, driven_supply={driven_supply}"
        );
    }

    #[test]
    fn cathode_bias_recovers_after_overload() {
        let mut stage = stage();
        for _ in 0..48_000 {
            stage.process(0.0, 0.5);
        }
        let idle_bias = stage.operating_point().cathode_bias_voltage;

        for sample_idx in 0..12_000 {
            let input = (std::f32::consts::TAU * 110.0 * sample_idx as f32 / 48_000.0).sin() * 1.4;
            stage.process(input, 0.5);
        }
        let overloaded_bias = stage.operating_point().cathode_bias_voltage;

        for _ in 0..48_000 {
            stage.process(0.0, 0.5);
        }
        let recovered_bias = stage.operating_point().cathode_bias_voltage;

        assert!(
            (recovered_bias - idle_bias).abs() < (overloaded_bias - idle_bias).abs(),
            "idle_bias={idle_bias}, overloaded_bias={overloaded_bias}, recovered_bias={recovered_bias}"
        );
    }

    #[test]
    fn processing_cost_stays_below_realtime_budget() {
        let mut stage = stage();
        let sample_count = 48_000;
        let start = Instant::now();
        let mut sum = 0.0;
        for sample_idx in 0..sample_count {
            let input = (std::f32::consts::TAU * 110.0 * sample_idx as f32 / 48_000.0).sin() * 0.7;
            sum += stage.process(input, 0.7);
        }
        let elapsed = start.elapsed();

        assert!(sum.is_finite());
        assert!(
            elapsed < Duration::from_millis(100),
            "elapsed={elapsed:?} for {sample_count} samples"
        );
    }

    #[test]
    fn transformer_blocks_dc() {
        let mut transformer = transformer();
        let mut sum = 0.0;
        for sample_idx in 0..48_000 {
            let output = transformer.process(0.5);
            if sample_idx >= 47_000 {
                sum += output.abs();
            }
        }

        assert!(sum / 1_000.0 < 0.01, "settled_dc={}", sum / 1_000.0);
    }

    #[test]
    fn transformer_rolls_off_leakage_highs() {
        let mut low = transformer();
        let mut high = transformer();
        let low_rms = transformer_sine_rms(&mut low, 1_000.0, 0.2);
        let high_rms = transformer_sine_rms(&mut high, 18_000.0, 0.2);

        assert!(
            low_rms > high_rms * 1.15,
            "low_rms={low_rms}, high_rms={high_rms}"
        );
    }

    #[test]
    fn transformer_core_flux_compresses_sustained_low_end() {
        let mut light = transformer();
        let mut heavy = transformer();
        let light_rms = transformer_sine_rms(&mut light, 80.0, 0.1);
        let heavy_rms = transformer_sine_rms(&mut heavy, 80.0, 1.0);
        let linear_ratio = heavy_rms / light_rms;

        assert!(
            linear_ratio < 9.4,
            "light_rms={light_rms}, heavy_rms={heavy_rms}, linear_ratio={linear_ratio}"
        );
    }

    #[test]
    fn transformer_reset_clears_flux_history() {
        let mut transformer = transformer();
        for sample_idx in 0..12_000 {
            let input = (std::f32::consts::TAU * 80.0 * sample_idx as f32 / 48_000.0).sin();
            transformer.process(input);
        }
        assert!(transformer.operating_point().core_flux.abs() > 0.0);
        transformer.reset();
        assert_eq!(transformer.operating_point().core_flux, 0.0);
    }

    #[test]
    fn supply_network_orders_rails_from_power_to_preamp() {
        let mut supply = supply();
        for _ in 0..48_000 {
            supply.process(0.003, 0.002, 0.080, 0.6);
        }
        let operating_point = supply.operating_point();

        assert!(operating_point.power_voltage > operating_point.phase_inverter_voltage);
        assert!(operating_point.phase_inverter_voltage > operating_point.preamp_voltage);
    }

    #[test]
    fn supply_network_sags_under_power_current() {
        let mut quiet = supply();
        let mut loud = supply();
        for _ in 0..48_000 {
            quiet.process(0.002, 0.001, 0.020, 1.0);
            loud.process(0.002, 0.001, 0.120, 1.0);
        }

        assert!(
            loud.operating_point().power_voltage < quiet.operating_point().power_voltage - 10.0,
            "quiet={:?}, loud={:?}",
            quiet.operating_point(),
            loud.operating_point()
        );
    }

    #[test]
    fn supply_network_recovers_after_overload() {
        let mut supply = supply();
        for _ in 0..48_000 {
            supply.process(0.003, 0.002, 0.140, 1.0);
        }
        let sagged = supply.operating_point().power_voltage;
        for _ in 0..96_000 {
            supply.process(0.001, 0.001, 0.020, 0.5);
        }
        let recovered = supply.operating_point().power_voltage;

        assert!(
            recovered > sagged + 10.0,
            "sagged={sagged}, recovered={recovered}"
        );
    }

    fn transformer_sine_rms(
        transformer: &mut OutputTransformerStage,
        frequency: f32,
        amplitude: f32,
    ) -> f32 {
        let mut sum = 0.0;
        let mut count = 0;
        for sample_idx in 0..48_000 {
            let input = (std::f32::consts::TAU * frequency * sample_idx as f32 / 48_000.0).sin()
                * amplitude;
            let output = transformer.process(input);
            if sample_idx >= 24_000 {
                sum += output * output;
                count += 1;
            }
        }
        (sum / count as f32).sqrt()
    }
}
