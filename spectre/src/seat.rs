//! Seat management for multi-user support

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, debug};

/// A seat represents a physical or virtual user interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seat {
    /// Seat identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Currently active session ID
    pub active_session: Option<String>,
    /// List of session IDs on this seat
    pub sessions: Vec<String>,
    /// Available VTs
    pub vts: Vec<u32>,
    /// Current VT
    pub current_vt: Option<u32>,
    /// Is this the default seat
    pub is_default: bool,
    /// Can this seat handle TTY
    pub can_tty: bool,
    /// Can this seat handle graphical
    pub can_graphical: bool,
    /// Connected devices
    pub devices: Vec<SeatDevice>,
}

impl Seat {
    pub fn new(id: &str, is_default: bool) -> Self {
        Self {
            id: id.to_string(),
            name: if is_default { "Default Seat".to_string() } else { id.to_string() },
            active_session: None,
            sessions: Vec::new(),
            vts: (1..=12).collect(), // Standard Linux VTs
            current_vt: None,
            is_default,
            can_tty: true,
            can_graphical: true,
            devices: Vec::new(),
        }
    }

    /// Add a session to this seat
    pub fn add_session(&mut self, session_id: &str) {
        if !self.sessions.contains(&session_id.to_string()) {
            self.sessions.push(session_id.to_string());
        }
    }

    /// Remove a session from this seat
    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.retain(|s| s != session_id);
        if self.active_session.as_deref() == Some(session_id) {
            self.active_session = self.sessions.first().cloned();
        }
    }

    /// Set active session
    pub fn set_active(&mut self, session_id: &str) {
        if self.sessions.contains(&session_id.to_string()) {
            self.active_session = Some(session_id.to_string());
        }
    }

    /// Get next available VT
    pub fn next_vt(&self) -> Option<u32> {
        // Find first VT not in use
        // In practice, would check against active sessions
        self.vts.first().copied()
    }

    /// Allocate a VT
    pub fn allocate_vt(&mut self) -> Option<u32> {
        // Simple allocation - in practice would track which VTs are in use
        Some(7) // Default graphical VT
    }

    /// Add a device to seat
    pub fn add_device(&mut self, device: SeatDevice) {
        if !self.devices.iter().any(|d| d.path == device.path) {
            self.devices.push(device);
        }
    }

    /// Remove a device from seat
    pub fn remove_device(&mut self, device_path: &str) {
        self.devices.retain(|d| d.path != device_path);
    }
}

/// Device attached to a seat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeatDevice {
    /// Device path (e.g., /dev/dri/card0)
    pub path: String,
    /// Device type
    pub device_type: DeviceType,
    /// Subsystem
    pub subsystem: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DeviceType {
    Graphics,
    Keyboard,
    Mouse,
    Sound,
    Other,
}

/// Seat manager
pub struct SeatManager {
    seats: HashMap<String, Seat>,
    default_seat: Option<String>,
}

impl SeatManager {
    pub fn new() -> Result<Self> {
        let mut manager = Self {
            seats: HashMap::new(),
            default_seat: None,
        };

        // Create default seat
        let default_seat = Seat::new("seat0", true);
        manager.default_seat = Some(default_seat.id.clone());
        manager.seats.insert(default_seat.id.clone(), default_seat);

        // Enumerate additional seats if multi-seat is supported
        manager.enumerate_seats()?;

        Ok(manager)
    }

    /// Enumerate hardware seats
    fn enumerate_seats(&mut self) -> Result<()> {
        // Check for udev/logind seats
        let seat_path = Path::new("/run/systemd/seats");
        if seat_path.exists() {
            for entry in std::fs::read_dir(seat_path)? {
                let entry = entry?;
                let seat_id = entry.file_name().to_string_lossy().to_string();

                if !self.seats.contains_key(&seat_id) {
                    let seat = Seat::new(&seat_id, false);
                    self.seats.insert(seat_id, seat);
                }
            }
        }

        // Enumerate devices on default seat
        self.enumerate_devices("seat0")?;

        Ok(())
    }

    /// Enumerate devices for a seat
    fn enumerate_devices(&mut self, seat_id: &str) -> Result<()> {
        let seat = self.seats.get_mut(seat_id)
            .ok_or_else(|| anyhow!("Seat not found: {}", seat_id))?;

        // Check for graphics devices
        if Path::new("/dev/dri/card0").exists() {
            seat.add_device(SeatDevice {
                path: "/dev/dri/card0".to_string(),
                device_type: DeviceType::Graphics,
                subsystem: "drm".to_string(),
            });
        }

        // Check for input devices
        if Path::new("/dev/input").exists() {
            for entry in std::fs::read_dir("/dev/input")? {
                let entry = entry?;
                let path = entry.path().to_string_lossy().to_string();

                if path.contains("event") {
                    seat.add_device(SeatDevice {
                        path,
                        device_type: DeviceType::Keyboard, // Would need to check actual type
                        subsystem: "input".to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    /// Get a seat by ID
    pub fn get(&self, id: &str) -> Option<&Seat> {
        self.seats.get(id)
    }

    /// Get mutable seat by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Seat> {
        self.seats.get_mut(id)
    }

    /// Get the default seat
    pub fn get_default_seat(&self) -> Option<String> {
        self.default_seat.clone()
    }

    /// Get default seat reference
    pub fn default_seat(&self) -> Option<&Seat> {
        self.default_seat.as_ref().and_then(|id| self.seats.get(id))
    }

    /// List all seats
    pub fn all(&self) -> impl Iterator<Item = &Seat> {
        self.seats.values()
    }

    /// Add a session to a seat
    pub fn add_session(&mut self, seat_id: &str, session_id: &str) -> Result<()> {
        let seat = self.seats.get_mut(seat_id)
            .ok_or_else(|| anyhow!("Seat not found: {}", seat_id))?;

        seat.add_session(session_id);
        info!("Added session {} to seat {}", session_id, seat_id);

        Ok(())
    }

    /// Remove a session from a seat
    pub fn remove_session(&mut self, seat_id: &str, session_id: &str) -> Result<()> {
        let seat = self.seats.get_mut(seat_id)
            .ok_or_else(|| anyhow!("Seat not found: {}", seat_id))?;

        seat.remove_session(session_id);
        info!("Removed session {} from seat {}", session_id, seat_id);

        Ok(())
    }

    /// Switch active session on a seat
    pub fn switch_session(&mut self, seat_id: &str, session_id: &str) -> Result<()> {
        let seat = self.seats.get_mut(seat_id)
            .ok_or_else(|| anyhow!("Seat not found: {}", seat_id))?;

        if !seat.sessions.contains(&session_id.to_string()) {
            return Err(anyhow!("Session {} not on seat {}", session_id, seat_id));
        }

        seat.set_active(session_id);

        // Switch VT if needed
        // In practice, would use ioctl to switch VT

        info!("Switched to session {} on seat {}", session_id, seat_id);

        Ok(())
    }

    /// Switch to a specific VT
    pub fn switch_vt(&self, vt: u32) -> Result<()> {
        debug!("Switching to VT {}", vt);

        // Would use ioctl VT_ACTIVATE on /dev/console
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::AsRawFd;

            let console = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/console")?;

            // VT_ACTIVATE = 0x5606
            const VT_ACTIVATE: libc::c_ulong = 0x5606;
            const VT_WAITACTIVE: libc::c_ulong = 0x5607;

            unsafe {
                libc::ioctl(console.as_raw_fd(), VT_ACTIVATE, vt as libc::c_int);
                libc::ioctl(console.as_raw_fd(), VT_WAITACTIVE, vt as libc::c_int);
            }
        }

        Ok(())
    }

    /// Get current VT
    pub fn current_vt(&self) -> Result<u32> {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::AsRawFd;

            let console = std::fs::OpenOptions::new()
                .read(true)
                .open("/dev/console")?;

            #[repr(C)]
            struct VtStat {
                v_active: u16,
                v_signal: u16,
                v_state: u16,
            }

            // VT_GETSTATE = 0x5603
            const VT_GETSTATE: libc::c_ulong = 0x5603;

            let mut stat = VtStat {
                v_active: 0,
                v_signal: 0,
                v_state: 0,
            };

            unsafe {
                libc::ioctl(console.as_raw_fd(), VT_GETSTATE, &mut stat);
            }

            return Ok(stat.v_active as u32);
        }

        #[cfg(not(target_os = "linux"))]
        Ok(0)
    }
}

impl Default for SeatManager {
    fn default() -> Self {
        Self::new().expect("Failed to create seat manager")
    }
}
