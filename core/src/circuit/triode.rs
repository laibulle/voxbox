const NEWTON_ITERATIONS: usize = 12;
const NEWTON_TOLERANCE: f32 = 1e-5;

#[derive(Clone, Copy)]
pub struct TriodeParams {
    pub mu: f32,
    pub ex: f32,
    pub kg1: f32,
    pub kp: f32,
    pub kvb: f32,
}

impl TriodeParams {
    pub const ECC83: Self = Self {
        mu: 100.0,
        ex: 1.4,
        kg1: 1060.0,
        kp: 600.0,
        kvb: 300.0,
    };
}

#[derive(Clone, Copy)]
pub struct CommonCathodeParams {
    pub sample_rate: f32,
    pub plate_resistance: f32,
    pub cathode_resistance: f32,
    pub supply_resistance: f32,
    pub supply_capacitance: f32,
    pub nominal_supply_voltage: f32,
    pub input_gain: f32,
    pub output_scale: f32,
    pub triode: TriodeParams,
}

pub struct CommonCathodeStage {
    params: CommonCathodeParams,
    supply_voltage: f32,
    last_plate_voltage: f32,
    last_cathode_voltage: f32,
}

impl CommonCathodeStage {
    pub fn new(params: CommonCathodeParams) -> Self {
        let quiescent_plate = params.nominal_supply_voltage * 0.62;
        Self {
            params,
            supply_voltage: params.nominal_supply_voltage,
            last_plate_voltage: quiescent_plate,
            last_cathode_voltage: 1.5,
        }
    }

    pub fn reset(&mut self) {
        self.supply_voltage = self.params.nominal_supply_voltage;
        self.last_plate_voltage = self.params.nominal_supply_voltage * 0.62;
        self.last_cathode_voltage = 1.5;
    }

    pub fn supply_voltage(&self) -> f32 {
        self.supply_voltage
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let grid_voltage = input * self.params.input_gain;
        let (plate_voltage, cathode_voltage, plate_current) =
            self.solve_operating_point(grid_voltage);

        self.update_supply(plate_current);
        self.last_plate_voltage = plate_voltage;
        self.last_cathode_voltage = cathode_voltage;

        let centered_plate = plate_voltage - self.supply_voltage * 0.62;
        -centered_plate * self.params.output_scale
    }

    fn solve_operating_point(&self, grid_voltage: f32) -> (f32, f32, f32) {
        let mut plate_voltage = self
            .last_plate_voltage
            .clamp(1.0, self.supply_voltage.max(1.0));
        let mut cathode_voltage = self
            .last_cathode_voltage
            .clamp(0.0, self.supply_voltage.max(1.0));

        for _ in 0..NEWTON_ITERATIONS {
            let plate_current = self.triode_current(plate_voltage, grid_voltage, cathode_voltage);
            let cathode_current = cathode_voltage / self.params.cathode_resistance;

            let f_plate = (self.supply_voltage - plate_voltage) / self.params.plate_resistance
                - plate_current;
            let f_cathode = plate_current - cathode_current;

            if f_plate.abs().max(f_cathode.abs()) < NEWTON_TOLERANCE {
                break;
            }

            let dv = 0.05;
            let di_dplate =
                (self.triode_current(plate_voltage + dv, grid_voltage, cathode_voltage)
                    - self.triode_current(plate_voltage - dv, grid_voltage, cathode_voltage))
                    / (2.0 * dv);
            let di_dcathode =
                (self.triode_current(plate_voltage, grid_voltage, cathode_voltage + dv)
                    - self.triode_current(plate_voltage, grid_voltage, cathode_voltage - dv))
                    / (2.0 * dv);

            let j11 = -1.0 / self.params.plate_resistance - di_dplate;
            let j12 = -di_dcathode;
            let j21 = di_dplate;
            let j22 = di_dcathode - 1.0 / self.params.cathode_resistance;
            let determinant = j11 * j22 - j12 * j21;
            if determinant.abs() < 1e-12 {
                break;
            }

            let delta_plate = (-f_plate * j22 + j12 * f_cathode) / determinant;
            let delta_cathode = (j21 * f_plate - j11 * f_cathode) / determinant;

            plate_voltage = (plate_voltage + delta_plate).clamp(1.0, self.supply_voltage.max(1.0));
            cathode_voltage = (cathode_voltage + delta_cathode).clamp(0.0, self.supply_voltage);
        }

        let plate_current = self.triode_current(plate_voltage, grid_voltage, cathode_voltage);
        (plate_voltage, cathode_voltage, plate_current)
    }

    fn triode_current(&self, plate_voltage: f32, grid_voltage: f32, cathode_voltage: f32) -> f32 {
        let plate_to_cathode = (plate_voltage - cathode_voltage).max(0.0);
        let grid_to_cathode = grid_voltage - cathode_voltage;
        let params = self.params.triode;

        let shaping = (params.kp * (1.0 / params.mu + grid_to_cathode / plate_to_cathode.max(1.0)))
            .exp()
            .ln_1p()
            / params.kp;
        let knee = (plate_to_cathode / params.kvb).sqrt();
        let conduction = (plate_to_cathode / params.kg1) * shaping.max(0.0).powf(params.ex) * knee;
        conduction.clamp(0.0, 0.040)
    }

    fn update_supply(&mut self, plate_current: f32) {
        let target =
            self.params.nominal_supply_voltage - plate_current * self.params.supply_resistance;
        let coefficient = 1.0
            - (-1.0
                / (self.params.sample_rate
                    * self.params.supply_resistance
                    * self.params.supply_capacitance))
                .exp();
        self.supply_voltage += coefficient * (target - self.supply_voltage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage() -> CommonCathodeStage {
        CommonCathodeStage::new(CommonCathodeParams {
            sample_rate: 48_000.0,
            plate_resistance: 100_000.0,
            cathode_resistance: 1_500.0,
            supply_resistance: 10_000.0,
            supply_capacitance: 22e-6,
            nominal_supply_voltage: 280.0,
            input_gain: 1.8,
            output_scale: 0.018,
            triode: TriodeParams::ECC83,
        })
    }

    #[test]
    fn silence_converges_to_finite_operating_point() {
        let mut stage = stage();
        for _ in 0..1024 {
            assert!(stage.process(0.0).is_finite());
        }
        assert!(stage.supply_voltage().is_finite());
    }

    #[test]
    fn sustained_drive_drops_supply_voltage() {
        let mut stage = stage();
        for _ in 0..2048 {
            stage.process(0.0);
        }
        let idle_supply = stage.supply_voltage();

        for sample_idx in 0..24_000 {
            let input = (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 48_000.0).sin() * 0.8;
            stage.process(input);
        }

        assert!(
            stage.supply_voltage() < idle_supply - 0.05,
            "idle={}, driven={}",
            idle_supply,
            stage.supply_voltage()
        );
    }

    #[test]
    fn louder_input_changes_operating_point_more() {
        let mut quiet = stage();
        let mut loud = stage();

        for sample_idx in 0..12_000 {
            let phase = std::f32::consts::TAU * 110.0 * sample_idx as f32 / 48_000.0;
            quiet.process(phase.sin() * 0.1);
            loud.process(phase.sin() * 0.8);
        }

        assert!(loud.supply_voltage() < quiet.supply_voltage());
    }
}
