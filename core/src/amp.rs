use rill_core_wdf::{filters::RcPole, WdfElement};

pub const AMP_LATENCY: usize = 16;
const OVERSAMPLING_FACTOR: f32 = 2.0;
const HALF_BAND_TAPS: usize = 33;
const TONE_STACK_NODES: usize = 7;

#[derive(Clone, Copy)]
pub struct AmpControls {
    pub volume: f32,
    pub bass: f32,
    pub treble: f32,
    pub cut: f32,
    pub output: f32,
}

/// Real-time graybox model of a JMI AC30/6 fitted with the OS/010 Top Boost unit.
///
/// The processing stages follow the reference topology while keeping the
/// nonlinear tube and transformer behavior deliberately compact. The complete
/// amp runs at 2x sample rate to reduce aliasing from every nonlinear stage.
pub struct VoxAmp {
    upsampler: FirFilter,
    core: AmpCore,
    downsampler: FirFilter,
}

impl VoxAmp {
    pub fn new(sample_rate: f32) -> Self {
        let coefficients = half_band_coefficients();
        Self {
            upsampler: FirFilter::new(coefficients),
            core: AmpCore::new(sample_rate * OVERSAMPLING_FACTOR),
            downsampler: FirFilter::new(coefficients),
        }
    }

    pub fn reset(&mut self) {
        self.upsampler.reset();
        self.core.reset();
        self.downsampler.reset();
    }

    #[inline]
    pub fn process(&mut self, input: f32, controls: AmpControls) -> f32 {
        let upsampled = self.upsampler.process(input * OVERSAMPLING_FACTOR);
        let output = self
            .downsampler
            .process(self.core.process(upsampled, controls));

        let upsampled = self.upsampler.process(0.0);
        self.downsampler
            .process(self.core.process(upsampled, controls));
        output
    }
}

struct AmpCore {
    sample_rate: f32,
    input_coupling: WdfHighpass,
    first_cathode_bypass: WdfHighpass,
    bright_filter: OnePoleLowpass,
    tone_stack: TopBoostToneStack,
    phase_inverter_coupling: WdfHighpass,
    cut_filter: OnePoleLowpass,
    transformer_highpass: WdfHighpass,
    transformer_lowpass: OnePoleLowpass,
    bias_envelope: EnvelopeFollower,
    supply_sag: EnvelopeFollower,
}

impl AmpCore {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            input_coupling: WdfHighpass::from_rc(sample_rate, 1_000_000.0, 47e-9),
            first_cathode_bypass: WdfHighpass::from_rc(sample_rate, 1_500.0, 25e-6),
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

    fn reset(&mut self) {
        *self = Self::new(self.sample_rate);
    }

    #[inline]
    fn process(&mut self, input: f32, controls: AmpControls) -> f32 {
        let input = self.input_coupling.process(input);

        // OS/010 uses a 500k volume control with a 100pF bright capacitor.
        // At lower settings the capacitor bypasses more high-frequency signal.
        let volume = controls.volume * controls.volume;
        let high = input - self.bright_filter.process(input);
        let volume_output = input * volume + high * (1.0 - volume) * 0.18;

        let first_bypass = self.first_cathode_bypass.process(volume_output);
        let first_drive = volume_output * 4.8 + first_bypass * 0.8;
        let first_stage = triode_stage(first_drive, 0.16);

        // The second OS/010 triode is a cathode follower. It adds a small
        // amount of asymmetry but primarily provides the low source impedance
        // needed to drive the passive tone network without smearing attacks.
        let cathode_follower = cathode_follower(first_stage);
        let toned = self
            .tone_stack
            .process(cathode_follower, controls.bass, controls.treble);

        // The long-tail pair produces opposed, slightly imbalanced outputs. The
        // Cut network sits across those outputs, before the EL84 grid couplers.
        let pi_input = self.phase_inverter_coupling.process(toned * 4.8);
        let phase_a = triode_stage(pi_input * 1.38, 0.045);
        let phase_b = triode_stage(-pi_input * 1.32, -0.035);
        let differential = (phase_a - phase_b) * 0.5;
        let cut_hz = 13_500.0 * (1.0 - controls.cut).powi(2) + 1_150.0;
        self.cut_filter.set_cutoff(self.sample_rate, cut_hz);
        let cut_output = self.cut_filter.process(differential);

        // Four hot cathode-biased EL84s behave as push-pull class AB, not ideal
        // class A. Bias shift and supply sag mainly appear when the output stage
        // is driven beyond its clean-current region.
        let current_demand = (cut_output.abs() * 1.60 - 0.62).max(0.0);
        let bias_shift = self.bias_envelope.process(current_demand);
        let sag = self.supply_sag.process(current_demand * current_demand);
        let drive = cut_output * 1.60 / (1.0 + bias_shift * 0.55 + sag * 0.22);
        let positive_bank = el84_bank(drive - bias_shift * 0.055);
        let negative_bank = el84_bank(-drive - bias_shift * 0.045);
        let power_output = (positive_bank - negative_bank) * 0.72;

        let transformer = self.transformer_highpass.process(power_output);
        let transformer = self.transformer_lowpass.process(transformer);
        transformer * controls.output
    }
}

struct FirFilter {
    coefficients: [f32; HALF_BAND_TAPS],
    history: [f32; HALF_BAND_TAPS],
    position: usize,
}

impl FirFilter {
    fn new(coefficients: [f32; HALF_BAND_TAPS]) -> Self {
        Self {
            coefficients,
            history: [0.0; HALF_BAND_TAPS],
            position: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        self.history[self.position] = input;
        let mut output = 0.0;
        let mut history_position = self.position;
        for coefficient in self.coefficients {
            output += coefficient * self.history[history_position];
            history_position = if history_position == 0 {
                HALF_BAND_TAPS - 1
            } else {
                history_position - 1
            };
        }
        self.position = (self.position + 1) % HALF_BAND_TAPS;
        output
    }

    fn reset(&mut self) {
        self.history.fill(0.0);
        self.position = 0;
    }
}

fn half_band_coefficients() -> [f32; HALF_BAND_TAPS] {
    let center = (HALF_BAND_TAPS - 1) as f32 * 0.5;
    let mut coefficients = [0.0; HALF_BAND_TAPS];
    let mut sum = 0.0;
    for (index, coefficient) in coefficients.iter_mut().enumerate() {
        let offset = index as f32 - center;
        let sinc = if offset == 0.0 {
            0.5
        } else {
            (std::f32::consts::PI * offset * 0.5).sin() / (std::f32::consts::PI * offset)
        };
        let phase = std::f32::consts::TAU * index as f32 / (HALF_BAND_TAPS - 1) as f32;
        let blackman = 0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos();
        *coefficient = sinc * blackman;
        sum += *coefficient;
    }
    for coefficient in &mut coefficients {
        *coefficient /= sum;
    }
    coefficients
}

struct TopBoostToneStack {
    treble_capacitor: TrapezoidalCapacitor,
    bass_coupling_capacitor: TrapezoidalCapacitor,
    bass_capacitor: TrapezoidalCapacitor,
    inverse_matrix: [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
    bass: f32,
    treble: f32,
}

impl TopBoostToneStack {
    fn new(sample_rate: f32) -> Self {
        Self {
            treble_capacitor: TrapezoidalCapacitor::new(50e-12, sample_rate),
            bass_coupling_capacitor: TrapezoidalCapacitor::new(22e-9, sample_rate),
            bass_capacitor: TrapezoidalCapacitor::new(22e-9, sample_rate),
            inverse_matrix: [[0.0; TONE_STACK_NODES]; TONE_STACK_NODES],
            bass: f32::NAN,
            treble: f32::NAN,
        }
    }

    #[inline]
    fn process(&mut self, input: f32, bass: f32, treble: f32) -> f32 {
        const SOURCE: usize = 0;
        const TREBLE_TOP: usize = 1;
        const TREBLE_BOTTOM: usize = 2;
        const SLOPE_NODE: usize = 3;
        const BASS_WIPER: usize = 4;
        const OUTPUT: usize = 6;

        if bass != self.bass || treble != self.treble {
            self.update_matrix(bass, treble);
        }
        let mut rhs = [0.0; TONE_STACK_NODES];

        // The second OS/010 triode is a cathode follower. OS/010 shows a 220k
        // load at OUT.
        stamp_source_rhs(&mut rhs, SOURCE, 820.0, input);
        self.treble_capacitor
            .stamp_rhs(&mut rhs, SOURCE, TREBLE_TOP);
        self.bass_coupling_capacitor
            .stamp_rhs(&mut rhs, TREBLE_BOTTOM, SLOPE_NODE);
        self.bass_capacitor
            .stamp_rhs(&mut rhs, SLOPE_NODE, BASS_WIPER);

        let voltages = multiply_tone_stack(self.inverse_matrix, rhs);
        self.treble_capacitor
            .update(voltages[SOURCE], voltages[TREBLE_TOP]);
        self.bass_coupling_capacitor
            .update(voltages[TREBLE_BOTTOM], voltages[SLOPE_NODE]);
        self.bass_capacitor
            .update(voltages[SLOPE_NODE], voltages[BASS_WIPER]);
        voltages[OUTPUT]
    }

    fn update_matrix(&mut self, bass: f32, treble: f32) {
        const SOURCE: usize = 0;
        const TREBLE_TOP: usize = 1;
        const TREBLE_BOTTOM: usize = 2;
        const SLOPE_NODE: usize = 3;
        const BASS_WIPER: usize = 4;
        const BASS_BOTTOM: usize = 5;
        const OUTPUT: usize = 6;
        const POT_OHMS: f32 = 1_000_000.0;

        let mut matrix = [[0.0; TONE_STACK_NODES]; TONE_STACK_NODES];
        stamp_resistor_to_ground(&mut matrix, SOURCE, 820.0);
        stamp_resistor_to_ground(&mut matrix, OUTPUT, 220_000.0);

        let treble_position = 1.0 - audio_taper(treble);
        stamp_resistor(
            &mut matrix,
            TREBLE_TOP,
            OUTPUT,
            pot_segment(POT_OHMS * treble_position),
        );
        stamp_resistor(
            &mut matrix,
            OUTPUT,
            TREBLE_BOTTOM,
            pot_segment(POT_OHMS * (1.0 - treble_position)),
        );

        // The OS/010 Bass pot is wired in the opposite direction to the
        // Treble pot: clockwise rotation moves the wiper toward C3.
        let bass_position = 1.0 - audio_taper(bass);
        stamp_resistor(
            &mut matrix,
            TREBLE_BOTTOM,
            BASS_WIPER,
            pot_segment(POT_OHMS * bass_position),
        );
        stamp_resistor(
            &mut matrix,
            BASS_WIPER,
            BASS_BOTTOM,
            pot_segment(POT_OHMS * (1.0 - bass_position)),
        );
        stamp_resistor_to_ground(&mut matrix, SLOPE_NODE, 100_000.0);
        stamp_resistor_to_ground(&mut matrix, BASS_BOTTOM, 10_000.0);

        self.treble_capacitor
            .stamp_conductance(&mut matrix, SOURCE, TREBLE_TOP);
        self.bass_coupling_capacitor
            .stamp_conductance(&mut matrix, TREBLE_BOTTOM, SLOPE_NODE);
        self.bass_capacitor
            .stamp_conductance(&mut matrix, SLOPE_NODE, BASS_WIPER);

        self.inverse_matrix = invert_tone_stack(matrix);
        self.bass = bass;
        self.treble = treble;
    }
}

struct TrapezoidalCapacitor {
    conductance: f32,
    previous_voltage: f32,
    previous_current: f32,
}

impl TrapezoidalCapacitor {
    fn new(capacitance: f32, sample_rate: f32) -> Self {
        Self {
            conductance: 2.0 * capacitance * sample_rate,
            previous_voltage: 0.0,
            previous_current: 0.0,
        }
    }

    fn stamp_conductance(
        &self,
        matrix: &mut [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
        a: usize,
        b: usize,
    ) {
        stamp_conductance(matrix, a, b, self.conductance);
    }

    fn stamp_rhs(&self, rhs: &mut [f32; TONE_STACK_NODES], a: usize, b: usize) {
        let history_current = -self.conductance * self.previous_voltage - self.previous_current;
        rhs[a] -= history_current;
        rhs[b] += history_current;
    }

    fn update(&mut self, a: f32, b: f32) {
        let voltage = a - b;
        self.previous_current =
            self.conductance * (voltage - self.previous_voltage) - self.previous_current;
        self.previous_voltage = voltage;
    }
}

fn audio_taper(position: f32) -> f32 {
    10.0_f32.powf(2.0 * position.clamp(0.0, 1.0) - 2.0)
}

fn pot_segment(resistance: f32) -> f32 {
    resistance.max(1.0)
}

fn stamp_resistor(
    matrix: &mut [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
    a: usize,
    b: usize,
    resistance: f32,
) {
    stamp_conductance(matrix, a, b, 1.0 / resistance);
}

fn stamp_conductance(
    matrix: &mut [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
    a: usize,
    b: usize,
    conductance: f32,
) {
    matrix[a][a] += conductance;
    matrix[b][b] += conductance;
    matrix[a][b] -= conductance;
    matrix[b][a] -= conductance;
}

fn stamp_resistor_to_ground(
    matrix: &mut [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
    node: usize,
    resistance: f32,
) {
    matrix[node][node] += 1.0 / resistance;
}

fn stamp_source_rhs(rhs: &mut [f32; TONE_STACK_NODES], node: usize, resistance: f32, source: f32) {
    rhs[node] += source / resistance;
}

fn solve_tone_stack(
    mut matrix: [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
    mut rhs: [f32; TONE_STACK_NODES],
) -> [f32; TONE_STACK_NODES] {
    for pivot in 0..TONE_STACK_NODES {
        let mut best_row = pivot;
        for row in pivot + 1..TONE_STACK_NODES {
            if matrix[row][pivot].abs() > matrix[best_row][pivot].abs() {
                best_row = row;
            }
        }
        if best_row != pivot {
            matrix.swap(pivot, best_row);
            rhs.swap(pivot, best_row);
        }

        let inverse_pivot = 1.0 / matrix[pivot][pivot];
        for value in &mut matrix[pivot][pivot..] {
            *value *= inverse_pivot;
        }
        rhs[pivot] *= inverse_pivot;
        let pivot_row = matrix[pivot];

        for row in 0..TONE_STACK_NODES {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            for (value, pivot_value) in matrix[row][pivot..].iter_mut().zip(&pivot_row[pivot..]) {
                *value -= factor * pivot_value;
            }
            rhs[row] -= factor * rhs[pivot];
        }
    }
    rhs
}

fn invert_tone_stack(
    matrix: [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
) -> [[f32; TONE_STACK_NODES]; TONE_STACK_NODES] {
    let mut inverse = [[0.0; TONE_STACK_NODES]; TONE_STACK_NODES];
    for column in 0..TONE_STACK_NODES {
        let mut basis = [0.0; TONE_STACK_NODES];
        basis[column] = 1.0;
        let solution = solve_tone_stack(matrix, basis);
        for (row, value) in solution.into_iter().enumerate() {
            inverse[row][column] = value;
        }
    }
    inverse
}

fn multiply_tone_stack(
    matrix: [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
    vector: [f32; TONE_STACK_NODES],
) -> [f32; TONE_STACK_NODES] {
    matrix.map(|row| row.into_iter().zip(vector).map(|(a, b)| a * b).sum())
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
fn cathode_follower(input: f32) -> f32 {
    input * 0.96 + input * input * 0.018
}

#[inline]
fn el84_bank(input: f32) -> f32 {
    let conducting = (input + 0.18).max(0.0);
    (conducting - 0.055 * conducting * conducting * conducting).tanh()
}

struct OnePoleLowpass {
    coefficient: f32,
    cutoff: f32,
    state: f32,
}

impl OnePoleLowpass {
    fn new(sample_rate: f32, cutoff: f32) -> Self {
        let mut filter = Self {
            coefficient: 0.0,
            cutoff: f32::NAN,
            state: 0.0,
        };
        filter.set_cutoff(sample_rate, cutoff);
        filter
    }

    fn set_cutoff(&mut self, sample_rate: f32, cutoff: f32) {
        if cutoff != self.cutoff {
            self.coefficient = 1.0 - (-std::f32::consts::TAU * cutoff / sample_rate).exp();
            self.cutoff = cutoff;
        }
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
            volume: 0.5,
            bass: 0.5,
            treble: 0.5,
            cut: 0.5,
            output: 1.0,
        }
    }

    fn sine_rms_at(amp: &mut VoxAmp, frequency: f32, amplitude: f32, controls: AmpControls) -> f32 {
        let sample_rate = 48_000.0;
        let mut sum = 0.0;
        for sample_idx in 0..9_600 {
            let input = (std::f32::consts::TAU * frequency * sample_idx as f32 / sample_rate).sin()
                * amplitude;
            let output = amp.process(input, controls);
            if sample_idx >= 4_800 {
                sum += output * output;
            }
        }
        (sum / 4_800.0).sqrt()
    }

    fn sine_rms(amp: &mut VoxAmp, frequency: f32, controls: AmpControls) -> f32 {
        sine_rms_at(amp, frequency, 0.02, controls)
    }

    fn tone_stack_rms(frequency: f32, bass: f32, treble: f32) -> f32 {
        let sample_rate = 96_000.0;
        let mut stack = TopBoostToneStack::new(sample_rate);
        let mut sum = 0.0;
        for sample_idx in 0..19_200 {
            let input = (std::f32::consts::TAU * frequency * sample_idx as f32 / sample_rate).sin();
            let output = stack.process(input, bass, treble);
            if sample_idx >= 9_600 {
                sum += output * output;
            }
        }
        (sum / 9_600.0).sqrt()
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
        controls.volume = 1.0;
        controls.bass = 1.0;
        controls.treble = 1.0;
        controls.cut = 0.0;
        controls.output = 2.0;

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

        let low = sine_rms(&mut VoxAmp::new(48_000.0), 250.0, low_bass);
        let high = sine_rms(&mut VoxAmp::new(48_000.0), 250.0, high_bass);
        assert!(
            high > low * 1.2,
            "low-bass level={low}, high-bass level={high}"
        );
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
        low_treble.volume = 0.1;
        low_treble.treble = 0.0;
        let mut high_treble = low_treble;
        high_treble.treble = 1.0;

        let low = sine_rms(&mut VoxAmp::new(48_000.0), 4_000.0, low_treble);
        let high = sine_rms(&mut VoxAmp::new(48_000.0), 4_000.0, high_treble);
        assert!(high > low * 1.2);
    }

    #[test]
    fn tone_stack_is_passive() {
        for frequency in [80.0, 500.0, 2_000.0, 8_000.0] {
            for bass in [0.0, 0.5, 1.0] {
                for treble in [0.0, 0.5, 1.0] {
                    assert!(tone_stack_rms(frequency, bass, treble) <= 1.0 / 2.0_f32.sqrt());
                }
            }
        }
    }

    #[test]
    fn tone_stack_blocks_dc() {
        let mut stack = TopBoostToneStack::new(96_000.0);
        let mut output = 0.0;
        for _ in 0..96_000 {
            output = stack.process(1.0, 0.5, 0.5);
        }
        assert!(output.abs() < 1e-3);
    }

    #[test]
    fn tone_stack_controls_are_interactive() {
        let bass_effect_with_low_treble =
            tone_stack_rms(90.0, 1.0, 0.0) / tone_stack_rms(90.0, 0.0, 0.0);
        let bass_effect_with_high_treble =
            tone_stack_rms(90.0, 1.0, 1.0) / tone_stack_rms(90.0, 0.0, 1.0);

        assert!(
            (bass_effect_with_low_treble - bass_effect_with_high_treble).abs() > 0.1,
            "bass effects: low treble={bass_effect_with_low_treble}, high treble={bass_effect_with_high_treble}"
        );
    }

    #[test]
    fn cathode_follower_preserves_small_signal_dynamics() {
        let quiet = cathode_follower(0.1);
        let loud = cathode_follower(0.2);
        assert!(loud / quiet > 1.95);
    }

    #[test]
    fn clean_setting_preserves_pick_dynamics() {
        let mut clean = controls();
        clean.volume = 0.28;

        let quiet = sine_rms_at(&mut VoxAmp::new(48_000.0), 440.0, 0.025, clean);
        let loud = sine_rms_at(&mut VoxAmp::new(48_000.0), 440.0, 0.05, clean);
        assert!(loud > quiet * 1.8);
    }

    #[test]
    fn volume_does_not_change_fixed_power_stage_gain() {
        let mut low = controls();
        low.volume = 0.2;
        let mut high = low;
        high.volume = 0.4;

        let low_level = sine_rms_at(&mut VoxAmp::new(48_000.0), 440.0, 0.01, low);
        let high_level = sine_rms_at(&mut VoxAmp::new(48_000.0), 440.0, 0.01, high);
        assert!(high_level > low_level * 2.5);
        assert!(high_level < low_level * 6.0);
    }

    #[test]
    fn fully_driven_setting_compresses_more_than_clean() {
        let mut clean = controls();
        clean.volume = 0.32;
        let mut driven = clean;
        driven.volume = 1.0;

        let clean_quiet = sine_rms_at(&mut VoxAmp::new(48_000.0), 440.0, 0.05, clean);
        let clean_loud = sine_rms_at(&mut VoxAmp::new(48_000.0), 440.0, 0.10, clean);
        let driven_quiet = sine_rms_at(&mut VoxAmp::new(48_000.0), 440.0, 0.05, driven);
        let driven_loud = sine_rms_at(&mut VoxAmp::new(48_000.0), 440.0, 0.10, driven);

        assert!(clean_loud / clean_quiet > driven_loud / driven_quiet);
    }

    #[test]
    fn oversampling_latency_matches_reported_latency() {
        let coefficients = half_band_coefficients();
        let mut upsampler = FirFilter::new(coefficients);
        let mut downsampler = FirFilter::new(coefficients);
        let mut output = Vec::new();

        for sample_idx in 0..64 {
            let first = upsampler.process((sample_idx == 0) as u8 as f32 * OVERSAMPLING_FACTOR);
            output.push(downsampler.process(first));
            let second = upsampler.process(0.0);
            downsampler.process(second);
        }

        let peak = output
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.abs().total_cmp(&right.abs()))
            .unwrap()
            .0;
        assert_eq!(peak, AMP_LATENCY);
    }

    #[test]
    fn oversampling_reduces_high_frequency_aliasing() {
        const SAMPLE_RATE: f32 = 48_000.0;
        const INPUT_FREQUENCY: f32 = 10_000.0;
        const ALIAS_FREQUENCY: f32 = 18_000.0;
        const SAMPLES: usize = 24_000;

        let mut driven = controls();
        driven.volume = 1.0;
        driven.treble = 1.0;
        driven.cut = 0.0;

        let mut base_rate = AmpCore::new(SAMPLE_RATE);
        let mut oversampled = VoxAmp::new(SAMPLE_RATE);
        let mut base_output = Vec::with_capacity(SAMPLES);
        let mut oversampled_output = Vec::with_capacity(SAMPLES);
        for sample_idx in 0..SAMPLES {
            let input = (std::f32::consts::TAU * INPUT_FREQUENCY * sample_idx as f32 / SAMPLE_RATE)
                .sin()
                * 0.35;
            base_output.push(base_rate.process(input, driven));
            oversampled_output.push(oversampled.process(input, driven));
        }

        let base_alias = tone_magnitude(&base_output[SAMPLES / 2..], ALIAS_FREQUENCY, SAMPLE_RATE);
        let oversampled_alias = tone_magnitude(
            &oversampled_output[SAMPLES / 2..],
            ALIAS_FREQUENCY,
            SAMPLE_RATE,
        );
        assert!(
            oversampled_alias < base_alias * 0.5,
            "alias magnitude: base={base_alias}, oversampled={oversampled_alias}"
        );
    }

    fn tone_magnitude(samples: &[f32], frequency: f32, sample_rate: f32) -> f32 {
        let (real, imaginary) =
            samples
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |(real, imaginary), (index, sample)| {
                    let phase = std::f32::consts::TAU * frequency * index as f32 / sample_rate;
                    (
                        real + sample * phase.cos(),
                        imaginary - sample * phase.sin(),
                    )
                });
        (real * real + imaginary * imaginary).sqrt() * 2.0 / samples.len() as f32
    }
}
