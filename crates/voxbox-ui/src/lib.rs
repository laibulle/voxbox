use nih_plug::params::InternalParamMut;
use nih_plug::prelude::Param;
use nih_plug_iced::iced::widget::{column, container, row, slider, text, toggler, Space};
use nih_plug_iced::iced::{Alignment, Element, Length, Task};
use nih_plug_iced::NihGuiContext;
use std::sync::Arc;
use voxbox_core::VoxBoxParams;

pub enum UIContext {
    Plugin(NihGuiContext),
    Standalone,
}

pub struct VoxBoxUI {
    pub params: Arc<VoxBoxParams>,
    pub context: UIContext,
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    GainChanged(f32),
    BassChanged(f32),
    CutChanged(f32),
    ToneChanged(f32),
    MasterChanged(f32),
    SpeakerIrToggled(bool),
}

impl VoxBoxUI {
    pub fn new(params: Arc<VoxBoxParams>, context: UIContext) -> Self {
        Self { params, context }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match &self.context {
            UIContext::Plugin(nih_ctx) => {
                let setter = nih_ctx.param_setter();
                match message {
                    Message::GainChanged(value) => {
                        setter.set_parameter_normalized(&self.params.gain, value);
                    }
                    Message::BassChanged(value) => {
                        setter.set_parameter_normalized(&self.params.bass, value);
                    }
                    Message::CutChanged(value) => {
                        setter.set_parameter_normalized(&self.params.cut, value);
                    }
                    Message::ToneChanged(value) => {
                        setter.set_parameter_normalized(&self.params.tone, value);
                    }
                    Message::MasterChanged(value) => {
                        setter.set_parameter_normalized(&self.params.master, value);
                    }
                    Message::SpeakerIrToggled(value) => {
                        setter.set_parameter(&self.params.speaker_ir, value);
                    }
                }
            }
            UIContext::Standalone => match message {
                Message::GainChanged(value) => unsafe {
                    self.params.gain._internal_set_normalized_value(value);
                },
                Message::BassChanged(value) => unsafe {
                    self.params.bass._internal_set_normalized_value(value);
                },
                Message::CutChanged(value) => unsafe {
                    self.params.cut._internal_set_normalized_value(value);
                },
                Message::ToneChanged(value) => unsafe {
                    self.params.tone._internal_set_normalized_value(value);
                },
                Message::MasterChanged(value) => unsafe {
                    self.params.master._internal_set_normalized_value(value);
                },
                Message::SpeakerIrToggled(value) => unsafe {
                    self.params
                        .speaker_ir
                        ._internal_set_normalized_value(if value { 1.0 } else { 0.0 });
                },
            },
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = column![
            text("VoxBox").size(40),
            Space::new().height(20.0),
            row![
                self.knob(
                    "Volume",
                    self.params.gain.unmodulated_normalized_value(),
                    Message::GainChanged
                ),
                self.knob(
                    "Bass",
                    self.params.bass.unmodulated_normalized_value(),
                    Message::BassChanged
                ),
                self.knob(
                    "Treble",
                    self.params.tone.unmodulated_normalized_value(),
                    Message::ToneChanged
                ),
                self.knob(
                    "Cut",
                    self.params.cut.unmodulated_normalized_value(),
                    Message::CutChanged
                ),
            ]
            .spacing(20)
            .align_y(Alignment::Center),
            Space::new().height(20.0),
            row![
                text("Speaker IR"),
                toggler(self.params.speaker_ir.value()).on_toggle(Message::SpeakerIrToggled),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            Space::new().height(20.0),
            self.knob(
                "Output Trim",
                self.params.master.unmodulated_normalized_value(),
                Message::MasterChanged
            ),
        ]
        .padding(20)
        .align_x(Alignment::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    fn knob<F>(&self, label: &'static str, value: f32, on_change: F) -> Element<'static, Message>
    where
        F: 'static + Fn(f32) -> Message,
    {
        column![
            text(label).size(16),
            slider(0.0..=1.0, value, on_change).width(Length::Fixed(100.0)),
            text(format!("{:.1}%", value * 100.0)).size(12),
        ]
        .align_x(Alignment::Center)
        .spacing(5)
        .into()
    }
}
