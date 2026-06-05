#[derive(Clone, Copy)]
pub struct BrightVolumeInputParams {
    pub sample_rate: f32,
    pub input_resistance: f32,
    pub input_coupling_capacitance: f32,
    pub bright_cutoff_hz: f32,
    pub bright_bypass_gain: f32,
}

pub struct BrightVolumeInputStage {
    params: BrightVolumeInputParams,
    input_lowpass: OnePole,
    bright_lowpass: OnePole,
}

#[derive(Clone, Copy)]
pub struct CutPresenceParams {
    pub sample_rate: f32,
    pub min_cutoff_hz: f32,
    pub max_cutoff_hz: f32,
    pub presence_gain: f32,
}

pub struct CutPresenceStage {
    params: CutPresenceParams,
    cut_lowpass: VariableOnePole,
}

impl BrightVolumeInputStage {
    pub fn new(params: BrightVolumeInputParams) -> Self {
        let input_cutoff = 1.0
            / (std::f32::consts::TAU * params.input_resistance * params.input_coupling_capacitance);
        Self {
            params,
            input_lowpass: OnePole::new(params.sample_rate, input_cutoff),
            bright_lowpass: OnePole::new(params.sample_rate, params.bright_cutoff_hz),
        }
    }

    pub fn reset(&mut self) {
        self.input_lowpass.reset();
        self.bright_lowpass.reset();
    }

    pub fn process(&mut self, input: f32, volume: f32) -> f32 {
        let coupled = input - self.input_lowpass.process(input);
        let volume = volume.clamp(0.0, 1.0);
        let volume_gain = volume * volume;
        let bright = coupled - self.bright_lowpass.process(coupled);

        coupled * volume_gain + bright * (1.0 - volume_gain) * self.params.bright_bypass_gain
    }
}

impl CutPresenceStage {
    pub fn new(params: CutPresenceParams) -> Self {
        Self {
            params,
            cut_lowpass: VariableOnePole::new(params.sample_rate, params.max_cutoff_hz),
        }
    }

    pub fn reset(&mut self) {
        self.cut_lowpass.reset();
    }

    pub fn process(&mut self, input: f32, cut: f32, presence: f32) -> f32 {
        let cut = cut.clamp(0.0, 1.0);
        let cutoff_hz = self.params.max_cutoff_hz * (1.0 - cut).powi(2) + self.params.min_cutoff_hz;
        self.cut_lowpass
            .set_cutoff(self.params.sample_rate, cutoff_hz);
        let cut_output = self.cut_lowpass.process(input);
        let presence = presence.clamp(0.0, 1.0);

        cut_output + (input - cut_output) * presence * self.params.presence_gain
    }
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

struct VariableOnePole {
    coefficient: f32,
    cutoff_hz: f32,
    state: f32,
}

impl VariableOnePole {
    fn new(sample_rate: f32, cutoff_hz: f32) -> Self {
        let mut filter = Self {
            coefficient: 0.0,
            cutoff_hz: f32::NAN,
            state: 0.0,
        };
        filter.set_cutoff(sample_rate, cutoff_hz);
        filter
    }

    fn set_cutoff(&mut self, sample_rate: f32, cutoff_hz: f32) {
        if cutoff_hz != self.cutoff_hz {
            self.coefficient = 1.0 - (-std::f32::consts::TAU * cutoff_hz / sample_rate).exp();
            self.cutoff_hz = cutoff_hz;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn stage() -> BrightVolumeInputStage {
        BrightVolumeInputStage::new(BrightVolumeInputParams {
            sample_rate: 48_000.0,
            input_resistance: 1_000_000.0,
            input_coupling_capacitance: 47e-9,
            bright_cutoff_hz: 2_900.0,
            bright_bypass_gain: 0.18,
        })
    }

    fn cut_presence() -> CutPresenceStage {
        CutPresenceStage::new(CutPresenceParams {
            sample_rate: 48_000.0,
            min_cutoff_hz: 1_150.0,
            max_cutoff_hz: 13_500.0,
            presence_gain: 0.35,
        })
    }

    #[test]
    fn input_coupling_blocks_dc() {
        let mut stage = stage();
        let mut sum = 0.0;
        for sample_idx in 0..96_000 {
            let output = stage.process(0.4, 1.0);
            if sample_idx >= 95_000 {
                sum += output.abs();
            }
        }

        assert!(sum / 1_000.0 < 0.01, "settled_dc={}", sum / 1_000.0);
    }

    #[test]
    fn volume_reduces_midband_level() {
        let mut open = stage();
        let mut low = stage();
        let open_rms = sine_rms(&mut open, 1_000.0, 0.1, 1.0);
        let low_rms = sine_rms(&mut low, 1_000.0, 0.1, 0.35);

        assert!(open_rms > low_rms * 5.0, "open={open_rms}, low={low_rms}");
    }

    #[test]
    fn bright_path_keeps_highs_when_volume_is_low() {
        let mut low_frequency = stage();
        let mut high_frequency = stage();
        let low_rms = sine_rms(&mut low_frequency, 300.0, 0.1, 0.15);
        let high_rms = sine_rms(&mut high_frequency, 5_000.0, 0.1, 0.15);

        assert!(
            high_rms > low_rms * 1.8,
            "low_rms={low_rms}, high_rms={high_rms}"
        );
    }

    #[test]
    fn reset_clears_filter_history() {
        let mut stage = stage();
        for _ in 0..24_000 {
            stage.process(0.3, 0.8);
        }
        stage.reset();
        let first = stage.process(0.0, 0.8);

        assert!(first.abs() < 1e-6, "first={first}");
    }

    #[test]
    fn cut_control_reduces_high_frequency_level() {
        let mut open = cut_presence();
        let mut cut = cut_presence();
        let open_rms = cut_presence_sine_rms(&mut open, 6_000.0, 0.2, 0.0, 0.0);
        let cut_rms = cut_presence_sine_rms(&mut cut, 6_000.0, 0.2, 1.0, 0.0);

        assert!(
            open_rms > cut_rms * 2.0,
            "open_rms={open_rms}, cut_rms={cut_rms}"
        );
    }

    #[test]
    fn presence_restores_some_high_frequency_level() {
        let mut dark = cut_presence();
        let mut present = cut_presence();
        let dark_rms = cut_presence_sine_rms(&mut dark, 6_000.0, 0.2, 1.0, 0.0);
        let present_rms = cut_presence_sine_rms(&mut present, 6_000.0, 0.2, 1.0, 1.0);

        assert!(
            present_rms > dark_rms * 1.15,
            "dark_rms={dark_rms}, present_rms={present_rms}"
        );
    }

    #[test]
    fn cut_presence_reset_clears_history() {
        let mut stage = cut_presence();
        for _ in 0..24_000 {
            stage.process(0.2, 0.8, 0.4);
        }
        stage.reset();
        let first = stage.process(0.0, 0.8, 0.4);

        assert!(first.abs() < 1e-6, "first={first}");
    }

    fn sine_rms(
        stage: &mut BrightVolumeInputStage,
        frequency: f32,
        amplitude: f32,
        volume: f32,
    ) -> f32 {
        let mut sum = 0.0;
        let mut count = 0;
        for sample_idx in 0..48_000 {
            let input = (std::f32::consts::TAU * frequency * sample_idx as f32 / 48_000.0).sin()
                * amplitude;
            let output = stage.process(input, volume);
            if sample_idx >= 24_000 {
                sum += output * output;
                count += 1;
            }
        }
        (sum / count as f32).sqrt()
    }

    fn cut_presence_sine_rms(
        stage: &mut CutPresenceStage,
        frequency: f32,
        amplitude: f32,
        cut: f32,
        presence: f32,
    ) -> f32 {
        let mut sum = 0.0;
        let mut count = 0;
        for sample_idx in 0..48_000 {
            let input = (std::f32::consts::TAU * frequency * sample_idx as f32 / 48_000.0).sin()
                * amplitude;
            let output = stage.process(input, cut, presence);
            if sample_idx >= 24_000 {
                sum += output * output;
                count += 1;
            }
        }
        (sum / count as f32).sqrt()
    }
}
