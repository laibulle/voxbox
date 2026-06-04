use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use nih_plug::prelude::Param;
use rtrb::RingBuffer;
use std::sync::Arc;
use voxbox_core::amp::{AmpControls, VoxAmp};
use voxbox_core::ir::SpeakerStage;
use voxbox_core::VoxBoxParams;
use voxbox_ui::{Message, UIContext, VoxBoxUI};

use iced::Element;

struct StandaloneApp {
    ui: VoxBoxUI,
}

impl StandaloneApp {
    fn new(params: Arc<VoxBoxParams>) -> (Self, iced::Task<Message>) {
        (
            Self {
                ui: VoxBoxUI::new(params, UIContext::Standalone),
            },
            iced::Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        self.ui.update(message)
    }

    fn view(&self) -> Element<'_, Message> {
        self.ui.view()
    }
}

fn main() -> Result<()> {
    let params = Arc::new(VoxBoxParams::default());
    let audio_params = params.clone();

    // Start audio thread
    std::thread::spawn(move || {
        if let Err(e) = run_audio(audio_params) {
            eprintln!("Audio error: {}", e);
        }
    });

    // Run Iced UI
    let app_params = params.clone();
    iced::application(
        move || StandaloneApp::new(app_params.clone()),
        StandaloneApp::update,
        StandaloneApp::view,
    )
    .title("VoxBox Standalone")
    .run()?;

    Ok(())
}

fn run_audio(params: Arc<VoxBoxParams>) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no input device"))?;
    let output_device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;

    let config: StreamConfig = device.default_input_config()?.into();
    let sample_rate = config.sample_rate.0;

    let (mut producer, mut consumer) = RingBuffer::<f32>::new(1024);

    let input_stream = device.build_input_stream(
        &config,
        move |data: &[f32], _| {
            for &sample in data {
                let _ = producer.push(sample);
            }
        },
        |e| eprintln!("Input error: {}", e),
        None,
    )?;

    let mut amp = VoxAmp::new(sample_rate as f32);
    let mut speaker = SpeakerStage::from_embedded_ir(sample_rate)?;

    let output_stream = output_device.build_output_stream(
        &config,
        move |data: &mut [f32], _| {
            let controls = AmpControls {
                volume: params.gain.modulated_plain_value(),
                bass: params.bass.modulated_plain_value(),
                treble: params.tone.modulated_plain_value(),
                cut: params.cut.modulated_plain_value(),
                output: params.master.modulated_plain_value(),
            };

            for sample in data.iter_mut() {
                let input = consumer.pop().unwrap_or(0.0);
                let amp_output = amp.process(input, controls);
                *sample = speaker.process(amp_output, params.speaker_ir.value());
            }
        },
        |e| eprintln!("Output error: {}", e),
        None,
    )?;

    input_stream.play()?;
    output_stream.play()?;

    loop {
        std::thread::park();
    }
}
