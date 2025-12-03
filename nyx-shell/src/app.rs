//! Main application state for Nyx Shell

use crate::config::ShellConfig;
use crate::dock::Dock;
use crate::messages::{DockMessage, Message, PanelMessage, WorkspaceMessage};
use crate::panel::Panel;
use crate::system::SystemStatus;
use crate::workspace::WorkspaceManager;
use iced::widget::{column, container, horizontal_space, row, vertical_space};
use iced::{executor, Application, Command, Element, Length, Subscription, Theme};
use nyx_theme::colors::NyxColors;
use std::time::Duration;

/// Main shell application
pub struct NyxShell {
    /// Shell configuration
    config: ShellConfig,
    /// Top panel
    panel: Panel,
    /// Dock
    dock: Dock,
    /// Workspace manager
    workspaces: WorkspaceManager,
    /// System status
    system: SystemStatus,
    /// Control center visible
    control_center_visible: bool,
    /// Assistant visible
    assistant_visible: bool,
    /// Activities overview visible
    activities_visible: bool,
}

impl Application for NyxShell {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let config = ShellConfig::load();

        let shell = Self {
            panel: Panel::new(config.panel.clone()),
            dock: Dock::new(config.dock.clone()),
            config,
            workspaces: WorkspaceManager::new(),
            system: SystemStatus::new(),
            control_center_visible: false,
            assistant_visible: false,
            activities_visible: false,
        };

        (shell, Command::none())
    }

    fn title(&self) -> String {
        String::from("Nyx Shell")
    }

    fn theme(&self) -> Theme {
        nyx_theme::dark_theme()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Tick => {
                self.system.refresh();
            }

            Message::Panel(panel_msg) => {
                self.handle_panel_message(panel_msg);
            }

            Message::Dock(dock_msg) => {
                self.handle_dock_message(dock_msg);
            }

            Message::Workspace(ws_msg) => {
                self.handle_workspace_message(ws_msg);
            }

            Message::System(_sys_msg) => {
                // Handle system events
            }

            Message::ToggleControlCenter => {
                self.control_center_visible = !self.control_center_visible;
                self.assistant_visible = false;
                self.activities_visible = false;
            }

            Message::ToggleAssistant => {
                self.assistant_visible = !self.assistant_visible;
                self.control_center_visible = false;
                self.activities_visible = false;
            }

            Message::ShowActivities => {
                self.activities_visible = true;
                self.control_center_visible = false;
                self.assistant_visible = false;
            }

            Message::HideActivities => {
                self.activities_visible = false;
            }

            Message::FontLoaded(_) => {}
        }

        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        // Tick every second for clock updates
        iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
    }

    fn view(&self) -> Element<Message> {
        // Main layout: Panel at top, content in middle, dock at bottom
        let panel = self.panel.view(
            &self.workspaces,
            &self.system.battery,
            &self.system.network,
            &self.system.audio,
        );

        let dock = self.dock.view();

        // Desktop area (between panel and dock)
        let desktop = container(
            column![
                vertical_space(),
                // Show overlays if active
                if self.activities_visible {
                    self.view_activities_overlay()
                } else if self.control_center_visible {
                    self.view_control_center_overlay()
                } else if self.assistant_visible {
                    self.view_assistant_overlay()
                } else {
                    container(horizontal_space()).into()
                },
                vertical_space(),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill);

        // Dock container at bottom
        let dock_container = container(
            row![horizontal_space(), dock, horizontal_space()]
                .width(Length::Fill)
                .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .padding(iced::Padding::from([0.0, 0.0, 16.0, 0.0]));

        // Full shell layout
        let shell = column![panel, desktop, dock_container]
            .width(Length::Fill)
            .height(Length::Fill);

        container(shell)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(NyxColors::VOID)),
                ..Default::default()
            })
            .into()
    }
}

impl NyxShell {
    fn handle_panel_message(&mut self, msg: PanelMessage) {
        match msg {
            PanelMessage::ActivitiesClicked => {
                self.activities_visible = !self.activities_visible;
                self.control_center_visible = false;
                self.assistant_visible = false;
            }

            PanelMessage::WorkspaceClicked(id) => {
                self.workspaces.set_active(id);
            }

            PanelMessage::TrayIconClicked(_icon) => {
                // Handle tray icon click
            }

            PanelMessage::ClockClicked => {
                // Show calendar
            }

            PanelMessage::UserMenuClicked => {
                // Show user menu
            }

            PanelMessage::QuickSettingsClicked => {
                self.control_center_visible = !self.control_center_visible;
            }
        }
    }

    fn handle_dock_message(&mut self, msg: DockMessage) {
        match msg {
            DockMessage::AppClicked(id) => {
                tracing::info!("App clicked: {}", id);
                // Launch or focus app
            }

            DockMessage::AppRightClicked(id) => {
                tracing::info!("App right-clicked: {}", id);
                // Show context menu
            }

            DockMessage::AppHovered(id) => {
                self.dock.set_hovered(id);
            }

            DockMessage::TogglePin(id) => {
                self.dock.toggle_pin(&id);
            }

            DockMessage::LaunchApp(id) => {
                tracing::info!("Launching app: {}", id);
            }

            DockMessage::FocusApp(id) => {
                tracing::info!("Focusing app: {}", id);
            }

            DockMessage::CloseApp(id) => {
                tracing::info!("Closing app: {}", id);
            }
        }
    }

    fn handle_workspace_message(&mut self, msg: WorkspaceMessage) {
        match msg {
            WorkspaceMessage::Switch(id) => {
                self.workspaces.set_active(id);
            }

            WorkspaceMessage::Create => {
                let count = self.workspaces.count();
                self.workspaces.create(format!("Workspace {}", count + 1));
            }

            WorkspaceMessage::Remove(id) => {
                self.workspaces.remove(id);
            }

            WorkspaceMessage::Rename(id, name) => {
                self.workspaces.rename(id, name);
            }

            WorkspaceMessage::MoveWindow(_id) => {
                // Move window to workspace
            }

            WorkspaceMessage::Reorder(_id, _pos) => {
                // Reorder workspaces
            }
        }
    }

    fn view_activities_overlay(&self) -> Element<Message> {
        use iced::widget::text;
        use nyx_theme::spacing::Spacing;
        use nyx_theme::widgets::card::card_style;
        use nyx_theme::widgets::CardVariant;

        container(
            container(
                column![
                    text("Activities")
                        .size(nyx_theme::Typography::SIZE_HEADLINE_LARGE)
                        .color(NyxColors::TEXT_BRIGHT),
                    text("Window overview and workspace management")
                        .size(nyx_theme::Typography::SIZE_BODY_MEDIUM)
                        .color(NyxColors::TEXT_SECONDARY),
                ]
                .spacing(Spacing::SM),
            )
            .padding(Spacing::XL)
            .style(card_style(CardVariant::Glass)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .into()
    }

    fn view_control_center_overlay(&self) -> Element<Message> {
        use iced::widget::text;
        use nyx_theme::spacing::Spacing;
        use nyx_theme::widgets::panel::quick_settings_style;

        container(
            container(
                column![
                    text("Quick Settings")
                        .size(nyx_theme::Typography::SIZE_HEADLINE_MEDIUM)
                        .color(NyxColors::TEXT_BRIGHT),
                    row![
                        self.view_quick_toggle("󰤨", "WiFi", true),
                        self.view_quick_toggle("󰂯", "Bluetooth", false),
                        self.view_quick_toggle("󰖩", "Airplane", false),
                    ]
                    .spacing(Spacing::SM),
                    row![
                        self.view_quick_toggle("󰃞", "Night Light", false),
                        self.view_quick_toggle("󰍹", "Do Not Disturb", false),
                        self.view_quick_toggle("󰌾", "Lock", false),
                    ]
                    .spacing(Spacing::SM),
                    // Volume slider placeholder
                    text("Volume")
                        .size(nyx_theme::Typography::SIZE_LABEL_MEDIUM)
                        .color(NyxColors::TEXT_SECONDARY),
                    // Brightness slider placeholder
                    text("Brightness")
                        .size(nyx_theme::Typography::SIZE_LABEL_MEDIUM)
                        .color(NyxColors::TEXT_SECONDARY),
                ]
                .spacing(Spacing::MD)
                .width(Length::Fixed(Spacing::CONTROL_CENTER_WIDTH)),
            )
            .padding(Spacing::LG)
            .style(quick_settings_style()),
        )
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .padding(iced::Padding::from([Spacing::XL, Spacing::MD, 0.0, 0.0]))
        .into()
    }

    fn view_quick_toggle(&self, icon: &str, label: &str, active: bool) -> Element<Message> {
        use iced::widget::{button, column, text};
        use nyx_theme::spacing::Spacing;
        use nyx_theme::widgets::panel::quick_toggle_style;

        let toggle = button(
            column![
                text(icon)
                    .size(nyx_theme::Typography::SIZE_ICON_LG)
                    .color(if active {
                        iced::Color::WHITE
                    } else {
                        NyxColors::TEXT_BRIGHT
                    }),
                text(label)
                    .size(nyx_theme::Typography::SIZE_LABEL_SMALL)
                    .color(if active {
                        iced::Color::WHITE
                    } else {
                        NyxColors::TEXT_SECONDARY
                    }),
            ]
            .spacing(Spacing::XS)
            .align_x(iced::Alignment::Center)
            .width(Length::Fixed(80.0))
            .padding(Spacing::SM),
        )
        .style(move |theme, status| {
            let base = quick_toggle_style(active)(theme);
            match status {
                iced::widget::button::Status::Hovered => iced::widget::button::Style {
                    background: Some(iced::Background::Color(if active {
                        NyxColors::AURORA_LIGHT
                    } else {
                        NyxColors::NEBULA
                    })),
                    text_color: base.text_color.unwrap_or(NyxColors::TEXT_BRIGHT),
                    border: base.border,
                    shadow: base.shadow,
                },
                _ => iced::widget::button::Style {
                    background: base.background,
                    text_color: base.text_color.unwrap_or(NyxColors::TEXT_BRIGHT),
                    border: base.border,
                    shadow: base.shadow,
                },
            }
        });

        toggle.into()
    }

    fn view_assistant_overlay(&self) -> Element<Message> {
        use iced::widget::{text, text_input};
        use nyx_theme::spacing::Spacing;
        use nyx_theme::widgets::card::card_style;
        use nyx_theme::widgets::input::input_style;
        use nyx_theme::widgets::{CardVariant, InputVariant};

        container(
            container(
                column![
                    row![
                        text("󰚩")
                            .size(nyx_theme::Typography::SIZE_ICON_LG)
                            .color(NyxColors::AURORA),
                        text("Nyx Assistant")
                            .size(nyx_theme::Typography::SIZE_HEADLINE_MEDIUM)
                            .color(NyxColors::TEXT_BRIGHT),
                    ]
                    .spacing(Spacing::SM)
                    .align_y(iced::Alignment::Center),
                    text_input("Ask me anything...", "")
                        .size(nyx_theme::Typography::SIZE_BODY_LARGE)
                        .padding(Spacing::MD)
                        .width(Length::Fixed(Spacing::ASSISTANT_WIDTH - Spacing::XL * 2.0))
                        .style(input_style(InputVariant::Search)),
                    text("Try: \"Open terminal\" or \"What's the weather?\"")
                        .size(nyx_theme::Typography::SIZE_BODY_SMALL)
                        .color(NyxColors::TEXT_MUTED),
                ]
                .spacing(Spacing::MD)
                .align_x(iced::Alignment::Center)
                .width(Length::Fixed(Spacing::ASSISTANT_WIDTH)),
            )
            .padding(Spacing::XL)
            .style(card_style(CardVariant::Glass)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Start)
        .padding(iced::Padding::from([100.0, 0.0, 0.0, 0.0]))
        .into()
    }
}
