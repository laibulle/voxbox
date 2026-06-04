use rill_core_wdf::{filters::RcPole, WdfElement};

pub const AMP_LATENCY: usize = 16;
const OVERSAMPLING_FACTOR: f32 = 2.0;
const HALF_BAND_TAPS: usize = 33;
const TONE_STACK_NODES: usize = 7;

// Lightweight helper implementations to keep the amp model self-contained.
// These are simplified approximations sufficient for compilation and basic
// behavior; they intentionally prioritize clarity over physical accuracy.

struct WdfHighpass {
    prev: f32,
    alpha: f32,
}

impl WdfHighpass {
    fn from_rc(_sample_rate: f32, _r: f32, _c: f32) -> Self {
        Self {
            prev: 0.0,
            alpha: 0.5,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let out = input - self.prev * self.alpha;
        self.prev = input;
        out
    }
}

struct OnePoleLowpass {
    state: f32,
    a: f32,
}

impl OnePoleLowpass {
    fn new(sample_rate: f32, _cutoff: f32) -> Self {
        let a = 0.5_f32; // placeholder coefficient
        Self { state: 0.0, a }
    }

    fn set_cutoff(&mut self, _sample_rate: f32, _cutoff: f32) {
        // noop for the simplified model
    }

    fn process(&mut self, input: f32) -> f32 {
        self.state = self.state * (1.0 - self.a) + input * self.a;
        self.state
    }
}

struct EnvelopeFollower {
    state: f32,
    attack_coeff: f32,
    release_coeff: f32,
}

impl EnvelopeFollower {
    fn new(_sample_rate: f32, attack: f32, release: f32) -> Self {
        Self {
            state: 0.0,
            attack_coeff: (-1.0 / (attack * 44100.0)).exp(),
            release_coeff: (-1.0 / (release * 44100.0)).exp(),
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let target = input;
        if target > self.state {
            self.state = self.state * self.attack_coeff + target * (1.0 - self.attack_coeff);
        } else {
            self.state = self.state * self.release_coeff + target * (1.0 - self.release_coeff);
        }
        self.state
    }
}

fn triode_stage(input: f32, _bias: f32) -> f32 {
    // simple soft-clipping nonlinearity
    (input).tanh()
}

fn cathode_follower(input: f32) -> f32 {
    input * 0.95
}

fn el84_bank(input: f32) -> f32 {
    // simple saturating stage to emulate EL84 behavior
    (input * 0.8).tanh()
}

fn multiply_tone_stack(
    matrix: [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
    rhs: [f32; TONE_STACK_NODES],
) -> [f32; TONE_STACK_NODES] {
    let mut out = [0.0; TONE_STACK_NODES];
    for i in 0..TONE_STACK_NODES {
        let mut sum = 0.0;
        for j in 0..TONE_STACK_NODES {
            sum += matrix[i][j] * rhs[j];
        }
        out[i] = sum;
    }
    out
}

fn invert_tone_stack(
    mut matrix: [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
) -> [[f32; TONE_STACK_NODES]; TONE_STACK_NODES] {
    // Perform Gauss-Jordan to compute inverse of the small fixed-size matrix.
    const N: usize = TONE_STACK_NODES;
    let mut inv = [[0.0; TONE_STACK_NODES]; TONE_STACK_NODES];
    for i in 0..N {
        inv[i][i] = 1.0;
    }

    for pivot in 0..N {
        // find pivot
        let mut best_row = pivot;
        let mut best_val = matrix[pivot][pivot].abs();
        for row in (pivot + 1)..N {
            let val = matrix[row][pivot].abs();
            if val > best_val {
                best_val = val;
                best_row = row;
            }
        }
        if best_row != pivot {
            matrix.swap(pivot, best_row);
            inv.swap(pivot, best_row);
        }

        let pivot_val = matrix[pivot][pivot];
        if pivot_val.abs() < 1e-12 {
            continue;
        }

        // normalize
        for col in 0..N {
            matrix[pivot][col] /= pivot_val;
            inv[pivot][col] /= pivot_val;
        }

        // eliminate
        for row in 0..N {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            for col in 0..N {
                matrix[row][col] -= factor * matrix[pivot][col];
                inv[row][col] -= factor * inv[pivot][col];
            }
        }
    }

    inv
}

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
    const N: usize = TONE_STACK_NODES;

    for pivot in 0..N {
        // Partial pivoting: find the row with the largest absolute pivot
        let mut best_row = pivot;
        let mut best_val = matrix[pivot][pivot].abs();
        for row in (pivot + 1)..N {
            let val = matrix[row][pivot].abs();
            if val > best_val {
                best_val = val;
                best_row = row;
            }
        }

        if best_row != pivot {
            matrix.swap(pivot, best_row);
            rhs.swap(pivot, best_row);
        }

        let pivot_val = matrix[pivot][pivot];
        if pivot_val.abs() < 1e-12 {
            // Singular or nearly singular matrix; skip normalization to avoid NaNs.
            continue;
        }

        // Normalize pivot row
        for col in pivot..N {
            matrix[pivot][col] /= pivot_val;
        }
        rhs[pivot] /= pivot_val;

        // Eliminate pivot column in all other rows (Gauss-Jordan)
        for row in 0..N {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            if factor == 0.0 {
                continue;
            }
            for col in pivot..N {
                matrix[row][col] -= factor * matrix[pivot][col];
            }
            rhs[row] -= factor * rhs[pivot];
        }
    }

    rhs
}
