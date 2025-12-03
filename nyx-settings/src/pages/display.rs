//! Display settings page

use iced::widget::{column, container, pick_list, row, slider, text, toggler};
use iced::{Alignment, Element, Length};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::card::card_style;
use nyx_theme::widgets::CardVariant;
use nyx_theme::Typography;

/// Display page state
#[derive(Debug, Clone)]
pub struct DisplayPage {
    /// Current resolution
    pub resolution: Resolution,
    /// Available resolutions
    pub resolutions: Vec<Resolution>,
    /// Refresh rate
    pub refresh_rate: u32,
    /// Scale factor (100-200%)
    pub scale: u32,
    /// Night light enabled
    pub night_light: bool,
    /// Night light intensity (0-100)
    pub night_light_intensity: u32,
    /// Night light schedule
    pub night_light_schedule: bool,
}

/// Display resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl Default for DisplayPage {
    fn default() -> Self {
        Self {
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            resolutions: vec![
                Resolution {
                    width: 3840,
                    height: 2160,
                },
                Resolution {
                    width: 2560,
                    height: 1440,
                },
                Resolution {
                    width: 1920,
                    height: 1080,
                },
                Resolution {
                    width: 1680,
                    height: 1050,
                },
                Resolution {
                    width: 1280,
                    height: 720,
                },
            ],
            refresh_rate: 60,
            scale: 100,
            night_light: false,
            night_light_intensity: 50,
            night_light_schedule: false,
        }
    }
}

/// Display page messages
#[derive(Debug, Clone)]
pub enum DisplayMessage {
    /// Set resolution
    SetResolution(Resolution),
    /// Set refresh rate
    SetRefreshRate(u32),
    /// Set scale
    SetScale(u32),
    /// Toggle night light
    ToggleNightLight(bool),
    /// Set night light intensity
    SetNightLightIntensity(u32),
    /// Toggle schedule
    ToggleSchedule(bool),
}

impl DisplayPage {
    /// Update state
    pub fn update(&mut self, message: DisplayMessage) {
        match message {
            DisplayMessage::SetResolution(res) => self.resolution = res,
            DisplayMessage::SetRefreshRate(rate) => self.refresh_rate = rate,
            DisplayMessage::SetScale(scale) => self.scale = scale,
            DisplayMessage::ToggleNightLight(enabled) => self.night_light = enabled,
            DisplayMessage::SetNightLightIntensity(intensity) => {
                self.night_light_intensity = intensity
            }
            DisplayMessage::ToggleSchedule(enabled) => self.night_light_schedule = enabled,
        }
    }

    /// View the page
    pub fn view(&self) -> Element<DisplayMessage> {
        let resolution_section = self.view_resolution_section();
        let night_light_section = self.view_night_light_section();

        column![
            text("Display")
                .size(Typography::SIZE_HEADLINE_LARGE)
                .color(NyxColors::TEXT_BRIGHT),
            text("Configure display resolution, scaling, and night light")
                .size(Typography::SIZE_BODY_MEDIUM)
                .color(NyxColors::TEXT_SECONDARY),
            container(column![resolution_section, night_light_section].spacing(Spacing::LG))
                .padding(Spacing::LG),
        ]
        .spacing(Spacing::MD)
        .width(Length::Fill)
        .into()
    }

    fn view_resolution_section(&self) -> Element<DisplayMessage> {
        container(
            column![
                text("Resolution & Scaling")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                row![
                    column![
                        text("Resolution")
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_SECONDARY),
                        pick_list(
                            self.resolutions.clone(),
                            Some(self.resolution),
                            DisplayMessage::SetResolution
                        )
                        .width(Length::Fixed(200.0)),
                    ]
                    .spacing(Spacing::XS),
                    column![
                        text("Refresh Rate")
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_SECONDARY),
                        text(format!("{} Hz", self.refresh_rate))
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_BRIGHT),
                    ]
                    .spacing(Spacing::XS),
                ]
                .spacing(Spacing::XL),
                column![
                    row![
                        text("Scale")
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_SECONDARY),
                        iced::widget::horizontal_space(),
                        text(format!("{}%", self.scale))
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_BRIGHT),
                    ],
                    slider(100..=200, self.scale as i32, |v| {
                        DisplayMessage::SetScale(v as u32)
                    }),
                ]
                .spacing(Spacing::XS),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn view_night_light_section(&self) -> Element<DisplayMessage> {
        container(
            column![
                row![
                    column![
                        text("Night Light")
                            .size(Typography::SIZE_TITLE_MEDIUM)
                            .color(NyxColors::TEXT_BRIGHT),
                        text("Reduce blue light for better sleep")
                            .size(Typography::SIZE_BODY_SMALL)
                            .color(NyxColors::TEXT_SECONDARY),
                    ]
                    .width(Length::Fill),
                    toggler(self.night_light)
                        .on_toggle(DisplayMessage::ToggleNightLight),
                ]
                .align_y(Alignment::Center),
                if self.night_light {
                    column![
                        column![
                            row![
                                text("Intensity")
                                    .size(Typography::SIZE_BODY_MEDIUM)
                                    .color(NyxColors::TEXT_SECONDARY),
                                iced::widget::horizontal_space(),
                                text(format!("{}%", self.night_light_intensity))
                                    .size(Typography::SIZE_BODY_MEDIUM)
                                    .color(NyxColors::TEXT_BRIGHT),
                            ],
                            slider(0..=100, self.night_light_intensity as i32, |v| {
                                DisplayMessage::SetNightLightIntensity(v as u32)
                            }),
                        ]
                        .spacing(Spacing::XS),
                        row![
                            column![
                                text("Automatic Schedule")
                                    .size(Typography::SIZE_BODY_MEDIUM)
                                    .color(NyxColors::TEXT_BRIGHT),
                                text("Sunset to Sunrise")
                                    .size(Typography::SIZE_BODY_SMALL)
                                    .color(NyxColors::TEXT_SECONDARY),
                            ]
                            .width(Length::Fill),
                            toggler(self.night_light_schedule)
                                .on_toggle(DisplayMessage::ToggleSchedule),
                        ]
                        .align_y(Alignment::Center),
                    ]
                    .spacing(Spacing::MD)
                } else {
                    column![]
                },
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }
}
