//! Theme configuration for Nyx OS
//!
//! Provides the main theme struct and iced theme integration.

use crate::colors::{AccentColor, ColorPalette, NyxColors};
use iced::theme::{Custom, Palette};
use iced::{Color, Theme};
use serde::{Deserialize, Serialize};

/// Theme mode (dark or light)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Dark theme (default for Nyx)
    #[default]
    Dark,
    /// Light theme
    Light,
    /// System preference (follows OS setting)
    System,
}

/// Main Nyx OS theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NyxTheme {
    /// Current theme mode
    pub mode: ThemeMode,
    /// Accent color
    pub accent: AccentColor,
    /// Custom accent hex (when accent is Custom)
    pub custom_accent_hex: Option<String>,
    /// Enable glassmorphism effects
    pub glassmorphism: bool,
    /// Enable animations
    pub animations: bool,
    /// Animation speed multiplier (1.0 = normal)
    pub animation_speed: f32,
    /// Enable blur effects (may impact performance)
    pub blur_effects: bool,
    /// Border radius scale (1.0 = default)
    pub corner_radius_scale: f32,
}

impl Default for NyxTheme {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            accent: AccentColor::Aurora,
            custom_accent_hex: None,
            glassmorphism: true,
            animations: true,
            animation_speed: 1.0,
            blur_effects: true,
            corner_radius_scale: 1.0,
        }
    }
}

impl NyxTheme {
    /// Create a new dark theme
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            ..Default::default()
        }
    }

    /// Create a new light theme
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            ..Default::default()
        }
    }

    /// Get the color palette for the current theme
    pub fn palette(&self) -> ColorPalette {
        match self.mode {
            ThemeMode::Dark | ThemeMode::System => ColorPalette::dark(),
            ThemeMode::Light => ColorPalette::light(),
        }
    }

    /// Get the current accent color
    pub fn accent_color(&self) -> Color {
        if self.accent == AccentColor::Custom {
            self.custom_accent_hex
                .as_ref()
                .and_then(|hex| parse_hex_color(hex))
                .unwrap_or(NyxColors::AURORA)
        } else {
            self.accent.to_color()
        }
    }

    /// Set accent color by name
    pub fn with_accent(mut self, accent: AccentColor) -> Self {
        self.accent = accent;
        self
    }

    /// Set custom accent color from hex
    pub fn with_custom_accent(mut self, hex: &str) -> Self {
        self.accent = AccentColor::Custom;
        self.custom_accent_hex = Some(hex.to_string());
        self
    }

    /// Toggle glassmorphism effects
    pub fn with_glassmorphism(mut self, enabled: bool) -> Self {
        self.glassmorphism = enabled;
        self
    }

    /// Set animation speed
    pub fn with_animation_speed(mut self, speed: f32) -> Self {
        self.animation_speed = speed.clamp(0.25, 4.0);
        self
    }

    /// Convert to iced Theme
    pub fn to_iced_theme(&self) -> Theme {
        create_theme(self.mode)
    }
}

/// Create an iced theme from a theme mode
pub fn create_theme(mode: ThemeMode) -> Theme {
    match mode {
        ThemeMode::Dark | ThemeMode::System => create_dark_theme(),
        ThemeMode::Light => create_light_theme(),
    }
}

/// Create the dark iced theme
fn create_dark_theme() -> Theme {
    let palette = Palette {
        background: NyxColors::MIDNIGHT,
        text: NyxColors::TEXT_BRIGHT,
        primary: NyxColors::AURORA,
        success: NyxColors::SUCCESS,
        danger: NyxColors::ERROR,
    };

    Theme::Custom(
        "Nyx Dark".to_string().into(),
        Box::new(Custom::new(palette)),
    )
}

/// Create the light iced theme
fn create_light_theme() -> Theme {
    let palette = Palette {
        background: NyxColors::LIGHT_BG,
        text: NyxColors::TEXT_DARK,
        primary: NyxColors::AURORA,
        success: NyxColors::SUCCESS,
        danger: NyxColors::ERROR,
    };

    Theme::Custom(
        "Nyx Light".to_string().into(),
        Box::new(Custom::new(palette)),
    )
}

/// Parse a hex color string to iced Color
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');

    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color::from_rgb8(r, g, b))
    } else if hex.len() == 8 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
        Some(Color::from_rgba8(r, g, b, a as f32 / 255.0))
    } else {
        None
    }
}

/// Convert a color to hex string
pub fn color_to_hex(color: Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        let color = parse_hex_color("#FF0000").unwrap();
        assert!((color.r - 1.0).abs() < 0.01);
        assert!(color.g.abs() < 0.01);
        assert!(color.b.abs() < 0.01);

        let color = parse_hex_color("00FF00").unwrap();
        assert!(color.r.abs() < 0.01);
        assert!((color.g - 1.0).abs() < 0.01);
        assert!(color.b.abs() < 0.01);
    }

    #[test]
    fn test_color_to_hex() {
        let hex = color_to_hex(Color::from_rgb(1.0, 0.0, 0.0));
        assert_eq!(hex, "#FF0000");
    }
}
