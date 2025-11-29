//! Shell management
//!
//! Implements Wayland shell protocols (xdg-shell, layer-shell, etc.)

use anyhow::Result;
use std::collections::HashMap;
use tracing::debug;

/// Shell manager
pub struct ShellManager {
    /// xdg-shell surfaces
    xdg_surfaces: HashMap<u64, XdgSurface>,
    /// Layer shell surfaces
    layer_surfaces: HashMap<u64, LayerSurface>,
}

impl ShellManager {
    /// Create new shell manager
    pub fn new() -> Result<Self> {
        Ok(Self {
            xdg_surfaces: HashMap::new(),
            layer_surfaces: HashMap::new(),
        })
    }

    /// Register an xdg-toplevel
    pub fn register_xdg_toplevel(&mut self, surface_id: u64, window_id: u64) {
        self.xdg_surfaces.insert(surface_id, XdgSurface {
            window_id,
            role: XdgRole::Toplevel(XdgToplevel::default()),
        });
    }

    /// Register an xdg-popup
    pub fn register_xdg_popup(&mut self, surface_id: u64, window_id: u64, parent: u64) {
        self.xdg_surfaces.insert(surface_id, XdgSurface {
            window_id,
            role: XdgRole::Popup(XdgPopup {
                parent,
                position: (0, 0),
            }),
        });
    }

    /// Register a layer surface
    pub fn register_layer_surface(&mut self, surface_id: u64, layer: Layer, anchor: Anchor) {
        self.layer_surfaces.insert(surface_id, LayerSurface {
            layer,
            anchor,
            exclusive_zone: 0,
            margin: (0, 0, 0, 0),
            keyboard_interactivity: KeyboardInteractivity::None,
        });
    }

    /// Get xdg surface
    pub fn get_xdg(&self, surface_id: u64) -> Option<&XdgSurface> {
        self.xdg_surfaces.get(&surface_id)
    }

    /// Get layer surface
    pub fn get_layer(&self, surface_id: u64) -> Option<&LayerSurface> {
        self.layer_surfaces.get(&surface_id)
    }

    /// Remove surface
    pub fn remove(&mut self, surface_id: u64) {
        self.xdg_surfaces.remove(&surface_id);
        self.layer_surfaces.remove(&surface_id);
    }
}

/// xdg-shell surface
#[derive(Debug, Clone)]
pub struct XdgSurface {
    pub window_id: u64,
    pub role: XdgRole,
}

/// xdg surface role
#[derive(Debug, Clone)]
pub enum XdgRole {
    Toplevel(XdgToplevel),
    Popup(XdgPopup),
}

/// xdg-toplevel state
#[derive(Debug, Clone, Default)]
pub struct XdgToplevel {
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub parent: Option<u64>,
    pub maximized: bool,
    pub fullscreen: bool,
    pub resizing: bool,
    pub activated: bool,
    pub min_size: Option<(u32, u32)>,
    pub max_size: Option<(u32, u32)>,
}

/// xdg-popup state
#[derive(Debug, Clone)]
pub struct XdgPopup {
    pub parent: u64,
    pub position: (i32, i32),
}

/// Layer surface
#[derive(Debug, Clone)]
pub struct LayerSurface {
    pub layer: Layer,
    pub anchor: Anchor,
    pub exclusive_zone: i32,
    pub margin: (i32, i32, i32, i32), // top, right, bottom, left
    pub keyboard_interactivity: KeyboardInteractivity,
}

/// Layer shell layer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

/// Layer shell anchor
#[derive(Debug, Clone, Copy, Default)]
pub struct Anchor {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

/// Keyboard interactivity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardInteractivity {
    None,
    Exclusive,
    OnDemand,
}
