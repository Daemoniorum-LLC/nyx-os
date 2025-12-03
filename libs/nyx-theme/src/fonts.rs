//! Typography definitions for Nyx OS theme
//!
//! Defines font families, sizes, weights, and line heights for a consistent
//! typographic hierarchy across all Nyx OS applications.

use iced::font::{Family, Weight};
use iced::Font;

/// Typography configuration for Nyx OS
#[derive(Debug, Clone, Copy)]
pub struct Typography;

impl Typography {
    // ═══════════════════════════════════════════════════════════════════════════
    // FONT FAMILIES
    // ═══════════════════════════════════════════════════════════════════════════

    /// Primary font family (Inter or system sans-serif)
    pub const FAMILY_PRIMARY: Family = Family::SansSerif;

    /// Monospace font family (JetBrains Mono or system monospace)
    pub const FAMILY_MONO: Family = Family::Monospace;

    // ═══════════════════════════════════════════════════════════════════════════
    // FONT SIZES (in pixels)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Display large - hero text, splash screens
    pub const SIZE_DISPLAY_LARGE: f32 = 48.0;
    /// Display medium - section headers
    pub const SIZE_DISPLAY_MEDIUM: f32 = 36.0;
    /// Display small - card titles
    pub const SIZE_DISPLAY_SMALL: f32 = 28.0;

    /// Headline large
    pub const SIZE_HEADLINE_LARGE: f32 = 24.0;
    /// Headline medium
    pub const SIZE_HEADLINE_MEDIUM: f32 = 20.0;
    /// Headline small
    pub const SIZE_HEADLINE_SMALL: f32 = 18.0;

    /// Title large
    pub const SIZE_TITLE_LARGE: f32 = 16.0;
    /// Title medium
    pub const SIZE_TITLE_MEDIUM: f32 = 15.0;
    /// Title small
    pub const SIZE_TITLE_SMALL: f32 = 14.0;

    /// Body large - primary content
    pub const SIZE_BODY_LARGE: f32 = 14.0;
    /// Body medium - default body text
    pub const SIZE_BODY_MEDIUM: f32 = 13.0;
    /// Body small - secondary content
    pub const SIZE_BODY_SMALL: f32 = 12.0;

    /// Label large - prominent labels
    pub const SIZE_LABEL_LARGE: f32 = 13.0;
    /// Label medium - default labels
    pub const SIZE_LABEL_MEDIUM: f32 = 12.0;
    /// Label small - subtle labels
    pub const SIZE_LABEL_SMALL: f32 = 11.0;

    /// Caption - small annotations
    pub const SIZE_CAPTION: f32 = 10.0;

    // ═══════════════════════════════════════════════════════════════════════════
    // PREDEFINED FONTS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Display font - bold for hero text
    pub const DISPLAY: Font = Font {
        family: Self::FAMILY_PRIMARY,
        weight: Weight::Bold,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    };

    /// Headline font - semibold for section headers
    pub const HEADLINE: Font = Font {
        family: Self::FAMILY_PRIMARY,
        weight: Weight::Semibold,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    };

    /// Title font - medium weight for titles
    pub const TITLE: Font = Font {
        family: Self::FAMILY_PRIMARY,
        weight: Weight::Medium,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    };

    /// Body font - regular weight for content
    pub const BODY: Font = Font {
        family: Self::FAMILY_PRIMARY,
        weight: Weight::Normal,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    };

    /// Label font - medium weight for labels
    pub const LABEL: Font = Font {
        family: Self::FAMILY_PRIMARY,
        weight: Weight::Medium,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    };

    /// Code font - monospace for code blocks
    pub const CODE: Font = Font {
        family: Self::FAMILY_MONO,
        weight: Weight::Normal,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    };

    /// Code bold font
    pub const CODE_BOLD: Font = Font {
        family: Self::FAMILY_MONO,
        weight: Weight::Bold,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    };

    // ═══════════════════════════════════════════════════════════════════════════
    // LINE HEIGHTS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Tight line height for headings
    pub const LINE_HEIGHT_TIGHT: f32 = 1.2;
    /// Normal line height for body text
    pub const LINE_HEIGHT_NORMAL: f32 = 1.5;
    /// Relaxed line height for reading
    pub const LINE_HEIGHT_RELAXED: f32 = 1.75;

    // ═══════════════════════════════════════════════════════════════════════════
    // LETTER SPACING
    // ═══════════════════════════════════════════════════════════════════════════

    /// Tight letter spacing for large text
    pub const LETTER_SPACING_TIGHT: f32 = -0.5;
    /// Normal letter spacing
    pub const LETTER_SPACING_NORMAL: f32 = 0.0;
    /// Wide letter spacing for labels/buttons
    pub const LETTER_SPACING_WIDE: f32 = 0.5;
}

/// Text style presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    DisplayLarge,
    DisplayMedium,
    DisplaySmall,
    HeadlineLarge,
    HeadlineMedium,
    HeadlineSmall,
    TitleLarge,
    TitleMedium,
    TitleSmall,
    BodyLarge,
    BodyMedium,
    BodySmall,
    LabelLarge,
    LabelMedium,
    LabelSmall,
    Caption,
    Code,
}

impl TextStyle {
    /// Get the font size for this style
    pub fn size(self) -> f32 {
        match self {
            TextStyle::DisplayLarge => Typography::SIZE_DISPLAY_LARGE,
            TextStyle::DisplayMedium => Typography::SIZE_DISPLAY_MEDIUM,
            TextStyle::DisplaySmall => Typography::SIZE_DISPLAY_SMALL,
            TextStyle::HeadlineLarge => Typography::SIZE_HEADLINE_LARGE,
            TextStyle::HeadlineMedium => Typography::SIZE_HEADLINE_MEDIUM,
            TextStyle::HeadlineSmall => Typography::SIZE_HEADLINE_SMALL,
            TextStyle::TitleLarge => Typography::SIZE_TITLE_LARGE,
            TextStyle::TitleMedium => Typography::SIZE_TITLE_MEDIUM,
            TextStyle::TitleSmall => Typography::SIZE_TITLE_SMALL,
            TextStyle::BodyLarge => Typography::SIZE_BODY_LARGE,
            TextStyle::BodyMedium => Typography::SIZE_BODY_MEDIUM,
            TextStyle::BodySmall => Typography::SIZE_BODY_SMALL,
            TextStyle::LabelLarge => Typography::SIZE_LABEL_LARGE,
            TextStyle::LabelMedium => Typography::SIZE_LABEL_MEDIUM,
            TextStyle::LabelSmall => Typography::SIZE_LABEL_SMALL,
            TextStyle::Caption => Typography::SIZE_CAPTION,
            TextStyle::Code => Typography::SIZE_BODY_MEDIUM,
        }
    }

    /// Get the font for this style
    pub fn font(self) -> Font {
        match self {
            TextStyle::DisplayLarge | TextStyle::DisplayMedium | TextStyle::DisplaySmall => {
                Typography::DISPLAY
            }
            TextStyle::HeadlineLarge | TextStyle::HeadlineMedium | TextStyle::HeadlineSmall => {
                Typography::HEADLINE
            }
            TextStyle::TitleLarge | TextStyle::TitleMedium | TextStyle::TitleSmall => {
                Typography::TITLE
            }
            TextStyle::BodyLarge | TextStyle::BodyMedium | TextStyle::BodySmall => Typography::BODY,
            TextStyle::LabelLarge | TextStyle::LabelMedium | TextStyle::LabelSmall => {
                Typography::LABEL
            }
            TextStyle::Caption => Typography::BODY,
            TextStyle::Code => Typography::CODE,
        }
    }
}
