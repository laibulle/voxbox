mod components;
mod models;
mod oversampling;

use models::AmpCore;
use oversampling::{half_band_coefficients, FirFilter, OVERSAMPLING_FACTOR};

pub const AMP_LATENCY: usize = 16;

#[derive(Clone, Copy)]
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

/// Oversampled facade around a dedicated amp model.
pub struct VoxAmp {
    upsampler: FirFilter,
    core: AmpCore,
    downsampler: FirFilter,
}

impl VoxAmp {
    pub fn new(sample_rate: f32) -> Self {
        Self::with_model(sample_rate, "ac30")
    }

    pub fn with_model(sample_rate: f32, model: &str) -> Self {
        let coefficients = half_band_coefficients();
        Self {
            upsampler: FirFilter::new(coefficients),
            core: AmpCore::new_with_model(sample_rate * OVERSAMPLING_FACTOR, model),
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

#[cfg(test)]
mod tests {
    use super::components::{cathode_follower, el84_bank, TopBoostToneStack};
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
    fn jcm800_output_is_finite_under_hot_input() {
        let mut amp = VoxAmp::with_model(48_000.0, "jcm800");
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
    fn nox_output_is_finite_under_hot_input() {
        let mut amp = VoxAmp::with_model(48_000.0, "nox");
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
    fn nox_supply_sags_under_sustained_load() {
        let mut amp = VoxAmp::with_model(48_000.0, "nox");
        let mut controls = controls();
        controls.volume = 1.0;
        controls.cut = 0.2;
        controls.output = 1.0;
        controls.sag = 1.0;

        let mut early = 0.0;
        let mut late = 0.0;
        for sample_idx in 0..24_000 {
            let input = (std::f32::consts::TAU * 110.0 * sample_idx as f32 / 48_000.0).sin() * 0.55;
            let output = amp.process(input, controls);
            if (512..2_560).contains(&sample_idx) {
                early += output * output;
            } else if sample_idx >= 21_952 {
                late += output * output;
            }
        }

        assert!(late < early * 0.96, "early={early}, late={late}");
    }

    #[test]
    fn nox_driven_voice_is_distinct_from_ac30() {
        let mut controls = controls();
        controls.volume = 0.76;
        controls.bass = 0.52;
        controls.treble = 0.61;
        controls.cut = 0.47;
        controls.drive = 0.68;
        controls.presence = 0.44;
        controls.sag = 0.70;

        let mut ac30 = VoxAmp::new(48_000.0);
        let mut nox = VoxAmp::with_model(48_000.0, "nox");
        let mut ac30_sum = 0.0;
        let mut nox_sum = 0.0;
        let mut difference_sum = 0.0;

        for sample_idx in 0..6_144 {
            let chord = (std::f32::consts::TAU * 196.0 * sample_idx as f32 / 48_000.0).sin()
                + (std::f32::consts::TAU * 247.0 * sample_idx as f32 / 48_000.0).sin() * 0.7
                + (std::f32::consts::TAU * 330.0 * sample_idx as f32 / 48_000.0).sin() * 0.45;
            let pick = if sample_idx % 1_571 < 80 { 1.35 } else { 1.0 };
            let input = chord * 0.055 * pick;
            let ac30_output = ac30.process(input, controls);
            let nox_output = nox.process(input, controls);

            if sample_idx >= 1_024 {
                ac30_sum += ac30_output * ac30_output;
                nox_sum += nox_output * nox_output;
                let difference = ac30_output - nox_output;
                difference_sum += difference * difference;
            }
        }

        assert!(nox_sum.is_finite());
        assert!(
            difference_sum > (ac30_sum + nox_sum) * 0.12,
            "ac30={ac30_sum}, nox={nox_sum}, difference={difference_sum}"
        );
    }

    #[test]
    fn standalone_nox_file_preset_stays_in_output_range() {
        let mut amp = VoxAmp::with_model(44_100.0, "nox");
        let mut speaker = SpeakerStage::from_embedded_ir(44_100).unwrap();
        let controls = AmpControls {
            volume: 0.76,
            bass: 0.52,
            treble: 0.61,
            cut: 0.47,
            output: 10.0_f32.powf(-22.0 / 20.0),
            drive: 0.68,
            presence: 0.44,
            sag: 0.70,
        };
        let samples = load_test_guitar_wav();
        let mut sum = 0.0;
        let mut peak = 0.0_f32;
        let mut count = 0;

        for input in samples.into_iter().take(44_100 * 8) {
            let output = speaker.process(amp.process(input, controls), true);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
            sum += output * output;
            count += 1;
        }

        let rms = (sum / count as f32).sqrt();
        assert!(rms > 0.003, "rms={rms}, peak={peak}");
        assert!(peak < 0.95, "rms={rms}, peak={peak}");
    }

    #[test]
    fn reset_preserves_selected_model() {
        let mut controls = controls();
        controls.volume = 0.8;
        controls.drive = 0.6;
        controls.presence = 0.5;

        let mut reset_jcm = VoxAmp::with_model(48_000.0, "jcm800");
        reset_jcm.reset();
        let mut fresh_jcm = VoxAmp::with_model(48_000.0, "jcm800");
        let mut ac30 = VoxAmp::new(48_000.0);

        let mut reset_sum = 0.0;
        let mut fresh_sum = 0.0;
        let mut ac30_sum = 0.0;
        for sample_idx in 0..2_048 {
            let input = (std::f32::consts::TAU * 440.0 * sample_idx as f32 / 48_000.0).sin() * 0.2;
            let reset_output = reset_jcm.process(input, controls);
            let fresh_output = fresh_jcm.process(input, controls);
            let ac30_output = ac30.process(input, controls);
            if sample_idx >= 512 {
                reset_sum += reset_output * reset_output;
                fresh_sum += fresh_output * fresh_output;
                ac30_sum += ac30_output * ac30_output;
            }
        }

        assert!((reset_sum - fresh_sum).abs() < 1e-6);
        assert!((reset_sum - ac30_sum).abs() > 1e-4);
    }

    #[test]
    fn dumble_model_has_distinct_overdrive_path() {
        let mut controls = controls();
        controls.volume = 0.72;
        controls.bass = 0.55;
        controls.treble = 0.60;
        controls.cut = 0.55;
        controls.drive = 0.75;
        controls.presence = 0.35;

        let mut ac30 = VoxAmp::new(48_000.0);
        let mut dumble = VoxAmp::with_model(48_000.0, "dumble");
        let mut ac30_sum = 0.0;
        let mut dumble_sum = 0.0;
        let mut difference_sum = 0.0;

        for sample_idx in 0..4_096 {
            let input = (std::f32::consts::TAU * 220.0 * sample_idx as f32 / 48_000.0).sin()
                * (0.08 + 0.16 * (sample_idx % 257 == 0) as u8 as f32);
            let ac30_output = ac30.process(input, controls);
            let dumble_output = dumble.process(input, controls);
            if sample_idx >= 1_024 {
                ac30_sum += ac30_output * ac30_output;
                dumble_sum += dumble_output * dumble_output;
                let difference = ac30_output - dumble_output;
                difference_sum += difference * difference;
            }
        }

        assert!(dumble_sum.is_finite());
        assert!(difference_sum > (ac30_sum + dumble_sum) * 0.05);
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
    fn el84_bank_stays_monotonic_under_overload() {
        let mut previous = el84_bank(-0.18);
        for step in 1..=480 {
            let input = -0.18 + step as f32 * 0.01;
            let output = el84_bank(input);
            assert!(
                output >= previous,
                "EL84 transfer folded back at input={input}: previous={previous}, output={output}"
            );
            previous = output;
        }
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

    fn load_test_guitar_wav() -> Vec<f32> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../samples/teenager-electric-guitar-smooth-chords-dry_94bpm_G_major.wav");
        let mut reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 44_100);
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
