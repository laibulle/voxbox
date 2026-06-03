use rill_core_wdf::{Capacitor, Resistor, SeriesAdapter, WdfElement};

#[derive(Clone, Copy)]
pub struct AmpControls {
    pub gain: f32,
    pub cut: f32,
    pub tone: f32,
    pub master: f32,
}

/// Graybox model of a bright, cathode-biased British combo.
///
/// The WDF networks model coupling/cathode RC behavior. The tube stages are
/// deliberately behavioral: asymmetric preamp saturation followed by a softer,
/// symmetric power-stage transfer.
pub struct VoxAmp {
    sample_rate: f32,
    input_coupling: SeriesAdapter<f32, Resistor<f32>, Capacitor<f32>>,
    cathode_bypass: SeriesAdapter<f32, Resistor<f32>, Capacitor<f32>>,
    dc_block_x: f32,
    dc_block_y: f32,
    cab_lowpass: OnePoleLowpass,
}

impl VoxAmp {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            input_coupling: coupling_network(sample_rate, 68_000.0, 22e-9),
            cathode_bypass: coupling_network(sample_rate, 1_500.0, 22e-6),
            dc_block_x: 0.0,
            dc_block_y: 0.0,
            cab_lowpass: OnePoleLowpass::new(sample_rate, 6_000.0),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.sample_rate);
    }

    pub fn process(&mut self, input: f32, controls: AmpControls) -> f32 {
        let input = self.input_coupling.process_sample(input);

        // The bypass branch increases stage gain above its RC corner.
        let bypass = self.cathode_bypass.process_sample(input);
        let drive = 1.0 + controls.gain * controls.gain * 24.0;
        let preamp = asymmetric_tube(input * drive + bypass * controls.gain * 0.8);

        // A compact proxy for the AC30 tone network: "tone" restores upper
        // mids before the phase inverter, while "cut" damps the power amp.
        let bright = preamp + (preamp - self.cab_lowpass.state) * controls.tone * 0.35;
        let power_drive = bright * (1.4 + controls.gain * 2.2);
        let power_amp = (power_drive - 0.08 * power_drive.powi(3)).tanh();

        let cutoff = 7_500.0 - controls.cut * 5_800.0;
        self.cab_lowpass.set_cutoff(self.sample_rate, cutoff);
        let speaker = self.cab_lowpass.process(power_amp);

        // Output transformer/coupling capacitor proxy.
        let dc_blocked = speaker - self.dc_block_x + 0.995 * self.dc_block_y;
        self.dc_block_x = speaker;
        self.dc_block_y = dc_blocked;

        dc_blocked * controls.master
    }
}

fn coupling_network(
    sample_rate: f32,
    resistance: f32,
    capacitance: f32,
) -> SeriesAdapter<f32, Resistor<f32>, Capacitor<f32>> {
    SeriesAdapter::new(
        Resistor::new(resistance),
        Capacitor::new(capacitance, sample_rate),
    )
}

fn asymmetric_tube(x: f32) -> f32 {
    let biased = x + 0.18;
    (biased.tanh() - 0.18_f32.tanh()) * 1.15
}

struct OnePoleLowpass {
    coefficient: f32,
    state: f32,
}

impl OnePoleLowpass {
    fn new(sample_rate: f32, cutoff: f32) -> Self {
        let mut filter = Self {
            coefficient: 0.0,
            state: 0.0,
        };
        filter.set_cutoff(sample_rate, cutoff);
        filter
    }

    fn set_cutoff(&mut self, sample_rate: f32, cutoff: f32) {
        self.coefficient = 1.0 - (-std::f32::consts::TAU * cutoff / sample_rate).exp();
    }

    fn process(&mut self, input: f32) -> f32 {
        self.state += self.coefficient * (input - self.state);
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_stays_silent() {
        let mut amp = VoxAmp::new(48_000.0);
        let controls = AmpControls {
            gain: 0.5,
            cut: 0.5,
            tone: 0.5,
            master: 1.0,
        };

        for _ in 0..1024 {
            assert!(amp.process(0.0, controls).abs() < 1e-6);
        }
    }

    #[test]
    fn output_is_finite_under_extreme_input() {
        let mut amp = VoxAmp::new(48_000.0);
        let controls = AmpControls {
            gain: 1.0,
            cut: 0.0,
            tone: 1.0,
            master: 2.0,
        };

        for sample in [0.0, 1.0, -1.0, 100.0, -100.0].into_iter().cycle().take(4096) {
            assert!(amp.process(sample, controls).is_finite());
        }
    }
}

