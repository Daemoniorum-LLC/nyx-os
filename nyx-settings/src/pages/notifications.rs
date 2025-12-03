//! Notifications settings page

use iced::widget::{column, container, row, text, toggler};
use iced::{Alignment, Element, Length};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::card::card_style;
use nyx_theme::widgets::CardVariant;
use nyx_theme::Typography;

/// Notifications page state
#[derive(Debug, Clone)]
pub struct NotificationsPage {
    /// Do not disturb
    pub dnd: bool,
    /// Show previews
    pub show_previews: bool,
    /// Show on lock screen
    pub show_on_lock: bool,
    /// Enable sounds
    pub sounds: bool,
    /// Notification badges
    pub badges: bool,
}

impl Default for NotificationsPage {
    fn default() -> Self {
        Self {
            dnd: false,
            show_previews: true,
            show_on_lock: true,
            sounds: true,
            badges: true,
        }
    }
}

/// Notifications messages
#[derive(Debug, Clone)]
pub enum NotificationsMessage {
    /// Toggle DND
    ToggleDnd(bool),
    /// Toggle previews
    TogglePreviews(bool),
    /// Toggle lock screen
    ToggleLockScreen(bool),
    /// Toggle sounds
    ToggleSounds(bool),
    /// Toggle badges
    ToggleBadges(bool),
}

impl NotificationsPage {
    /// Update state
    pub fn update(&mut self, message: NotificationsMessage) {
        match message {
            NotificationsMessage::ToggleDnd(enabled) => self.dnd = enabled,
            NotificationsMessage::TogglePreviews(enabled) => self.show_previews = enabled,
            NotificationsMessage::ToggleLockScreen(enabled) => self.show_on_lock = enabled,
            NotificationsMessage::ToggleSounds(enabled) => self.sounds = enabled,
            NotificationsMessage::ToggleBadges(enabled) => self.badges = enabled,
        }
    }

    /// View the page
    pub fn view(&self) -> Element<NotificationsMessage> {
        column![
            text("Notifications")
                .size(Typography::SIZE_HEADLINE_LARGE)
                .color(NyxColors::TEXT_BRIGHT),
            text("Configure how notifications appear")
                .size(Typography::SIZE_BODY_MEDIUM)
                .color(NyxColors::TEXT_SECONDARY),
            self.view_dnd_section(),
            self.view_settings_section(),
        ]
        .spacing(Spacing::MD)
        .width(Length::Fill)
        .padding(Spacing::LG)
        .into()
    }

    fn view_dnd_section(&self) -> Element<NotificationsMessage> {
        container(
            row![
                text(if self.dnd { "󰂛" } else { "󰂚" })
                    .size(Typography::SIZE_ICON_XL)
                    .color(if self.dnd {
                        NyxColors::AURORA
                    } else {
                        NyxColors::TEXT_MUTED
                    }),
                column![
                    text("Do Not Disturb")
                        .size(Typography::SIZE_BODY_LARGE)
                        .color(NyxColors::TEXT_BRIGHT),
                    text("Silence all notifications")
                        .size(Typography::SIZE_BODY_SMALL)
                        .color(NyxColors::TEXT_SECONDARY),
                ]
                .width(Length::Fill),
                toggler(self.dnd).on_toggle(NotificationsMessage::ToggleDnd),
            ]
            .spacing(Spacing::MD)
            .align_y(Alignment::Center),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn view_settings_section(&self) -> Element<NotificationsMessage> {
        container(
            column![
                text("Notification Settings")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                self.setting_row(
                    "Show Previews",
                    "Display notification content in banners",
                    self.show_previews,
                    NotificationsMessage::TogglePreviews
                ),
                self.setting_row(
                    "Show on Lock Screen",
                    "Display notifications when locked",
                    self.show_on_lock,
                    NotificationsMessage::ToggleLockScreen
                ),
                self.setting_row(
                    "Sounds",
                    "Play sounds for notifications",
                    self.sounds,
                    NotificationsMessage::ToggleSounds
                ),
                self.setting_row(
                    "Badges",
                    "Show unread count on app icons",
                    self.badges,
                    NotificationsMessage::ToggleBadges
                ),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn setting_row<F>(
        &self,
        title: &str,
        description: &str,
        enabled: bool,
        on_toggle: F,
    ) -> Element<NotificationsMessage>
    where
        F: Fn(bool) -> NotificationsMessage + 'static,
    {
        row![
            column![
                text(title)
                    .size(Typography::SIZE_BODY_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                text(description)
                    .size(Typography::SIZE_BODY_SMALL)
                    .color(NyxColors::TEXT_SECONDARY),
            ]
            .width(Length::Fill),
            toggler(enabled).on_toggle(on_toggle),
        ]
        .align_y(Alignment::Center)
        .into()
    }
}
