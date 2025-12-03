//! About system page

use iced::widget::{column, container, row, text, vertical_space};
use iced::{Alignment, Element, Length};
use nyx_theme::colors::NyxColors;
use nyx_theme::spacing::Spacing;
use nyx_theme::widgets::card::card_style;
use nyx_theme::widgets::CardVariant;
use nyx_theme::Typography;
use sysinfo::{System, SystemExt};

/// About page state
#[derive(Debug, Clone)]
pub struct AboutPage {
    /// Hostname
    pub hostname: String,
    /// OS name
    pub os_name: String,
    /// Kernel version
    pub kernel: String,
    /// CPU model
    pub cpu: String,
    /// Memory (formatted)
    pub memory: String,
    /// Graphics
    pub graphics: String,
    /// Disk usage
    pub disk: String,
}

impl Default for AboutPage {
    fn default() -> Self {
        Self::new()
    }
}

impl AboutPage {
    /// Create a new about page with system info
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu = sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let total_memory = sys.total_memory();
        let memory = format_bytes(total_memory);

        Self {
            hostname: System::host_name().unwrap_or_else(|| "nyx".to_string()),
            os_name: "Nyx OS".to_string(),
            kernel: System::kernel_version().unwrap_or_else(|| "unknown".to_string()),
            cpu,
            memory,
            graphics: "GPU".to_string(),
            disk: "256 GB".to_string(),
        }
    }

    /// View the page
    pub fn view(&self) -> Element<AboutMessage> {
        column![
            // Header with logo
            self.view_header(),
            vertical_space().height(Spacing::XL),
            // System info
            self.view_system_info(),
            vertical_space().height(Spacing::LG),
            // Hardware info
            self.view_hardware_info(),
        ]
        .spacing(Spacing::MD)
        .width(Length::Fill)
        .padding(Spacing::LG)
        .into()
    }

    fn view_header(&self) -> Element<AboutMessage> {
        container(
            column![
                text("󰀄")
                    .size(64.0)
                    .color(NyxColors::AURORA),
                text("Nyx OS")
                    .size(Typography::SIZE_DISPLAY_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                text("AI-Native Operating System")
                    .size(Typography::SIZE_BODY_LARGE)
                    .color(NyxColors::TEXT_SECONDARY),
                text("Version 0.1.0 (Technical Preview)")
                    .size(Typography::SIZE_BODY_SMALL)
                    .color(NyxColors::TEXT_MUTED),
            ]
            .spacing(Spacing::SM)
            .align_x(Alignment::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .padding(Spacing::XL)
        .style(card_style(CardVariant::Glass))
        .into()
    }

    fn view_system_info(&self) -> Element<AboutMessage> {
        container(
            column![
                text("System")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                self.info_row("Device Name", &self.hostname),
                self.info_row("Operating System", &self.os_name),
                self.info_row("Kernel", &self.kernel),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn view_hardware_info(&self) -> Element<AboutMessage> {
        container(
            column![
                text("Hardware")
                    .size(Typography::SIZE_TITLE_MEDIUM)
                    .color(NyxColors::TEXT_BRIGHT),
                self.info_row("Processor", &self.cpu),
                self.info_row("Memory", &self.memory),
                self.info_row("Graphics", &self.graphics),
                self.info_row("Storage", &self.disk),
            ]
            .spacing(Spacing::MD),
        )
        .padding(Spacing::LG)
        .style(card_style(CardVariant::Default))
        .into()
    }

    fn info_row(&self, label: &str, value: &str) -> Element<AboutMessage> {
        row![
            text(label)
                .size(Typography::SIZE_BODY_MEDIUM)
                .color(NyxColors::TEXT_SECONDARY)
                .width(Length::Fixed(150.0)),
            text(value)
                .size(Typography::SIZE_BODY_MEDIUM)
                .color(NyxColors::TEXT_BRIGHT),
        ]
        .spacing(Spacing::MD)
        .into()
    }
}

/// About page messages
#[derive(Debug, Clone)]
pub enum AboutMessage {
    /// Refresh system info
    Refresh,
    /// Copy system info to clipboard
    CopyInfo,
}

/// Format bytes to human-readable
fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    format!("{:.1} GB", bytes as f64 / GB as f64)
}
