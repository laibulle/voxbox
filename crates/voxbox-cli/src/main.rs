use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, SampleFormat, SampleRate, StreamConfig, SupportedStreamConfigRange,
};
use nih_plug::params::InternalParamMut;
use rtrb::RingBuffer;
use std::env;
use std::sync::Arc;
use std::thread;
use voxbox_core::amp::{AmpControls, VoxAmp};
use voxbox_core::ir::SpeakerStage;
use voxbox_core::VoxBoxParams;

struct Args {
    input_device: String,
    output_device: String,
    input_channel: usize,
    output_channels: Vec<usize>,
    sample_rate: u32,
    period_size: u32,
    initial_params: Arc<VoxBoxParams>,
    input_gain: f32,
    input_db: f32,
    output_db: f32,
}

fn main() -> Result<()> {
    let host = cpal::default_host();
    let args = parse_args(&host)?;
    let params = args.initial_params.clone();
    let audio_params = params.clone();

    eprintln!("VoxBox CLI running...");
    eprintln!(
        "Controls: Input {:+.1} dB, Volume {:.1}, Bass {:.1}, Treble {:.1}, Cut {:.1}, Output {:+.1} dB",
        args.input_db,
        audio_params.gain.value() * 10.0,
        audio_params.bass.value() * 10.0,
        audio_params.tone.value() * 10.0,
        audio_params.cut.value() * 10.0,
        args.output_db
    );
    eprintln!(
        "Speaker IR: {}",
        if audio_params.speaker_ir.value() {
            "enabled"
        } else {
            "disabled"
        }
    );

    thread::spawn(move || {
        if let Err(e) = run_audio(host, args, audio_params) {
            eprintln!("Audio error: {}", e);
        }
    });

    eprintln!("Press Ctrl-C to stop.");
    loop {
        thread::park();
    }
}

fn run_audio(host: cpal::Host, args: Args, params: Arc<VoxBoxParams>) -> Result<()> {
    let input_device = find_device(host.input_devices()?, &args.input_device, "input")?;
    let output_device = find_device(host.output_devices()?, &args.output_device, "output")?;

    let input_range = select_config(
        input_device.supported_input_configs()?,
        args.sample_rate,
        args.period_size,
        "input",
    )?;
    let output_range = select_config(
        output_device.supported_output_configs()?,
        args.sample_rate,
        args.period_size,
        "output",
    )?;
    let input_channels = input_range.channels() as usize;
    let output_channels = output_range.channels() as usize;

    let input_config = stream_config(&input_range, args.sample_rate, args.period_size);
    let output_config = stream_config(&output_range, args.sample_rate, args.period_size);
    let (mut producer, mut consumer) = RingBuffer::<f32>::new(args.period_size as usize * 8);
    let input_channel = args.input_channel;

    let input_stream = input_device.build_input_stream(
        &input_config,
        move |data: &[f32], _| {
            for frame in data.chunks_exact(input_channels) {
                let _ = producer.push(frame[input_channel]);
            }
        },
        |error| eprintln!("input stream error: {error}"),
        None,
    )?;

    let input_gain = args.input_gain;
    let mut amp = VoxAmp::new(args.sample_rate as f32);
    let mut speaker = SpeakerStage::from_embedded_ir(args.sample_rate)?;
    let selected_outputs = args.output_channels;

    let output_stream = output_device.build_output_stream(
        &output_config,
        move |data: &mut [f32], _| {
            let controls = AmpControls {
                volume: params.gain.value(),
                bass: params.bass.value(),
                treble: params.tone.value(),
                cut: params.cut.value(),
                output: params.master.value(),
            };
            let ir_enabled = params.speaker_ir.value();

            for frame in data.chunks_exact_mut(output_channels) {
                let input = consumer.pop().unwrap_or(0.0) * input_gain;
                let amp_output = amp.process(input, controls);
                let output = speaker.process(amp_output, ir_enabled);
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
    input_stream.play()?;

    loop {
        thread::park();
    }
}

fn parse_args(host: &cpal::Host) -> Result<Args> {
    let mut input_device = None;
    let mut output_device = None;
    let mut input_channel = 1;
    let mut output_channels = "1,2".to_owned();
    let mut sample_rate = 48_000;
    let mut period_size = 256;

    let initial_params = VoxBoxParams::default();
    let mut input_db = 0.0;
    let mut output_db = -9.0;

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
            "--input-channel" => {
                input_channel = next_value(&mut args, "--input-channel")?.parse()?;
            }
            "--output-channels" => output_channels = next_value(&mut args, "--output-channels")?,
            "--sample-rate" => sample_rate = next_value(&mut args, "--sample-rate")?.parse()?,
            "--period-size" => period_size = next_value(&mut args, "--period-size")?.parse()?,
            "--volume" | "--gain" => {
                let val = parse_pot(&mut args, "--volume")?;
                unsafe { initial_params.gain._internal_set_plain_value(val / 10.0) };
            }
            "--bass" => {
                let val = parse_pot(&mut args, "--bass")?;
                unsafe { initial_params.bass._internal_set_plain_value(val / 10.0) };
            }
            "--treble" | "--tone" => {
                let val = parse_pot(&mut args, "--treble")?;
                unsafe { initial_params.tone._internal_set_plain_value(val / 10.0) };
            }
            "--cut" => {
                let val = parse_pot(&mut args, "--cut")?;
                unsafe { initial_params.cut._internal_set_plain_value(val / 10.0) };
            }
            "--input-db" => input_db = next_value(&mut args, "--input-db")?.parse()?,
            "--output-db" => {
                output_db = next_value(&mut args, "--output-db")?.parse::<f32>()?;
                unsafe {
                    initial_params
                        .master
                        ._internal_set_plain_value(nih_plug::prelude::util::db_to_gain(output_db))
                };
            }
            "--ir" => unsafe {
                initial_params.speaker_ir._internal_set_plain_value(true);
            },
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

    let output_channels = output_channels
        .split(',')
        .map(|value| value.trim().parse::<usize>())
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(Args {
        input_device: input_device.unwrap_or_else(|| {
            host.default_input_device()
                .map(|d| d.name().unwrap_or_default())
                .unwrap_or_default()
        }),
        output_device: output_device.unwrap_or_else(|| {
            host.default_output_device()
                .map(|d| d.name().unwrap_or_default())
                .unwrap_or_default()
        }),
        input_channel: input_channel - 1,
        output_channels: output_channels.into_iter().map(|ch| ch - 1).collect(),
        sample_rate,
        period_size,
        initial_params: Arc::new(initial_params),
        input_gain: 10.0_f32.powf(input_db / 20.0),
        input_db,
        output_db,
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
        "Usage: voxbox-cli --device NAME [OPTIONS]\n\
         \n\
         Options:\n\
         \x20 --device NAME             Use the same input and output device\n\
         \x20 --input-device NAME       Input device name\n\
         \x20 --output-device NAME      Output device name\n\
         \x20 --input-channel N         One-based guitar input [default: 1]\n\
         \x20 --output-channels N,N     One-based monitor outputs [default: 1,2]\n\
         \x20 --sample-rate HZ          Sample rate [default: 48000]\n\
         \x20 --period-size SAMPLES     Buffer size [default: 256]\n\
         \x20 --volume N                Top Boost volume, 0-10 [default: 5.5]\n\
         \x20 --bass N                  Top Boost bass, 0-10 [default: 5.0]\n\
         \x20 --treble N                Top Boost treble, 0-10 [default: 6.0]\n\
         \x20 --cut N                   Power amp Cut, 0-10 [default: 3.5]\n\
         \x20 --input-db DB             Interface input calibration [default: 0]\n\
         \x20 --output-db DB            Safety output trim [default: -9]\n\
         \x20 --ir                      Enable the embedded speaker IR\n\
         \x20 --list-devices            List audio devices"
    );
}
