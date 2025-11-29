//! Output management
//!
//! Manages displays/monitors and their configurations.

use crate::config::{DisplayConfig, OutputConfig, Transform};
use anyhow::Result;
use std::collections::HashMap;
use tracing::{debug, info};

/// Output manager
pub struct OutputManager {
    /// Configuration
    config: DisplayConfig,
    /// Connected outputs
    outputs: HashMap<u32, Output>,
    /// Next output ID
    next_id: u32,
}

impl OutputManager {
    /// Create new output manager
    pub fn new(config: &DisplayConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            outputs: HashMap::new(),
            next_id: 1,
        })
    }

    /// Add a new output
    pub fn add_output(&mut self, name: String, make: String, model: String) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        // Check for configuration override
        let config = self.config.outputs.iter()
            .find(|c| c.name == name);

        let output = Output {
            id,
            name: name.clone(),
            make,
            model,
            enabled: config.map(|c| c.enabled).unwrap_or(true),
            position: config.map(|c| c.position).unwrap_or((0, 0)),
            resolution: config.and_then(|c| c.resolution).unwrap_or((1920, 1080)),
            refresh_rate: config.and_then(|c| c.refresh_rate).unwrap_or(self.config.refresh_rate),
            scale_factor: config.and_then(|c| c.scale_factor).unwrap_or(self.config.scale_factor),
            transform: config.map(|c| c.transform).unwrap_or(Transform::Normal),
            modes: Vec::new(),
            current_mode: None,
            dpms_state: DpmsState::On,
        };

        info!("Output added: {} ({}x{}@{}Hz)", name, output.resolution.0, output.resolution.1, output.refresh_rate);
        self.outputs.insert(id, output);
        id
    }

    /// Remove an output
    pub fn remove_output(&mut self, id: u32) -> Option<Output> {
        let output = self.outputs.remove(&id);
        if let Some(ref o) = output {
            info!("Output removed: {}", o.name);
        }
        output
    }

    /// Get an output by ID
    pub fn get(&self, id: u32) -> Option<&Output> {
        self.outputs.get(&id)
    }

    /// Get an output by ID (mutable)
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Output> {
        self.outputs.get_mut(&id)
    }

    /// Get output by name
    pub fn get_by_name(&self, name: &str) -> Option<&Output> {
        self.outputs.values().find(|o| o.name == name)
    }

    /// List all outputs
    pub fn list(&self) -> impl Iterator<Item = &Output> {
        self.outputs.values()
    }

    /// Get enabled outputs
    pub fn enabled(&self) -> impl Iterator<Item = &Output> {
        self.outputs.values().filter(|o| o.enabled)
    }

    /// Set output mode
    pub fn set_mode(&mut self, id: u32, width: u32, height: u32, refresh: u32) -> Result<()> {
        let output = self.outputs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("Output not found"))?;

        output.resolution = (width, height);
        output.refresh_rate = refresh;

        info!("Output {} mode changed to {}x{}@{}Hz", output.name, width, height, refresh);
        Ok(())
    }

    /// Set output position
    pub fn set_position(&mut self, id: u32, x: i32, y: i32) -> Result<()> {
        let output = self.outputs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("Output not found"))?;

        output.position = (x, y);
        debug!("Output {} position set to ({}, {})", output.name, x, y);
        Ok(())
    }

    /// Set output scale
    pub fn set_scale(&mut self, id: u32, scale: f32) -> Result<()> {
        let output = self.outputs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("Output not found"))?;

        output.scale_factor = scale;
        debug!("Output {} scale set to {}", output.name, scale);
        Ok(())
    }

    /// Set DPMS state
    pub fn set_dpms(&mut self, id: u32, state: DpmsState) -> Result<()> {
        let output = self.outputs.get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("Output not found"))?;

        output.dpms_state = state;
        debug!("Output {} DPMS state: {:?}", output.name, state);
        Ok(())
    }

    /// Get total desktop area
    pub fn total_area(&self) -> (i32, i32, u32, u32) {
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for output in self.enabled() {
            let (x, y) = output.position;
            let (w, h) = output.resolution;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + w as i32);
            max_y = max_y.max(y + h as i32);
        }

        if min_x == i32::MAX {
            (0, 0, 0, 0)
        } else {
            (min_x, min_y, (max_x - min_x) as u32, (max_y - min_y) as u32)
        }
    }
}

/// Output/display information
#[derive(Debug, Clone)]
pub struct Output {
    /// Output ID
    pub id: u32,
    /// Output name (e.g., "HDMI-A-1")
    pub name: String,
    /// Manufacturer
    pub make: String,
    /// Model
    pub model: String,
    /// Whether output is enabled
    pub enabled: bool,
    /// Position on desktop
    pub position: (i32, i32),
    /// Current resolution
    pub resolution: (u32, u32),
    /// Refresh rate (Hz)
    pub refresh_rate: u32,
    /// Scale factor
    pub scale_factor: f32,
    /// Transform (rotation/flip)
    pub transform: Transform,
    /// Available modes
    pub modes: Vec<OutputMode>,
    /// Current mode index
    pub current_mode: Option<usize>,
    /// DPMS state
    pub dpms_state: DpmsState,
}

impl Output {
    /// Get output area in logical coordinates
    pub fn logical_area(&self) -> (i32, i32, u32, u32) {
        let scaled_width = (self.resolution.0 as f32 / self.scale_factor) as u32;
        let scaled_height = (self.resolution.1 as f32 / self.scale_factor) as u32;
        (self.position.0, self.position.1, scaled_width, scaled_height)
    }

    /// Check if a point is within this output
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        let (ox, oy, ow, oh) = self.logical_area();
        x >= ox && x < ox + ow as i32 && y >= oy && y < oy + oh as i32
    }
}

/// Output mode
#[derive(Debug, Clone)]
pub struct OutputMode {
    /// Width
    pub width: u32,
    /// Height
    pub height: u32,
    /// Refresh rate (mHz)
    pub refresh: u32,
    /// Is preferred mode
    pub preferred: bool,
}

/// DPMS state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpmsState {
    On,
    Standby,
    Suspend,
    Off,
}
