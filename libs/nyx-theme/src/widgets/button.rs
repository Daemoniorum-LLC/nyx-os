//! Button styles for Nyx OS

use crate::colors::NyxColors;
use crate::spacing::Spacing;
use iced::widget::button::{self, Status, Style};
use iced::{Background, Border, Color, Shadow, Vector};

/// Button variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Primary action button (filled with accent)
    #[default]
    Primary,
    /// Secondary button (outlined)
    Secondary,
    /// Ghost/text button (no background)
    Ghost,
    /// Destructive/danger button
    Danger,
    /// Success button
    Success,
    /// Icon-only button
    Icon,
    /// Panel button (for shell panels)
    Panel,
}

/// Button sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// Small button
    Small,
    /// Medium button (default)
    #[default]
    Medium,
    /// Large button
    Large,
}

impl ButtonSize {
    /// Get height for this size
    pub fn height(self) -> f32 {
        match self {
            ButtonSize::Small => Spacing::BUTTON_HEIGHT_SM,
            ButtonSize::Medium => Spacing::BUTTON_HEIGHT_MD,
            ButtonSize::Large => Spacing::BUTTON_HEIGHT_LG,
        }
    }

    /// Get horizontal padding for this size
    pub fn padding_h(self) -> f32 {
        match self {
            ButtonSize::Small => Spacing::SM,
            ButtonSize::Medium => Spacing::MD,
            ButtonSize::Large => Spacing::LG,
        }
    }
}

/// Create a button style function for the given variant
pub fn button_style(variant: ButtonVariant) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| match variant {
        ButtonVariant::Primary => primary_style(status),
        ButtonVariant::Secondary => secondary_style(status),
        ButtonVariant::Ghost => ghost_style(status),
        ButtonVariant::Danger => danger_style(status),
        ButtonVariant::Success => success_style(status),
        ButtonVariant::Icon => icon_style(status),
        ButtonVariant::Panel => panel_style(status),
    }
}

fn primary_style(status: Status) -> Style {
    let (background, text) = match status {
        Status::Active => (NyxColors::AURORA, Color::WHITE),
        Status::Hovered => (NyxColors::AURORA_LIGHT, Color::WHITE),
        Status::Pressed => (NyxColors::AURORA_DARK, Color::WHITE),
        Status::Disabled => (
            Color::from_rgba(NyxColors::AURORA.r, NyxColors::AURORA.g, NyxColors::AURORA.b, 0.4),
            Color::from_rgba(1.0, 1.0, 1.0, 0.5),
        ),
    };

    Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Spacing::RADIUS_SM.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.2),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 4.0,
        },
    }
}

fn secondary_style(status: Status) -> Style {
    let (background, border_color, text) = match status {
        Status::Active => (Color::TRANSPARENT, NyxColors::BORDER_DARK, NyxColors::TEXT_BRIGHT),
        Status::Hovered => (NyxColors::NEBULA, NyxColors::BORDER_DARK, NyxColors::TEXT_BRIGHT),
        Status::Pressed => (NyxColors::TWILIGHT, NyxColors::AURORA, NyxColors::TEXT_BRIGHT),
        Status::Disabled => (
            Color::TRANSPARENT,
            Color::from_rgba(1.0, 1.0, 1.0, 0.1),
            NyxColors::TEXT_MUTED,
        ),
    };

    Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: Spacing::RADIUS_SM.into(),
        },
        shadow: Shadow::default(),
    }
}

fn ghost_style(status: Status) -> Style {
    let (background, text) = match status {
        Status::Active => (Color::TRANSPARENT, NyxColors::TEXT_SECONDARY),
        Status::Hovered => (NyxColors::NEBULA, NyxColors::TEXT_BRIGHT),
        Status::Pressed => (NyxColors::TWILIGHT, NyxColors::TEXT_BRIGHT),
        Status::Disabled => (Color::TRANSPARENT, NyxColors::TEXT_MUTED),
    };

    Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Spacing::RADIUS_SM.into(),
        },
        shadow: Shadow::default(),
    }
}

fn danger_style(status: Status) -> Style {
    let (background, text) = match status {
        Status::Active => (NyxColors::ERROR, Color::WHITE),
        Status::Hovered => (crate::colors::lighten(NyxColors::ERROR, 0.1), Color::WHITE),
        Status::Pressed => (crate::colors::darken(NyxColors::ERROR, 0.1), Color::WHITE),
        Status::Disabled => (
            Color::from_rgba(NyxColors::ERROR.r, NyxColors::ERROR.g, NyxColors::ERROR.b, 0.4),
            Color::from_rgba(1.0, 1.0, 1.0, 0.5),
        ),
    };

    Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Spacing::RADIUS_SM.into(),
        },
        shadow: Shadow::default(),
    }
}

fn success_style(status: Status) -> Style {
    let (background, text) = match status {
        Status::Active => (NyxColors::SUCCESS, Color::WHITE),
        Status::Hovered => (crate::colors::lighten(NyxColors::SUCCESS, 0.1), Color::WHITE),
        Status::Pressed => (crate::colors::darken(NyxColors::SUCCESS, 0.1), Color::WHITE),
        Status::Disabled => (
            Color::from_rgba(
                NyxColors::SUCCESS.r,
                NyxColors::SUCCESS.g,
                NyxColors::SUCCESS.b,
                0.4,
            ),
            Color::from_rgba(1.0, 1.0, 1.0, 0.5),
        ),
    };

    Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Spacing::RADIUS_SM.into(),
        },
        shadow: Shadow::default(),
    }
}

fn icon_style(status: Status) -> Style {
    let (background, text) = match status {
        Status::Active => (Color::TRANSPARENT, NyxColors::TEXT_SECONDARY),
        Status::Hovered => (NyxColors::NEBULA, NyxColors::TEXT_BRIGHT),
        Status::Pressed => (NyxColors::TWILIGHT, NyxColors::AURORA),
        Status::Disabled => (Color::TRANSPARENT, NyxColors::TEXT_MUTED),
    };

    Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Spacing::RADIUS_CIRCLE.into(),
        },
        shadow: Shadow::default(),
    }
}

fn panel_style(status: Status) -> Style {
    let (background, text) = match status {
        Status::Active => (Color::TRANSPARENT, NyxColors::TEXT_BRIGHT),
        Status::Hovered => (
            Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            NyxColors::TEXT_BRIGHT,
        ),
        Status::Pressed => (Color::from_rgba(1.0, 1.0, 1.0, 0.12), NyxColors::AURORA),
        Status::Disabled => (Color::TRANSPARENT, NyxColors::TEXT_MUTED),
    };

    Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Spacing::RADIUS_SM.into(),
        },
        shadow: Shadow::default(),
    }
}
