//! Nyx Assistant - AI-powered command palette and assistant for Nyx OS
//!
//! A Spotlight-like interface providing:
//! - Application launching with fuzzy search
//! - Natural language commands
//! - File and folder search
//! - Calculator
//! - System commands
//! - AI-powered suggestions

mod app;
mod commands;
mod search;

use app::NyxAssistant;
use iced::Application;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> iced::Result {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nyx_assistant=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Nyx Assistant");

    // Calculate center position
    let width = 680.0;
    let height = 480.0;

    // Run the assistant
    NyxAssistant::run(iced::Settings {
        window: iced::window::Settings {
            size: iced::Size::new(width, height),
            position: iced::window::Position::Centered,
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
