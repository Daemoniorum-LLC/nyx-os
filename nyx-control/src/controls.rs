//! Control widgets for Nyx Control

use iced::widget::{button, column, container, row, slider, text, toggler};
use iced::{Alignment, Element, Length};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::button::{button_style, ButtonVariant};
use nyx_theme::widgets::panel::quick_toggle_style;
use nyx_theme::Typography;

/// Message types for controls
#[derive(Debug, Clone)]
pub enum ControlMessage {
    /// Toggle WiFi
    ToggleWifi,
    /// Toggle Bluetooth
    ToggleBluetooth,
    /// Toggle Airplane mode
    ToggleAirplane,
    /// Toggle Night Light
    ToggleNightLight,
    /// Toggle Do Not Disturb
    ToggleDnd,
    /// Volume changed
    VolumeChanged(u8),
    /// Brightness changed
    BrightnessChanged(u8),
    /// Toggle volume mute
    ToggleMute,
    /// Power action
    PowerAction(PowerAction),
    /// Open settings
    OpenSettings,
    /// Open WiFi settings
    OpenWifiSettings,
    /// Open Bluetooth settings
    OpenBluetoothSettings,
    /// Open display settings
    OpenDisplaySettings,
    /// Open sound settings
    OpenSoundSettings,
}

/// Power actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Lock,
    Suspend,
    Restart,
    Shutdown,
}

/// Quick toggle tile widget
pub fn quick_toggle<'a, Message>(
    icon: &'a str,
    label: &'a str,
    active: bool,
    on_press: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let tile = button(
        column![
            text(icon)
                .size(Typography::SIZE_ICON_LG)
                .color(if active {
                    iced::Color::WHITE
                } else {
                    NyxColors::TEXT_BRIGHT
                }),
            text(label)
                .size(Typography::SIZE_LABEL_SMALL)
                .color(if active {
                    iced::Color::WHITE
                } else {
                    NyxColors::TEXT_SECONDARY
                }),
        ]
        .spacing(Spacing::XS)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .padding(Spacing::SM),
    )
    .width(Length::Fill)
    .style(move |theme, status| {
        let base = quick_toggle_style(active)(theme);
        match status {
            iced::widget::button::Status::Hovered => iced::widget::button::Style {
                background: Some(iced::Background::Color(if active {
                    NyxColors::AURORA_LIGHT
                } else {
                    NyxColors::NEBULA
                })),
                text_color: base.text_color.unwrap_or(NyxColors::TEXT_BRIGHT),
                border: base.border,
                shadow: base.shadow,
            },
            _ => iced::widget::button::Style {
                background: base.background,
                text_color: base.text_color.unwrap_or(NyxColors::TEXT_BRIGHT),
                border: base.border,
                shadow: base.shadow,
            },
        }
    })
    .on_press(on_press);

    tile.into()
}

/// Slider control with icon and label
pub fn slider_control<'a, Message>(
    icon: &'a str,
    label: &'a str,
    value: u8,
    on_change: impl Fn(u8) -> Message + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let slider_widget = slider(0..=100, value as i32, move |v| on_change(v as u8))
        .width(Length::Fill)
        .step(1);

    container(
        row![
            text(icon)
                .size(Typography::SIZE_ICON_MD)
                .color(NyxColors::TEXT_SECONDARY),
            column![
                text(label)
                    .size(Typography::SIZE_LABEL_SMALL)
                    .color(NyxColors::TEXT_SECONDARY),
                slider_widget,
            ]
            .spacing(Spacing::XS)
            .width(Length::Fill),
            text(format!("{}%", value))
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::TEXT_SECONDARY),
        ]
        .spacing(Spacing::MD)
        .align_y(Alignment::Center)
        .padding(Spacing::SM),
    )
    .style(|_theme| iced::widget::container::Style {
        background: Some(iced::Background::Color(NyxColors::DUSK)),
        border: iced::Border {
            color: NyxColors::BORDER_DARK,
            width: 1.0,
            radius: Spacing::RADIUS_MD.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Power action button
pub fn power_button<'a, Message>(
    icon: &'a str,
    label: &'a str,
    on_press: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    button(
        column![
            text(icon)
                .size(Typography::SIZE_ICON_MD)
                .color(NyxColors::TEXT_BRIGHT),
            text(label)
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::TEXT_SECONDARY),
        ]
        .spacing(Spacing::XS)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .padding(Spacing::SM),
    )
    .width(Length::Fill)
    .style(button_style(ButtonVariant::Ghost))
    .on_press(on_press)
    .into()
}

/// Section header
pub fn section_header<'a>(title: &'a str) -> Element<'a, ControlMessage> {
    text(title)
        .size(Typography::SIZE_LABEL_MEDIUM)
        .color(NyxColors::TEXT_MUTED)
        .into()
}

/// Settings row with arrow
pub fn settings_row<'a, Message>(
    icon: &'a str,
    label: &'a str,
    value: Option<&'a str>,
    on_press: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    button(
        row![
            text(icon)
                .size(Typography::SIZE_ICON_MD)
                .color(NyxColors::TEXT_SECONDARY),
            column![
                text(label)
                    .size(Typography::SIZE_BODY_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                if let Some(val) = value {
                    text(val)
                        .size(Typography::SIZE_LABEL_SMALL)
                        .color(NyxColors::TEXT_SECONDARY)
                } else {
                    text("")
                },
            ]
            .width(Length::Fill),
            text("󰅂")
                .size(Typography::SIZE_ICON_SM)
                .color(NyxColors::TEXT_MUTED),
        ]
        .spacing(Spacing::MD)
        .align_y(Alignment::Center)
        .padding(Spacing::SM),
    )
    .width(Length::Fill)
    .style(button_style(ButtonVariant::Ghost))
    .on_press(on_press)
    .into()
}
