mod components;
mod models;
mod oversampling;

use models::AmpCore;
use oversampling::{half_band_coefficients, FirFilter, OVERSAMPLING_FACTOR};
use std::path::PathBuf;

pub const AMP_LATENCY: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct AmpControls {
    pub volume: f32,
    pub bass: f32,
    pub treble: f32,
    pub cut: f32,
    pub output: f32,
    pub drive: f32,
    pub presence: f32,
    pub sag: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Nox30OperatingPoint {
    pub input_volume_output_v: f32,
    pub first_stage_output_v: f32,
    pub follower_output_v: f32,
    pub tone_stack_output_v: f32,
    pub preamp_send_v: f32,
    pub phase_inverter_input_v: f32,
    pub phase_inverter_output_v: f32,
    pub power_stage_output_v: f32,
    pub output_transformer_output_v: f32,
    pub preamp_voltage: f32,
    pub phase_inverter_voltage: f32,
    pub power_voltage: f32,
    pub first_stage_plate_current: f32,
    pub first_stage_cathode_voltage: f32,
    pub follower_plate_current: f32,
    pub follower_cathode_voltage: f32,
    pub drive_stage_plate_current: f32,
    pub recovery_stage_plate_current: f32,
    pub first_stage_shadow_output_v: Option<f32>,
    pub first_stage_shadow_error_v: Option<f32>,
    pub phase_inverter_plate_a_current: f32,
    pub phase_inverter_plate_b_current: f32,
    pub phase_inverter_cathode_voltage: f32,
    pub power_positive_current: f32,
    pub power_negative_current: f32,
    pub power_positive_screen_current: f32,
    pub power_negative_screen_current: f32,
    pub power_screen_voltage: f32,
    pub power_cathode_bias_voltage: f32,
    pub power_attack_current: f32,
    pub transformer_core_flux: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeuralCellMode {
    Shadow,
    Replace,
}

pub fn configure_nox30_first_stage_neural(descriptor_path: Option<PathBuf>, mode: NeuralCellMode) {
    models::configure_nox30_first_stage_neural(descriptor_path, mode);
}

#[derive(Clone, Copy, Debug)]
pub struct ComponentBoundary {
    pub id: &'static str,
    pub role: &'static str,
}

pub const NOX30_COMPONENT_BOUNDARIES: &[ComponentBoundary] = &[
    ComponentBoundary {
        id: "input_volume",
        role: "Input coupling, volume attenuation, and bright bypass network",
    },
    ComponentBoundary {
        id: "first_stage",
        role: "First nonlinear ECC83 common-cathode gain stage",
    },
    ComponentBoundary {
        id: "cathode_follower",
        role: "ECC83 cathode follower driving the tone stack",
    },
    ComponentBoundary {
        id: "tone_stack",
        role: "Top-boost passive tone network",
    },
    ComponentBoundary {
        id: "drive_stage",
        role: "Additional nonlinear ECC83 drive stage",
    },
    ComponentBoundary {
        id: "recovery_stage",
        role: "Post-drive nonlinear ECC83 recovery stage",
    },
    ComponentBoundary {
        id: "phase_inverter",
        role: "Shared-cathode long-tail-pair phase inverter",
    },
    ComponentBoundary {
        id: "cut_presence",
        role: "Cut and presence shaping network",
    },
    ComponentBoundary {
        id: "power_stage",
        role: "Push-pull EL84 plate-feedback power stage",
    },
    ComponentBoundary {
        id: "supply_network",
        role: "Shared B+ rail and sag network",
    },
    ComponentBoundary {
        id: "output_transformer",
        role: "Output transformer filtering and core-flux state",
    },
];

/// Oversampled facade around a dedicated amp model.
pub struct VoxAmp {
    upsampler: FirFilter,
    core: AmpCore,
    downsampler: FirFilter,
    oversampled: bool,
}

impl VoxAmp {
    pub fn new(sample_rate: f32) -> Self {
        Self::with_model(sample_rate, "nox30")
    }

    pub fn with_model(sample_rate: f32, model: &str) -> Self {
        let coefficients = half_band_coefficients();
        let oversampled = !matches!(model, "nox30");
        let core_sample_rate = if oversampled {
            sample_rate * OVERSAMPLING_FACTOR
        } else {
            sample_rate
        };
        Self {
            upsampler: FirFilter::new(coefficients),
            core: AmpCore::new_with_model(core_sample_rate, model),
            downsampler: FirFilter::new(coefficients),
            oversampled,
        }
    }

    pub fn reset(&mut self) {
        self.upsampler.reset();
        self.core.reset();
        self.downsampler.reset();
    }

    pub fn nox30_operating_point(&self) -> Option<Nox30OperatingPoint> {
        self.core.nox30_operating_point()
    }

    #[inline]
    pub fn process(&mut self, input: f32, controls: AmpControls) -> f32 {
        if !self.oversampled {
            return self.core.process(input, controls);
        }

        let upsampled = self.upsampler.process(input * OVERSAMPLING_FACTOR);
        let output = self
            .downsampler
            .process(self.core.process(upsampled, controls));

        let upsampled = self.upsampler.process(0.0);
        self.downsampler
            .process(self.core.process(upsampled, controls));
        output
    }

    #[inline]
    pub fn process_with_fx_loop(
        &mut self,
        input: f32,
        controls: AmpControls,
        mut process_fx: impl FnMut(f32) -> f32,
    ) -> f32 {
        if !self.oversampled {
            return self.core.process_with_fx_loop(input, controls, process_fx);
        }

        let amp_output = self.process(input, controls);
        process_fx(amp_output)
    }
}

#[cfg(test)]
mod tests {
    use super::components::{cathode_follower, TopBoostToneStack};
    use super::models::AmpCore;
    use super::oversampling::{half_band_coefficients, FirFilter, OVERSAMPLING_FACTOR};
    use super::*;
    use crate::ir::SpeakerStage;
    use std::path::Path;

    fn controls() -> AmpControls {
        AmpControls {
            volume: 0.5,
            bass: 0.5,
            treble: 0.5,
            cut: 0.5,
            output: 1.0,
            drive: 0.0,
            presence: 0.0,
            sag: 0.0,
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
    fn default_amp_silence_stays_finite_and_settles() {
        let mut amp = VoxAmp::new(48_000.0);
        let mut output = 0.0;
        for _ in 0..48_000 {
            output = amp.process(0.0, controls());
            assert!(output.is_finite());
        }
        assert!(output.abs() < 0.02, "settled output={output}");
    }

    #[test]
    fn default_amp_output_is_finite_under_hot_input() {
        let mut amp = VoxAmp::new(48_000.0);
        let mut controls = controls();
        controls.volume = 1.0;
        controls.bass = 1.0;
        controls.treble = 1.0;
        controls.cut = 0.0;
        controls.output = 2.0;

        for sample in [0.0, 0.5, -0.5, 1.0, -1.0, 4.0, -4.0]
            .into_iter()
            .cycle()
            .take(4096)
        {
            assert!(amp.process(sample, controls).is_finite());
        }
    }

    #[test]
    fn sheriff800_output_is_finite_under_hot_input() {
        let mut amp = VoxAmp::with_model(48_000.0, "sheriff800");
        let mut controls = controls();
        controls.volume = 0.9;
        controls.bass = 0.62;
        controls.treble = 0.60;
        controls.cut = 0.54;
        controls.drive = 0.7;
        controls.presence = 0.62;
        controls.sag = 0.35;

        for sample in [0.0, 0.5, -0.5, 1.0, -1.0, 4.0, -4.0]
            .into_iter()
            .cycle()
            .take(4096)
        {
            assert!(amp.process(sample, controls).is_finite());
        }
    }

    #[test]
    fn nox30_output_is_finite_under_hot_input() {
        let mut amp = VoxAmp::with_model(48_000.0, "nox30");
        let mut controls = controls();
        controls.volume = 0.9;
        controls.bass = 0.6;
        controls.treble = 0.6;
        controls.cut = 0.45;
        controls.sag = 0.8;

        for sample in [0.0, 0.5, -0.5, 1.0, -1.0, 4.0, -4.0]
            .into_iter()
            .cycle()
            .take(4096)
        {
            assert!(amp.process(sample, controls).is_finite());
        }
    }

    #[test]
    fn nox30_supply_sag_reduces_settled_level() {
        let mut stiff = VoxAmp::with_model(48_000.0, "nox30");
        let mut saggy = VoxAmp::with_model(48_000.0, "nox30");
        let mut stiff_controls = controls();
        stiff_controls.volume = 1.0;
        stiff_controls.cut = 0.2;
        stiff_controls.output = 1.0;
        stiff_controls.sag = 0.0;
        let mut saggy_controls = stiff_controls;
        saggy_controls.sag = 1.0;

        let mut stiff_late = 0.0;
        let mut saggy_late = 0.0;
        for sample_idx in 0..36_000 {
            let input = (std::f32::consts::TAU * 110.0 * sample_idx as f32 / 48_000.0).sin() * 0.55;
            let stiff_output = stiff.process(input, stiff_controls);
            let saggy_output = saggy.process(input, saggy_controls);
            if sample_idx >= 24_000 {
                stiff_late += stiff_output * stiff_output;
                saggy_late += saggy_output * saggy_output;
            }
        }

        assert!(
            saggy_late < stiff_late * 0.92,
            "stiff={stiff_late}, saggy={saggy_late}"
        );
    }

    #[test]
    fn default_amp_is_nox30() {
        let mut controls = controls();
        controls.volume = 0.76;
        controls.bass = 0.52;
        controls.treble = 0.61;
        controls.cut = 0.47;
        controls.drive = 0.68;
        controls.presence = 0.44;
        controls.sag = 0.70;

        let mut default_amp = VoxAmp::new(48_000.0);
        let mut nox30 = VoxAmp::with_model(48_000.0, "nox30");
        let mut difference_sum = 0.0;

        for sample_idx in 0..6_144 {
            let chord = (std::f32::consts::TAU * 196.0 * sample_idx as f32 / 48_000.0).sin()
                + (std::f32::consts::TAU * 247.0 * sample_idx as f32 / 48_000.0).sin() * 0.7
                + (std::f32::consts::TAU * 330.0 * sample_idx as f32 / 48_000.0).sin() * 0.45;
            let pick = if sample_idx % 1_571 < 80 { 1.35 } else { 1.0 };
            let input = chord * 0.055 * pick;
            let default_output = default_amp.process(input, controls);
            let nox30_output = nox30.process(input, controls);

            if sample_idx >= 1_024 {
                let difference = default_output - nox30_output;
                difference_sum += difference * difference;
            }
        }

        assert!(
            difference_sum < 1e-9,
            "default/nox30 mismatch: difference={difference_sum}"
        );
    }

    #[test]
    fn nox30_identity_fx_loop_matches_full_process() {
        let mut controls = controls();
        controls.volume = 0.56;
        controls.bass = 0.56;
        controls.treble = 0.58;
        controls.cut = 0.44;
        controls.output = 10.0_f32.powf(-9.0 / 20.0);
        controls.drive = 0.24;
        controls.presence = 0.34;
        controls.sag = 0.46;

        let mut full = VoxAmp::with_model(44_100.0, "nox30");
        let mut split = VoxAmp::with_model(44_100.0, "nox30");
        let mut difference_sum = 0.0;

        for sample_idx in 0..88_200 {
            let input = (std::f32::consts::TAU * 147.0 * sample_idx as f32 / 44_100.0).sin() * 0.06
                + (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 44_100.0).sin() * 0.03;
            let full_output = full.process(input, controls);
            let split_output = split.process_with_fx_loop(input, controls, |send| send);
            if sample_idx >= 44_100 {
                difference_sum += (full_output - split_output).abs();
            }
        }

        assert!(difference_sum < 1e-6, "difference_sum={difference_sum}");
    }

    #[test]
    fn standalone_nox30_file_rig_stays_in_output_range() {
        let mut amp = VoxAmp::with_model(48_000.0, "nox30");
        let mut speaker = SpeakerStage::from_embedded_ir(48_000).unwrap();
        let controls = AmpControls {
            volume: 0.76,
            bass: 0.52,
            treble: 0.61,
            cut: 0.47,
            output: 10.0_f32.powf(-18.0 / 20.0),
            drive: 0.68,
            presence: 0.44,
            sag: 0.70,
        };
        let samples = load_test_guitar_wav();
        let mut sum = 0.0;
        let mut peak = 0.0_f32;
        let mut count = 0;

        for input in samples.into_iter().take(44_100 * 2) {
            let output = speaker.process(amp.process(input, controls), true);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
            sum += output * output;
            count += 1;
        }

        let rms = (sum / count as f32).sqrt();
        assert!(rms > 0.003, "rms={rms}, peak={peak}, count={count}");
        assert!(peak < 0.95, "rms={rms}, peak={peak}");
    }

    #[test]
    fn standalone_nox30_file_rig_keeps_producing_output() {
        let mut amp = VoxAmp::with_model(48_000.0, "nox30");
        let mut speaker = SpeakerStage::from_embedded_ir(48_000).unwrap();
        let controls = AmpControls {
            volume: 0.76,
            bass: 0.52,
            treble: 0.61,
            cut: 0.47,
            output: 10.0_f32.powf(-18.0 / 20.0),
            drive: 0.68,
            presence: 0.44,
            sag: 0.70,
        };
        let samples = load_test_guitar_wav();
        let mut second_sums = [0.0; 8];
        let mut second_counts = [0; 8];

        for (sample_idx, input) in samples.into_iter().cycle().take(48_000 * 8).enumerate() {
            let output = speaker.process(amp.process(input, controls), true);
            assert!(output.is_finite());
            let second = sample_idx / 48_000;
            second_sums[second] += output * output;
            second_counts[second] += 1;
        }

        for (second, (&sum, &count)) in second_sums.iter().zip(&second_counts).enumerate() {
            let rms = (sum / count as f32).sqrt();
            assert!(rms > 0.0002, "second={second}, rms={rms}");
        }
    }

    #[test]
    fn nox30_exposes_replaceable_component_boundaries() {
        let ids: Vec<_> = NOX30_COMPONENT_BOUNDARIES
            .iter()
            .map(|boundary| boundary.id)
            .collect();

        for expected in [
            "input_volume",
            "first_stage",
            "cathode_follower",
            "tone_stack",
            "drive_stage",
            "recovery_stage",
            "phase_inverter",
            "cut_presence",
            "power_stage",
            "supply_network",
            "output_transformer",
        ] {
            assert!(ids.contains(&expected), "missing boundary {expected}");
        }
    }

    #[test]
    fn nox30_operating_point_exposes_shared_state() {
        let mut amp = VoxAmp::with_model(44_100.0, "nox30");
        let controls = AmpControls {
            volume: 0.76,
            bass: 0.52,
            treble: 0.61,
            cut: 0.47,
            output: 10.0_f32.powf(-18.0 / 20.0),
            drive: 0.68,
            presence: 0.44,
            sag: 0.70,
        };
        let samples = load_test_guitar_wav();
        for input in samples.into_iter().cycle().take(44_100 * 2) {
            amp.process(input, controls);
        }

        let operating_point = amp.nox30_operating_point().unwrap();
        assert!((150.0..=340.0).contains(&operating_point.power_voltage));
        assert!((150.0..=operating_point.power_voltage)
            .contains(&operating_point.phase_inverter_voltage));
        assert!((120.0..=operating_point.phase_inverter_voltage)
            .contains(&operating_point.preamp_voltage));
        assert!(operating_point.first_stage_plate_current.is_finite());
        assert!(operating_point.follower_plate_current.is_finite());
        assert!(operating_point.phase_inverter_plate_a_current.is_finite());
        assert!(operating_point.phase_inverter_plate_b_current.is_finite());
        assert!(operating_point.power_positive_current.is_finite());
        assert!(operating_point.power_negative_current.is_finite());
        assert!(operating_point.transformer_core_flux.is_finite());
    }

    #[test]
    fn nox30_sample_render_metrics_stay_in_fixture_band() {
        let mut amp = VoxAmp::with_model(48_000.0, "nox30");
        let mut speaker = SpeakerStage::from_embedded_ir(48_000).unwrap();
        let controls = AmpControls {
            volume: 0.76,
            bass: 0.52,
            treble: 0.61,
            cut: 0.47,
            output: 10.0_f32.powf(-18.0 / 20.0),
            drive: 0.68,
            presence: 0.44,
            sag: 0.70,
        };
        let samples = load_test_guitar_wav();
        let mut sum = 0.0;
        let mut peak = 0.0_f32;
        let mut checksum = 0.0_f64;
        let mut count = 0;

        for (sample_idx, input) in samples.into_iter().cycle().take(48_000 * 4).enumerate() {
            let output = speaker.process(amp.process(input, controls), true);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
            sum += output * output;
            checksum += output as f64 * ((sample_idx % 257) as f64 + 1.0);
            count += 1;
        }

        let rms = (sum / count as f32).sqrt();
        let normalized_checksum = checksum / count as f64;
        assert!(
            (0.030..0.180).contains(&rms),
            "rms={rms}, peak={peak}, checksum={normalized_checksum}"
        );
        assert!(
            (0.15..0.95).contains(&peak),
            "rms={rms}, peak={peak}, checksum={normalized_checksum}"
        );
        assert!(
            (-0.80..0.80).contains(&normalized_checksum),
            "rms={rms}, peak={peak}, checksum={normalized_checksum}"
        );
    }

    #[test]
    fn nox30_processing_cost_has_realtime_headroom() {
        let mut amp = VoxAmp::with_model(44_100.0, "nox30");
        let controls = AmpControls {
            volume: 0.76,
            bass: 0.52,
            treble: 0.61,
            cut: 0.47,
            output: 10.0_f32.powf(-18.0 / 20.0),
            drive: 0.68,
            presence: 0.44,
            sag: 0.70,
        };
        let mut sum = 0.0;
        let sample_count = 44_100;
        let start = std::time::Instant::now();

        for sample_idx in 0..sample_count {
            let t = sample_idx as f32 / 44_100.0;
            let chord = (std::f32::consts::TAU * 196.0 * t).sin()
                + (std::f32::consts::TAU * 247.0 * t).sin() * 0.7
                + (std::f32::consts::TAU * 330.0 * t).sin() * 0.45;
            let pick = if sample_idx % 1_571 < 80 { 1.35 } else { 1.0 };
            sum += amp.process(chord * 0.055 * pick, controls);
        }

        let elapsed = start.elapsed();
        assert!(sum.is_finite());
        assert!(
            elapsed < std::time::Duration::from_millis(4_000),
            "elapsed={elapsed:?} for {sample_count} nox30 samples"
        );
    }

    #[test]
    fn reset_preserves_selected_model() {
        let mut controls = controls();
        controls.volume = 0.8;
        controls.drive = 0.6;
        controls.presence = 0.5;

        let mut reset_sheriff = VoxAmp::with_model(48_000.0, "sheriff800");
        reset_sheriff.reset();
        let mut fresh_sheriff = VoxAmp::with_model(48_000.0, "sheriff800");
        let mut nox30 = VoxAmp::new(48_000.0);

        let mut reset_sum = 0.0;
        let mut fresh_sum = 0.0;
        let mut nox30_sum = 0.0;
        for sample_idx in 0..2_048 {
            let input = (std::f32::consts::TAU * 440.0 * sample_idx as f32 / 48_000.0).sin() * 0.2;
            let reset_output = reset_sheriff.process(input, controls);
            let fresh_output = fresh_sheriff.process(input, controls);
            let nox30_output = nox30.process(input, controls);
            if sample_idx >= 512 {
                reset_sum += reset_output * reset_output;
                fresh_sum += fresh_output * fresh_output;
                nox30_sum += nox30_output * nox30_output;
            }
        }

        assert!((reset_sum - fresh_sum).abs() < 1e-6);
        assert!((reset_sum - nox30_sum).abs() > 1e-4);
    }

    #[test]
    fn dumbler_model_has_distinct_overdrive_path() {
        let mut controls = controls();
        controls.volume = 0.72;
        controls.bass = 0.55;
        controls.treble = 0.60;
        controls.cut = 0.55;
        controls.drive = 0.75;
        controls.presence = 0.35;

        let mut nox30 = VoxAmp::new(48_000.0);
        let mut dumbler = VoxAmp::with_model(48_000.0, "dumbler");
        let mut nox30_sum = 0.0;
        let mut dumbler_sum = 0.0;
        let mut difference_sum = 0.0;

        for sample_idx in 0..4_096 {
            let input = (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 48_000.0).sin()
                * (0.08 + 0.16 * (sample_idx % 257 == 0) as u8 as f32);
            let nox30_output = nox30.process(input, controls);
            let dumbler_output = dumbler.process(input, controls);
            if sample_idx >= 1_024 {
                nox30_sum += nox30_output * nox30_output;
                dumbler_sum += dumbler_output * dumbler_output;
                let difference = nox30_output - dumbler_output;
                difference_sum += difference * difference;
            }
        }

        assert!(dumbler_sum.is_finite());
        assert!(difference_sum > (nox30_sum + dumbler_sum) * 0.05);
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

        let mut base_rate = AmpCore::new_with_model(SAMPLE_RATE, "sheriff800");
        let mut oversampled = VoxAmp::with_model(SAMPLE_RATE, "sheriff800");
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

    fn load_test_guitar_wav() -> Vec<f32> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lab/references/tone3000-inputs/Brit - Guitar.wav");
        let mut reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 48_000);
        let channels = spec.channels as usize;
        assert!(channels >= 1);

        match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .enumerate()
                .filter_map(|(index, sample)| (index % channels == 0).then(|| sample.unwrap()))
                .collect(),
            hound::SampleFormat::Int => {
                let scale = 2.0_f32.powi(spec.bits_per_sample as i32 - 1);
                reader
                    .samples::<i32>()
                    .enumerate()
                    .filter_map(|(index, sample)| {
                        (index % channels == 0).then(|| sample.unwrap() as f32 / scale)
                    })
                    .collect()
            }
        }
    }
}
