use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, SampleFormat, SampleRate, StreamConfig, SupportedStreamConfigRange,
};
use rtrb::{Consumer, RingBuffer};
use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use voxbox::amp::{AmpControls, NoxOperatingPoint, VoxAmp};
use voxbox::ir::SpeakerStage;

const RMS_SCALE: f64 = 1_000_000_000.0;
const NEAR_CLIP_LEVEL: f32 = 0.98;
const CLIP_LEVEL: f32 = 1.0;
const MONITOR_LOG_LINES: usize = 5_000;
const VU_WIDTH: usize = 28;

#[derive(Default)]
struct MonitorStats {
    input_sum_squares: AtomicU64,
    output_sum_squares: AtomicU64,
    input_count: AtomicU64,
    output_count: AtomicU64,
    input_peak_bits: AtomicU32,
    output_peak_bits: AtomicU32,
    input_near_clips: AtomicU64,
    output_near_clips: AtomicU64,
    input_clips: AtomicU64,
    output_clips: AtomicU64,
    input_overruns: AtomicU64,
    output_underruns: AtomicU64,
}

#[derive(Default)]
struct MonitorSnapshot {
    input_sum_squares: u64,
    output_sum_squares: u64,
    input_count: u64,
    output_count: u64,
    input_peak: f32,
    output_peak: f32,
    input_near_clips: u64,
    output_near_clips: u64,
    input_clips: u64,
    output_clips: u64,
    input_overruns: u64,
    output_underruns: u64,
}

#[derive(Default)]
struct ComponentTelemetry {
    preamp_voltage_bits: AtomicU32,
    phase_inverter_voltage_bits: AtomicU32,
    power_voltage_bits: AtomicU32,
    first_stage_current_bits: AtomicU32,
    phase_inverter_current_bits: AtomicU32,
    power_current_bits: AtomicU32,
    power_cathode_bias_bits: AtomicU32,
    transformer_flux_bits: AtomicU32,
}

#[derive(Clone, Copy, Default)]
struct ComponentTelemetrySnapshot {
    preamp_voltage: f32,
    phase_inverter_voltage: f32,
    power_voltage: f32,
    first_stage_current: f32,
    phase_inverter_current: f32,
    power_current: f32,
    power_cathode_bias: f32,
    transformer_flux: f32,
}

impl MonitorStats {
    fn record_input(&self, sample: f32) {
        self.record_sample(
            sample,
            &self.input_sum_squares,
            &self.input_count,
            &self.input_peak_bits,
            &self.input_near_clips,
            &self.input_clips,
        );
    }

    fn record_output(&self, sample: f32) {
        self.record_sample(
            sample,
            &self.output_sum_squares,
            &self.output_count,
            &self.output_peak_bits,
            &self.output_near_clips,
            &self.output_clips,
        );
    }

    fn record_input_overrun(&self) {
        self.input_overruns.fetch_add(1, Ordering::Relaxed);
    }

    fn record_output_underrun(&self) {
        self.output_underruns.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot_and_reset(&self) -> MonitorSnapshot {
        MonitorSnapshot {
            input_sum_squares: self.input_sum_squares.swap(0, Ordering::Relaxed),
            output_sum_squares: self.output_sum_squares.swap(0, Ordering::Relaxed),
            input_count: self.input_count.swap(0, Ordering::Relaxed),
            output_count: self.output_count.swap(0, Ordering::Relaxed),
            input_peak: f32::from_bits(self.input_peak_bits.swap(0, Ordering::Relaxed)),
            output_peak: f32::from_bits(self.output_peak_bits.swap(0, Ordering::Relaxed)),
            input_near_clips: self.input_near_clips.swap(0, Ordering::Relaxed),
            output_near_clips: self.output_near_clips.swap(0, Ordering::Relaxed),
            input_clips: self.input_clips.swap(0, Ordering::Relaxed),
            output_clips: self.output_clips.swap(0, Ordering::Relaxed),
            input_overruns: self.input_overruns.swap(0, Ordering::Relaxed),
            output_underruns: self.output_underruns.swap(0, Ordering::Relaxed),
        }
    }

    fn record_sample(
        &self,
        sample: f32,
        sum_squares: &AtomicU64,
        count: &AtomicU64,
        peak_bits: &AtomicU32,
        near_clips: &AtomicU64,
        clips: &AtomicU64,
    ) {
        let magnitude = sample.abs();
        let square = (magnitude as f64 * magnitude as f64 * RMS_SCALE).round() as u64;
        sum_squares.fetch_add(square, Ordering::Relaxed);
        count.fetch_add(1, Ordering::Relaxed);
        update_peak(peak_bits, magnitude);
        if magnitude >= NEAR_CLIP_LEVEL {
            near_clips.fetch_add(1, Ordering::Relaxed);
        }
        if magnitude >= CLIP_LEVEL {
            clips.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl ComponentTelemetry {
    fn record_nox(&self, operating_point: NoxOperatingPoint) {
        self.preamp_voltage_bits
            .store(operating_point.preamp_voltage.to_bits(), Ordering::Relaxed);
        self.phase_inverter_voltage_bits.store(
            operating_point.phase_inverter_voltage.to_bits(),
            Ordering::Relaxed,
        );
        self.power_voltage_bits
            .store(operating_point.power_voltage.to_bits(), Ordering::Relaxed);
        self.first_stage_current_bits.store(
            operating_point.first_stage_plate_current.to_bits(),
            Ordering::Relaxed,
        );
        self.phase_inverter_current_bits.store(
            (operating_point.phase_inverter_plate_a_current
                + operating_point.phase_inverter_plate_b_current)
                .to_bits(),
            Ordering::Relaxed,
        );
        self.power_current_bits.store(
            (operating_point.power_positive_current + operating_point.power_negative_current)
                .to_bits(),
            Ordering::Relaxed,
        );
        self.power_cathode_bias_bits.store(
            operating_point.power_cathode_bias_voltage.to_bits(),
            Ordering::Relaxed,
        );
        self.transformer_flux_bits.store(
            operating_point.transformer_core_flux.to_bits(),
            Ordering::Relaxed,
        );
    }

    fn snapshot(&self) -> ComponentTelemetrySnapshot {
        ComponentTelemetrySnapshot {
            preamp_voltage: f32::from_bits(self.preamp_voltage_bits.load(Ordering::Relaxed)),
            phase_inverter_voltage: f32::from_bits(
                self.phase_inverter_voltage_bits.load(Ordering::Relaxed),
            ),
            power_voltage: f32::from_bits(self.power_voltage_bits.load(Ordering::Relaxed)),
            first_stage_current: f32::from_bits(
                self.first_stage_current_bits.load(Ordering::Relaxed),
            ),
            phase_inverter_current: f32::from_bits(
                self.phase_inverter_current_bits.load(Ordering::Relaxed),
            ),
            power_current: f32::from_bits(self.power_current_bits.load(Ordering::Relaxed)),
            power_cathode_bias: f32::from_bits(
                self.power_cathode_bias_bits.load(Ordering::Relaxed),
            ),
            transformer_flux: f32::from_bits(self.transformer_flux_bits.load(Ordering::Relaxed)),
        }
    }
}

fn update_peak(peak_bits: &AtomicU32, magnitude: f32) {
    let magnitude_bits = magnitude.to_bits();
    let mut current = peak_bits.load(Ordering::Relaxed);
    while magnitude_bits > current {
        match peak_bits.compare_exchange_weak(
            current,
            magnitude_bits,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(value) => current = value,
        }
    }
}

fn rms_from_scaled(sum_squares: u64, count: u64) -> f32 {
    if count == 0 {
        0.0
    } else {
        (sum_squares as f64 / RMS_SCALE / count as f64).sqrt() as f32
    }
}

fn dbfs(level: f32) -> f32 {
    if level > 0.0 {
        20.0 * level.log10()
    } else {
        f32::NEG_INFINITY
    }
}

fn format_dbfs(level: f32) -> String {
    let db = dbfs(level);
    if db.is_finite() {
        format!("{db:+.1}")
    } else {
        "-inf".to_owned()
    }
}

fn format_audio_monitor(stats: &MonitorSnapshot) -> String {
    let in_rms = rms_from_scaled(stats.input_sum_squares, stats.input_count);
    let out_rms = rms_from_scaled(stats.output_sum_squares, stats.output_count);
    format!(
        "MON input rms {:.5} ({} dBFS) peak {:.5} ({} dBFS) near/clip {}/{} | output rms {:.5} ({} dBFS) peak {:.5} ({} dBFS) near/clip {}/{} | xrun in/out {}/{}",
        in_rms,
        format_dbfs(in_rms),
        stats.input_peak,
        format_dbfs(stats.input_peak),
        stats.input_near_clips,
        stats.input_clips,
        out_rms,
        format_dbfs(out_rms),
        stats.output_peak,
        format_dbfs(stats.output_peak),
        stats.output_near_clips,
        stats.output_clips,
        stats.input_overruns,
        stats.output_underruns
    )
}

fn format_component_telemetry(components: ComponentTelemetrySnapshot) -> String {
    format!(
        "CMP rails pre/pi/pwr {:.0}/{:.0}/{:.0} V | I first/pi/pwr {:.2}/{:.2}/{:.1} mA | cath {:.2} V | flux {:+.5}",
        components.preamp_voltage,
        components.phase_inverter_voltage,
        components.power_voltage,
        components.first_stage_current * 1_000.0,
        components.phase_inverter_current * 1_000.0,
        components.power_current * 1_000.0,
        components.power_cathode_bias,
        components.transformer_flux,
    )
}

fn vu_meter(level: f32, width: usize) -> String {
    let db = dbfs(level);
    let normalized = if db.is_finite() {
        ((db + 60.0) / 60.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = (normalized * width as f32).round() as usize;
    format!(
        "[{}{}]",
        "#".repeat(filled),
        ".".repeat(width.saturating_sub(filled))
    )
}

fn format_monitor_dashboard(
    stats: &MonitorSnapshot,
    components: Option<ComponentTelemetrySnapshot>,
    model: &str,
    log_path: &Path,
) -> String {
    let in_rms = rms_from_scaled(stats.input_sum_squares, stats.input_count);
    let out_rms = rms_from_scaled(stats.output_sum_squares, stats.output_count);
    let mut text = format!(
        "VoxBox monitor  model {model}  log {}\n\
         input  rms {:>6} dBFS {} peak {:>6} dBFS near/clip {}/{}\n\
         output rms {:>6} dBFS {} peak {:>6} dBFS near/clip {}/{}\n\
         xrun in/out {}/{}\n",
        log_path.display(),
        format_dbfs(in_rms),
        vu_meter(in_rms, VU_WIDTH),
        format_dbfs(stats.input_peak),
        stats.input_near_clips,
        stats.input_clips,
        format_dbfs(out_rms),
        vu_meter(out_rms, VU_WIDTH),
        format_dbfs(stats.output_peak),
        stats.output_near_clips,
        stats.output_clips,
        stats.input_overruns,
        stats.output_underruns
    );

    if let Some(components) = components {
        text.push_str(&format!(
            "rails pre/pi/pwr {:>5.0}/{:>5.0}/{:>5.0} V\n\
             current first/pi/pwr {:>5.2}/{:>5.2}/{:>5.1} mA\n\
             cath {:>5.2} V   flux {:+.5}\n",
            components.preamp_voltage,
            components.phase_inverter_voltage,
            components.power_voltage,
            components.first_stage_current * 1_000.0,
            components.phase_inverter_current * 1_000.0,
            components.power_current * 1_000.0,
            components.power_cathode_bias,
            components.transformer_flux,
        ));
    }

    text.push_str("Press Ctrl-C to stop.\n");
    text
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

struct RotatingMonitorLog {
    path: PathBuf,
    capacity: usize,
    lines: VecDeque<String>,
}

impl RotatingMonitorLog {
    fn new(path: PathBuf, capacity: usize) -> Self {
        Self {
            path,
            capacity,
            lines: VecDeque::with_capacity(capacity.min(1024)),
        }
    }

    fn push_many(&mut self, lines: impl IntoIterator<Item = String>) -> io::Result<()> {
        for line in lines {
            self.lines.push_back(line);
            while self.lines.len() > self.capacity {
                self.lines.pop_front();
            }
        }
        self.flush()
    }

    fn flush(&self) -> io::Result<()> {
        let mut file = File::create(&self.path)?;
        for line in &self.lines {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }
}

struct WavInput {
    path: PathBuf,
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
}

enum RuntimeInput {
    Live(Consumer<f32>),
    Wav { samples: Vec<f32>, position: usize },
}

impl RuntimeInput {
    fn next_sample(&mut self, monitoring: &MonitorStats, monitor: bool) -> f32 {
        match self {
            Self::Live(consumer) => match consumer.pop() {
                Ok(sample) => sample,
                Err(_) => {
                    if monitor {
                        monitoring.record_output_underrun();
                    }
                    0.0
                }
            },
            Self::Wav { samples, position } => {
                let sample = samples[*position];
                *position = (*position + 1) % samples.len();
                if monitor {
                    monitoring.record_input(sample);
                }
                sample
            }
        }
    }
}

struct Args {
    input_device: Option<String>,
    output_device: String,
    input_wav: Option<PathBuf>,
    input_channel: usize,
    output_channels: Vec<usize>,
    sample_rate: u32,
    period_size: u32,
    controls: AmpControls,
    input_db: f32,
    input_gain: f32,
    output_db: f32,
    ir: bool,
    monitor: bool,
    monitor_log: PathBuf,
    model: String,
}

fn load_wav_input(path: &Path, input_channel: usize) -> Result<WavInput> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("could not open input WAV '{}'", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if input_channel >= channels {
        bail!(
            "input channel {} is unavailable; '{}' has {} channel(s)",
            input_channel + 1,
            path.display(),
            channels
        );
    }

    let mut samples = Vec::new();
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (index, sample) in reader.samples::<f32>().enumerate() {
                let sample = sample.with_context(|| {
                    format!("could not read float sample from '{}'", path.display())
                })?;
                if index % channels == input_channel {
                    samples.push(sample);
                }
            }
        }
        hound::SampleFormat::Int => {
            let scale = 2.0_f32.powi(spec.bits_per_sample as i32 - 1);
            for (index, sample) in reader.samples::<i32>().enumerate() {
                let sample = sample.with_context(|| {
                    format!("could not read int sample from '{}'", path.display())
                })? as f32
                    / scale;
                if index % channels == input_channel {
                    samples.push(sample);
                }
            }
        }
    }

    if samples.is_empty() {
        bail!("input WAV '{}' contains no samples", path.display());
    }

    Ok(WavInput {
        path: path.to_path_buf(),
        samples,
        channels,
        sample_rate: spec.sample_rate,
    })
}

fn main() -> Result<()> {
    let host = cpal::default_host();
    let args = parse_args(&host)?;
    let output_device = find_device(host.output_devices()?, &args.output_device, "output")?;

    let output_range = select_config(
        output_device.supported_output_configs()?,
        args.sample_rate,
        args.period_size,
        "output",
    )?;
    let output_channels = output_range.channels() as usize;

    if let Some(channel) = args
        .output_channels
        .iter()
        .find(|&&ch| ch >= output_channels)
    {
        bail!(
            "output channel {} is unavailable; '{}' exposes {} output channels",
            channel + 1,
            args.output_device,
            output_channels
        );
    }

    let output_config = stream_config(&output_range, args.sample_rate, args.period_size);
    let monitoring = Arc::new(MonitorStats::default());
    let component_telemetry = Arc::new(ComponentTelemetry::default());

    let (input_stream, input_description, input_channels, mut input_source) =
        if let Some(path) = &args.input_wav {
            let wav = load_wav_input(path, args.input_channel)?;
            if wav.sample_rate != args.sample_rate {
                bail!(
                    "input WAV '{}' is {} Hz, but --sample-rate is {}; use a matching sample rate",
                    wav.path.display(),
                    wav.sample_rate,
                    args.sample_rate
                );
            }
            let description = format!(
                "WAV '{}' channel {}",
                wav.path.display(),
                args.input_channel + 1
            );
            (
                None,
                description,
                wav.channels,
                RuntimeInput::Wav {
                    samples: wav.samples,
                    position: 0,
                },
            )
        } else {
            let input_device_name = args
                .input_device
                .as_ref()
                .context("missing --device, --input-device, or --input-wav")?;
            let input_device = find_device(host.input_devices()?, input_device_name, "input")?;
            let input_range = select_config(
                input_device.supported_input_configs()?,
                args.sample_rate,
                args.period_size,
                "input",
            )?;
            let input_channels = input_range.channels() as usize;
            if args.input_channel >= input_channels {
                bail!(
                    "input channel {} is unavailable; '{}' exposes {} input channels",
                    args.input_channel + 1,
                    input_device_name,
                    input_channels
                );
            }
            let input_config = stream_config(&input_range, args.sample_rate, args.period_size);
            let (mut producer, consumer) = RingBuffer::<f32>::new(args.period_size as usize * 8);
            let input_channel = args.input_channel;
            let monitor_enabled = args.monitor;
            let monitoring_input = monitoring.clone();
            let input_stream = input_device.build_input_stream(
                &input_config,
                move |data: &[f32], _| {
                    for frame in data.chunks_exact(input_channels) {
                        let sample = frame[input_channel];
                        if monitor_enabled {
                            monitoring_input.record_input(sample);
                        }
                        if producer.push(sample).is_err() && monitor_enabled {
                            monitoring_input.record_input_overrun();
                        }
                    }
                },
                |error| eprintln!("input stream error: {error}"),
                None,
            )?;
            (
                Some(input_stream),
                format!(
                    "device '{input_device_name}' channel {}",
                    args.input_channel + 1
                ),
                input_channels,
                RuntimeInput::Live(consumer),
            )
        };

    let monitor_enabled = args.monitor;
    let controls = args.controls;
    let input_gain = args.input_gain;
    let mut amp = VoxAmp::with_model(args.sample_rate as f32, &args.model);
    let mut speaker = args
        .ir
        .then(|| SpeakerStage::from_embedded_ir(args.sample_rate))
        .transpose()?;
    let ir_enabled = speaker.is_some();
    let selected_outputs = args.output_channels.clone();
    let monitoring_output = monitoring.clone();
    let component_output = component_telemetry.clone();
    let output_stream = output_device.build_output_stream(
        &output_config,
        move |data: &mut [f32], _| {
            for frame in data.chunks_exact_mut(output_channels) {
                let input =
                    input_source.next_sample(&monitoring_output, monitor_enabled) * input_gain;
                let amp_output = amp.process(input, controls);
                if monitor_enabled {
                    if let Some(operating_point) = amp.nox_operating_point() {
                        component_output.record_nox(operating_point);
                    }
                }
                let output = speaker
                    .as_mut()
                    .map_or(amp_output, |speaker| speaker.process(amp_output, true));
                if monitor_enabled {
                    monitoring_output.record_output(output);
                }
                frame.fill(0.0);
                for &channel in &selected_outputs {
                    frame[channel] = output;
                }
            }
        },
        |error| eprintln!("output stream error: {error}"),
        None,
    )?;

    output_stream.play()?;
    if let Some(input_stream) = &input_stream {
        input_stream.play()?;
    }
    if args.monitor {
        let monitor = monitoring.clone();
        let components = component_telemetry.clone();
        let monitor_model = args.model.clone();
        let monitor_log_path = args.monitor_log.clone();
        let show_components = monitor_model == "nox";
        std::thread::spawn(move || {
            let mut log = RotatingMonitorLog::new(monitor_log_path.clone(), MONITOR_LOG_LINES);
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let stats = monitor.snapshot_and_reset();
                let component_snapshot = show_components.then(|| components.snapshot());
                let timestamp = unix_timestamp();
                let mut log_lines =
                    vec![format!("ts={timestamp} {}", format_audio_monitor(&stats))];
                if let Some(components) = component_snapshot {
                    log_lines.push(format!(
                        "ts={timestamp} {}",
                        format_component_telemetry(components)
                    ));
                }

                let _ = log.push_many(log_lines);

                eprint!(
                    "\x1B[2J\x1B[H{}",
                    format_monitor_dashboard(
                        &stats,
                        component_snapshot,
                        &monitor_model,
                        &monitor_log_path
                    )
                );
                let _ = io::stderr().flush();
            }
        });
    }
    eprintln!(
        "VoxBox running: {} input channels, {} output channels, {} Hz, {} samples",
        input_channels, output_channels, args.sample_rate, args.period_size
    );
    eprintln!("Input source: {input_description}");
    eprintln!(
        "Speaker IR: {}",
        if ir_enabled { "enabled" } else { "disabled" }
    );
    eprintln!(
        "Controls: Model {}, Input {:+.1} dB, Volume {:.1}, Bass {:.1}, Treble {:.1}, Cut/Mid {:.1}, Output {:+.1} dB",
        args.model,
        args.input_db,
        controls.volume * 10.0,
        controls.bass * 10.0,
        controls.treble * 10.0,
        controls.cut * 10.0,
        args.output_db
    );
    eprintln!("Press Ctrl-C to stop.");

    loop {
        thread::park();
    }
}

fn parse_args(host: &cpal::Host) -> Result<Args> {
    let mut input_device = None;
    let mut output_device = None;
    let mut input_wav = None;
    let mut input_channel = 1;
    let mut output_channels = "1,2".to_owned();
    let mut sample_rate = 48_000;
    let mut period_size = 256;
    let mut volume = 5.5;
    let mut bass = 5.0;
    let mut treble = 6.0;
    let mut cut = 3.5;
    let mut drive = 0.0;
    let mut presence = 0.0;
    let mut sag = 0.0;
    let mut input_db = 0.0;
    let mut output_db = -9.0;
    let mut ir = false;
    let mut monitor = false;
    let mut monitor_log = PathBuf::from("voxbox-monitor.log");
    let mut model = "ac30".to_owned();
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--device" => {
                let name = next_value(&mut args, "--device")?;
                input_device = Some(name.clone());
                output_device = Some(name);
            }
            "--input-device" => input_device = Some(next_value(&mut args, "--input-device")?),
            "--output-device" => output_device = Some(next_value(&mut args, "--output-device")?),
            "--input-wav" => input_wav = Some(PathBuf::from(next_value(&mut args, "--input-wav")?)),
            "--input-channel" => {
                input_channel = next_value(&mut args, "--input-channel")?.parse()?
            }
            "--output-channels" => output_channels = next_value(&mut args, "--output-channels")?,
            "--sample-rate" => sample_rate = next_value(&mut args, "--sample-rate")?.parse()?,
            "--period-size" => period_size = next_value(&mut args, "--period-size")?.parse()?,
            "--volume" | "--gain" => volume = parse_pot(&mut args, "--volume")?,
            "--bass" => bass = parse_pot(&mut args, "--bass")?,
            "--treble" | "--tone" => treble = parse_pot(&mut args, "--treble")?,
            "--cut" => cut = parse_pot(&mut args, "--cut")?,
            "--input-db" => input_db = next_value(&mut args, "--input-db")?.parse()?,
            "--output-db" => output_db = next_value(&mut args, "--output-db")?.parse()?,
            "--ir" => ir = true,
            "--preset" => {
                let name = next_value(&mut args, "--preset")?.to_lowercase();
                match name.as_str() {
                    "dumble" | "dumble-clean" => {
                        // Dumble Overdrive Special - clean
                        volume = 6.5;
                        bass = 5.5;
                        treble = 6.0;
                        cut = 3.8;
                        output_db = -12.0;
                        drive = 2.0;
                        presence = 1.0;
                        sag = 0.2;
                        model = "dumble".to_owned();
                    }
                    "dumble-crunch" => {
                        volume = 8.0;
                        bass = 5.2;
                        treble = 6.0;
                        cut = 4.4;
                        output_db = -14.0;
                        drive = 4.5;
                        presence = 1.5;
                        sag = 0.3;
                        model = "dumble".to_owned();
                    }
                    "dumble-driven" => {
                        volume = 9.5;
                        bass = 5.0;
                        treble = 5.8;
                        cut = 5.0;
                        output_db = -16.0;
                        drive = 7.5;
                        presence = 2.0;
                        sag = 0.45;
                        model = "dumble".to_owned();
                    }
                    "jcm800" | "jcm800-crunch" => {
                        volume = 7.0;
                        bass = 6.0;
                        treble = 6.2;
                        cut = 5.0;
                        output_db = -18.0;
                        drive = 4.0;
                        presence = 5.5;
                        sag = 0.25;
                        model = "jcm800".to_owned();
                    }
                    "jcm800-driven" => {
                        volume = 8.6;
                        bass = 6.2;
                        treble = 6.0;
                        cut = 5.4;
                        output_db = -20.0;
                        drive = 7.0;
                        presence = 6.2;
                        sag = 0.35;
                        model = "jcm800".to_owned();
                    }
                    "nox" | "nox-edge" => {
                        volume = 6.4;
                        bass = 5.8;
                        treble = 5.6;
                        cut = 4.5;
                        output_db = -18.0;
                        drive = 2.5;
                        presence = 3.0;
                        sag = 5.0;
                        model = "nox".to_owned();
                    }
                    "nox-driven" => {
                        volume = 7.6;
                        bass = 5.2;
                        treble = 6.1;
                        cut = 4.7;
                        output_db = -18.0;
                        drive = 6.8;
                        presence = 4.4;
                        sag = 7.0;
                        model = "nox".to_owned();
                    }
                    _ => bail!("unknown preset '{name}'"),
                }
            }
            "--model" => {
                model = match next_value(&mut args, "--model")?.to_lowercase().as_str() {
                    "ac30" | "vox" => "ac30".to_owned(),
                    "dumble" | "ods" => "dumble".to_owned(),
                    "jcm800" | "jcm-800" | "marshall" => "jcm800".to_owned(),
                    "nox" => "nox".to_owned(),
                    name => bail!("unknown model '{name}'"),
                };
            }
            "--drive" => drive = parse_pot(&mut args, "--drive")?,
            "--presence" => presence = parse_pot(&mut args, "--presence")?,
            "--sag" => sag = parse_pot(&mut args, "--sag")?,
            "--monitor" => monitor = true,
            "--monitor-log" => monitor_log = PathBuf::from(next_value(&mut args, "--monitor-log")?),
            "--list-devices" => {
                print_devices(host)?;
                std::process::exit(0);
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => bail!("unknown argument '{arg}'; use --help"),
        }
    }

    if input_channel == 0 {
        bail!("--input-channel is one-based and must be at least 1");
    }
    if sample_rate == 0 {
        bail!("--sample-rate must be greater than zero");
    }
    if period_size == 0 {
        bail!("--period-size must be greater than zero");
    }
    if !(-60.0..=6.0).contains(&output_db) {
        bail!("--output-db must be between -60 and +6");
    }
    if !(-60.0..=24.0).contains(&input_db) {
        bail!("--input-db must be between -60 and +24");
    }
    let output_channels = output_channels
        .split(',')
        .map(|value| value.trim().parse::<usize>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if output_channels.is_empty() || output_channels.contains(&0) {
        bail!("--output-channels must contain one-based channel numbers");
    }

    Ok(Args {
        input_device,
        output_device: output_device.context("missing --device or --output-device")?,
        input_wav,
        input_channel: input_channel - 1,
        output_channels: output_channels.into_iter().map(|ch| ch - 1).collect(),
        sample_rate,
        period_size,
        controls: AmpControls {
            volume: volume / 10.0,
            bass: bass / 10.0,
            treble: treble / 10.0,
            cut: cut / 10.0,
            output: 10.0_f32.powf(output_db / 20.0),
            drive: drive / 10.0,
            presence: presence / 10.0,
            sag: sag / 10.0,
        },
        input_db,
        input_gain: 10.0_f32.powf(input_db / 20.0),
        output_db,
        ir,
        monitor,
        monitor_log,
        model,
    })
}

fn parse_pot(args: &mut impl Iterator<Item = String>, option: &str) -> Result<f32> {
    let value = next_value(args, option)?.parse::<f32>()?;
    if !(0.0..=10.0).contains(&value) {
        bail!("{option} must be between 0 and 10");
    }
    Ok(value)
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("missing value for {option}"))
}

fn find_device(
    devices: impl Iterator<Item = Device>,
    wanted: &str,
    direction: &str,
) -> Result<Device> {
    devices
        .filter_map(|device| device.name().ok().map(|name| (device, name)))
        .find(|(_, name)| name == wanted)
        .map(|(device, _)| device)
        .with_context(|| {
            format!("could not find {direction} device '{wanted}'; use --list-devices")
        })
}

fn select_config(
    configs: impl Iterator<Item = SupportedStreamConfigRange>,
    sample_rate: u32,
    period_size: u32,
    direction: &str,
) -> Result<SupportedStreamConfigRange> {
    let rate = SampleRate(sample_rate);
    configs
        .filter(|config| config.sample_format() == SampleFormat::F32)
        .find(|config| {
            (config.min_sample_rate()..=config.max_sample_rate()).contains(&rate)
                && match config.buffer_size() {
                    cpal::SupportedBufferSize::Range { min, max } => {
                        (*min..=*max).contains(&period_size)
                    }
                    cpal::SupportedBufferSize::Unknown => true,
                }
        })
        .with_context(|| {
            format!(
                "no f32 {direction} configuration supports {sample_rate} Hz / {period_size} samples"
            )
        })
}

fn stream_config(
    range: &SupportedStreamConfigRange,
    sample_rate: u32,
    period_size: u32,
) -> StreamConfig {
    StreamConfig {
        channels: range.channels(),
        sample_rate: SampleRate(sample_rate),
        buffer_size: BufferSize::Fixed(period_size),
    }
}

fn print_devices(host: &cpal::Host) -> Result<()> {
    eprintln!("Input devices:");
    for device in host.input_devices()? {
        eprintln!("  {}", device.name()?);
    }
    eprintln!("Output devices:");
    for device in host.output_devices()? {
        eprintln!("  {}", device.name()?);
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "Usage: voxbox-standalone --device NAME [OPTIONS]\n\
         \n\
         Options:\n\
         \x20 --device NAME             Use the same input and output device\n\
         \x20 --input-device NAME       Input device name\n\
         \x20 --output-device NAME      Output device name\n\
         \x20 --input-wav PATH          Loop a mono/stereo WAV file instead of live input\n\
         \x20 --input-channel N         One-based guitar input [default: 1]\n\
         \x20 --output-channels N,N     One-based monitor outputs [default: 1,2]\n\
         \x20 --sample-rate HZ          Sample rate [default: 48000]\n\
         \x20 --period-size SAMPLES     Buffer size [default: 256]\n\
         \x20 --volume N                Top Boost volume, 0-10 [default: 5.5]\n\
         \x20 --bass N                  Top Boost bass, 0-10 [default: 5.0]\n\
         \x20 --treble N                Top Boost treble, 0-10 [default: 6.0]\n\
         \x20 --cut N                   Power amp Cut, 0-10 [default: 3.5]\n\
         \x20 --model NAME              Amp model: ac30, dumble, jcm800, nox [default: ac30]\n\
         \x20 --input-db DB             Interface input calibration [default: 0]\n\
         \x20 --output-db DB            Safety output trim [default: -9]\n\
         \x20 --monitor                 Show refreshing input/output VU meters\n\
         \x20 --monitor-log PATH        Rotating monitor log [default: voxbox-monitor.log]\n\
         \x20 --ir                      Enable the embedded 200 ms speaker IR\n\
         \x20 --list-devices            List CoreAudio devices"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pot_values_use_real_amp_scale() {
        let mut valid = ["7.5".to_owned()].into_iter();
        assert_eq!(parse_pot(&mut valid, "--volume").unwrap(), 7.5);

        let mut too_high = ["10.1".to_owned()].into_iter();
        assert!(parse_pot(&mut too_high, "--volume").is_err());

        let mut negative = ["-0.1".to_owned()].into_iter();
        assert!(parse_pot(&mut negative, "--cut").is_err());
    }

    #[test]
    fn loads_sample_wav_input_channel() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../samples/teenager-electric-guitar-smooth-chords-dry_94bpm_G_major.wav");
        let wav = load_wav_input(&path, 0).unwrap();

        assert_eq!(wav.sample_rate, 44_100);
        assert_eq!(wav.channels, 2);
        assert!(!wav.samples.is_empty());
        assert!(wav.samples.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn formats_component_telemetry_for_monitoring() {
        let telemetry = ComponentTelemetrySnapshot {
            preamp_voltage: 274.2,
            phase_inverter_voltage: 296.7,
            power_voltage: 314.8,
            first_stage_current: 0.00124,
            phase_inverter_current: 0.0028,
            power_current: 0.071,
            power_cathode_bias: 9.4,
            transformer_flux: -0.00031,
        };

        let line = format_component_telemetry(telemetry);

        assert!(line.starts_with("CMP rails pre/pi/pwr 274/297/315 V"));
        assert!(line.contains("I first/pi/pwr 1.24/2.80/71.0 mA"));
        assert!(line.contains("cath 9.40 V"));
        assert!(line.contains("flux -0.00031"));
    }

    #[test]
    fn formats_monitor_dashboard_as_vu_meters() {
        let stats = MonitorSnapshot {
            input_sum_squares: (0.25 * RMS_SCALE) as u64,
            output_sum_squares: (0.01 * RMS_SCALE) as u64,
            input_count: 1,
            output_count: 1,
            input_peak: 0.8,
            output_peak: 0.2,
            input_near_clips: 1,
            output_near_clips: 0,
            input_clips: 0,
            output_clips: 0,
            input_overruns: 2,
            output_underruns: 3,
        };

        let dashboard = format_monitor_dashboard(
            &stats,
            Some(ComponentTelemetrySnapshot {
                preamp_voltage: 274.2,
                phase_inverter_voltage: 296.7,
                power_voltage: 314.8,
                first_stage_current: 0.00124,
                phase_inverter_current: 0.0028,
                power_current: 0.071,
                power_cathode_bias: 9.4,
                transformer_flux: -0.00031,
            }),
            "nox",
            Path::new("voxbox-monitor.log"),
        );

        assert!(dashboard.contains("VoxBox monitor  model nox"));
        assert!(dashboard.contains("input  rms"));
        assert!(dashboard.contains("-6.0 dBFS ["));
        assert!(dashboard.contains("output rms"));
        assert!(dashboard.contains("-20.0 dBFS ["));
        assert!(dashboard.contains("rails pre/pi/pwr"));
        assert!(dashboard.contains("Press Ctrl-C to stop."));
    }

    #[test]
    fn rotating_monitor_log_keeps_latest_lines() {
        let path =
            std::env::temp_dir().join(format!("voxbox-monitor-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mut log = RotatingMonitorLog::new(path.clone(), 3);
        log.push_many(["a", "b", "c", "d"].into_iter().map(str::to_owned))
            .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines = text.lines().collect::<Vec<_>>();
        assert_eq!(lines, vec!["b", "c", "d"]);

        let _ = std::fs::remove_file(path);
    }
}
