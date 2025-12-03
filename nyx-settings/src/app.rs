//! Main application for Nyx Settings

use crate::pages::about::{AboutMessage, AboutPage};
use crate::pages::appearance::{AppearanceMessage, AppearancePage};
use crate::pages::display::{DisplayMessage, DisplayPage};
use crate::pages::network::{NetworkMessage, NetworkPage};
use crate::pages::notifications::{NotificationsMessage, NotificationsPage};
use crate::pages::power::{PowerMessage, PowerPage};
use crate::pages::sound::{SoundMessage, SoundPage};
use crate::pages::SettingsPage;
use iced::widget::{button, column, container, horizontal_rule, row, scrollable, text};
use iced::{executor, Alignment, Application, Command, Element, Length, Theme};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::button::{button_style, ButtonVariant};
use nyx_theme::Typography;

/// Main settings application
pub struct NyxSettings {
    /// Current page
    current_page: SettingsPage,
    /// Network page state
    network: NetworkPage,
    /// Display page state
    display: DisplayPage,
    /// Sound page state
    sound: SoundPage,
    /// Appearance page state
    appearance: AppearancePage,
    /// Notifications page state
    notifications: NotificationsPage,
    /// Power page state
    power: PowerPage,
    /// About page state
    about: AboutPage,
}

/// Application message
#[derive(Debug, Clone)]
pub enum Message {
    /// Navigate to page
    NavigateTo(SettingsPage),
    /// Network page message
    Network(NetworkMessage),
    /// Display page message
    Display(DisplayMessage),
    /// Sound page message
    Sound(SoundMessage),
    /// Appearance page message
    Appearance(AppearanceMessage),
    /// Notifications page message
    Notifications(NotificationsMessage),
    /// Power page message
    Power(PowerMessage),
    /// About page message
    About(AboutMessage),
}

impl Application for NyxSettings {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                current_page: SettingsPage::default(),
                network: NetworkPage::new(),
                display: DisplayPage::default(),
                sound: SoundPage::default(),
                appearance: AppearancePage::default(),
                notifications: NotificationsPage::default(),
                power: PowerPage::default(),
                about: AboutPage::new(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        format!("Settings - {}", self.current_page.title())
    }

    fn theme(&self) -> Theme {
        nyx_theme::dark_theme()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::NavigateTo(page) => {
                self.current_page = page;
            }
            Message::Network(msg) => self.network.update(msg),
            Message::Display(msg) => self.display.update(msg),
            Message::Sound(msg) => self.sound.update(msg),
            Message::Appearance(msg) => self.appearance.update(msg),
            Message::Notifications(msg) => self.notifications.update(msg),
            Message::Power(msg) => self.power.update(msg),
            Message::About(_msg) => {}
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        let sidebar = self.view_sidebar();
        let content = self.view_content();

        row![sidebar, content]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl NyxSettings {
    fn view_sidebar(&self) -> Element<Message> {
        let header = container(
            row![
                text("󰒓")
                    .size(Typography::SIZE_ICON_LG)
                    .color(NyxColors::AURORA),
                text("Settings")
                    .size(Typography::SIZE_HEADLINE_SMALL)
                    .color(NyxColors::TEXT_BRIGHT),
            ]
            .spacing(Spacing::SM)
            .align_y(Alignment::Center),
        )
        .padding(Spacing::LG);

        let nav_items: Vec<Element<Message>> = SettingsPage::all()
            .iter()
            .map(|page| self.view_nav_item(*page))
            .collect();

        let navigation = scrollable(column(nav_items).spacing(Spacing::XXS).padding(Spacing::SM));

        container(
            column![header, horizontal_rule(1), navigation]
                .width(Length::Fixed(Spacing::SIDEBAR_EXPANDED)),
        )
        .height(Length::Fill)
        .style(|_theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(NyxColors::TWILIGHT)),
            border: iced::Border {
                color: NyxColors::BORDER_DARK,
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
    }

    fn view_nav_item(&self, page: SettingsPage) -> Element<Message> {
        let is_selected = self.current_page == page;

        button(
            row![
                text(page.icon())
                    .size(Typography::SIZE_ICON_MD)
                    .color(if is_selected {
                        NyxColors::AURORA
                    } else {
                        NyxColors::TEXT_SECONDARY
                    }),
                column![
                    text(page.title())
                        .size(Typography::SIZE_BODY_MEDIUM)
                        .color(if is_selected {
                            NyxColors::TEXT_BRIGHT
                        } else {
                            NyxColors::TEXT_SECONDARY
                        }),
                    text(page.description())
                        .size(Typography::SIZE_LABEL_SMALL)
                        .color(NyxColors::TEXT_MUTED),
                ]
                .spacing(Spacing::XXS),
            ]
            .spacing(Spacing::MD)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .padding(Spacing::SM),
        )
        .width(Length::Fill)
        .style(move |_theme, status| {
            let background = match status {
                iced::widget::button::Status::Hovered => NyxColors::NEBULA,
                _ if is_selected => NyxColors::DUSK,
                _ => iced::Color::TRANSPARENT,
            };

            iced::widget::button::Style {
                background: Some(iced::Background::Color(background)),
                text_color: NyxColors::TEXT_BRIGHT,
                border: iced::Border {
                    color: if is_selected {
                        NyxColors::AURORA
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    width: if is_selected { 2.0 } else { 0.0 },
                    radius: Spacing::RADIUS_SM.into(),
                },
                shadow: iced::Shadow::default(),
            }
        })
        .on_press(Message::NavigateTo(page))
        .into()
    }

    fn view_content(&self) -> Element<Message> {
        let page_content = match self.current_page {
            SettingsPage::Network => self.network.view().map(Message::Network),
            SettingsPage::Bluetooth => self.view_placeholder("Bluetooth"),
            SettingsPage::Display => self.display.view().map(Message::Display),
            SettingsPage::Sound => self.sound.view().map(Message::Sound),
            SettingsPage::Appearance => self.appearance.view().map(Message::Appearance),
            SettingsPage::Notifications => self.notifications.view().map(Message::Notifications),
            SettingsPage::Power => self.power.view().map(Message::Power),
            SettingsPage::Users => self.view_placeholder("Users"),
            SettingsPage::About => self.about.view().map(Message::About),
        };

        container(scrollable(page_content).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(NyxColors::MIDNIGHT)),
                ..Default::default()
            })
            .into()
    }

    fn view_placeholder(&self, name: &str) -> Element<Message> {
        container(
            column![
                text("󰏗")
                    .size(64.0)
                    .color(NyxColors::TEXT_MUTED),
                text(format!("{} Settings", name))
                    .size(Typography::SIZE_HEADLINE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                text("Coming soon...")
                    .size(Typography::SIZE_BODY_MEDIUM)
                    .color(NyxColors::TEXT_MUTED),
            ]
            .spacing(Spacing::MD)
            .align_x(Alignment::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .into()
    }
}
