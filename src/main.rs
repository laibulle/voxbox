use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, SampleFormat, SampleRate, StreamConfig, SupportedStreamConfigRange,
};
use rtrb::RingBuffer;
use std::env;
use std::thread;
use voxbox::amp::{AmpControls, VoxAmp};
use voxbox::ir::SpeakerStage;

struct Args {
    input_device: String,
    output_device: String,
    input_channel: usize,
    output_channels: Vec<usize>,
    sample_rate: u32,
    period_size: u32,
    ir: bool,
}

fn main() -> Result<()> {
    let host = cpal::default_host();
    let args = parse_args(&host)?;
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

    if args.input_channel >= input_channels {
        bail!(
            "input channel {} is unavailable; '{}' exposes {} input channels",
            args.input_channel + 1,
            args.input_device,
            input_channels
        );
    }
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

    let controls = AmpControls {
        gain: 0.55,
        cut: 0.35,
        tone: 0.6,
        master: 10.0_f32.powf(-9.0 / 20.0),
    };
    let mut amp = VoxAmp::new(args.sample_rate as f32);
    let mut speaker = args
        .ir
        .then(|| SpeakerStage::from_embedded_ir(args.sample_rate))
        .transpose()?;
    let ir_enabled = speaker.is_some();
    let selected_outputs = args.output_channels;
    let output_stream = output_device.build_output_stream(
        &output_config,
        move |data: &mut [f32], _| {
            for frame in data.chunks_exact_mut(output_channels) {
                let input = consumer.pop().unwrap_or(0.0);
                let amp_output = amp.process(input, controls);
                let output = speaker
                    .as_mut()
                    .map_or(amp_output, |speaker| speaker.process(amp_output, true));
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
    eprintln!(
        "VoxBox running: {} input channels, {} output channels, {} Hz, {} samples",
        input_channels, output_channels, args.sample_rate, args.period_size
    );
    eprintln!(
        "Speaker IR: {}",
        if ir_enabled { "enabled" } else { "disabled" }
    );
    eprintln!("Press Ctrl-C to stop.");

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
    let mut ir = false;
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
                input_channel = next_value(&mut args, "--input-channel")?.parse()?
            }
            "--output-channels" => output_channels = next_value(&mut args, "--output-channels")?,
            "--sample-rate" => sample_rate = next_value(&mut args, "--sample-rate")?.parse()?,
            "--period-size" => period_size = next_value(&mut args, "--period-size")?.parse()?,
            "--ir" => ir = true,
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
    let output_channels = output_channels
        .split(',')
        .map(|value| value.trim().parse::<usize>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if output_channels.is_empty() || output_channels.contains(&0) {
        bail!("--output-channels must contain one-based channel numbers");
    }

    Ok(Args {
        input_device: input_device.context("missing --device or --input-device")?,
        output_device: output_device.context("missing --device or --output-device")?,
        input_channel: input_channel - 1,
        output_channels: output_channels.into_iter().map(|ch| ch - 1).collect(),
        sample_rate,
        period_size,
        ir,
    })
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
         \x20 --input-channel N         One-based guitar input [default: 1]\n\
         \x20 --output-channels N,N     One-based monitor outputs [default: 1,2]\n\
         \x20 --sample-rate HZ          Sample rate [default: 48000]\n\
         \x20 --period-size SAMPLES     Buffer size [default: 256]\n\
         \x20 --ir                      Enable the embedded 200 ms speaker IR\n\
         \x20 --list-devices            List CoreAudio devices"
    );
}
