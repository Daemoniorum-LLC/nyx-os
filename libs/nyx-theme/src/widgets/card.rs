//! Card/container styles for Nyx OS

use crate::colors::NyxColors;
use crate::spacing::Spacing;
use iced::widget::container::{self, Style};
use iced::{Background, Border, Color, Shadow, Vector};

/// Card variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardVariant {
    /// Default card with subtle elevation
    #[default]
    Default,
    /// Elevated card with shadow
    Elevated,
    /// Outlined card with border
    Outlined,
    /// Glass card with transparency (for overlays)
    Glass,
    /// Flat card (no elevation)
    Flat,
    /// Interactive card (hover effects)
    Interactive,
}

/// Create a container style function for the given card variant
pub fn card_style(variant: CardVariant) -> impl Fn(&iced::Theme) -> Style {
    move |_theme| match variant {
        CardVariant::Default => default_card_style(),
        CardVariant::Elevated => elevated_card_style(),
        CardVariant::Outlined => outlined_card_style(),
        CardVariant::Glass => glass_card_style(),
        CardVariant::Flat => flat_card_style(),
        CardVariant::Interactive => interactive_card_style(),
    }
}

fn default_card_style() -> Style {
    Style {
        background: Some(Background::Color(NyxColors::TWILIGHT)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: NyxColors::BORDER_DARK,
            width: 1.0,
            radius: Spacing::RADIUS_MD.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.15),
            offset: Vector::new(0.0, 2.0),
            blur_radius: Spacing::SHADOW_SM,
        },
    }
}

fn elevated_card_style() -> Style {
    Style {
        background: Some(Background::Color(NyxColors::DUSK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: NyxColors::BORDER_DARK,
            width: 1.0,
            radius: Spacing::RADIUS_MD.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, 4.0),
            blur_radius: Spacing::SHADOW_MD,
        },
    }
}

fn outlined_card_style() -> Style {
    Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: NyxColors::BORDER_DARK,
            width: 1.0,
            radius: Spacing::RADIUS_MD.into(),
        },
        shadow: Shadow::default(),
    }
}

fn glass_card_style() -> Style {
    Style {
        background: Some(Background::Color(NyxColors::GLASS_DARK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.1),
            width: 1.0,
            radius: Spacing::RADIUS_LG.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            offset: Vector::new(0.0, 8.0),
            blur_radius: Spacing::SHADOW_LG,
        },
    }
}

fn flat_card_style() -> Style {
    Style {
        background: Some(Background::Color(NyxColors::TWILIGHT)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Spacing::RADIUS_MD.into(),
        },
        shadow: Shadow::default(),
    }
}

fn interactive_card_style() -> Style {
    Style {
        background: Some(Background::Color(NyxColors::TWILIGHT)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: NyxColors::BORDER_DARK,
            width: 1.0,
            radius: Spacing::RADIUS_MD.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.15),
            offset: Vector::new(0.0, 2.0),
            blur_radius: Spacing::SHADOW_SM,
        },
    }
}

/// Panel container style (for shell panels)
pub fn panel_container_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::GLASS_DARK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
    }
}

/// Modal/dialog container style
pub fn modal_container_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::TWILIGHT)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: NyxColors::BORDER_DARK,
            width: 1.0,
            radius: Spacing::RADIUS_XL.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: Vector::new(0.0, 16.0),
            blur_radius: Spacing::SHADOW_XL,
        },
    }
}

/// Tooltip container style
pub fn tooltip_container_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::DUSK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: NyxColors::BORDER_DARK,
            width: 1.0,
            radius: Spacing::RADIUS_SM.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            offset: Vector::new(0.0, 4.0),
            blur_radius: Spacing::SHADOW_SM,
        },
    }
}
