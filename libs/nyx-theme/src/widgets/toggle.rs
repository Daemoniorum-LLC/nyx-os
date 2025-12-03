//! Toggle/switch styles for Nyx OS

use crate::colors::NyxColors;
use crate::spacing::Spacing;
use iced::widget::toggler::{self, Status, Style};
use iced::{Background, Color};

/// Create a toggle switch style
pub fn toggle_style() -> impl Fn(&iced::Theme, Status) -> Style {
    |_theme, status| {
        let (background, foreground, background_border) = match status {
            Status::Active { is_toggled } => {
                if is_toggled {
                    (
                        NyxColors::AURORA,
                        Color::WHITE,
                        NyxColors::AURORA,
                    )
                } else {
                    (
                        NyxColors::DUSK,
                        NyxColors::TEXT_SECONDARY,
                        NyxColors::BORDER_DARK,
                    )
                }
            }
            Status::Hovered { is_toggled } => {
                if is_toggled {
                    (
                        NyxColors::AURORA_LIGHT,
                        Color::WHITE,
                        NyxColors::AURORA_LIGHT,
                    )
                } else {
                    (
                        NyxColors::NEBULA,
                        NyxColors::TEXT_BRIGHT,
                        Color::from_rgba(1.0, 1.0, 1.0, 0.15),
                    )
                }
            }
            Status::Disabled => (
                NyxColors::TWILIGHT,
                NyxColors::TEXT_MUTED,
                NyxColors::BORDER_DARK,
            ),
        };

        Style {
            background: Background::Color(background),
            background_border_width: 1.0,
            background_border_color: background_border,
            foreground,
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
        }
    }
}

/// Create a compact toggle style for quick settings tiles
pub fn compact_toggle_style() -> impl Fn(&iced::Theme, Status) -> Style {
    |_theme, status| {
        let (background, foreground) = match status {
            Status::Active { is_toggled } => {
                if is_toggled {
                    (NyxColors::AURORA, Color::WHITE)
                } else {
                    (
                        Color::from_rgba(1.0, 1.0, 1.0, 0.1),
                        NyxColors::TEXT_SECONDARY,
                    )
                }
            }
            Status::Hovered { is_toggled } => {
                if is_toggled {
                    (NyxColors::AURORA_LIGHT, Color::WHITE)
                } else {
                    (
                        Color::from_rgba(1.0, 1.0, 1.0, 0.15),
                        NyxColors::TEXT_BRIGHT,
                    )
                }
            }
            Status::Disabled => (
                Color::from_rgba(1.0, 1.0, 1.0, 0.05),
                NyxColors::TEXT_MUTED,
            ),
        };

        Style {
            background: Background::Color(background),
            background_border_width: 0.0,
            background_border_color: Color::TRANSPARENT,
            foreground,
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
        }
    }
}
