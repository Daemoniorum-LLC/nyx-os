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

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // SETTINGS PAGE ENUM TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_default_page() {
        let page = SettingsPage::default();
        assert_eq!(page, SettingsPage::Network);
    }

    #[test]
    fn test_page_equality() {
        assert_eq!(SettingsPage::Network, SettingsPage::Network);
        assert_ne!(SettingsPage::Network, SettingsPage::Display);
    }

    #[test]
    fn test_page_copy() {
        let page = SettingsPage::Sound;
        let copy = page;
        assert_eq!(page, copy);
    }

    #[test]
    fn test_page_clone() {
        let page = SettingsPage::About;
        let cloned = page.clone();
        assert_eq!(page, cloned);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PAGE TITLE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_page_titles() {
        assert_eq!(SettingsPage::Network.title(), "Network");
        assert_eq!(SettingsPage::Bluetooth.title(), "Bluetooth");
        assert_eq!(SettingsPage::Display.title(), "Display");
        assert_eq!(SettingsPage::Sound.title(), "Sound");
        assert_eq!(SettingsPage::Appearance.title(), "Appearance");
        assert_eq!(SettingsPage::Notifications.title(), "Notifications");
        assert_eq!(SettingsPage::Power.title(), "Power");
        assert_eq!(SettingsPage::Users.title(), "Users");
        assert_eq!(SettingsPage::About.title(), "About");
    }

    #[test]
    fn test_all_pages_have_titles() {
        for page in SettingsPage::all() {
            assert!(!page.title().is_empty());
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PAGE ICON TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_all_pages_have_icons() {
        for page in SettingsPage::all() {
            assert!(!page.icon().is_empty());
        }
    }

    #[test]
    fn test_icons_are_unique() {
        let all_pages = SettingsPage::all();
        for (i, page) in all_pages.iter().enumerate() {
            for (j, other) in all_pages.iter().enumerate() {
                if i != j {
                    assert_ne!(page.icon(), other.icon(), "Icons should be unique");
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PAGE DESCRIPTION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_all_pages_have_descriptions() {
        for page in SettingsPage::all() {
            assert!(!page.description().is_empty());
        }
    }

    #[test]
    fn test_description_content() {
        // Check that descriptions contain relevant keywords
        assert!(SettingsPage::Network.description().contains("WiFi"));
        assert!(SettingsPage::Display.description().contains("Resolution"));
        assert!(SettingsPage::Sound.description().contains("Volume"));
        assert!(SettingsPage::Power.description().contains("Battery"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ALL PAGES TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_all_pages_count() {
        let all = SettingsPage::all();
        assert_eq!(all.len(), 9);
    }

    #[test]
    fn test_all_pages_contains_network() {
        assert!(SettingsPage::all().contains(&SettingsPage::Network));
    }

    #[test]
    fn test_all_pages_contains_about() {
        assert!(SettingsPage::all().contains(&SettingsPage::About));
    }

    #[test]
    fn test_network_is_first() {
        let all = SettingsPage::all();
        assert_eq!(all[0], SettingsPage::Network);
    }

    #[test]
    fn test_about_is_last() {
        let all = SettingsPage::all();
        assert_eq!(all[all.len() - 1], SettingsPage::About);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SERIALIZATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_page_serialization() {
        let page = SettingsPage::Display;
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains("Display"));
    }

    #[test]
    fn test_page_deserialization() {
        let json = "\"Sound\"";
        let page: SettingsPage = serde_json::from_str(json).unwrap();
        assert_eq!(page, SettingsPage::Sound);
    }

    #[test]
    fn test_page_roundtrip() {
        for page in SettingsPage::all() {
            let json = serde_json::to_string(page).unwrap();
            let deserialized: SettingsPage = serde_json::from_str(&json).unwrap();
            assert_eq!(*page, deserialized);
        }
    }
}
