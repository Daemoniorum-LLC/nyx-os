//! # Nyx Theme
//!
//! Design system and theming library for Nyx OS next-generation GUI components.
//!
//! This library provides:
//! - Color palettes (dark/light themes with accent colors)
//! - Typography scales
//! - Spacing and layout constants
//! - Glassmorphism and modern visual effects
//! - Reusable styled widgets

pub mod colors;
pub mod fonts;
pub mod icons;
pub mod spacing;
pub mod theme;
pub mod widgets;

pub use colors::{ColorPalette, NyxColors};
pub use fonts::Typography;
pub use spacing::Spacing;
pub use theme::{NyxTheme, ThemeMode};

use iced::Theme;

/// Get the current Nyx theme
pub fn nyx_theme(mode: ThemeMode) -> Theme {
    theme::create_theme(mode)
}

/// Convenience function for dark theme
pub fn dark_theme() -> Theme {
    nyx_theme(ThemeMode::Dark)
}

/// Convenience function for light theme
pub fn light_theme() -> Theme {
    nyx_theme(ThemeMode::Light)
}
