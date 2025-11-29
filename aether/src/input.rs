//! Input handling
//!
//! Manages keyboard, mouse, touchpad, and touchscreen input.

use crate::config::InputConfig;
use anyhow::Result;
use std::collections::HashSet;
use tracing::debug;

/// Input state
pub struct InputState {
    /// Configuration
    config: InputConfig,
    /// Currently pressed keys
    pressed_keys: HashSet<u32>,
    /// Keyboard modifiers state
    modifiers: Modifiers,
    /// Pointer position
    pointer_position: (f64, f64),
    /// Pressed pointer buttons
    pointer_buttons: HashSet<u32>,
    /// Focused window ID
    keyboard_focus: Option<u64>,
    /// Pointer focus window ID
    pointer_focus: Option<u64>,
}

impl InputState {
    /// Create new input state
    pub fn new(config: &InputConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            pressed_keys: HashSet::new(),
            modifiers: Modifiers::default(),
            pointer_position: (0.0, 0.0),
            pointer_buttons: HashSet::new(),
            keyboard_focus: None,
            pointer_focus: None,
        })
    }

    /// Handle key press
    pub fn key_press(&mut self, keycode: u32) {
        self.pressed_keys.insert(keycode);
        self.update_modifiers(keycode, true);
    }

    /// Handle key release
    pub fn key_release(&mut self, keycode: u32) {
        self.pressed_keys.remove(&keycode);
        self.update_modifiers(keycode, false);
    }

    /// Handle pointer motion
    pub fn pointer_motion(&mut self, x: f64, y: f64) {
        self.pointer_position = (x, y);
    }

    /// Handle pointer button press
    pub fn pointer_button_press(&mut self, button: u32) {
        self.pointer_buttons.insert(button);
    }

    /// Handle pointer button release
    pub fn pointer_button_release(&mut self, button: u32) {
        self.pointer_buttons.remove(&button);
    }

    /// Get pointer position
    pub fn pointer_position(&self) -> (f64, f64) {
        self.pointer_position
    }

    /// Check if a key is pressed
    pub fn is_key_pressed(&self, keycode: u32) -> bool {
        self.pressed_keys.contains(&keycode)
    }

    /// Check if a pointer button is pressed
    pub fn is_button_pressed(&self, button: u32) -> bool {
        self.pointer_buttons.contains(&button)
    }

    /// Get current modifiers
    pub fn modifiers(&self) -> &Modifiers {
        &self.modifiers
    }

    /// Set keyboard focus
    pub fn set_keyboard_focus(&mut self, window_id: Option<u64>) {
        self.keyboard_focus = window_id;
    }

    /// Get keyboard focus
    pub fn keyboard_focus(&self) -> Option<u64> {
        self.keyboard_focus
    }

    /// Set pointer focus
    pub fn set_pointer_focus(&mut self, window_id: Option<u64>) {
        self.pointer_focus = window_id;
    }

    /// Get pointer focus
    pub fn pointer_focus(&self) -> Option<u64> {
        self.pointer_focus
    }

    fn update_modifiers(&mut self, keycode: u32, pressed: bool) {
        // XKB keycodes for common modifiers
        match keycode {
            50 | 62 => self.modifiers.shift = pressed,        // Shift
            37 | 105 => self.modifiers.ctrl = pressed,        // Ctrl
            64 | 108 => self.modifiers.alt = pressed,         // Alt
            133 | 134 => self.modifiers.logo = pressed,       // Super/Logo
            66 => self.modifiers.caps_lock = pressed,         // Caps Lock
            77 => self.modifiers.num_lock = pressed,          // Num Lock
            _ => {}
        }
    }
}

/// Keyboard modifier state
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl Modifiers {
    /// Check if any modifier is active
    pub fn any(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.logo
    }

    /// Get modifier state as u32 mask
    pub fn as_mask(&self) -> u32 {
        let mut mask = 0u32;
        if self.shift { mask |= 1; }
        if self.ctrl { mask |= 4; }
        if self.alt { mask |= 8; }
        if self.logo { mask |= 64; }
        if self.caps_lock { mask |= 2; }
        if self.num_lock { mask |= 16; }
        mask
    }
}

/// Input event
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Key event
    Key {
        keycode: u32,
        state: KeyState,
        time: u32,
    },
    /// Pointer motion
    PointerMotion {
        x: f64,
        y: f64,
        time: u32,
    },
    /// Pointer motion (relative)
    PointerMotionRelative {
        dx: f64,
        dy: f64,
        time: u32,
    },
    /// Pointer button
    PointerButton {
        button: u32,
        state: ButtonState,
        time: u32,
    },
    /// Scroll (axis)
    Scroll {
        axis: ScrollAxis,
        value: f64,
        time: u32,
    },
    /// Touch event
    Touch {
        slot: i32,
        x: f64,
        y: f64,
        state: TouchState,
        time: u32,
    },
}

/// Key state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
}

/// Button state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

/// Scroll axis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
}

/// Touch state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchState {
    Down,
    Up,
    Motion,
}
