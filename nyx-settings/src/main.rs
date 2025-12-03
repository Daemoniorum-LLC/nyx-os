//! Nyx Settings - System configuration application for Nyx OS
//!
//! A comprehensive settings application with pages for:
//! - Network (WiFi, Ethernet, VPN)
//! - Bluetooth
//! - Display (Resolution, Night Light, Scaling)
//! - Sound (Volume, Input/Output devices)
//! - Appearance (Theme, Colors, Fonts)
//! - Notifications
//! - Power
//! - Users
//! - About

mod app;
mod pages;

use app::NyxSettings;
use iced::Application;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> iced::Result {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nyx_settings=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Nyx Settings");

    // Run settings app
    NyxSettings::run(iced::Settings {
        window: iced::window::Settings {
            size: iced::Size::new(1000.0, 700.0),
            position: iced::window::Position::Centered,
            min_size: Some(iced::Size::new(800.0, 500.0)),
            decorations: true,
            ..Default::default()
        },
        antialiasing: true,
        ..Default::default()
    })
}
