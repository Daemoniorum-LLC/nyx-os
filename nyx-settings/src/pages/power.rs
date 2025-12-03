//! Power settings page

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::button::{button_style, ButtonVariant};
use nyx_theme::widgets::card::card_style;
use nyx_theme::widgets::CardVariant;
use nyx_theme::Typography;

/// Power page state
#[derive(Debug, Clone)]
pub struct PowerPage {
    /// Current profile
    pub profile: PowerProfile,
    /// Battery percentage
    pub battery: Option<u8>,
    /// Is charging
    pub charging: bool,
    /// Time remaining
    pub time_remaining: Option<String>,
}

/// Power profile
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PowerProfile {
    /// Power saver
    PowerSaver,
    /// Balanced
    #[default]
    Balanced,
    /// Performance
    Performance,
}

impl Default for PowerPage {
    fn default() -> Self {
        Self {
            profile: PowerProfile::Balanced,
            battery: Some(85),
            charging: false,
            time_remaining: Some("3h 45m remaining".to_string()),
        }
    }
}

/// Power messages
#[derive(Debug, Clone)]
pub enum PowerMessage {
    /// Set profile
    SetProfile(PowerProfile),
}

impl PowerPage {
    /// Update state
    pub fn update(&mut self, message: PowerMessage) {
        match message {
            PowerMessage::SetProfile(profile) => self.profile = profile,
        }
    }

    /// View the page
    pub fn view(&self) -> Element<PowerMessage> {
        column![
            text("Power")
                .size(Typography::SIZE_HEADLINE_LARGE)
                .color(NyxColors::TEXT_BRIGHT),
            text("Manage power settings and battery")
                .size(Typography::SIZE_BODY_MEDIUM)
                .color(NyxColors::TEXT_SECONDARY),
            if self.battery.is_some() {
                self.view_battery_section()
            } else {
                column![]
            },
            self.view_profile_section(),
        ]
        .spacing(Spacing::MD)
        .width(Length::Fill)
        .padding(Spacing::LG)
        .into()
    }

    fn view_battery_section(&self) -> Element<PowerMessage> {
        let battery = self.battery.unwrap_or(0);
        let icon = if self.charging {
            "󰂄"
        } else if battery > 80 {
            "󰁹"
        } else if battery > 60 {
            "󰂀"
        } else if battery > 40 {
            "󰁾"
        } else if battery > 20 {
            "󰁻"
        } else {
            "󰂃"
        };

        container(
            row![
                text(icon)
                    .size(64.0)
                    .color(if battery <= 20 && !self.charging {
                        NyxColors::ERROR
                    } else {
                        NyxColors::SUCCESS
                    }),
                column![
                    text(format!("{}%", battery))
                        .size(Typography::SIZE_DISPLAY_SMALL)
                        .color(NyxColors::TEXT_BRIGHT),
                    if let Some(ref time) = self.time_remaining {
                        text(time)
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_SECONDARY)
                    } else if self.charging {
                        text("Charging")
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::SUCCESS)
                    } else {
                        text("On battery")
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_SECONDARY)
                    },
                ],
            ]
            .spacing(Spacing::LG)
            .align_y(Alignment::Center),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn view_profile_section(&self) -> Element<PowerMessage> {
        container(
            column![
                text("Power Mode")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                row![
                    self.profile_button(
                        "Power Saver",
                        "Extend battery life",
                        "󱈏",
                        PowerProfile::PowerSaver
                    ),
                    self.profile_button(
                        "Balanced",
                        "Default performance",
                        "󱐋",
                        PowerProfile::Balanced
                    ),
                    self.profile_button(
                        "Performance",
                        "Maximum power",
                        "󱐌",
                        PowerProfile::Performance
                    ),
                ]
                .spacing(Spacing::MD),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn profile_button(
        &self,
        title: &str,
        description: &str,
        icon: &str,
        profile: PowerProfile,
    ) -> Element<PowerMessage> {
        let is_selected = self.profile == profile;

        button(
            column![
                text(icon)
                    .size(Typography::SIZE_ICON_XL)
                    .color(if is_selected {
                        NyxColors::AURORA
                    } else {
                        NyxColors::TEXT_SECONDARY
                    }),
                text(title)
                    .size(Typography::SIZE_BODY_MEDIUM)
                    .color(if is_selected {
                        NyxColors::TEXT_BRIGHT
                    } else {
                        NyxColors::TEXT_SECONDARY
                    }),
                text(description)
                    .size(Typography::SIZE_LABEL_SMALL)
                    .color(NyxColors::TEXT_MUTED),
            ]
            .spacing(Spacing::XS)
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .padding(Spacing::MD),
        )
        .width(Length::Fill)
        .style(if is_selected {
            button_style(ButtonVariant::Primary)
        } else {
            button_style(ButtonVariant::Secondary)
        })
        .on_press(PowerMessage::SetProfile(profile))
        .into()
    }
}
