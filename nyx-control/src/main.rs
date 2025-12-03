//! Nyx Control - Quick settings and control center for Nyx OS
//!
//! A floating control panel providing quick access to:
//! - WiFi, Bluetooth, Airplane mode toggles
//! - Volume and brightness controls
//! - Night light and Do Not Disturb
//! - Power options
//! - System status

mod app;
mod controls;

use app::NyxControl;
use iced::Application;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> iced::Result {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nyx_control=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Nyx Control");

    // Run the control center
    NyxControl::run(iced::Settings {
        window: iced::window::Settings {
            size: iced::Size::new(380.0, 520.0),
            position: iced::window::Position::Specific(iced::Point::new(
                1920.0 - 380.0 - 16.0,
                48.0,
            )),
            decorations: false,
            transparent: true,
            level: iced::window::Level::AlwaysOnTop,
            resizable: false,
            ..Default::default()
        },
        antialiasing: true,
        ..Default::default()
    })
}
