//! Dock component for Nyx Shell

use crate::config::DockConfig;
use crate::messages::{DockMessage, Message};
use iced::widget::{button, column, container, row, text, Column, Row};
use iced::{Alignment, Element, Length, Padding};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::button::{button_style, ButtonVariant};
use nyx_theme::widgets::panel::{dock_item_style, dock_style, running_indicator_style};
use nyx_theme::Typography;

/// Application in the dock
#[derive(Debug, Clone)]
pub struct DockApp {
    /// Application ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Icon (text/emoji for now)
    pub icon: String,
    /// Is running
    pub running: bool,
    /// Is pinned
    pub pinned: bool,
    /// Number of windows
    pub window_count: usize,
}

impl DockApp {
    /// Create a new dock app
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        icon: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            running: false,
            pinned: true,
            window_count: 0,
        }
    }

    /// Set running state
    pub fn with_running(mut self, running: bool) -> Self {
        self.running = running;
        self
    }

    /// Set window count
    pub fn with_windows(mut self, count: usize) -> Self {
        self.window_count = count;
        if count > 0 {
            self.running = true;
        }
        self
    }
}

/// Dock state
pub struct Dock {
    /// Dock configuration
    config: DockConfig,
    /// Applications in dock
    apps: Vec<DockApp>,
    /// Currently hovered app
    hovered: Option<String>,
}

impl Default for Dock {
    fn default() -> Self {
        Self::new(DockConfig::default())
    }
}

impl Dock {
    /// Create a new dock
    pub fn new(config: DockConfig) -> Self {
        let apps = vec![
            DockApp::new("nyx-assistant", "Assistant", "󰚩"),
            DockApp::new("umbra", "Terminal", "󰆍").with_running(true).with_windows(2),
            DockApp::new("nyx-files", "Files", "󰉋"),
            DockApp::new("nyx-browser", "Browser", "󰈹").with_running(true).with_windows(1),
            DockApp::new("nyx-code", "Code", "󰨞"),
            DockApp::new("nyx-settings", "Settings", "󰒓"),
        ];

        Self {
            config,
            apps,
            hovered: None,
        }
    }

    /// Set hovered app
    pub fn set_hovered(&mut self, app_id: Option<String>) {
        self.hovered = app_id;
    }

    /// Get apps
    pub fn apps(&self) -> &[DockApp] {
        &self.apps
    }

    /// Add an app to the dock
    pub fn add_app(&mut self, app: DockApp) {
        if !self.apps.iter().any(|a| a.id == app.id) {
            self.apps.push(app);
        }
    }

    /// Remove an app from the dock
    pub fn remove_app(&mut self, id: &str) {
        self.apps.retain(|a| a.id != id);
    }

    /// Toggle pin status
    pub fn toggle_pin(&mut self, id: &str) {
        if let Some(app) = self.apps.iter_mut().find(|a| a.id == id) {
            app.pinned = !app.pinned;
        }
    }

    /// Render the dock
    pub fn view(&self) -> Element<Message> {
        let dock_items: Vec<Element<Message>> = self
            .apps
            .iter()
            .map(|app| self.view_dock_item(app))
            .collect();

        let dock_content = Row::with_children(dock_items)
            .spacing(Spacing::SM)
            .align_y(Alignment::Center)
            .padding(Padding::from([Spacing::SM, Spacing::MD]));

        container(dock_content)
            .style(dock_style())
            .into()
    }

    fn view_dock_item(&self, app: &DockApp) -> Element<Message> {
        let is_hovered = self.hovered.as_ref() == Some(&app.id);
        let icon_size = if is_hovered && self.config.magnification {
            self.config.icon_size as f32 * 1.2
        } else {
            self.config.icon_size as f32
        };

        // Icon button
        let icon_btn = button(
            container(
                text(&app.icon)
                    .size(icon_size * 0.6)
                    .color(NyxColors::TEXT_BRIGHT),
            )
            .width(Length::Fixed(icon_size))
            .height(Length::Fixed(icon_size))
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
        )
        .style(button_style(ButtonVariant::Icon))
        .padding(Spacing::XS)
        .on_press(Message::Dock(DockMessage::AppClicked(app.id.clone())));

        // Running indicator
        let indicator = if app.running && self.config.show_running {
            container(text(""))
                .width(Length::Fixed(6.0))
                .height(Length::Fixed(6.0))
                .style(running_indicator_style())
        } else {
            container(text(""))
                .width(Length::Fixed(6.0))
                .height(Length::Fixed(6.0))
        };

        // Stack icon and indicator
        column![
            container(icon_btn).style(dock_item_style(app.running, is_hovered)),
            indicator,
        ]
        .spacing(Spacing::XXS)
        .align_x(Alignment::Center)
        .into()
    }
}
