//! Input/text field styles for Nyx OS

use crate::colors::NyxColors;
use crate::spacing::Spacing;
use iced::widget::text_input::{self, Status, Style};
use iced::{Background, Border, Color};

/// Input variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputVariant {
    /// Default input style
    #[default]
    Default,
    /// Filled input (solid background)
    Filled,
    /// Ghost input (minimal, for inline editing)
    Ghost,
    /// Search input (with search styling)
    Search,
}

/// Create a text input style function for the given variant
pub fn input_style(variant: InputVariant) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| match variant {
        InputVariant::Default => default_input_style(status),
        InputVariant::Filled => filled_input_style(status),
        InputVariant::Ghost => ghost_input_style(status),
        InputVariant::Search => search_input_style(status),
    }
}

fn default_input_style(status: Status) -> Style {
    let (background, border_color) = match status {
        Status::Active => (NyxColors::TWILIGHT, NyxColors::BORDER_DARK),
        Status::Hovered => (NyxColors::TWILIGHT, Color::from_rgba(1.0, 1.0, 1.0, 0.15)),
        Status::Focused => (NyxColors::TWILIGHT, NyxColors::AURORA),
        Status::Disabled => (NyxColors::DUSK, Color::from_rgba(1.0, 1.0, 1.0, 0.05)),
    };

    Style {
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: Spacing::RADIUS_SM.into(),
        },
        icon: NyxColors::TEXT_SECONDARY,
        placeholder: NyxColors::TEXT_MUTED,
        value: NyxColors::TEXT_BRIGHT,
        selection: Color::from_rgba(NyxColors::AURORA.r, NyxColors::AURORA.g, NyxColors::AURORA.b, 0.3),
    }
}

fn filled_input_style(status: Status) -> Style {
    let (background, border_color) = match status {
        Status::Active => (NyxColors::DUSK, Color::TRANSPARENT),
        Status::Hovered => (NyxColors::NEBULA, Color::TRANSPARENT),
        Status::Focused => (NyxColors::DUSK, NyxColors::AURORA),
        Status::Disabled => (NyxColors::TWILIGHT, Color::TRANSPARENT),
    };

    Style {
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: if matches!(status, Status::Focused) { 2.0 } else { 0.0 },
            radius: Spacing::RADIUS_SM.into(),
        },
        icon: NyxColors::TEXT_SECONDARY,
        placeholder: NyxColors::TEXT_MUTED,
        value: NyxColors::TEXT_BRIGHT,
        selection: Color::from_rgba(NyxColors::AURORA.r, NyxColors::AURORA.g, NyxColors::AURORA.b, 0.3),
    }
}

fn ghost_input_style(status: Status) -> Style {
    let (background, border_color) = match status {
        Status::Active => (Color::TRANSPARENT, Color::TRANSPARENT),
        Status::Hovered => (Color::from_rgba(1.0, 1.0, 1.0, 0.05), Color::TRANSPARENT),
        Status::Focused => (Color::from_rgba(1.0, 1.0, 1.0, 0.05), NyxColors::AURORA),
        Status::Disabled => (Color::TRANSPARENT, Color::TRANSPARENT),
    };

    Style {
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: if matches!(status, Status::Focused) { 1.0 } else { 0.0 },
            radius: Spacing::RADIUS_SM.into(),
        },
        icon: NyxColors::TEXT_SECONDARY,
        placeholder: NyxColors::TEXT_MUTED,
        value: NyxColors::TEXT_BRIGHT,
        selection: Color::from_rgba(NyxColors::AURORA.r, NyxColors::AURORA.g, NyxColors::AURORA.b, 0.3),
    }
}

fn search_input_style(status: Status) -> Style {
    let (background, border_color) = match status {
        Status::Active => (NyxColors::DUSK, Color::TRANSPARENT),
        Status::Hovered => (NyxColors::NEBULA, Color::TRANSPARENT),
        Status::Focused => (NyxColors::DUSK, NyxColors::AURORA),
        Status::Disabled => (NyxColors::TWILIGHT, Color::TRANSPARENT),
    };

    Style {
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: if matches!(status, Status::Focused) { 2.0 } else { 0.0 },
            radius: Spacing::RADIUS_PILL.into(),
        },
        icon: NyxColors::TEXT_SECONDARY,
        placeholder: NyxColors::TEXT_MUTED,
        value: NyxColors::TEXT_BRIGHT,
        selection: Color::from_rgba(NyxColors::AURORA.r, NyxColors::AURORA.g, NyxColors::AURORA.b, 0.3),
    }
}
