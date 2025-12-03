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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_palette_has_dark_background() {
        let palette = ColorPalette::dark();
        // Dark mode background should be dark (low RGB values)
        assert!(palette.background.r < 0.2);
        assert!(palette.background.g < 0.2);
        assert!(palette.background.b < 0.2);
    }

    #[test]
    fn test_light_palette_has_light_background() {
        let palette = ColorPalette::light();
        // Light mode background should be light (high RGB values)
        assert!(palette.background.r > 0.9);
        assert!(palette.background.g > 0.9);
        assert!(palette.background.b > 0.9);
    }

    #[test]
    fn test_dark_palette_text_is_bright() {
        let palette = ColorPalette::dark();
        // Dark mode text should be bright for contrast
        assert!(palette.text_primary.r > 0.9);
        assert!(palette.text_primary.g > 0.9);
    }

    #[test]
    fn test_light_palette_text_is_dark() {
        let palette = ColorPalette::light();
        // Light mode text should be dark for contrast
        assert!(palette.text_primary.r < 0.2);
        assert!(palette.text_primary.g < 0.2);
    }

    #[test]
    fn test_accent_colors_are_distinct() {
        let aurora = AccentColor::Aurora.to_color();
        let ethereal = AccentColor::Ethereal.to_color();
        let celestial = AccentColor::Celestial.to_color();

        // Each accent should have a dominant color channel
        assert!(aurora.b > aurora.r && aurora.b > aurora.g); // Purple-blue
        assert!(ethereal.g > ethereal.r); // Cyan-teal
        assert!(celestial.r > celestial.b); // Pink-magenta
    }

    #[test]
    fn test_lighten_increases_brightness() {
        let color = Color::from_rgb(0.5, 0.5, 0.5);
        let lighter = lighten(color, 0.2);

        assert!(lighter.r > color.r);
        assert!(lighter.g > color.g);
        assert!(lighter.b > color.b);
    }

    #[test]
    fn test_lighten_clamps_at_max() {
        let color = Color::from_rgb(0.95, 0.95, 0.95);
        let lighter = lighten(color, 0.2);

        assert!((lighter.r - 1.0).abs() < 0.001);
        assert!((lighter.g - 1.0).abs() < 0.001);
        assert!((lighter.b - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_darken_decreases_brightness() {
        let color = Color::from_rgb(0.5, 0.5, 0.5);
        let darker = darken(color, 0.2);

        assert!(darker.r < color.r);
        assert!(darker.g < color.g);
        assert!(darker.b < color.b);
    }

    #[test]
    fn test_darken_clamps_at_min() {
        let color = Color::from_rgb(0.05, 0.05, 0.05);
        let darker = darken(color, 0.2);

        assert!(darker.r.abs() < 0.001);
        assert!(darker.g.abs() < 0.001);
        assert!(darker.b.abs() < 0.001);
    }

    #[test]
    fn test_with_alpha_preserves_rgb() {
        let color = Color::from_rgb(0.5, 0.6, 0.7);
        let with_alpha = with_alpha(color, 0.5);

        assert!((with_alpha.r - color.r).abs() < 0.001);
        assert!((with_alpha.g - color.g).abs() < 0.001);
        assert!((with_alpha.b - color.b).abs() < 0.001);
        assert!((with_alpha.a - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_accent_hover_is_lighter() {
        let base = AccentColor::Aurora.to_color();
        let hover = AccentColor::Aurora.to_hover_color();

        // Hover should be lighter (higher values)
        assert!(hover.r >= base.r);
        assert!(hover.g >= base.g);
        assert!(hover.b >= base.b);
    }

    #[test]
    fn test_accent_pressed_is_darker() {
        let base = AccentColor::Aurora.to_color();
        let pressed = AccentColor::Aurora.to_pressed_color();

        // Pressed should be darker (lower values)
        assert!(pressed.r <= base.r);
        assert!(pressed.g <= base.g);
        assert!(pressed.b <= base.b);
    }

    #[test]
    fn test_all_accent_colors() {
        // Ensure all accent colors can be converted without panic
        let accents = [
            AccentColor::Aurora,
            AccentColor::Ethereal,
            AccentColor::Celestial,
            AccentColor::Emerald,
            AccentColor::Azure,
            AccentColor::Amber,
            AccentColor::Custom,
        ];

        for accent in accents {
            let _ = accent.to_color();
            let _ = accent.to_hover_color();
            let _ = accent.to_pressed_color();
        }
    }

    #[test]
    fn test_default_accent_is_aurora() {
        assert_eq!(AccentColor::default(), AccentColor::Aurora);
    }

    #[test]
    fn test_semantic_colors_are_appropriate() {
        // Success should be greenish
        assert!(NyxColors::SUCCESS.g > NyxColors::SUCCESS.r);
        // Error should be reddish
        assert!(NyxColors::ERROR.r > NyxColors::ERROR.g);
        // Warning should be yellowish (high R and G)
        assert!(NyxColors::WARNING.r > 0.8);
        assert!(NyxColors::WARNING.g > 0.6);
    }
}
