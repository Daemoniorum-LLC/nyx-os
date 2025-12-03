//! Top panel component for Nyx Shell

use crate::config::PanelConfig;
use crate::messages::{AudioStatus, BatteryStatus, Message, NetworkStatus, PanelMessage};
use crate::workspace::WorkspaceManager;
use chrono::Local;
use iced::widget::{button, container, horizontal_space, row, text, Row};
use iced::{Alignment, Element, Length, Padding};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::button::{button_style, ButtonVariant};
use nyx_theme::widgets::panel::top_panel_style;
use nyx_theme::Typography;

/// Top panel state
pub struct Panel {
    /// Panel configuration
    config: PanelConfig,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new(PanelConfig::default())
    }
}

impl Panel {
    /// Create a new panel
    pub fn new(config: PanelConfig) -> Self {
        Self { config }
    }

    /// Get current time formatted string
    fn format_time(&self) -> String {
        let now = Local::now();

        if self.config.clock_24h {
            if self.config.show_date {
                now.format("%a %b %d  %H:%M").to_string()
            } else {
                now.format("%H:%M").to_string()
            }
        } else if self.config.show_date {
            now.format("%a %b %d  %I:%M %p").to_string()
        } else {
            now.format("%I:%M %p").to_string()
        }
    }

    /// Render the panel
    pub fn view(
        &self,
        workspaces: &WorkspaceManager,
        battery: &BatteryStatus,
        network: &NetworkStatus,
        audio: &AudioStatus,
    ) -> Element<Message> {
        // Left section: Activities button + workspace indicators
        let left_section = self.view_left_section(workspaces);

        // Center section: Clock
        let center_section = self.view_center_section();

        // Right section: System tray + quick settings
        let right_section = self.view_right_section(battery, network, audio);

        let panel_content = row![left_section, center_section, right_section]
            .spacing(Spacing::MD)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fixed(Spacing::PANEL_HEIGHT));

        container(panel_content)
            .width(Length::Fill)
            .height(Length::Fixed(Spacing::PANEL_HEIGHT))
            .padding(Padding::from([0.0, Spacing::MD]))
            .style(top_panel_style())
            .into()
    }

    fn view_left_section(&self, workspaces: &WorkspaceManager) -> Element<Message> {
        let mut items: Vec<Element<Message>> = Vec::new();

        // Activities button
        if self.config.show_activities {
            let activities_btn = button(
                text("Activities")
                    .size(Typography::SIZE_LABEL_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
            )
            .padding(Padding::from([Spacing::XS, Spacing::SM]))
            .style(button_style(ButtonVariant::Panel))
            .on_press(Message::Panel(PanelMessage::ActivitiesClicked));

            items.push(activities_btn.into());
        }

        // Workspace indicators
        if self.config.show_workspaces {
            for workspace in workspaces.workspaces() {
                let style = if workspace.active {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Panel
                };

                let ws_btn = button(
                    text(&workspace.name)
                        .size(Typography::SIZE_LABEL_SMALL)
                        .color(if workspace.active {
                            iced::Color::WHITE
                        } else {
                            NyxColors::TEXT_SECONDARY
                        }),
                )
                .padding(Padding::from([Spacing::XXS, Spacing::SM]))
                .style(button_style(style))
                .on_press(Message::Panel(PanelMessage::WorkspaceClicked(workspace.id)));

                items.push(ws_btn.into());
            }
        }

        Row::with_children(items)
            .spacing(Spacing::XS)
            .align_y(Alignment::Center)
            .into()
    }

    fn view_center_section(&self) -> Element<Message> {
        let clock_text = self.format_time();

        let clock_btn = button(
            text(&clock_text)
                .size(Typography::SIZE_LABEL_MEDIUM)
                .color(NyxColors::TEXT_BRIGHT),
        )
        .padding(Padding::from([Spacing::XS, Spacing::MD]))
        .style(button_style(ButtonVariant::Panel))
        .on_press(Message::Panel(PanelMessage::ClockClicked));

        row![horizontal_space(), clock_btn, horizontal_space()]
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .into()
    }

    fn view_right_section(
        &self,
        battery: &BatteryStatus,
        network: &NetworkStatus,
        audio: &AudioStatus,
    ) -> Element<Message> {
        let mut items: Vec<Element<Message>> = Vec::new();

        // System tray icons
        if self.config.show_tray {
            // Network indicator
            let network_icon = if network.connected {
                match network.connection_type {
                    crate::messages::ConnectionType::Wifi => "󰤨",     // WiFi connected
                    crate::messages::ConnectionType::Ethernet => "󰈀", // Ethernet
                    _ => "󰤭",                                        // Disconnected
                }
            } else {
                "󰤭" // Disconnected
            };

            items.push(
                text(network_icon)
                    .size(Typography::SIZE_ICON_MD)
                    .color(NyxColors::TEXT_BRIGHT)
                    .into(),
            );

            // Volume indicator
            let volume_icon = if audio.muted || audio.volume == 0 {
                "󰖁" // Muted
            } else if audio.volume < 33 {
                "󰕿" // Low
            } else if audio.volume < 66 {
                "󰖀" // Medium
            } else {
                "󰕾" // High
            };

            items.push(
                text(volume_icon)
                    .size(Typography::SIZE_ICON_MD)
                    .color(NyxColors::TEXT_BRIGHT)
                    .into(),
            );

            // Battery indicator (if applicable)
            if battery.percentage > 0 || battery.plugged {
                let battery_icon = if battery.charging {
                    "󰂄" // Charging
                } else if battery.percentage > 80 {
                    "󰁹" // Full
                } else if battery.percentage > 60 {
                    "󰂀" // High
                } else if battery.percentage > 40 {
                    "󰁾" // Medium
                } else if battery.percentage > 20 {
                    "󰁻" // Low
                } else {
                    "󰂃" // Critical
                };

                items.push(
                    row![
                        text(battery_icon)
                            .size(Typography::SIZE_ICON_MD)
                            .color(if battery.percentage <= 20 && !battery.charging {
                                NyxColors::ERROR
                            } else {
                                NyxColors::TEXT_BRIGHT
                            }),
                        text(format!("{}%", battery.percentage))
                            .size(Typography::SIZE_LABEL_SMALL)
                            .color(NyxColors::TEXT_SECONDARY),
                    ]
                    .spacing(Spacing::XXS)
                    .align_y(Alignment::Center)
                    .into(),
            );
            }
        }

        // Quick settings button
        let settings_btn = button(
            text("󰒓")
                .size(Typography::SIZE_ICON_MD)
                .color(NyxColors::TEXT_BRIGHT),
        )
        .padding(Padding::from([Spacing::XS, Spacing::SM]))
        .style(button_style(ButtonVariant::Panel))
        .on_press(Message::ToggleControlCenter);

        items.push(settings_btn.into());

        // User menu
        let user_btn = button(
            text("󰀄")
                .size(Typography::SIZE_ICON_MD)
                .color(NyxColors::TEXT_BRIGHT),
        )
        .padding(Padding::from([Spacing::XS, Spacing::SM]))
        .style(button_style(ButtonVariant::Panel))
        .on_press(Message::Panel(PanelMessage::UserMenuClicked));

        items.push(user_btn.into());

        Row::with_children(items)
            .spacing(Spacing::SM)
            .align_y(Alignment::Center)
            .into()
    }
}
