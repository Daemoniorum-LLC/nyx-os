//! Main application for Nyx Control

use crate::controls::{
    power_button, quick_toggle, section_header, settings_row, slider_control, ControlMessage,
    PowerAction,
};
use iced::widget::{column, container, horizontal_rule, row, scrollable, text, vertical_space};
use iced::{executor, Alignment, Application, Command, Element, Length, Subscription, Theme};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::panel::quick_settings_style;
use nyx_theme::Typography;
use std::time::Duration;

/// Control center state
#[derive(Debug, Clone)]
pub struct ControlState {
    /// WiFi enabled
    pub wifi: bool,
    /// Bluetooth enabled
    pub bluetooth: bool,
    /// Airplane mode
    pub airplane: bool,
    /// Night light enabled
    pub night_light: bool,
    /// Do Not Disturb enabled
    pub dnd: bool,
    /// Volume level (0-100)
    pub volume: u8,
    /// Volume muted
    pub muted: bool,
    /// Brightness level (0-100)
    pub brightness: u8,
    /// WiFi network name
    pub wifi_network: Option<String>,
    /// Connected Bluetooth device
    pub bt_device: Option<String>,
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            wifi: true,
            bluetooth: false,
            airplane: false,
            night_light: false,
            dnd: false,
            volume: 65,
            muted: false,
            brightness: 80,
            wifi_network: Some("Nyx-Network".to_string()),
            bt_device: None,
        }
    }
}

/// Main control center application
pub struct NyxControl {
    /// Control state
    state: ControlState,
}

/// Application message
#[derive(Debug, Clone)]
pub enum Message {
    /// Control message
    Control(ControlMessage),
    /// Tick for updates
    Tick,
    /// Close the control center
    Close,
}

impl Application for NyxControl {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                state: ControlState::default(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Nyx Control")
    }

    fn theme(&self) -> Theme {
        nyx_theme::dark_theme()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Control(ctrl_msg) => self.handle_control(ctrl_msg),
            Message::Tick => {}
            Message::Close => {
                return iced::window::close(iced::window::Id::MAIN);
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(Duration::from_secs(5)).map(|_| Message::Tick)
    }

    fn view(&self) -> Element<Message> {
        let content = scrollable(
            column![
                // Header
                self.view_header(),
                vertical_space().height(Spacing::MD),
                // Quick toggles grid
                self.view_quick_toggles(),
                vertical_space().height(Spacing::MD),
                // Sliders
                self.view_sliders(),
                vertical_space().height(Spacing::MD),
                // Network section
                section_header("Network").map(Message::Control),
                self.view_network_section(),
                vertical_space().height(Spacing::MD),
                // Power section
                section_header("Power").map(Message::Control),
                self.view_power_section(),
            ]
            .spacing(Spacing::SM)
            .padding(Spacing::MD),
        );

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(quick_settings_style())
            .into()
    }
}

impl NyxControl {
    fn handle_control(&mut self, msg: ControlMessage) {
        match msg {
            ControlMessage::ToggleWifi => {
                self.state.wifi = !self.state.wifi;
                if !self.state.wifi {
                    self.state.wifi_network = None;
                }
            }
            ControlMessage::ToggleBluetooth => {
                self.state.bluetooth = !self.state.bluetooth;
                if !self.state.bluetooth {
                    self.state.bt_device = None;
                }
            }
            ControlMessage::ToggleAirplane => {
                self.state.airplane = !self.state.airplane;
                if self.state.airplane {
                    self.state.wifi = false;
                    self.state.bluetooth = false;
                }
            }
            ControlMessage::ToggleNightLight => {
                self.state.night_light = !self.state.night_light;
            }
            ControlMessage::ToggleDnd => {
                self.state.dnd = !self.state.dnd;
            }
            ControlMessage::VolumeChanged(v) => {
                self.state.volume = v;
                if v > 0 {
                    self.state.muted = false;
                }
            }
            ControlMessage::BrightnessChanged(v) => {
                self.state.brightness = v;
            }
            ControlMessage::ToggleMute => {
                self.state.muted = !self.state.muted;
            }
            ControlMessage::PowerAction(action) => {
                tracing::info!("Power action: {:?}", action);
                // In a real implementation, this would trigger system actions
            }
            ControlMessage::OpenSettings => {
                tracing::info!("Opening settings");
            }
            ControlMessage::OpenWifiSettings => {
                tracing::info!("Opening WiFi settings");
            }
            ControlMessage::OpenBluetoothSettings => {
                tracing::info!("Opening Bluetooth settings");
            }
            ControlMessage::OpenDisplaySettings => {
                tracing::info!("Opening display settings");
            }
            ControlMessage::OpenSoundSettings => {
                tracing::info!("Opening sound settings");
            }
        }
    }

    fn view_header(&self) -> Element<Message> {
        row![
            text("Quick Settings")
                .size(Typography::SIZE_HEADLINE_MEDIUM)
                .color(NyxColors::TEXT_BRIGHT),
            iced::widget::horizontal_space(),
            iced::widget::button(
                text("󰒓")
                    .size(Typography::SIZE_ICON_MD)
                    .color(NyxColors::TEXT_SECONDARY)
            )
            .style(nyx_theme::widgets::button::button_style(
                nyx_theme::widgets::ButtonVariant::Ghost
            ))
            .on_press(Message::Control(ControlMessage::OpenSettings)),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    fn view_quick_toggles(&self) -> Element<Message> {
        column![
            row![
                quick_toggle(
                    if self.state.wifi { "󰤨" } else { "󰤭" },
                    "WiFi",
                    self.state.wifi,
                    Message::Control(ControlMessage::ToggleWifi)
                ),
                quick_toggle(
                    if self.state.bluetooth { "󰂯" } else { "󰂲" },
                    "Bluetooth",
                    self.state.bluetooth,
                    Message::Control(ControlMessage::ToggleBluetooth)
                ),
                quick_toggle(
                    "󰀝",
                    "Airplane",
                    self.state.airplane,
                    Message::Control(ControlMessage::ToggleAirplane)
                ),
            ]
            .spacing(Spacing::SM),
            row![
                quick_toggle(
                    "󰖔",
                    "Night Light",
                    self.state.night_light,
                    Message::Control(ControlMessage::ToggleNightLight)
                ),
                quick_toggle(
                    if self.state.dnd { "󰂛" } else { "󰂚" },
                    "Do Not Disturb",
                    self.state.dnd,
                    Message::Control(ControlMessage::ToggleDnd)
                ),
                quick_toggle(
                    "󰌾",
                    "Lock",
                    false,
                    Message::Control(ControlMessage::PowerAction(PowerAction::Lock))
                ),
            ]
            .spacing(Spacing::SM),
        ]
        .spacing(Spacing::SM)
        .into()
    }

    fn view_sliders(&self) -> Element<Message> {
        let volume_icon = if self.state.muted || self.state.volume == 0 {
            "󰖁"
        } else if self.state.volume < 33 {
            "󰕿"
        } else if self.state.volume < 66 {
            "󰖀"
        } else {
            "󰕾"
        };

        let brightness_icon = if self.state.brightness < 33 {
            "󰃞"
        } else if self.state.brightness < 66 {
            "󰃟"
        } else {
            "󰃠"
        };

        column![
            slider_control(volume_icon, "Volume", self.state.volume, |v| {
                Message::Control(ControlMessage::VolumeChanged(v))
            }),
            slider_control(
                brightness_icon,
                "Brightness",
                self.state.brightness,
                |v| { Message::Control(ControlMessage::BrightnessChanged(v)) }
            ),
        ]
        .spacing(Spacing::SM)
        .into()
    }

    fn view_network_section(&self) -> Element<Message> {
        column![
            settings_row(
                "󰤨",
                "WiFi",
                self.state.wifi_network.as_deref(),
                Message::Control(ControlMessage::OpenWifiSettings)
            ),
            settings_row(
                "󰂯",
                "Bluetooth",
                self.state.bt_device.as_deref(),
                Message::Control(ControlMessage::OpenBluetoothSettings)
            ),
        ]
        .spacing(Spacing::XS)
        .into()
    }

    fn view_power_section(&self) -> Element<Message> {
        row![
            power_button(
                "󰌾",
                "Lock",
                Message::Control(ControlMessage::PowerAction(PowerAction::Lock))
            ),
            power_button(
                "󰤄",
                "Sleep",
                Message::Control(ControlMessage::PowerAction(PowerAction::Suspend))
            ),
            power_button(
                "󰜉",
                "Restart",
                Message::Control(ControlMessage::PowerAction(PowerAction::Restart))
            ),
            power_button(
                "󰐥",
                "Shutdown",
                Message::Control(ControlMessage::PowerAction(PowerAction::Shutdown))
            ),
        ]
        .spacing(Spacing::SM)
        .into()
    }
}
