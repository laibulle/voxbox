use rill_core_wdf::{filters::RcPole, WdfElement};

const TONE_STACK_NODES: usize = 7;

pub(super) struct TopBoostToneStack {
    treble_capacitor: TrapezoidalCapacitor,
    bass_coupling_capacitor: TrapezoidalCapacitor,
    bass_capacitor: TrapezoidalCapacitor,
    inverse_matrix: [[f32; TONE_STACK_NODES]; TONE_STACK_NODES],
    bass: f32,
    treble: f32,
}

impl TopBoostToneStack {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self::new_with_caps(sample_rate, 150e-12, 22e-9, 22e-9)
    }

    pub(super) fn new_with_caps(
        sample_rate: f32,
        treble_cap: f32,
        bass_coupling: f32,
        bass_cap: f32,
    ) -> Self {
        Self {
            treble_capacitor: TrapezoidalCapacitor::new(treble_cap, sample_rate),
            bass_coupling_capacitor: TrapezoidalCapacitor::new(bass_coupling, sample_rate),
            bass_capacitor: TrapezoidalCapacitor::new(bass_cap, sample_rate),
            inverse_matrix: [[0.0; TONE_STACK_NODES]; TONE_STACK_NODES],
            bass: f32::NAN,
            treble: f32::NAN,
        }
    }

    #[inline]
    pub(super) fn process(&mut self, input: f32, bass: f32, treble: f32) -> f32 {
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
        stamp_resistor(&mut matrix, SOURCE, SLOPE_NODE, 100_000.0);
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

pub(super) struct EnvelopeFollower {
    attack: f32,
    release: f32,
    state: f32,
}

impl EnvelopeFollower {
    pub(super) fn new(sample_rate: f32, attack_seconds: f32, release_seconds: f32) -> Self {
        Self {
            attack: (-1.0 / (sample_rate * attack_seconds)).exp(),
            release: (-1.0 / (sample_rate * release_seconds)).exp(),
            state: 0.0,
        }
    }

    #[inline]
    pub(super) fn process(&mut self, input: f32) -> f32 {
        let coefficient = if input > self.state {
            self.attack
        } else {
            self.release
        };
        self.state = input + coefficient * (self.state - input);
        self.state
    }
}

pub(super) struct WdfHighpass {
    lowpass: RcPole<f32>,
}

impl WdfHighpass {
    pub(super) fn from_rc(sample_rate: f32, resistance: f32, capacitance: f32) -> Self {
        let cutoff = 1.0 / (std::f32::consts::TAU * resistance * capacitance);
        let g = std::f32::consts::PI * cutoff / sample_rate;
        Self {
            lowpass: RcPole::new(g / (1.0 + g)),
        }
    }

    #[inline]
    pub(super) fn process(&mut self, input: f32) -> f32 {
        input - self.lowpass.process_incident(input)
    }
}

#[inline]
pub(super) fn triode_stage(input: f32, bias: f32) -> f32 {
    let biased = input + bias;
    (biased.tanh() - bias.tanh()) * 1.08
}

#[inline]
pub(super) fn cathode_follower(input: f32) -> f32 {
    input * 0.96 + input * input * 0.018
}

#[inline]
pub(super) fn el34_bank(input: f32) -> f32 {
    let conducting = (input + 0.12).max(0.0);
    let compressed_current = conducting * 1.05 / (1.0 + conducting * 0.16);
    compressed_current.tanh()
}

#[inline]
pub(super) fn six_l6_bank(input: f32) -> f32 {
    let conducting = (input + 0.10).max(0.0);
    let compressed_current = conducting * 0.98 / (1.0 + conducting * 0.12);
    compressed_current.tanh()
}

pub(super) struct OnePoleLowpass {
    coefficient: f32,
    cutoff: f32,
    state: f32,
}

impl OnePoleLowpass {
    pub(super) fn new(sample_rate: f32, cutoff: f32) -> Self {
        let mut filter = Self {
            coefficient: 0.0,
            cutoff: f32::NAN,
            state: 0.0,
        };
        filter.set_cutoff(sample_rate, cutoff);
        filter
    }

    pub(super) fn set_cutoff(&mut self, sample_rate: f32, cutoff: f32) {
        if cutoff != self.cutoff {
            self.coefficient = 1.0 - (-std::f32::consts::TAU * cutoff / sample_rate).exp();
            self.cutoff = cutoff;
        }
    }

    #[inline]
    pub(super) fn process(&mut self, input: f32) -> f32 {
        self.state += self.coefficient * (input - self.state);
        self.state
    }
}
