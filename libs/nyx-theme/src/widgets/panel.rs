//! Panel and shell component styles for Nyx OS

use crate::colors::NyxColors;
use crate::spacing::Spacing;
use iced::widget::container::Style;
use iced::{Background, Border, Color, Shadow, Vector};

/// Top panel style (the main desktop panel at the top)
pub fn top_panel_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::GLASS_DARK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.04),
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
    }
}

/// Dock style (the application dock at the bottom)
pub fn dock_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::GLASS_DARK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            width: 1.0,
            radius: Spacing::RADIUS_LG.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 16.0,
        },
    }
}

/// Dock item style (individual dock icons)
pub fn dock_item_style(active: bool, hovered: bool) -> impl Fn(&iced::Theme) -> Style {
    move |_theme| {
        let background = if active {
            Color::from_rgba(1.0, 1.0, 1.0, 0.15)
        } else if hovered {
            Color::from_rgba(1.0, 1.0, 1.0, 0.10)
        } else {
            Color::TRANSPARENT
        };

        Style {
            background: Some(Background::Color(background)),
            text_color: Some(NyxColors::TEXT_BRIGHT),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: Spacing::RADIUS_MD.into(),
            },
            shadow: Shadow::default(),
        }
    }
}

/// Running indicator style (dot below dock icons)
pub fn running_indicator_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::AURORA)),
        text_color: None,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Spacing::RADIUS_CIRCLE.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(
                NyxColors::AURORA.r,
                NyxColors::AURORA.g,
                NyxColors::AURORA.b,
                0.5,
            ),
            offset: Vector::new(0.0, 0.0),
            blur_radius: 4.0,
        },
    }
}

/// Quick settings panel style
pub fn quick_settings_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::GLASS_DARK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            width: 1.0,
            radius: Spacing::RADIUS_XL.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
    }
}

/// Quick toggle tile style
pub fn quick_toggle_style(active: bool) -> impl Fn(&iced::Theme) -> Style {
    move |_theme| {
        let (background, border_color) = if active {
            (NyxColors::AURORA, Color::from_rgba(1.0, 1.0, 1.0, 0.2))
        } else {
            (NyxColors::DUSK, Color::from_rgba(1.0, 1.0, 1.0, 0.08))
        };

        Style {
            background: Some(Background::Color(background)),
            text_color: Some(if active {
                Color::WHITE
            } else {
                NyxColors::TEXT_BRIGHT
            }),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: Spacing::RADIUS_MD.into(),
            },
            shadow: Shadow::default(),
        }
    }
}

/// Slider tile style (for brightness/volume in quick settings)
pub fn slider_tile_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::DUSK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
            width: 1.0,
            radius: Spacing::RADIUS_MD.into(),
        },
        shadow: Shadow::default(),
    }
}

/// Workspace thumbnail style
pub fn workspace_thumbnail_style(active: bool) -> impl Fn(&iced::Theme) -> Style {
    move |_theme| {
        let (border_color, border_width) = if active {
            (NyxColors::AURORA, 2.0)
        } else {
            (NyxColors::BORDER_DARK, 1.0)
        };

        Style {
            background: Some(Background::Color(NyxColors::DUSK)),
            text_color: Some(NyxColors::TEXT_BRIGHT),
            border: Border {
                color: border_color,
                width: border_width,
                radius: Spacing::RADIUS_SM.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.2),
                offset: Vector::new(0.0, 2.0),
                blur_radius: 4.0,
            },
        }
    }
}

/// Notification style
pub fn notification_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::GLASS_DARK)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            width: 1.0,
            radius: Spacing::RADIUS_LG.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
    }
}

/// Popover/menu style
pub fn popover_style() -> impl Fn(&iced::Theme) -> Style {
    |_theme| Style {
        background: Some(Background::Color(NyxColors::TWILIGHT)),
        text_color: Some(NyxColors::TEXT_BRIGHT),
        border: Border {
            color: NyxColors::BORDER_DARK,
            width: 1.0,
            radius: Spacing::RADIUS_MD.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 20.0,
        },
    }
}

/// Menu item style
pub fn menu_item_style(selected: bool) -> impl Fn(&iced::Theme) -> Style {
    move |_theme| {
        let background = if selected {
            NyxColors::AURORA
        } else {
            Color::TRANSPARENT
        };

        Style {
            background: Some(Background::Color(background)),
            text_color: Some(if selected {
                Color::WHITE
            } else {
                NyxColors::TEXT_BRIGHT
            }),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: Spacing::RADIUS_SM.into(),
            },
            shadow: Shadow::default(),
        }
    }
}
