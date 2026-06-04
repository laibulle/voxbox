use rill_core_wdf::{filters::RcPole, WdfElement};

#[derive(Clone, Copy)]
pub struct AmpControls {
    pub gain: f32,
    pub bass: f32,
    pub tone: f32,
    pub cut: f32,
    pub master: f32,
}

/// Real-time graybox model of a JMI AC30/6 fitted with the OS/010 Top Boost unit.
///
/// The processing stages follow the reference topology while keeping the
/// nonlinear tube and transformer behavior deliberately compact.
pub struct VoxAmp {
    sample_rate: f32,
    input_coupling: WdfHighpass,
    first_cathode_bypass: WdfHighpass,
    recovery_cathode_bypass: WdfHighpass,
    bright_filter: OnePoleLowpass,
    tone_stack: TopBoostToneStack,
    phase_inverter_coupling: WdfHighpass,
    cut_filter: OnePoleLowpass,
    transformer_highpass: WdfHighpass,
    transformer_lowpass: OnePoleLowpass,
    bias_envelope: EnvelopeFollower,
    supply_sag: EnvelopeFollower,
}

impl VoxAmp {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            input_coupling: WdfHighpass::from_rc(sample_rate, 1_000_000.0, 47e-9),
            first_cathode_bypass: WdfHighpass::from_rc(sample_rate, 1_500.0, 25e-6),
            recovery_cathode_bypass: WdfHighpass::from_rc(sample_rate, 1_500.0, 25e-6),
            bright_filter: OnePoleLowpass::new(sample_rate, 2_900.0),
            tone_stack: TopBoostToneStack::new(sample_rate),
            phase_inverter_coupling: WdfHighpass::from_rc(sample_rate, 1_000_000.0, 47e-9),
            cut_filter: OnePoleLowpass::new(sample_rate, 12_000.0),
            transformer_highpass: WdfHighpass::from_rc(sample_rate, 100_000.0, 47e-9),
            transformer_lowpass: OnePoleLowpass::new(sample_rate, 14_000.0),
            bias_envelope: EnvelopeFollower::new(sample_rate, 0.004, 0.120),
            supply_sag: EnvelopeFollower::new(sample_rate, 0.012, 0.260),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.sample_rate);
    }

    #[inline]
    pub fn process(&mut self, input: f32, controls: AmpControls) -> f32 {
        let input = self.input_coupling.process(input);

        // OS/010 uses a 500k volume control with a 100pF bright capacitor.
        // At lower settings the capacitor bypasses more high-frequency signal.
        let volume = 0.025 + controls.gain * controls.gain * 0.975;
        let high = input - self.bright_filter.process(input);
        let volume_output = input * volume + high * (1.0 - volume) * 0.9;

        let first_bypass = self.first_cathode_bypass.process(volume_output);
        let first_drive = volume_output * (5.0 + controls.gain * 7.0) + first_bypass * 1.8;
        let first_stage = triode_stage(first_drive, 0.16);

        let toned = self
            .tone_stack
            .process(first_stage, controls.bass, controls.tone);
        let recovery_bypass = self.recovery_cathode_bypass.process(toned);
        let recovery = triode_stage(toned * 5.2 + recovery_bypass * 1.6, 0.12);

        // The long-tail pair produces opposed, slightly imbalanced outputs. The
        // Cut network sits across those outputs, before the EL84 grid couplers.
        let pi_input = self.phase_inverter_coupling.process(recovery);
        let phase_a = triode_stage(pi_input * 1.75, 0.045);
        let phase_b = triode_stage(-pi_input * 1.68, -0.035);
        let differential = (phase_a - phase_b) * 0.5;
        let cut_hz = 13_500.0 * (1.0 - controls.cut).powi(2) + 1_150.0;
        self.cut_filter.set_cutoff(self.sample_rate, cut_hz);
        let cut_output = self.cut_filter.process(differential);

        // Four hot cathode-biased EL84s behave as push-pull class AB, not ideal
        // class A. Average level raises cathode bias and sags the GZ34 supply.
        let level = cut_output.abs();
        let bias_shift = self.bias_envelope.process(level);
        let sag = self.supply_sag.process(level);
        let drive =
            cut_output * (2.3 + controls.gain * 1.6) / (1.0 + bias_shift * 1.8 + sag * 0.75);
        let positive_bank = el84_bank(drive - bias_shift * 0.055);
        let negative_bank = el84_bank(-drive - bias_shift * 0.045);
        let power_output = (positive_bank - negative_bank) * 0.72;

        let transformer = self.transformer_highpass.process(power_output);
        let transformer = self.transformer_lowpass.process(transformer);
        transformer * controls.master
    }
}

struct TopBoostToneStack {
    bass_split: OnePoleLowpass,
    treble_split: OnePoleLowpass,
}

impl TopBoostToneStack {
    fn new(sample_rate: f32) -> Self {
        Self {
            bass_split: OnePoleLowpass::new(sample_rate, 170.0),
            treble_split: OnePoleLowpass::new(sample_rate, 1_850.0),
        }
    }

    #[inline]
    fn process(&mut self, input: f32, bass: f32, treble: f32) -> f32 {
        let low = self.bass_split.process(input);
        let high = input - self.treble_split.process(input);
        let mid = input - low - high;
        let bass = bass * bass;
        let treble = treble * treble;

        // The OS/010 1M controls are strongly interactive. Raising both creates
        // the characteristic mid scoop; backing either down restores mids.
        let low_gain = 0.18 + bass * 1.45;
        let high_gain = 0.16 + treble * 1.85;
        let mid_gain = 0.62 - bass * treble * 0.42 + (1.0 - bass) * (1.0 - treble) * 0.18;
        low * low_gain + mid * mid_gain + high * high_gain
    }
}

struct EnvelopeFollower {
    attack: f32,
    release: f32,
    state: f32,
}

impl EnvelopeFollower {
    fn new(sample_rate: f32, attack_seconds: f32, release_seconds: f32) -> Self {
        Self {
            attack: (-1.0 / (sample_rate * attack_seconds)).exp(),
            release: (-1.0 / (sample_rate * release_seconds)).exp(),
            state: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let coefficient = if input > self.state {
            self.attack
        } else {
            self.release
        };
        self.state = input + coefficient * (self.state - input);
        self.state
    }
}

struct WdfHighpass {
    lowpass: RcPole<f32>,
}

impl WdfHighpass {
    fn from_rc(sample_rate: f32, resistance: f32, capacitance: f32) -> Self {
        let cutoff = 1.0 / (std::f32::consts::TAU * resistance * capacitance);
        let g = std::f32::consts::PI * cutoff / sample_rate;
        Self {
            lowpass: RcPole::new(g / (1.0 + g)),
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        input - self.lowpass.process_incident(input)
    }
}

#[inline]
fn triode_stage(input: f32, bias: f32) -> f32 {
    let biased = input + bias;
    (biased.tanh() - bias.tanh()) * 1.08
}

#[inline]
fn el84_bank(input: f32) -> f32 {
    let conducting = (input + 0.18).max(0.0);
    (conducting - 0.055 * conducting * conducting * conducting).tanh()
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

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        self.state += self.coefficient * (input - self.state);
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controls() -> AmpControls {
        AmpControls {
            gain: 0.5,
            bass: 0.5,
            tone: 0.5,
            cut: 0.5,
            master: 1.0,
        }
    }

    fn sine_rms(amp: &mut VoxAmp, frequency: f32, controls: AmpControls) -> f32 {
        let sample_rate = 48_000.0;
        let mut sum = 0.0;
        for sample_idx in 0..9_600 {
            let input =
                (std::f32::consts::TAU * frequency * sample_idx as f32 / sample_rate).sin() * 0.02;
            let output = amp.process(input, controls);
            if sample_idx >= 4_800 {
                sum += output * output;
            }
        }
        (sum / 4_800.0).sqrt()
    }

    #[test]
    fn silence_stays_silent() {
        let mut amp = VoxAmp::new(48_000.0);
        for _ in 0..1024 {
            assert!(amp.process(0.0, controls()).abs() < 1e-6);
        }
    }

    #[test]
    fn output_is_finite_under_extreme_input() {
        let mut amp = VoxAmp::new(48_000.0);
        let mut controls = controls();
        controls.gain = 1.0;
        controls.bass = 1.0;
        controls.tone = 1.0;
        controls.cut = 0.0;
        controls.master = 2.0;

        for sample in [0.0, 1.0, -1.0, 100.0, -100.0]
            .into_iter()
            .cycle()
            .take(4096)
        {
            assert!(amp.process(sample, controls).is_finite());
        }
    }

    #[test]
    fn bass_control_changes_low_frequency_response() {
        let mut low_bass = controls();
        low_bass.bass = 0.0;
        let mut high_bass = low_bass;
        high_bass.bass = 1.0;

        let low = sine_rms(&mut VoxAmp::new(48_000.0), 90.0, low_bass);
        let high = sine_rms(&mut VoxAmp::new(48_000.0), 90.0, high_bass);
        assert!(high > low * 1.2);
    }

    #[test]
    fn cut_control_reduces_high_frequency_response() {
        let mut open = controls();
        open.cut = 0.0;
        let mut cut = open;
        cut.cut = 1.0;

        let open_level = sine_rms(&mut VoxAmp::new(48_000.0), 5_000.0, open);
        let cut_level = sine_rms(&mut VoxAmp::new(48_000.0), 5_000.0, cut);
        assert!(open_level > cut_level * 1.4);
    }

    #[test]
    fn treble_control_changes_high_frequency_response() {
        let mut low_treble = controls();
        low_treble.gain = 0.1;
        low_treble.tone = 0.0;
        let mut high_treble = low_treble;
        high_treble.tone = 1.0;

        let low = sine_rms(&mut VoxAmp::new(48_000.0), 4_000.0, low_treble);
        let high = sine_rms(&mut VoxAmp::new(48_000.0), 4_000.0, high_treble);
        assert!(high > low * 1.2);
    }
}
