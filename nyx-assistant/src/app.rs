//! Main application for Nyx Assistant

use crate::commands::{CommandKind, CommandResult};
use crate::search::SearchEngine;
use iced::keyboard;
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_input,
    vertical_space,
};
use iced::{
    executor, Alignment, Application, Command, Element, Event, Length, Subscription, Theme,
};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::button::{button_style, ButtonVariant};
use nyx_theme::widgets::card::card_style;
use nyx_theme::widgets::input::input_style;
use nyx_theme::widgets::{CardVariant, InputVariant};
use nyx_theme::Typography;

/// Main assistant application
pub struct NyxAssistant {
    /// Search query
    query: String,
    /// Search engine
    search: SearchEngine,
    /// Search results
    results: Vec<CommandResult>,
    /// Selected result index
    selected: usize,
    /// Is loading AI response
    loading: bool,
}

/// Application message
#[derive(Debug, Clone)]
pub enum Message {
    /// Query text changed
    QueryChanged(String),
    /// Search submitted (Enter pressed)
    Submit,
    /// Result clicked
    ResultClicked(usize),
    /// Keyboard navigation
    KeyPressed(keyboard::Key),
    /// Close assistant
    Close,
    /// Execute command
    Execute(CommandResult),
    /// AI response received
    AiResponse(String),
    /// Focus the input
    FocusInput,
}

impl Application for NyxAssistant {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let search = SearchEngine::new();
        let results = search.get_suggestions();

        (
            Self {
                query: String::new(),
                search,
                results,
                selected: 0,
                loading: false,
            },
            iced::widget::text_input::focus(text_input::Id::new("search-input")),
        )
    }

    fn title(&self) -> String {
        String::from("Nyx Assistant")
    }

    fn theme(&self) -> Theme {
        nyx_theme::dark_theme()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::QueryChanged(query) => {
                self.query = query.clone();
                self.results = self.search.search(&query);
                self.selected = 0;
            }

            Message::Submit => {
                if let Some(result) = self.results.get(self.selected).cloned() {
                    return self.execute_command(result);
                }
            }

            Message::ResultClicked(index) => {
                if let Some(result) = self.results.get(index).cloned() {
                    return self.execute_command(result);
                }
            }

            Message::KeyPressed(key) => match key {
                keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                    if self.selected < self.results.len().saturating_sub(1) {
                        self.selected += 1;
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    return iced::window::close(iced::window::Id::MAIN);
                }
                keyboard::Key::Named(keyboard::key::Named::Tab) => {
                    // Cycle through results
                    if self.results.is_empty() {
                        return Command::none();
                    }
                    self.selected = (self.selected + 1) % self.results.len();
                }
                _ => {}
            },

            Message::Close => {
                return iced::window::close(iced::window::Id::MAIN);
            }

            Message::Execute(result) => {
                return self.execute_command(result);
            }

            Message::AiResponse(_response) => {
                self.loading = false;
                // Display AI response
            }

            Message::FocusInput => {
                return iced::widget::text_input::focus(text_input::Id::new("search-input"));
            }
        }

        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::event::listen_with(|event, _status, _id| {
            if let Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers: _,
                ..
            }) = event
            {
                match key {
                    keyboard::Key::Named(
                        keyboard::key::Named::ArrowDown
                        | keyboard::key::Named::ArrowUp
                        | keyboard::key::Named::Escape
                        | keyboard::key::Named::Tab,
                    ) => Some(Message::KeyPressed(key)),
                    _ => None,
                }
            } else {
                None
            }
        })
    }

    fn view(&self) -> Element<Message> {
        // Header with search input
        let header = self.view_header();

        // Results list
        let results = self.view_results();

        // Footer with hints
        let footer = self.view_footer();

        let content = column![header, results, footer]
            .spacing(Spacing::SM)
            .padding(Spacing::LG);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(card_style(CardVariant::Glass))
            .into()
    }
}

impl NyxAssistant {
    fn execute_command(&self, result: CommandResult) -> Command<Message> {
        tracing::info!("Executing: {:?}", result);

        match result.kind {
            CommandKind::Application => {
                // Launch application
                tracing::info!("Launching app: {}", result.id);
            }
            CommandKind::System => {
                // Execute system command
                tracing::info!("System command: {}", result.id);
            }
            CommandKind::WebSearch => {
                // Open web search
                tracing::info!("Web search: {}", result.title);
            }
            CommandKind::AiQuery => {
                // Send to AI
                tracing::info!("AI query: {}", result.title);
            }
            CommandKind::Calculator => {
                // Copy to clipboard
                tracing::info!("Calculator result: {}", result.title);
            }
            CommandKind::Settings => {
                // Open settings page
                tracing::info!("Opening settings: {}", result.id);
            }
            _ => {}
        }

        // Close assistant after execution
        iced::window::close(iced::window::Id::MAIN)
    }

    fn view_header(&self) -> Element<Message> {
        let icon = text("󰚩")
            .size(Typography::SIZE_ICON_XL)
            .color(NyxColors::AURORA);

        let input = text_input("Search apps, files, or ask anything...", &self.query)
            .id(text_input::Id::new("search-input"))
            .size(Typography::SIZE_BODY_LARGE)
            .padding(Spacing::MD)
            .width(Length::Fill)
            .style(input_style(InputVariant::Ghost))
            .on_input(Message::QueryChanged)
            .on_submit(Message::Submit);

        let close_btn = button(
            text("󰅖")
                .size(Typography::SIZE_ICON_MD)
                .color(NyxColors::TEXT_MUTED),
        )
        .style(button_style(ButtonVariant::Ghost))
        .on_press(Message::Close);

        container(
            row![icon, input, close_btn]
                .spacing(Spacing::MD)
                .align_y(Alignment::Center),
        )
        .padding(Spacing::SM)
        .style(|_theme| iced::widget::container::Style {
            border: iced::Border {
                color: NyxColors::BORDER_DARK,
                width: 0.0,
                radius: Spacing::RADIUS_MD.into(),
            },
            ..Default::default()
        })
        .into()
    }

    fn view_results(&self) -> Element<Message> {
        if self.results.is_empty() {
            return container(
                column![
                    vertical_space(),
                    text("No results found")
                        .size(Typography::SIZE_BODY_MEDIUM)
                        .color(NyxColors::TEXT_MUTED),
                    text("Try a different search term")
                        .size(Typography::SIZE_BODY_SMALL)
                        .color(NyxColors::TEXT_MUTED),
                    vertical_space(),
                ]
                .align_x(Alignment::Center)
                .width(Length::Fill),
            )
            .height(Length::Fill)
            .into();
        }

        let result_items: Vec<Element<Message>> = self
            .results
            .iter()
            .enumerate()
            .map(|(i, result)| self.view_result_item(i, result))
            .collect();

        scrollable(
            column(result_items)
                .spacing(Spacing::XS)
                .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    fn view_result_item(&self, index: usize, result: &CommandResult) -> Element<Message> {
        let is_selected = index == self.selected;

        let icon = text(&result.icon)
            .size(Typography::SIZE_ICON_LG)
            .color(if is_selected {
                NyxColors::AURORA
            } else {
                NyxColors::TEXT_SECONDARY
            });

        let title = text(&result.title)
            .size(Typography::SIZE_BODY_MEDIUM)
            .color(if is_selected {
                NyxColors::TEXT_BRIGHT
            } else {
                NyxColors::TEXT_BRIGHT
            });

        let subtitle = if let Some(ref sub) = result.subtitle {
            text(sub)
                .size(Typography::SIZE_BODY_SMALL)
                .color(NyxColors::TEXT_MUTED)
        } else {
            text("")
        };

        let kind_badge = self.view_kind_badge(result.kind);

        let content = row![
            icon,
            column![title, subtitle].spacing(Spacing::XXS).width(Length::Fill),
            kind_badge,
        ]
        .spacing(Spacing::MD)
        .align_y(Alignment::Center)
        .padding(Spacing::SM);

        let item = button(content)
            .width(Length::Fill)
            .style(move |_theme, status| {
                let background = match status {
                    iced::widget::button::Status::Hovered => NyxColors::NEBULA,
                    _ if is_selected => NyxColors::TWILIGHT,
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
                        width: if is_selected { 1.0 } else { 0.0 },
                        radius: Spacing::RADIUS_SM.into(),
                    },
                    shadow: iced::Shadow::default(),
                }
            })
            .on_press(Message::ResultClicked(index));

        item.into()
    }

    fn view_kind_badge(&self, kind: CommandKind) -> Element<Message> {
        let (label, color) = match kind {
            CommandKind::Application => ("App", NyxColors::AURORA),
            CommandKind::System => ("System", NyxColors::INFO),
            CommandKind::Calculator => ("=", NyxColors::SUCCESS),
            CommandKind::WebSearch => ("Web", NyxColors::ETHEREAL),
            CommandKind::AiQuery => ("AI", NyxColors::CELESTIAL),
            CommandKind::Settings => ("Settings", NyxColors::WARNING),
            CommandKind::File => ("File", NyxColors::TEXT_SECONDARY),
            CommandKind::Folder => ("Folder", NyxColors::TEXT_SECONDARY),
            CommandKind::Recent => ("Recent", NyxColors::TEXT_MUTED),
        };

        container(
            text(label)
                .size(Typography::SIZE_LABEL_SMALL)
                .color(color),
        )
        .padding(Spacing::XS)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgba(
                color.r, color.g, color.b, 0.15,
            ))),
            border: iced::Border {
                radius: Spacing::RADIUS_SM.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    }

    fn view_footer(&self) -> Element<Message> {
        let hints = row![
            text("↑↓")
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::TEXT_MUTED),
            text("Navigate")
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::TEXT_MUTED),
            text("·").color(NyxColors::TEXT_MUTED),
            text("⏎")
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::TEXT_MUTED),
            text("Select")
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::TEXT_MUTED),
            text("·").color(NyxColors::TEXT_MUTED),
            text("Esc")
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::TEXT_MUTED),
            text("Close")
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::TEXT_MUTED),
            horizontal_space(),
            text("Powered by Nyx AI")
                .size(Typography::SIZE_LABEL_SMALL)
                .color(NyxColors::AURORA),
        ]
        .spacing(Spacing::XS)
        .align_y(Alignment::Center);

        container(hints)
            .padding(Spacing::SM)
            .style(|_theme| iced::widget::container::Style {
                border: iced::Border {
                    color: NyxColors::BORDER_DARK,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
