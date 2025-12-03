//! Color definitions for Nyx OS theme
//!
//! The color palette is inspired by the night sky (Nyx being the Greek goddess of night),
//! featuring deep purples, electric blues, and ethereal accent colors.

use iced::Color;
use serde::{Deserialize, Serialize};

/// A complete color palette for Nyx OS
#[derive(Debug, Clone, Copy)]
pub struct ColorPalette {
    /// Primary background color
    pub background: Color,
    /// Secondary/elevated background
    pub surface: Color,
    /// Tertiary/card background
    pub surface_elevated: Color,

    /// Primary text color
    pub text_primary: Color,
    /// Secondary/muted text
    pub text_secondary: Color,
    /// Disabled/placeholder text
    pub text_disabled: Color,

    /// Primary accent color
    pub accent: Color,
    /// Accent hover state
    pub accent_hover: Color,
    /// Accent pressed state
    pub accent_pressed: Color,

    /// Success color (green)
    pub success: Color,
    /// Warning color (amber)
    pub warning: Color,
    /// Error/danger color (red)
    pub error: Color,
    /// Info color (blue)
    pub info: Color,

    /// Border colors
    pub border: Color,
    pub border_focus: Color,

    /// Overlay/scrim color (for modals, etc.)
    pub overlay: Color,

    /// Glassmorphism effect color
    pub glass: Color,
}

/// Nyx OS color constants
pub struct NyxColors;

impl NyxColors {
    // ═══════════════════════════════════════════════════════════════════════════
    // NIGHT SKY PALETTE (Primary Theme Colors)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Deep void - darkest background
    pub const VOID: Color = Color::from_rgb(0.035, 0.035, 0.055);
    /// Midnight - primary background
    pub const MIDNIGHT: Color = Color::from_rgb(0.055, 0.055, 0.082);
    /// Twilight - elevated surfaces
    pub const TWILIGHT: Color = Color::from_rgb(0.082, 0.082, 0.118);
    /// Dusk - cards and elevated content
    pub const DUSK: Color = Color::from_rgb(0.110, 0.110, 0.157);
    /// Nebula - hover states
    pub const NEBULA: Color = Color::from_rgb(0.145, 0.145, 0.200);

    // ═══════════════════════════════════════════════════════════════════════════
    // ETHEREAL ACCENTS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Aurora - primary accent (electric purple-blue)
    pub const AURORA: Color = Color::from_rgb(0.506, 0.373, 0.961);
    /// Aurora light variant
    pub const AURORA_LIGHT: Color = Color::from_rgb(0.608, 0.502, 0.976);
    /// Aurora dark variant
    pub const AURORA_DARK: Color = Color::from_rgb(0.392, 0.255, 0.878);

    /// Ethereal - secondary accent (cyan-teal)
    pub const ETHEREAL: Color = Color::from_rgb(0.298, 0.835, 0.910);
    /// Celestial - tertiary accent (pink-magenta)
    pub const CELESTIAL: Color = Color::from_rgb(0.910, 0.388, 0.690);

    // ═══════════════════════════════════════════════════════════════════════════
    // SEMANTIC COLORS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Success green
    pub const SUCCESS: Color = Color::from_rgb(0.298, 0.851, 0.569);
    /// Warning amber
    pub const WARNING: Color = Color::from_rgb(0.969, 0.757, 0.282);
    /// Error red
    pub const ERROR: Color = Color::from_rgb(0.937, 0.345, 0.396);
    /// Info blue
    pub const INFO: Color = Color::from_rgb(0.376, 0.620, 0.980);

    // ═══════════════════════════════════════════════════════════════════════════
    // TEXT COLORS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Pure white for primary text
    pub const TEXT_BRIGHT: Color = Color::from_rgb(0.965, 0.965, 0.980);
    /// Off-white for secondary text
    pub const TEXT_SECONDARY: Color = Color::from_rgb(0.675, 0.675, 0.725);
    /// Muted for disabled/tertiary text
    pub const TEXT_MUTED: Color = Color::from_rgb(0.475, 0.475, 0.525);

    /// Dark text (for light mode)
    pub const TEXT_DARK: Color = Color::from_rgb(0.067, 0.067, 0.090);
    pub const TEXT_DARK_SECONDARY: Color = Color::from_rgb(0.333, 0.333, 0.373);

    // ═══════════════════════════════════════════════════════════════════════════
    // LIGHT MODE COLORS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Light background
    pub const LIGHT_BG: Color = Color::from_rgb(0.976, 0.976, 0.988);
    /// Light surface
    pub const LIGHT_SURFACE: Color = Color::from_rgb(1.0, 1.0, 1.0);
    /// Light elevated surface
    pub const LIGHT_ELEVATED: Color = Color::from_rgb(0.965, 0.965, 0.976);

    // ═══════════════════════════════════════════════════════════════════════════
    // SPECIAL EFFECTS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Glassmorphism background (semi-transparent)
    pub const GLASS_DARK: Color = Color::from_rgba(0.055, 0.055, 0.082, 0.75);
    pub const GLASS_LIGHT: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.70);

    /// Border colors
    pub const BORDER_DARK: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.08);
    pub const BORDER_LIGHT: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.08);
    pub const BORDER_FOCUS: Color = Color::from_rgba(0.506, 0.373, 0.961, 0.60);

    /// Overlay/scrim
    pub const OVERLAY: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.60);

    /// Transparent
    pub const TRANSPARENT: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
}

impl ColorPalette {
    /// Create the dark mode color palette
    pub fn dark() -> Self {
        Self {
            background: NyxColors::MIDNIGHT,
            surface: NyxColors::TWILIGHT,
            surface_elevated: NyxColors::DUSK,

            text_primary: NyxColors::TEXT_BRIGHT,
            text_secondary: NyxColors::TEXT_SECONDARY,
            text_disabled: NyxColors::TEXT_MUTED,

            accent: NyxColors::AURORA,
            accent_hover: NyxColors::AURORA_LIGHT,
            accent_pressed: NyxColors::AURORA_DARK,

            success: NyxColors::SUCCESS,
            warning: NyxColors::WARNING,
            error: NyxColors::ERROR,
            info: NyxColors::INFO,

            border: NyxColors::BORDER_DARK,
            border_focus: NyxColors::BORDER_FOCUS,

            overlay: NyxColors::OVERLAY,
            glass: NyxColors::GLASS_DARK,
        }
    }

    /// Create the light mode color palette
    pub fn light() -> Self {
        Self {
            background: NyxColors::LIGHT_BG,
            surface: NyxColors::LIGHT_SURFACE,
            surface_elevated: NyxColors::LIGHT_ELEVATED,

            text_primary: NyxColors::TEXT_DARK,
            text_secondary: NyxColors::TEXT_DARK_SECONDARY,
            text_disabled: NyxColors::TEXT_MUTED,

            accent: NyxColors::AURORA,
            accent_hover: NyxColors::AURORA_LIGHT,
            accent_pressed: NyxColors::AURORA_DARK,

            success: NyxColors::SUCCESS,
            warning: NyxColors::WARNING,
            error: NyxColors::ERROR,
            info: NyxColors::INFO,

            border: NyxColors::BORDER_LIGHT,
            border_focus: NyxColors::BORDER_FOCUS,

            overlay: NyxColors::OVERLAY,
            glass: NyxColors::GLASS_LIGHT,
        }
    }
}

/// Theme accent color options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AccentColor {
    /// Aurora purple-blue (default)
    #[default]
    Aurora,
    /// Ethereal cyan-teal
    Ethereal,
    /// Celestial pink-magenta
    Celestial,
    /// Success green
    Emerald,
    /// Info blue
    Azure,
    /// Warning amber
    Amber,
    /// Custom hex color
    Custom,
}

impl AccentColor {
    /// Get the actual color for this accent
    pub fn to_color(self) -> Color {
        match self {
            AccentColor::Aurora => NyxColors::AURORA,
            AccentColor::Ethereal => NyxColors::ETHEREAL,
            AccentColor::Celestial => NyxColors::CELESTIAL,
            AccentColor::Emerald => NyxColors::SUCCESS,
            AccentColor::Azure => NyxColors::INFO,
            AccentColor::Amber => NyxColors::WARNING,
            AccentColor::Custom => NyxColors::AURORA, // Fallback
        }
    }

    /// Get hover variant of the accent
    pub fn to_hover_color(self) -> Color {
        let base = self.to_color();
        lighten(base, 0.12)
    }

    /// Get pressed variant of the accent
    pub fn to_pressed_color(self) -> Color {
        let base = self.to_color();
        darken(base, 0.12)
    }
}

/// Lighten a color by a percentage
pub fn lighten(color: Color, amount: f32) -> Color {
    Color::from_rgb(
        (color.r + amount).min(1.0),
        (color.g + amount).min(1.0),
        (color.b + amount).min(1.0),
    )
}

/// Darken a color by a percentage
pub fn darken(color: Color, amount: f32) -> Color {
    Color::from_rgb(
        (color.r - amount).max(0.0),
        (color.g - amount).max(0.0),
        (color.b - amount).max(0.0),
    )
}

/// Adjust alpha of a color
pub fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, alpha)
}
