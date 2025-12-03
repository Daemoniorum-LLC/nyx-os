//! Network settings page

use iced::widget::{column, container, row, text, toggler, vertical_space};
use iced::{Alignment, Element, Length};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::card::card_style;
use nyx_theme::widgets::CardVariant;
use nyx_theme::Typography;

/// Network page state
#[derive(Debug, Clone, Default)]
pub struct NetworkPage {
    /// WiFi enabled
    pub wifi_enabled: bool,
    /// Current network
    pub current_network: Option<String>,
    /// Signal strength
    pub signal_strength: u8,
    /// Available networks
    pub available_networks: Vec<NetworkInfo>,
}

/// Network info
#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
}

/// Network messages
#[derive(Debug, Clone)]
pub enum NetworkMessage {
    /// Toggle WiFi
    ToggleWifi(bool),
    /// Connect to network
    Connect(String),
    /// Disconnect
    Disconnect,
    /// Refresh networks
    Refresh,
}

impl NetworkPage {
    /// Create new network page
    pub fn new() -> Self {
        Self {
            wifi_enabled: true,
            current_network: Some("Nyx-Network".to_string()),
            signal_strength: 75,
            available_networks: vec![
                NetworkInfo {
                    ssid: "Nyx-Network".to_string(),
                    signal: 75,
                    secured: true,
                },
                NetworkInfo {
                    ssid: "Guest-Network".to_string(),
                    signal: 60,
                    secured: false,
                },
                NetworkInfo {
                    ssid: "Neighbor-5G".to_string(),
                    signal: 40,
                    secured: true,
                },
            ],
        }
    }

    /// Update state
    pub fn update(&mut self, message: NetworkMessage) {
        match message {
            NetworkMessage::ToggleWifi(enabled) => {
                self.wifi_enabled = enabled;
                if !enabled {
                    self.current_network = None;
                }
            }
            NetworkMessage::Connect(ssid) => {
                self.current_network = Some(ssid);
            }
            NetworkMessage::Disconnect => {
                self.current_network = None;
            }
            NetworkMessage::Refresh => {
                // Refresh network list
            }
        }
    }

    /// View the page
    pub fn view(&self) -> Element<NetworkMessage> {
        column![
            text("Network")
                .size(Typography::SIZE_HEADLINE_LARGE)
                .color(NyxColors::TEXT_BRIGHT),
            text("Manage WiFi and network connections")
                .size(Typography::SIZE_BODY_MEDIUM)
                .color(NyxColors::TEXT_SECONDARY),
            vertical_space().height(Spacing::MD),
            self.view_wifi_toggle(),
            if self.wifi_enabled {
                self.view_networks()
            } else {
                column![]
            },
        ]
        .spacing(Spacing::MD)
        .width(Length::Fill)
        .padding(Spacing::LG)
        .into()
    }

    fn view_wifi_toggle(&self) -> Element<NetworkMessage> {
        container(
            row![
                text("󰤨")
                    .size(Typography::SIZE_ICON_LG)
                    .color(if self.wifi_enabled {
                        NyxColors::AURORA
                    } else {
                        NyxColors::TEXT_MUTED
                    }),
                column![
                    text("WiFi")
                        .size(Typography::SIZE_BODY_LARGE)
                        .color(NyxColors::TEXT_BRIGHT),
                    if let Some(ref network) = self.current_network {
                        text(format!("Connected to {}", network))
                            .size(Typography::SIZE_BODY_SMALL)
                            .color(NyxColors::TEXT_SECONDARY)
                    } else {
                        text("Not connected")
                            .size(Typography::SIZE_BODY_SMALL)
                            .color(NyxColors::TEXT_MUTED)
                    },
                ]
                .width(Length::Fill),
                toggler(self.wifi_enabled).on_toggle(NetworkMessage::ToggleWifi),
            ]
            .spacing(Spacing::MD)
            .align_y(Alignment::Center),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn view_networks(&self) -> Element<NetworkMessage> {
        let network_items: Vec<Element<NetworkMessage>> = self
            .available_networks
            .iter()
            .map(|net| self.view_network_item(net))
            .collect();

        container(
            column![
                text("Available Networks")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                column(network_items).spacing(Spacing::XS),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn view_network_item(&self, network: &NetworkInfo) -> Element<NetworkMessage> {
        let is_connected = self.current_network.as_ref() == Some(&network.ssid);

        let signal_icon = if network.signal > 66 {
            "󰤨"
        } else if network.signal > 33 {
            "󰤥"
        } else {
            "󰤟"
        };

        row![
            text(signal_icon)
                .size(Typography::SIZE_ICON_MD)
                .color(NyxColors::TEXT_SECONDARY),
            column![
                text(&network.ssid)
                    .size(Typography::SIZE_BODY_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                row![
                    if network.secured {
                        text("󰌾 Secured")
                            .size(Typography::SIZE_LABEL_SMALL)
                            .color(NyxColors::TEXT_MUTED)
                    } else {
                        text("Open")
                            .size(Typography::SIZE_LABEL_SMALL)
                            .color(NyxColors::WARNING)
                    },
                    if is_connected {
                        text(" · Connected")
                            .size(Typography::SIZE_LABEL_SMALL)
                            .color(NyxColors::SUCCESS)
                    } else {
                        text("")
                    },
                ],
            ]
            .width(Length::Fill),
            if is_connected {
                text("󰄬")
                    .size(Typography::SIZE_ICON_MD)
                    .color(NyxColors::SUCCESS)
            } else {
                text("")
            },
        ]
        .spacing(Spacing::MD)
        .align_y(Alignment::Center)
        .padding(Spacing::SM)
        .into()
    }
}
