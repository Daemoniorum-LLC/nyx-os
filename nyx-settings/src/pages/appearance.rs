//! Appearance settings page

use iced::widget::{button, column, container, row, text, toggler};
use iced::{Alignment, Element, Length};
use nyx_theme::colors::{AccentColor, NyxColors};
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::button::{button_style, ButtonVariant};
use nyx_theme::widgets::card::card_style;
use nyx_theme::widgets::CardVariant;
use nyx_theme::{ThemeMode, Typography};

/// Appearance page state
#[derive(Debug, Clone)]
pub struct AppearancePage {
    /// Current theme mode
    pub theme_mode: ThemeMode,
    /// Accent color
    pub accent: AccentColor,
    /// Enable animations
    pub animations: bool,
    /// Enable blur effects
    pub blur_effects: bool,
}

impl Default for AppearancePage {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
            accent: AccentColor::Aurora,
            animations: true,
            blur_effects: true,
        }
    }
}

/// Appearance page messages
#[derive(Debug, Clone)]
pub enum AppearanceMessage {
    /// Set theme mode
    SetThemeMode(ThemeMode),
    /// Set accent color
    SetAccent(AccentColor),
    /// Toggle animations
    ToggleAnimations(bool),
    /// Toggle blur effects
    ToggleBlur(bool),
}

impl AppearancePage {
    /// Update state
    pub fn update(&mut self, message: AppearanceMessage) {
        match message {
            AppearanceMessage::SetThemeMode(mode) => {
                self.theme_mode = mode;
            }
            AppearanceMessage::SetAccent(accent) => {
                self.accent = accent;
            }
            AppearanceMessage::ToggleAnimations(enabled) => {
                self.animations = enabled;
            }
            AppearanceMessage::ToggleBlur(enabled) => {
                self.blur_effects = enabled;
            }
        }
    }

    /// View the page
    pub fn view(&self) -> Element<AppearanceMessage> {
        let theme_section = self.view_theme_section();
        let accent_section = self.view_accent_section();
        let effects_section = self.view_effects_section();

        column![
            text("Appearance")
                .size(Typography::SIZE_HEADLINE_LARGE)
                .color(NyxColors::TEXT_BRIGHT),
            text("Customize the look and feel of Nyx OS")
                .size(Typography::SIZE_BODY_MEDIUM)
                .color(NyxColors::TEXT_SECONDARY),
            container(column![theme_section, accent_section, effects_section].spacing(Spacing::LG))
                .padding(Spacing::LG),
        ]
        .spacing(Spacing::MD)
        .width(Length::Fill)
        .into()
    }

    fn view_theme_section(&self) -> Element<AppearanceMessage> {
        let dark_btn = self.theme_button("Dark", ThemeMode::Dark, "󰖔");
        let light_btn = self.theme_button("Light", ThemeMode::Light, "󰖨");
        let auto_btn = self.theme_button("Auto", ThemeMode::System, "󰁪");

        container(
            column![
                text("Theme")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                row![dark_btn, light_btn, auto_btn].spacing(Spacing::MD),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn theme_button(
        &self,
        label: &str,
        mode: ThemeMode,
        icon: &str,
    ) -> Element<AppearanceMessage> {
        let is_selected = self.theme_mode == mode;

        button(
            column![
                text(icon)
                    .size(Typography::SIZE_ICON_XL)
                    .color(if is_selected {
                        NyxColors::AURORA
                    } else {
                        NyxColors::TEXT_SECONDARY
                    }),
                text(label)
                    .size(Typography::SIZE_LABEL_MEDIUM)
                    .color(if is_selected {
                        NyxColors::TEXT_BRIGHT
                    } else {
                        NyxColors::TEXT_SECONDARY
                    }),
            ]
            .spacing(Spacing::SM)
            .align_x(Alignment::Center)
            .width(Length::Fixed(100.0))
            .padding(Spacing::MD),
        )
        .style(if is_selected {
            button_style(ButtonVariant::Primary)
        } else {
            button_style(ButtonVariant::Secondary)
        })
        .on_press(AppearanceMessage::SetThemeMode(mode))
        .into()
    }

    fn view_accent_section(&self) -> Element<AppearanceMessage> {
        let accents = [
            (AccentColor::Aurora, "Aurora", NyxColors::AURORA),
            (AccentColor::Ethereal, "Ethereal", NyxColors::ETHEREAL),
            (AccentColor::Celestial, "Celestial", NyxColors::CELESTIAL),
            (AccentColor::Emerald, "Emerald", NyxColors::SUCCESS),
            (AccentColor::Azure, "Azure", NyxColors::INFO),
            (AccentColor::Amber, "Amber", NyxColors::WARNING),
        ];

        let accent_buttons: Vec<Element<AppearanceMessage>> = accents
            .iter()
            .map(|(accent, name, color)| {
                let is_selected = self.accent == *accent;

                button(
                    container(text(""))
                        .width(Length::Fixed(32.0))
                        .height(Length::Fixed(32.0))
                        .style(move |_theme| iced::widget::container::Style {
                            background: Some(iced::Background::Color(*color)),
                            border: iced::Border {
                                color: if is_selected {
                                    NyxColors::TEXT_BRIGHT
                                } else {
                                    iced::Color::TRANSPARENT
                                },
                                width: if is_selected { 3.0 } else { 0.0 },
                                radius: Spacing::RADIUS_CIRCLE.into(),
                            },
                            ..Default::default()
                        }),
                )
                .style(button_style(ButtonVariant::Ghost))
                .on_press(AppearanceMessage::SetAccent(*accent))
                .into()
            })
            .collect();

        container(
            column![
                text("Accent Color")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                row(accent_buttons).spacing(Spacing::SM),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn view_effects_section(&self) -> Element<AppearanceMessage> {
        container(
            column![
                text("Effects")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                row![
                    column![
                        text("Animations")
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_BRIGHT),
                        text("Enable interface animations")
                            .size(Typography::SIZE_BODY_SMALL)
                            .color(NyxColors::TEXT_SECONDARY),
                    ]
                    .width(Length::Fill),
                    toggler(self.animations)
                        .on_toggle(AppearanceMessage::ToggleAnimations),
                ]
                .align_y(Alignment::Center),
                row![
                    column![
                        text("Blur Effects")
                            .size(Typography::SIZE_BODY_MEDIUM)
                            .color(NyxColors::TEXT_BRIGHT),
                        text("Enable glassmorphism blur (may impact performance)")
                            .size(Typography::SIZE_BODY_SMALL)
                            .color(NyxColors::TEXT_SECONDARY),
                    ]
                    .width(Length::Fill),
                    toggler(self.blur_effects)
                        .on_toggle(AppearanceMessage::ToggleBlur),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // APPEARANCE PAGE DEFAULT TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_appearance_page_default() {
        let page = AppearancePage::default();
        assert_eq!(page.theme_mode, ThemeMode::Dark);
        assert_eq!(page.accent, AccentColor::Aurora);
        assert!(page.animations);
        assert!(page.blur_effects);
    }

    #[test]
    fn test_appearance_page_default_theme_is_dark() {
        let page = AppearancePage::default();
        assert_eq!(page.theme_mode, ThemeMode::Dark);
    }

    #[test]
    fn test_appearance_page_default_accent_is_aurora() {
        let page = AppearancePage::default();
        assert_eq!(page.accent, AccentColor::Aurora);
    }

    #[test]
    fn test_appearance_page_animations_enabled_by_default() {
        let page = AppearancePage::default();
        assert!(page.animations);
    }

    #[test]
    fn test_appearance_page_blur_enabled_by_default() {
        let page = AppearancePage::default();
        assert!(page.blur_effects);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // THEME MODE UPDATE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_set_theme_mode_light() {
        let mut page = AppearancePage::default();
        page.update(AppearanceMessage::SetThemeMode(ThemeMode::Light));
        assert_eq!(page.theme_mode, ThemeMode::Light);
    }

    #[test]
    fn test_set_theme_mode_dark() {
        let mut page = AppearancePage {
            theme_mode: ThemeMode::Light,
            ..Default::default()
        };
        page.update(AppearanceMessage::SetThemeMode(ThemeMode::Dark));
        assert_eq!(page.theme_mode, ThemeMode::Dark);
    }

    #[test]
    fn test_set_theme_mode_system() {
        let mut page = AppearancePage::default();
        page.update(AppearanceMessage::SetThemeMode(ThemeMode::System));
        assert_eq!(page.theme_mode, ThemeMode::System);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ACCENT COLOR UPDATE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_set_accent_ethereal() {
        let mut page = AppearancePage::default();
        page.update(AppearanceMessage::SetAccent(AccentColor::Ethereal));
        assert_eq!(page.accent, AccentColor::Ethereal);
    }

    #[test]
    fn test_set_accent_celestial() {
        let mut page = AppearancePage::default();
        page.update(AppearanceMessage::SetAccent(AccentColor::Celestial));
        assert_eq!(page.accent, AccentColor::Celestial);
    }

    #[test]
    fn test_set_accent_emerald() {
        let mut page = AppearancePage::default();
        page.update(AppearanceMessage::SetAccent(AccentColor::Emerald));
        assert_eq!(page.accent, AccentColor::Emerald);
    }

    #[test]
    fn test_set_accent_azure() {
        let mut page = AppearancePage::default();
        page.update(AppearanceMessage::SetAccent(AccentColor::Azure));
        assert_eq!(page.accent, AccentColor::Azure);
    }

    #[test]
    fn test_set_accent_amber() {
        let mut page = AppearancePage::default();
        page.update(AppearanceMessage::SetAccent(AccentColor::Amber));
        assert_eq!(page.accent, AccentColor::Amber);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // EFFECTS TOGGLE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_toggle_animations_off() {
        let mut page = AppearancePage::default();
        assert!(page.animations);
        page.update(AppearanceMessage::ToggleAnimations(false));
        assert!(!page.animations);
    }

    #[test]
    fn test_toggle_animations_on() {
        let mut page = AppearancePage {
            animations: false,
            ..Default::default()
        };
        page.update(AppearanceMessage::ToggleAnimations(true));
        assert!(page.animations);
    }

    #[test]
    fn test_toggle_blur_off() {
        let mut page = AppearancePage::default();
        assert!(page.blur_effects);
        page.update(AppearanceMessage::ToggleBlur(false));
        assert!(!page.blur_effects);
    }

    #[test]
    fn test_toggle_blur_on() {
        let mut page = AppearancePage {
            blur_effects: false,
            ..Default::default()
        };
        page.update(AppearanceMessage::ToggleBlur(true));
        assert!(page.blur_effects);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // APPEARANCE MESSAGE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_appearance_message_clone() {
        let msg = AppearanceMessage::SetAccent(AccentColor::Aurora);
        let cloned = msg.clone();
        if let AppearanceMessage::SetAccent(accent) = cloned {
            assert_eq!(accent, AccentColor::Aurora);
        } else {
            panic!("Expected SetAccent");
        }
    }

    #[test]
    fn test_appearance_message_debug() {
        let msg = AppearanceMessage::ToggleBlur(true);
        let debug = format!("{:?}", msg);
        assert!(debug.contains("ToggleBlur"));
    }
}
