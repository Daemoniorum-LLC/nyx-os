//! Sound settings page

use iced::widget::{column, container, row, slider, text, toggler};
use iced::{Alignment, Element, Length};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::card::card_style;
use nyx_theme::widgets::CardVariant;
use nyx_theme::Typography;

/// Sound page state
#[derive(Debug, Clone)]
pub struct SoundPage {
    /// Output volume (0-100)
    pub volume: u32,
    /// Output muted
    pub muted: bool,
    /// Input volume (0-100)
    pub input_volume: u32,
    /// Input muted
    pub input_muted: bool,
    /// Output device
    pub output_device: String,
    /// Input device
    pub input_device: String,
}

impl Default for SoundPage {
    fn default() -> Self {
        Self {
            volume: 65,
            muted: false,
            input_volume: 80,
            input_muted: false,
            output_device: "Built-in Speakers".to_string(),
            input_device: "Built-in Microphone".to_string(),
        }
    }
}

/// Sound messages
#[derive(Debug, Clone)]
pub enum SoundMessage {
    /// Set volume
    SetVolume(u32),
    /// Toggle mute
    ToggleMute(bool),
    /// Set input volume
    SetInputVolume(u32),
    /// Toggle input mute
    ToggleInputMute(bool),
}

impl SoundPage {
    /// Update state
    pub fn update(&mut self, message: SoundMessage) {
        match message {
            SoundMessage::SetVolume(vol) => {
                self.volume = vol;
                if vol > 0 {
                    self.muted = false;
                }
            }
            SoundMessage::ToggleMute(muted) => self.muted = muted,
            SoundMessage::SetInputVolume(vol) => {
                self.input_volume = vol;
                if vol > 0 {
                    self.input_muted = false;
                }
            }
            SoundMessage::ToggleInputMute(muted) => self.input_muted = muted,
        }
    }

    /// View the page
    pub fn view(&self) -> Element<SoundMessage> {
        column![
            text("Sound")
                .size(Typography::SIZE_HEADLINE_LARGE)
                .color(NyxColors::TEXT_BRIGHT),
            text("Configure audio input and output")
                .size(Typography::SIZE_BODY_MEDIUM)
                .color(NyxColors::TEXT_SECONDARY),
            self.view_output_section(),
            self.view_input_section(),
        ]
        .spacing(Spacing::MD)
        .width(Length::Fill)
        .padding(Spacing::LG)
        .into()
    }

    fn view_output_section(&self) -> Element<SoundMessage> {
        let volume_icon = if self.muted || self.volume == 0 {
            "󰖁"
        } else if self.volume < 33 {
            "󰕿"
        } else if self.volume < 66 {
            "󰖀"
        } else {
            "󰕾"
        };

        container(
            column![
                text("Output")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                row![
                    text(volume_icon)
                        .size(Typography::SIZE_ICON_LG)
                        .color(if self.muted {
                            NyxColors::TEXT_MUTED
                        } else {
                            NyxColors::AURORA
                        }),
                    column![
                        row![
                            text("Volume")
                                .size(Typography::SIZE_BODY_MEDIUM)
                                .color(NyxColors::TEXT_SECONDARY),
                            iced::widget::horizontal_space(),
                            text(format!("{}%", self.volume))
                                .size(Typography::SIZE_BODY_MEDIUM)
                                .color(NyxColors::TEXT_BRIGHT),
                        ],
                        slider(0..=100, self.volume as i32, |v| {
                            SoundMessage::SetVolume(v as u32)
                        }),
                    ]
                    .spacing(Spacing::XS)
                    .width(Length::Fill),
                    toggler(self.muted).on_toggle(SoundMessage::ToggleMute),
                ]
                .spacing(Spacing::MD)
                .align_y(Alignment::Center),
                row![
                    text("Output Device")
                        .size(Typography::SIZE_BODY_MEDIUM)
                        .color(NyxColors::TEXT_SECONDARY),
                    iced::widget::horizontal_space(),
                    text(&self.output_device)
                        .size(Typography::SIZE_BODY_MEDIUM)
                        .color(NyxColors::TEXT_BRIGHT),
                ],
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn view_input_section(&self) -> Element<SoundMessage> {
        let mic_icon = if self.input_muted { "󰍭" } else { "󰍬" };

        container(
            column![
                text("Input")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                row![
                    text(mic_icon)
                        .size(Typography::SIZE_ICON_LG)
                        .color(if self.input_muted {
                            NyxColors::TEXT_MUTED
                        } else {
                            NyxColors::SUCCESS
                        }),
                    column![
                        row![
                            text("Input Level")
                                .size(Typography::SIZE_BODY_MEDIUM)
                                .color(NyxColors::TEXT_SECONDARY),
                            iced::widget::horizontal_space(),
                            text(format!("{}%", self.input_volume))
                                .size(Typography::SIZE_BODY_MEDIUM)
                                .color(NyxColors::TEXT_BRIGHT),
                        ],
                        slider(0..=100, self.input_volume as i32, |v| {
                            SoundMessage::SetInputVolume(v as u32)
                        }),
                    ]
                    .spacing(Spacing::XS)
                    .width(Length::Fill),
                    toggler(self.input_muted).on_toggle(SoundMessage::ToggleInputMute),
                ]
                .spacing(Spacing::MD)
                .align_y(Alignment::Center),
                row![
                    text("Input Device")
                        .size(Typography::SIZE_BODY_MEDIUM)
                        .color(NyxColors::TEXT_SECONDARY),
                    iced::widget::horizontal_space(),
                    text(&self.input_device)
                        .size(Typography::SIZE_BODY_MEDIUM)
                        .color(NyxColors::TEXT_BRIGHT),
                ],
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }
}
