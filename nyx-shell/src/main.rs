//! Nyx Shell - Next-generation desktop shell for Nyx OS
//!
//! The main desktop shell providing:
//! - Top panel with system tray, clock, and quick settings
//! - Bottom dock for application launching
//! - Workspace management
//! - Window overview (Activities)

mod app;
mod config;
mod panel;
mod dock;
mod workspace;
mod system;
mod messages;

use app::NyxShell;
use iced::Application;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> iced::Result {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nyx_shell=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Nyx Shell");

    // Run the shell
    NyxShell::run(iced::Settings {
        window: iced::window::Settings {
            size: iced::Size::new(1920.0, 1080.0),
            position: iced::window::Position::Centered,
            decorations: false,
            transparent: true,
            level: iced::window::Level::AlwaysOnTop,
            ..Default::default()
        },
        antialiasing: true,
        ..Default::default()
    })
}
