//! Spacing and layout constants for Nyx OS theme
//!
//! Provides a consistent spacing scale and layout primitives based on
//! an 4px base unit system.

/// Spacing constants based on 4px base unit
#[derive(Debug, Clone, Copy)]
pub struct Spacing;

impl Spacing {
    // ═══════════════════════════════════════════════════════════════════════════
    // BASE UNITS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Base unit (4px)
    pub const UNIT: f32 = 4.0;

    // ═══════════════════════════════════════════════════════════════════════════
    // SPACING SCALE
    // ═══════════════════════════════════════════════════════════════════════════

    /// None (0px)
    pub const NONE: f32 = 0.0;
    /// Extra extra small (2px)
    pub const XXS: f32 = 2.0;
    /// Extra small (4px)
    pub const XS: f32 = 4.0;
    /// Small (8px)
    pub const SM: f32 = 8.0;
    /// Medium (12px)
    pub const MD: f32 = 12.0;
    /// Large (16px)
    pub const LG: f32 = 16.0;
    /// Extra large (24px)
    pub const XL: f32 = 24.0;
    /// Extra extra large (32px)
    pub const XXL: f32 = 32.0;
    /// 3X large (48px)
    pub const XXXL: f32 = 48.0;
    /// 4X large (64px)
    pub const XXXXL: f32 = 64.0;

    // ═══════════════════════════════════════════════════════════════════════════
    // COMPONENT SPACING
    // ═══════════════════════════════════════════════════════════════════════════

    /// Inline spacing between icons and text
    pub const INLINE_ICON: f32 = 8.0;
    /// Spacing between form elements
    pub const FORM_GAP: f32 = 16.0;
    /// Spacing between stacked items
    pub const STACK_GAP: f32 = 12.0;
    /// Card internal padding
    pub const CARD_PADDING: f32 = 16.0;
    /// Modal internal padding
    pub const MODAL_PADDING: f32 = 24.0;
    /// Page margin
    pub const PAGE_MARGIN: f32 = 24.0;

    // ═══════════════════════════════════════════════════════════════════════════
    // BORDER RADIUS
    // ═══════════════════════════════════════════════════════════════════════════

    /// No radius (sharp corners)
    pub const RADIUS_NONE: f32 = 0.0;
    /// Small radius (4px) - buttons, inputs
    pub const RADIUS_SM: f32 = 4.0;
    /// Medium radius (8px) - cards, panels
    pub const RADIUS_MD: f32 = 8.0;
    /// Large radius (12px) - modals, popovers
    pub const RADIUS_LG: f32 = 12.0;
    /// Extra large radius (16px) - large panels
    pub const RADIUS_XL: f32 = 16.0;
    /// Pill radius (9999px) - pills, tags
    pub const RADIUS_PILL: f32 = 9999.0;
    /// Circle (50%) - avatars, icons
    pub const RADIUS_CIRCLE: f32 = 9999.0;

    // ═══════════════════════════════════════════════════════════════════════════
    // SIZING
    // ═══════════════════════════════════════════════════════════════════════════

    /// Icon sizes
    pub const ICON_XS: f32 = 12.0;
    pub const ICON_SM: f32 = 16.0;
    pub const ICON_MD: f32 = 20.0;
    pub const ICON_LG: f32 = 24.0;
    pub const ICON_XL: f32 = 32.0;

    /// Avatar sizes
    pub const AVATAR_XS: f32 = 24.0;
    pub const AVATAR_SM: f32 = 32.0;
    pub const AVATAR_MD: f32 = 40.0;
    pub const AVATAR_LG: f32 = 48.0;
    pub const AVATAR_XL: f32 = 64.0;

    /// Button heights
    pub const BUTTON_HEIGHT_SM: f32 = 28.0;
    pub const BUTTON_HEIGHT_MD: f32 = 36.0;
    pub const BUTTON_HEIGHT_LG: f32 = 44.0;

    /// Input heights
    pub const INPUT_HEIGHT_SM: f32 = 32.0;
    pub const INPUT_HEIGHT_MD: f32 = 40.0;
    pub const INPUT_HEIGHT_LG: f32 = 48.0;

    // ═══════════════════════════════════════════════════════════════════════════
    // PANEL & SHELL DIMENSIONS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Top panel height
    pub const PANEL_HEIGHT: f32 = 32.0;
    /// Dock height
    pub const DOCK_HEIGHT: f32 = 64.0;
    /// Dock icon size
    pub const DOCK_ICON_SIZE: f32 = 48.0;
    /// Sidebar width (collapsed)
    pub const SIDEBAR_COLLAPSED: f32 = 56.0;
    /// Sidebar width (expanded)
    pub const SIDEBAR_EXPANDED: f32 = 240.0;
    /// Control center width
    pub const CONTROL_CENTER_WIDTH: f32 = 360.0;
    /// Assistant width
    pub const ASSISTANT_WIDTH: f32 = 680.0;

    // ═══════════════════════════════════════════════════════════════════════════
    // SHADOWS (elevation levels)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Shadow blur for low elevation
    pub const SHADOW_SM: f32 = 4.0;
    /// Shadow blur for medium elevation
    pub const SHADOW_MD: f32 = 8.0;
    /// Shadow blur for high elevation
    pub const SHADOW_LG: f32 = 16.0;
    /// Shadow blur for highest elevation (modals)
    pub const SHADOW_XL: f32 = 24.0;

    // ═══════════════════════════════════════════════════════════════════════════
    // ANIMATION
    // ═══════════════════════════════════════════════════════════════════════════

    /// Fast animation duration (100ms)
    pub const DURATION_FAST: u64 = 100;
    /// Normal animation duration (200ms)
    pub const DURATION_NORMAL: u64 = 200;
    /// Slow animation duration (300ms)
    pub const DURATION_SLOW: u64 = 300;
    /// Very slow animation duration (500ms)
    pub const DURATION_SLOWER: u64 = 500;
}

/// Padding helper struct
#[derive(Debug, Clone, Copy, Default)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    /// Create uniform padding
    pub fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Create symmetric padding (vertical, horizontal)
    pub fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Create padding with individual values
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Convert to iced Padding
    pub fn to_iced(self) -> iced::Padding {
        iced::Padding {
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            left: self.left,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacing_scale_is_ordered() {
        assert!(Spacing::NONE < Spacing::XXS);
        assert!(Spacing::XXS < Spacing::XS);
        assert!(Spacing::XS < Spacing::SM);
        assert!(Spacing::SM < Spacing::MD);
        assert!(Spacing::MD < Spacing::LG);
        assert!(Spacing::LG < Spacing::XL);
        assert!(Spacing::XL < Spacing::XXL);
        assert!(Spacing::XXL < Spacing::XXXL);
        assert!(Spacing::XXXL < Spacing::XXXXL);
    }

    #[test]
    fn test_spacing_base_unit() {
        assert!((Spacing::UNIT - 4.0).abs() < 0.01);
        assert!((Spacing::XS - Spacing::UNIT).abs() < 0.01);
    }

    #[test]
    fn test_spacing_multiples_of_base() {
        // XS should be 1x base (4)
        assert!((Spacing::XS - 4.0).abs() < 0.01);
        // SM should be 2x base (8)
        assert!((Spacing::SM - 8.0).abs() < 0.01);
        // LG should be 4x base (16)
        assert!((Spacing::LG - 16.0).abs() < 0.01);
    }

    #[test]
    fn test_icon_sizes_ordered() {
        assert!(Spacing::ICON_XS < Spacing::ICON_SM);
        assert!(Spacing::ICON_SM < Spacing::ICON_MD);
        assert!(Spacing::ICON_MD < Spacing::ICON_LG);
        assert!(Spacing::ICON_LG < Spacing::ICON_XL);
    }

    #[test]
    fn test_avatar_sizes_ordered() {
        assert!(Spacing::AVATAR_XS < Spacing::AVATAR_SM);
        assert!(Spacing::AVATAR_SM < Spacing::AVATAR_MD);
        assert!(Spacing::AVATAR_MD < Spacing::AVATAR_LG);
        assert!(Spacing::AVATAR_LG < Spacing::AVATAR_XL);
    }

    #[test]
    fn test_button_heights_ordered() {
        assert!(Spacing::BUTTON_HEIGHT_SM < Spacing::BUTTON_HEIGHT_MD);
        assert!(Spacing::BUTTON_HEIGHT_MD < Spacing::BUTTON_HEIGHT_LG);
    }

    #[test]
    fn test_input_heights_ordered() {
        assert!(Spacing::INPUT_HEIGHT_SM < Spacing::INPUT_HEIGHT_MD);
        assert!(Spacing::INPUT_HEIGHT_MD < Spacing::INPUT_HEIGHT_LG);
    }

    #[test]
    fn test_radius_ordered() {
        assert!(Spacing::RADIUS_NONE < Spacing::RADIUS_SM);
        assert!(Spacing::RADIUS_SM < Spacing::RADIUS_MD);
        assert!(Spacing::RADIUS_MD < Spacing::RADIUS_LG);
        assert!(Spacing::RADIUS_LG < Spacing::RADIUS_XL);
    }

    #[test]
    fn test_radius_pill_and_circle_are_large() {
        assert!(Spacing::RADIUS_PILL > 1000.0);
        assert!(Spacing::RADIUS_CIRCLE > 1000.0);
    }

    #[test]
    fn test_shadow_sizes_ordered() {
        assert!(Spacing::SHADOW_SM < Spacing::SHADOW_MD);
        assert!(Spacing::SHADOW_MD < Spacing::SHADOW_LG);
        assert!(Spacing::SHADOW_LG < Spacing::SHADOW_XL);
    }

    #[test]
    fn test_duration_ordered() {
        assert!(Spacing::DURATION_FAST < Spacing::DURATION_NORMAL);
        assert!(Spacing::DURATION_NORMAL < Spacing::DURATION_SLOW);
        assert!(Spacing::DURATION_SLOW < Spacing::DURATION_SLOWER);
    }

    #[test]
    fn test_shell_dimensions_reasonable() {
        assert!(Spacing::PANEL_HEIGHT > 0.0);
        assert!(Spacing::DOCK_HEIGHT > Spacing::PANEL_HEIGHT);
        assert!(Spacing::SIDEBAR_COLLAPSED < Spacing::SIDEBAR_EXPANDED);
        assert!(Spacing::CONTROL_CENTER_WIDTH > 0.0);
        assert!(Spacing::ASSISTANT_WIDTH > Spacing::CONTROL_CENTER_WIDTH);
    }

    #[test]
    fn test_padding_all() {
        let padding = Padding::all(16.0);
        assert!((padding.top - 16.0).abs() < 0.01);
        assert!((padding.right - 16.0).abs() < 0.01);
        assert!((padding.bottom - 16.0).abs() < 0.01);
        assert!((padding.left - 16.0).abs() < 0.01);
    }

    #[test]
    fn test_padding_symmetric() {
        let padding = Padding::symmetric(8.0, 16.0);
        assert!((padding.top - 8.0).abs() < 0.01);
        assert!((padding.bottom - 8.0).abs() < 0.01);
        assert!((padding.right - 16.0).abs() < 0.01);
        assert!((padding.left - 16.0).abs() < 0.01);
    }

    #[test]
    fn test_padding_new() {
        let padding = Padding::new(1.0, 2.0, 3.0, 4.0);
        assert!((padding.top - 1.0).abs() < 0.01);
        assert!((padding.right - 2.0).abs() < 0.01);
        assert!((padding.bottom - 3.0).abs() < 0.01);
        assert!((padding.left - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_padding_default() {
        let padding = Padding::default();
        assert!(padding.top.abs() < 0.01);
        assert!(padding.right.abs() < 0.01);
        assert!(padding.bottom.abs() < 0.01);
        assert!(padding.left.abs() < 0.01);
    }

    #[test]
    fn test_padding_to_iced() {
        let padding = Padding::new(1.0, 2.0, 3.0, 4.0);
        let iced_padding = padding.to_iced();
        assert!((iced_padding.top - 1.0).abs() < 0.01);
        assert!((iced_padding.right - 2.0).abs() < 0.01);
        assert!((iced_padding.bottom - 3.0).abs() < 0.01);
        assert!((iced_padding.left - 4.0).abs() < 0.01);
    }
}
