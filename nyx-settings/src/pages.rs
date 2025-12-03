//! Settings pages for Nyx Settings

pub mod about;
pub mod appearance;
pub mod display;
pub mod network;
pub mod notifications;
pub mod power;
pub mod sound;

use iced::Element;
use serde::{Deserialize, Serialize};

/// Settings page identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SettingsPage {
    /// WiFi, Ethernet, VPN settings
    #[default]
    Network,
    /// Bluetooth devices
    Bluetooth,
    /// Display configuration
    Display,
    /// Audio settings
    Sound,
    /// Theme and appearance
    Appearance,
    /// Notification preferences
    Notifications,
    /// Power and battery
    Power,
    /// User accounts
    Users,
    /// System information
    About,
}

impl SettingsPage {
    /// Get the page title
    pub fn title(&self) -> &'static str {
        match self {
            SettingsPage::Network => "Network",
            SettingsPage::Bluetooth => "Bluetooth",
            SettingsPage::Display => "Display",
            SettingsPage::Sound => "Sound",
            SettingsPage::Appearance => "Appearance",
            SettingsPage::Notifications => "Notifications",
            SettingsPage::Power => "Power",
            SettingsPage::Users => "Users",
            SettingsPage::About => "About",
        }
    }

    /// Get the page icon
    pub fn icon(&self) -> &'static str {
        match self {
            SettingsPage::Network => "󰤨",
            SettingsPage::Bluetooth => "󰂯",
            SettingsPage::Display => "󰍹",
            SettingsPage::Sound => "󰕾",
            SettingsPage::Appearance => "󰏘",
            SettingsPage::Notifications => "󰂚",
            SettingsPage::Power => "󰂄",
            SettingsPage::Users => "󰀄",
            SettingsPage::About => "󰋽",
        }
    }

    /// Get the page description
    pub fn description(&self) -> &'static str {
        match self {
            SettingsPage::Network => "WiFi, Ethernet, VPN",
            SettingsPage::Bluetooth => "Devices and connections",
            SettingsPage::Display => "Resolution, scaling, night light",
            SettingsPage::Sound => "Volume, input/output devices",
            SettingsPage::Appearance => "Theme, colors, fonts",
            SettingsPage::Notifications => "Alerts and badges",
            SettingsPage::Power => "Battery and power saving",
            SettingsPage::Users => "Accounts and passwords",
            SettingsPage::About => "System information",
        }
    }

    /// Get all pages
    pub fn all() -> &'static [SettingsPage] {
        &[
            SettingsPage::Network,
            SettingsPage::Bluetooth,
            SettingsPage::Display,
            SettingsPage::Sound,
            SettingsPage::Appearance,
            SettingsPage::Notifications,
            SettingsPage::Power,
            SettingsPage::Users,
            SettingsPage::About,
        ]
    }
}
