use iced::widget::{button, checkbox, column, container, progress_bar, row, slider, text};
use iced::{Alignment, Background, Color, Element, Length, Vector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Amp,
    Pedal,
}

struct SkeuoButton;

impl button::StyleSheet for SkeuoButton {
    type Style = iced::theme::Theme;

    fn active(&self, _style: &Self::Style) -> button::Appearance {
        button::Appearance {
            background: Some(Background::Color(Color::from_rgb(0.22, 0.20, 0.17))),
            border_radius: 16.0.into(),
            border_width: 1.0,
            border_color: Color::from_rgb(0.80, 0.75, 0.62),
            shadow_offset: Vector::new(0.0, 3.0),
            text_color: Color::from_rgb(0.95, 0.92, 0.82),
            ..button::Appearance::default()
        }
    }
}

struct SkeuoContainer(Color);

impl container::StyleSheet for SkeuoContainer {
    type Style = iced::theme::Theme;

    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            text_color: Some(Color::WHITE),
            background: Some(Background::Color(self.0)),
            border_radius: 18.0.into(),
            border_width: 1.0,
            border_color: Color::from_rgb(0.70, 0.63, 0.51),
            ..container::Appearance::default()
        }
    }
}

struct SkeuoProgressBar;

impl progress_bar::StyleSheet for SkeuoProgressBar {
    type Style = iced::theme::Theme;

    fn appearance(&self, _style: &Self::Style) -> progress_bar::Appearance {
        progress_bar::Appearance {
            background: Background::Color(Color::from_rgb(0.13, 0.11, 0.09)),
            bar: Background::Color(Color::from_rgb(0.82, 0.75, 0.60)),
            border_radius: 10.0.into(),
        }
    }
}

fn skeuo_container(background: Color) -> iced::theme::Container {
    iced::theme::Container::Custom(Box::new(SkeuoContainer(background)))
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectDevice(usize),
    ToggleBypass(bool),
    ToggleDumble(bool),
    SetModel(Model),
    GainChanged(f32),
    BassChanged(f32),
    TrebleChanged(f32),
    CutChanged(f32),
    MasterChanged(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Ac30,
    Dumble,
}

#[derive(Debug, Clone)]
pub struct DeviceState {
    pub name: String,
    pub kind: DeviceKind,
    pub bypassed: bool,
    pub gain: f32,
    pub bass: f32,
    pub treble: f32,
    pub cut: f32,
    pub master: f32,
    pub dumble: bool,
    pub model: Model,
}

impl DeviceState {
    pub fn new_amp(name: &str) -> Self {
        Self {
            name: name.to_string(),
            kind: DeviceKind::Amp,
            bypassed: false,
            gain: 0.55,
            bass: 0.50,
            treble: 0.60,
            cut: 0.35,
            master: 0.50,
            dumble: false,
            model: Model::Ac30,
        }
    }

    pub fn new_pedal(name: &str) -> Self {
        Self {
            name: name.to_string(),
            kind: DeviceKind::Pedal,
            bypassed: false,
            gain: 0.40,
            bass: 0.45,
            treble: 0.50,
            cut: 0.30,
            master: 0.70,
            dumble: false,
            model: Model::Ac30,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VoxBoxUi {
    pub devices: Vec<DeviceState>,
    pub selected_index: usize,
}

impl Default for VoxBoxUi {
    fn default() -> Self {
        Self {
            devices: vec![
                DeviceState::new_amp("VoxBox Top Boost"),
                DeviceState::new_pedal("Crunch Pedal"),
                DeviceState::new_pedal("Reverb Pedal"),
            ],
            selected_index: 0,
        }
    }
}

impl VoxBoxUi {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::SelectDevice(index) => {
                if index < self.devices.len() {
                    self.selected_index = index;
                }
            }
            Message::ToggleBypass(value) => {
                if let Some(device) = self.devices.get_mut(self.selected_index) {
                    device.bypassed = value;
                }
            }
            Message::ToggleDumble(value) => {
                if let Some(device) = self.devices.get_mut(self.selected_index) {
                    device.dumble = value;
                }
            }
            Message::SetModel(m) => {
                if let Some(device) = self.devices.get_mut(self.selected_index) {
                    device.model = m;
                }
            }
            Message::GainChanged(value) => {
                if let Some(device) = self.devices.get_mut(self.selected_index) {
                    device.gain = value;
                }
            }
            Message::BassChanged(value) => {
                if let Some(device) = self.devices.get_mut(self.selected_index) {
                    device.bass = value;
                }
            }
            Message::TrebleChanged(value) => {
                if let Some(device) = self.devices.get_mut(self.selected_index) {
                    device.treble = value;
                }
            }
            Message::CutChanged(value) => {
                if let Some(device) = self.devices.get_mut(self.selected_index) {
                    device.cut = value;
                }
            }
            Message::MasterChanged(value) => {
                if let Some(device) = self.devices.get_mut(self.selected_index) {
                    device.master = value;
                }
            }
        }
    }

    fn render_knob(&self, label: &str, value: f32) -> Element<'_, Message> {
        container(
            column![
                container(text(format!("{:.0}", value * 10.0)).size(16))
                    .width(Length::Fixed(52.0))
                    .height(Length::Fixed(52.0))
                    .center_x()
                    .center_y()
                    .style(skeuo_container(Color::from_rgb(0.20, 0.18, 0.15)))
                    .padding(8),
                text(label).size(12),
            ]
            .spacing(8)
            .align_items(Alignment::Center),
        )
        .style(skeuo_container(Color::from_rgb(0.24, 0.20, 0.16)))
        .padding(10)
        .width(Length::Fixed(84.0))
        .into()
    }

    fn render_amp_faceplate(&self, selected: &DeviceState) -> Element<'_, Message> {
        let status_color = if selected.bypassed {
            Color::from_rgb(0.66, 0.12, 0.12)
        } else {
            Color::from_rgb(0.20, 0.76, 0.24)
        };

        container(
            column![
                row![
                    column![
                        text(&selected.name).size(24),
                        text("Classic Tube Tone").size(14),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                    container(
                        text(if selected.bypassed { "MUTE" } else { "LIVE" })
                            .size(14)
                            .horizontal_alignment(iced::alignment::Horizontal::Center),
                    )
                    .padding(10)
                    .style(skeuo_container(status_color))
                    .width(Length::Fixed(90.0)),
                ]
                .spacing(16)
                .align_items(Alignment::Center),
                    row![
                        button(text("AC30").size(12))
                            .on_press(Message::SetModel(Model::Ac30))
                            .style(iced::theme::Button::custom(SkeuoButton))
                            .padding(6),
                        button(text("DUMBLE").size(12))
                            .on_press(Message::SetModel(Model::Dumble))
                            .style(iced::theme::Button::custom(SkeuoButton))
                            .padding(6),
                    ]
                    .spacing(8),
                {
                    if selected.model == Model::Ac30 {
                        row![
                            column![self.render_knob("Gain", selected.gain), slider(0.0..=1.0, selected.gain, |v| Message::GainChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                            column![self.render_knob("Bass", selected.bass), slider(0.0..=1.0, selected.bass, |v| Message::BassChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                            column![self.render_knob("Treble", selected.treble), slider(0.0..=1.0, selected.treble, |v| Message::TrebleChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                            column![self.render_knob("Cut", selected.cut), slider(0.0..=1.0, selected.cut, |v| Message::CutChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                            column![self.render_knob("Master", selected.master), slider(0.0..=1.0, selected.master, |v| Message::MasterChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                        ]
                        .spacing(12)
                        .align_items(Alignment::Center)
                    } else {
                        // Dumble-style controls: Drive, Presence, Tone, Level
                        row![
                            column![self.render_knob("Drive", selected.gain), slider(0.0..=1.0, selected.gain, |v| Message::GainChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                            column![self.render_knob("Presence", selected.treble), slider(0.0..=1.0, selected.treble, |v| Message::TrebleChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                            column![self.render_knob("Tone", selected.bass), slider(0.0..=1.0, selected.bass, |v| Message::BassChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                            column![self.render_knob("Level", selected.master), slider(0.0..=1.0, selected.master, |v| Message::MasterChanged(v))]
                                .spacing(6)
                                .align_items(Alignment::Center),
                        ]
                        .spacing(12)
                        .align_items(Alignment::Center)
                    }
                },
                row![
                    checkbox("Dumble ODS", selected.dumble, |v| Message::ToggleDumble(v)),
                ]
                .spacing(8)
                .align_items(Alignment::Center),
            ]
            .spacing(20),
        )
        .style(skeuo_container(Color::from_rgb(0.12, 0.10, 0.08)))
        .padding(16)
        .width(Length::Fill)
        .into()
    }

    fn render_pedal_box(&self, selected: &DeviceState) -> Element<'_, Message> {
        let pedal_color = Color::from_rgb(0.14, 0.11, 0.09);
        let led_color = if selected.bypassed {
            Color::from_rgb(0.60, 0.06, 0.06)
        } else {
            Color::from_rgb(0.14, 0.92, 0.38)
        };

        container(
            column![
                text(&selected.name).size(22),
                row![
                    container(text(if selected.bypassed { "OFF" } else { "ON" }).size(14))
                        .padding(10)
                        .style(skeuo_container(Color::from_rgb(0.10, 0.09, 0.08))),
                    container(text(" "))
                        .width(Length::Fixed(16.0))
                        .height(Length::Fixed(16.0))
                        .style(skeuo_container(led_color))
                        .padding(4),
                    text("Stomp Switch").size(14),
                ]
                .spacing(12)
                .align_items(Alignment::Center),
                row![
                    column![self.render_knob("Gain", selected.gain), slider(0.0..=1.0, selected.gain, |v| Message::GainChanged(v))]
                        .spacing(6)
                        .align_items(Alignment::Center),
                    column![self.render_knob("Tone", selected.treble), slider(0.0..=1.0, selected.treble, |v| Message::TrebleChanged(v))]
                        .spacing(6)
                        .align_items(Alignment::Center),
                    column![self.render_knob("Level", selected.master), slider(0.0..=1.0, selected.master, |v| Message::MasterChanged(v))]
                        .spacing(6)
                        .align_items(Alignment::Center),
                ]
                .spacing(14)
                .align_items(Alignment::Center),
            ]
            .spacing(18),
        )
        .style(skeuo_container(pedal_color))
        .padding(16)
        .width(Length::Fill)
        .into()
    }

    fn render_pedal_module(
        &self,
        index: usize,
        device: &DeviceState,
        selected: bool,
    ) -> Element<'_, Message> {
        let label_color = if selected {
            Color::from_rgb(0.96, 0.88, 0.58)
        } else if device.kind == DeviceKind::Amp {
            Color::from_rgb(0.94, 0.84, 0.53)
        } else {
            Color::from_rgb(0.90, 0.42, 0.16)
        };

        let state_text = if device.bypassed { "OFF" } else { "ON" };

        button(
            column![
                row![
                    text(&device.name)
                        .size(14)
                        .width(Length::Fill)
                        .horizontal_alignment(iced::alignment::Horizontal::Left),
                    text(state_text).size(12),
                    text(if selected { "SELECTED" } else { "" }).size(10),
                ]
                .spacing(8)
                .align_items(Alignment::Center),
                row![container(
                    text(match device.kind {
                        DeviceKind::Amp => "AMP",
                        DeviceKind::Pedal => "PEDAL",
                    })
                    .size(10)
                    .horizontal_alignment(iced::alignment::Horizontal::Center)
                )
                .padding(6)
                .style(skeuo_container(label_color)),]
                .align_items(Alignment::Center),
            ]
            .spacing(12)
            .padding(12)
            .width(Length::Fill),
        )
        .on_press(Message::SelectDevice(index))
        .style(iced::theme::Button::custom(SkeuoButton))
        .padding(0)
        .width(Length::FillPortion(1))
        .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let selected = &self.devices[self.selected_index];

        let header = container(
            row![
                column![text("VoxBox").size(28), text("Amp + Pedalboard").size(14),]
                    .spacing(4)
                    .width(Length::Fill),
                row![
                    button(text("AMP").size(12))
                        .style(iced::theme::Button::custom(SkeuoButton))
                        .padding(10),
                    button(text("FX").size(12))
                        .style(iced::theme::Button::custom(SkeuoButton))
                        .padding(10),
                    button(text("CAB").size(12))
                        .style(iced::theme::Button::custom(SkeuoButton))
                        .padding(10),
                ]
                .spacing(10),
            ]
            .align_items(Alignment::Center)
            .spacing(20),
        )
        .style(skeuo_container(Color::from_rgb(0.10, 0.09, 0.08)))
        .padding(16)
        .width(Length::Fill);

        let pedalboard = self.devices.iter().enumerate().fold(
            row![].spacing(14).align_items(Alignment::Center),
            |row, (index, device)| {
                row.push(self.render_pedal_module(index, device, index == self.selected_index))
            },
        );

        let pedalboard = container(pedalboard)
            .style(skeuo_container(Color::from_rgb(0.12, 0.10, 0.08)))
            .padding(10)
            .width(Length::Fill);

        let selected_panel = if selected.kind == DeviceKind::Amp {
            match selected.model {
                Model::Ac30 => self.render_amp_faceplate(selected),
                Model::Dumble => self.render_amp_faceplate(selected),
            }
        } else {
            self.render_pedal_box(selected)
        };

        let meters = container(
            row![
                column![
                    text("Input").size(14),
                    progress_bar(0.0..=1.0, selected.gain.clamp(0.0, 1.0))
                        .style(iced::theme::ProgressBar::Custom(Box::new(SkeuoProgressBar))),
                ]
                .spacing(8)
                .width(Length::Fill),
                column![
                    text("Output").size(14),
                    progress_bar(0.0..=1.0, selected.master.clamp(0.0, 1.0))
                        .style(iced::theme::ProgressBar::Custom(Box::new(SkeuoProgressBar))),
                ]
                .spacing(8)
                .width(Length::Fill),
            ]
            .spacing(20),
        )
        .padding(12)
        .style(skeuo_container(Color::from_rgb(0.13, 0.11, 0.09)))
        .width(Length::Fill);

        let layout = column![header, pedalboard, selected_panel, meters]
            .spacing(16)
            .padding(20)
            .width(Length::Fill)
            .align_items(Alignment::Center);

        container(layout)
            .center_x()
            .center_y()
            .width(Length::Fill)
            .height(Length::Fill)
            .style(skeuo_container(Color::from_rgb(0.09, 0.08, 0.07)))
            .into()
    }
}
