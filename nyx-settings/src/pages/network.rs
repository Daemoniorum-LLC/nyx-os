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

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // NETWORK INFO TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_network_info_creation() {
        let info = NetworkInfo {
            ssid: "TestNetwork".to_string(),
            signal: 85,
            secured: true,
        };
        assert_eq!(info.ssid, "TestNetwork");
        assert_eq!(info.signal, 85);
        assert!(info.secured);
    }

    #[test]
    fn test_network_info_clone() {
        let info = NetworkInfo {
            ssid: "Test".to_string(),
            signal: 50,
            secured: false,
        };
        let cloned = info.clone();
        assert_eq!(info.ssid, cloned.ssid);
        assert_eq!(info.signal, cloned.signal);
        assert_eq!(info.secured, cloned.secured);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NETWORK PAGE NEW TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_network_page_new() {
        let page = NetworkPage::new();
        assert!(page.wifi_enabled);
        assert!(page.current_network.is_some());
        assert!(!page.available_networks.is_empty());
    }

    #[test]
    fn test_network_page_new_connected_to_nyx() {
        let page = NetworkPage::new();
        assert_eq!(page.current_network.as_deref(), Some("Nyx-Network"));
    }

    #[test]
    fn test_network_page_new_signal_strength() {
        let page = NetworkPage::new();
        assert_eq!(page.signal_strength, 75);
    }

    #[test]
    fn test_network_page_new_has_networks() {
        let page = NetworkPage::new();
        assert_eq!(page.available_networks.len(), 3);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NETWORK PAGE DEFAULT TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_network_page_default() {
        let page = NetworkPage::default();
        assert!(!page.wifi_enabled);
        assert!(page.current_network.is_none());
        assert!(page.available_networks.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NETWORK PAGE UPDATE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_toggle_wifi_off() {
        let mut page = NetworkPage::new();
        assert!(page.wifi_enabled);
        assert!(page.current_network.is_some());

        page.update(NetworkMessage::ToggleWifi(false));

        assert!(!page.wifi_enabled);
        assert!(page.current_network.is_none());
    }

    #[test]
    fn test_toggle_wifi_on() {
        let mut page = NetworkPage::default();
        assert!(!page.wifi_enabled);

        page.update(NetworkMessage::ToggleWifi(true));

        assert!(page.wifi_enabled);
    }

    #[test]
    fn test_connect_to_network() {
        let mut page = NetworkPage::new();
        page.update(NetworkMessage::Disconnect);
        assert!(page.current_network.is_none());

        page.update(NetworkMessage::Connect("NewNetwork".to_string()));

        assert_eq!(page.current_network.as_deref(), Some("NewNetwork"));
    }

    #[test]
    fn test_disconnect() {
        let mut page = NetworkPage::new();
        assert!(page.current_network.is_some());

        page.update(NetworkMessage::Disconnect);

        assert!(page.current_network.is_none());
    }

    #[test]
    fn test_refresh_preserves_state() {
        let mut page = NetworkPage::new();
        let network_count = page.available_networks.len();

        page.update(NetworkMessage::Refresh);

        // Refresh should not change state in this implementation
        assert_eq!(page.available_networks.len(), network_count);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NETWORK MESSAGE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_network_message_clone() {
        let msg = NetworkMessage::Connect("Test".to_string());
        let cloned = msg.clone();
        if let NetworkMessage::Connect(ssid) = cloned {
            assert_eq!(ssid, "Test");
        } else {
            panic!("Expected Connect");
        }
    }

    #[test]
    fn test_network_message_debug() {
        let msg = NetworkMessage::ToggleWifi(true);
        let debug = format!("{:?}", msg);
        assert!(debug.contains("ToggleWifi"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NETWORK PAGE INITIAL STATE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_initial_networks_have_varied_signals() {
        let page = NetworkPage::new();
        let signals: Vec<u8> = page.available_networks.iter().map(|n| n.signal).collect();
        // Should have different signal strengths
        let unique: std::collections::HashSet<u8> = signals.into_iter().collect();
        assert!(unique.len() >= 2);
    }

    #[test]
    fn test_initial_networks_mix_of_secured() {
        let page = NetworkPage::new();
        let secured_count = page.available_networks.iter().filter(|n| n.secured).count();
        let open_count = page.available_networks.iter().filter(|n| !n.secured).count();
        // Should have both types
        assert!(secured_count >= 1);
        assert!(open_count >= 1);
    }
}
