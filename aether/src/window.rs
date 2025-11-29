//! Window management
//!
//! Manages windows and their state.

use crate::config::WindowConfig;
use anyhow::Result;
use std::collections::HashMap;
use tracing::{debug, info};

/// Window manager
pub struct WindowManager {
    /// Configuration
    config: WindowConfig,
    /// Windows by ID
    windows: HashMap<u64, Window>,
    /// Window stacking order (bottom to top)
    stacking: Vec<u64>,
    /// Focused window
    focused: Option<u64>,
    /// Next window ID
    next_id: u64,
}

impl WindowManager {
    /// Create new window manager
    pub fn new(config: &WindowConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            windows: HashMap::new(),
            stacking: Vec::new(),
            focused: None,
            next_id: 1,
        })
    }

    /// Create a new window
    pub fn create_window(&mut self, client_id: u32, title: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let window = Window {
            id,
            client_id,
            title,
            geometry: WindowGeometry::default(),
            state: WindowState::Normal,
            decorations: self.config.decorations,
            visible: true,
            mapped: false,
        };

        info!("Window created: {} (client={})", id, client_id);
        self.windows.insert(id, window);
        self.stacking.push(id);
        id
    }

    /// Destroy a window
    pub fn destroy_window(&mut self, id: u64) -> Option<Window> {
        self.stacking.retain(|&wid| wid != id);
        if self.focused == Some(id) {
            self.focused = self.stacking.last().copied();
        }
        let window = self.windows.remove(&id);
        if window.is_some() {
            info!("Window destroyed: {}", id);
        }
        window
    }

    /// Get window by ID
    pub fn get(&self, id: u64) -> Option<&Window> {
        self.windows.get(&id)
    }

    /// Get window by ID (mutable)
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    /// Focus a window
    pub fn focus(&mut self, id: u64) {
        if self.windows.contains_key(&id) {
            self.focused = Some(id);
            // Raise to top
            self.stacking.retain(|&wid| wid != id);
            self.stacking.push(id);
            debug!("Window focused: {}", id);
        }
    }

    /// Get focused window
    pub fn focused(&self) -> Option<u64> {
        self.focused
    }

    /// Get visible windows in stacking order
    pub fn visible_windows(&self) -> Vec<&Window> {
        self.stacking.iter()
            .filter_map(|id| self.windows.get(id))
            .filter(|w| w.visible && w.mapped)
            .collect()
    }

    /// Set window geometry
    pub fn set_geometry(&mut self, id: u64, geom: WindowGeometry) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.geometry = geom;
        }
    }

    /// Set window state
    pub fn set_state(&mut self, id: u64, state: WindowState) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.state = state;
        }
    }

    /// Map window (make visible)
    pub fn map(&mut self, id: u64) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.mapped = true;
        }
    }

    /// Unmap window (hide)
    pub fn unmap(&mut self, id: u64) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.mapped = false;
        }
    }

    /// Find window at position
    pub fn window_at(&self, x: i32, y: i32) -> Option<u64> {
        // Search from top to bottom
        for &id in self.stacking.iter().rev() {
            if let Some(window) = self.windows.get(&id) {
                if window.visible && window.mapped && window.contains_point(x, y) {
                    return Some(id);
                }
            }
        }
        None
    }
}

/// Window information
#[derive(Debug, Clone)]
pub struct Window {
    /// Window ID
    pub id: u64,
    /// Client ID
    pub client_id: u32,
    /// Window title
    pub title: String,
    /// Geometry
    pub geometry: WindowGeometry,
    /// State
    pub state: WindowState,
    /// Has decorations
    pub decorations: bool,
    /// Is visible
    pub visible: bool,
    /// Is mapped
    pub mapped: bool,
}

impl Window {
    /// Check if point is inside window
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        let g = &self.geometry;
        x >= g.x && x < g.x + g.width as i32 &&
        y >= g.y && y < g.y + g.height as i32
    }
}

/// Window geometry
#[derive(Debug, Clone, Default)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Window state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Maximized,
    Fullscreen,
    Minimized,
}
