use std::env;
use std::path::{Path, PathBuf};
use voxbox::amp::{AmpControls, VoxAmp};
use voxbox::ir::{SpeakerStage, CONVOLUTION_LATENCY};

const DEFAULT_DI: &str = "tests/fixtures/ac30/input-di.wav";
const DEFAULT_REFERENCE: &str = "tests/fixtures/ac30/reference-ac30.wav";
const MAX_SECONDS: usize = 30;

#[test]
#[ignore = "requires a paired DI and real AC30 reference recording"]
fn compares_voxbox_to_real_ac30_recording() {
    let di_path = fixture_path("VOXBOX_AC30_DI", DEFAULT_DI);
    let reference_path = fixture_path("VOXBOX_AC30_REFERENCE", DEFAULT_REFERENCE);
    let (sample_rate, di) = read_mono_wav(&di_path);
    let (reference_rate, reference) = read_mono_wav(&reference_path);
    assert_eq!(
        sample_rate, reference_rate,
        "DI and reference must have the same sample rate"
    );

    let controls = AmpControls {
        volume: pot("VOXBOX_AC30_VOLUME", 6.0),
        bass: pot("VOXBOX_AC30_BASS", 5.0),
        treble: pot("VOXBOX_AC30_TREBLE", 6.0),
        cut: pot("VOXBOX_AC30_CUT", 3.5),
        output: 1.0,
    };
    let use_ir =
        env::var("VOXBOX_AC30_REFERENCE_KIND").unwrap_or_else(|_| "rig".to_owned()) != "amp";
    let input_gain = db_to_gain(number("VOXBOX_AC30_INPUT_DB", 0.0));
    let model = process_model(sample_rate, &di, controls, input_gain, use_ir);
    let max_samples = sample_rate as usize * MAX_SECONDS;
    let length = model.len().min(reference.len()).min(max_samples);
    assert!(
        length >= sample_rate as usize,
        "reference must be at least one second"
    );

    let model = &model[..length];
    let reference = &reference[..length];
    let lag = best_lag(model, reference, sample_rate as usize / 2);
    let (model, reference) = align(model, reference, lag);
    let gain = best_gain(model, reference);
    let polarity = if gain < 0.0 { "inverted" } else { "normal" };
    let model: Vec<_> = model.iter().map(|sample| sample * gain).collect();

    let correlation = correlation(&model, reference);
    let nmse_db = nmse_db(&model, reference);
    let envelope_error_db = envelope_error_db(&model, reference, sample_rate);
    let band_error_db = band_energy_error_db(&model, reference, sample_rate);

    eprintln!(
        "\nAC30 comparison\n  files: {} / {}\n  kind: {}\n  alignment: {lag} samples\n  polarity: {polarity}\n  gain match: {:+.2} dB\n  correlation: {:.4}\n  NMSE: {:.2} dB\n  envelope error: {:.2} dB\n  band-energy error: {:.2} dB",
        di_path.display(),
        reference_path.display(),
        if use_ir { "amp+cab+mic" } else { "amp/loadbox" },
        gain.abs().max(1e-12).log10() * 20.0,
        correlation,
        nmse_db,
        envelope_error_db,
        band_error_db,
    );

    assert!(
        correlation >= number("VOXBOX_AC30_MIN_CORRELATION", 0.60),
        "waveform correlation is too low"
    );
    assert!(
        nmse_db <= number("VOXBOX_AC30_MAX_NMSE_DB", -2.0),
        "normalized mean-square error is too high"
    );
    assert!(
        envelope_error_db <= number("VOXBOX_AC30_MAX_ENVELOPE_ERROR_DB", 4.0),
        "dynamic-envelope error is too high"
    );
    assert!(
        band_error_db <= number("VOXBOX_AC30_MAX_BAND_ERROR_DB", 5.0),
        "frequency-balance error is too high"
    );
}

fn process_model(
    sample_rate: u32,
    input: &[f32],
    controls: AmpControls,
    input_gain: f32,
    use_ir: bool,
) -> Vec<f32> {
    let mut amp = VoxAmp::new(sample_rate as f32);
    let mut speaker = use_ir.then(|| SpeakerStage::from_embedded_ir(sample_rate).unwrap());
    let mut output = Vec::with_capacity(input.len() + CONVOLUTION_LATENCY);
    for &sample in input {
        let amp_output = amp.process(sample * input_gain, controls);
        output.push(
            speaker
                .as_mut()
                .map_or(amp_output, |speaker| speaker.process(amp_output, true)),
        );
    }
    output
}

fn read_mono_wav(path: &Path) -> (u32, Vec<f32>) {
    let mut reader =
        hound::WavReader::open(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let spec = reader.spec();
    assert_eq!(spec.channels, 1, "{} must be mono", path.display());
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| sample.unwrap())
            .collect(),
        hound::SampleFormat::Int => {
            let scale = 2.0_f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|sample| sample.unwrap() as f32 / scale)
                .collect()
        }
    };
    (spec.sample_rate, samples)
}

fn best_lag(model: &[f32], reference: &[f32], max_lag: usize) -> isize {
    const DECIMATION: usize = 16;
    const MAX_ALIGNMENT_SAMPLES: usize = 30_000;
    let model: Vec<_> = model.iter().step_by(DECIMATION).copied().collect();
    let reference: Vec<_> = reference.iter().step_by(DECIMATION).copied().collect();
    let model = &model[..model.len().min(MAX_ALIGNMENT_SAMPLES)];
    let reference = &reference[..reference.len().min(MAX_ALIGNMENT_SAMPLES)];
    let max_lag = max_lag / DECIMATION;
    let mut best = (f64::NEG_INFINITY, 0);
    for lag in -(max_lag as isize)..=max_lag as isize {
        let (left, right) = align(model, reference, lag);
        let score = normalized_dot(left, right).abs();
        if score > best.0 {
            best = (score, lag);
        }
    }
    best.1 * DECIMATION as isize
}

fn align<'a>(model: &'a [f32], reference: &'a [f32], lag: isize) -> (&'a [f32], &'a [f32]) {
    let (model_start, reference_start) = if lag >= 0 {
        (lag as usize, 0)
    } else {
        (0, (-lag) as usize)
    };
    if model_start >= model.len() || reference_start >= reference.len() {
        return (&[], &[]);
    }
    let length = (model.len() - model_start).min(reference.len() - reference_start);
    (
        &model[model_start..model_start + length],
        &reference[reference_start..reference_start + length],
    )
}

fn best_gain(model: &[f32], reference: &[f32]) -> f32 {
    let numerator: f64 = model
        .iter()
        .zip(reference)
        .map(|(&model, &reference)| model as f64 * reference as f64)
        .sum();
    let denominator: f64 = model.iter().map(|&sample| (sample as f64).powi(2)).sum();
    (numerator / denominator.max(1e-20)) as f32
}

fn normalized_dot(left: &[f32], right: &[f32]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let dot: f64 = left
        .iter()
        .zip(right)
        .map(|(&left, &right)| left as f64 * right as f64)
        .sum();
    let left_energy: f64 = left.iter().map(|&sample| (sample as f64).powi(2)).sum();
    let right_energy: f64 = right.iter().map(|&sample| (sample as f64).powi(2)).sum();
    dot / (left_energy * right_energy).sqrt().max(1e-20)
}

fn correlation(left: &[f32], right: &[f32]) -> f32 {
    normalized_dot(left, right) as f32
}

fn nmse_db(model: &[f32], reference: &[f32]) -> f32 {
    let error: f64 = model
        .iter()
        .zip(reference)
        .map(|(&model, &reference)| ((model - reference) as f64).powi(2))
        .sum();
    let reference_energy: f64 = reference
        .iter()
        .map(|&sample| (sample as f64).powi(2))
        .sum();
    10.0 * (error / reference_energy.max(1e-20)).max(1e-20).log10() as f32
}

fn envelope_error_db(model: &[f32], reference: &[f32], sample_rate: u32) -> f32 {
    let block = (sample_rate as usize / 100).max(1);
    let errors: Vec<_> = model
        .chunks(block)
        .zip(reference.chunks(block))
        .filter_map(|(model, reference)| {
            let reference = rms(reference);
            (reference > 1e-5).then(|| (20.0 * (rms(model).max(1e-8) / reference).log10()).abs())
        })
        .collect();
    errors.iter().sum::<f32>() / errors.len().max(1) as f32
}

fn band_energy_error_db(model: &[f32], reference: &[f32], sample_rate: u32) -> f32 {
    let model = band_rms(model, sample_rate);
    let reference = band_rms(reference, sample_rate);
    model
        .iter()
        .zip(reference)
        .map(|(&model, reference)| (20.0 * (model.max(1e-8) / reference.max(1e-8)).log10()).abs())
        .sum::<f32>()
        / model.len() as f32
}

fn band_rms(samples: &[f32], sample_rate: u32) -> [f32; 3] {
    let low_coefficient = one_pole_coefficient(sample_rate, 250.0);
    let high_coefficient = one_pole_coefficient(sample_rate, 2_500.0);
    let mut low_state = 0.0;
    let mut high_state = 0.0;
    let mut energy = [0.0_f64; 3];
    for &sample in samples {
        low_state += low_coefficient * (sample - low_state);
        high_state += high_coefficient * (sample - high_state);
        let bands = [low_state, high_state - low_state, sample - high_state];
        for (target, band) in energy.iter_mut().zip(bands) {
            *target += (band as f64).powi(2);
        }
    }
    energy.map(|energy| (energy / samples.len().max(1) as f64).sqrt() as f32)
}

fn one_pole_coefficient(sample_rate: u32, cutoff: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * cutoff / sample_rate as f32).exp()
}

fn rms(samples: &[f32]) -> f32 {
    (samples
        .iter()
        .map(|&sample| (sample as f64).powi(2))
        .sum::<f64>()
        / samples.len().max(1) as f64)
        .sqrt() as f32
}

fn fixture_path(variable: &str, default: &str) -> PathBuf {
    env::var(variable).map_or_else(|_| PathBuf::from(default), PathBuf::from)
}

fn pot(variable: &str, default: f32) -> f32 {
    number(variable, default) / 10.0
}

fn number(variable: &str, default: f32) -> f32 {
    env::var(variable)
        .map(|value| {
            value
                .parse()
                .unwrap_or_else(|_| panic!("{variable} must be a number"))
        })
        .unwrap_or(default)
}

#[test]
fn comparison_metrics_handle_delay_polarity_and_gain() {
    let reference: Vec<f32> = (0..48_000)
        .scan(0x1234_5678_u32, |state, _| {
            *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            Some((*state as f32 / u32::MAX as f32) * 2.0 - 1.0)
        })
        .collect();
    let mut model = vec![0.0; 128];
    model.extend(reference.iter().map(|sample| sample * -0.42));

    let lag = best_lag(&model, &reference, 1_000);
    let (model, reference) = align(&model, &reference, lag);
    let gain = best_gain(model, reference);
    let model: Vec<_> = model.iter().map(|sample| sample * gain).collect();

    assert_eq!(lag, 128);
    assert!(correlation(&model, reference) > 0.999);
    assert!(nmse_db(&model, reference) < -30.0);
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
