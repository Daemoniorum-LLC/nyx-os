//! Audio sources (capture devices)

use crate::config::{AudioFormat, Config};
use crate::device::AudioDevice;
use crate::stream::AudioStream;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Audio source (input device wrapper)
pub struct Source {
    /// Source name
    pub name: String,
    /// Device info
    pub device: AudioDevice,
    /// Audio format
    pub format: AudioFormat,
    /// Volume (0-100)
    pub volume: u32,
    /// Muted
    pub muted: bool,
    /// Connected streams
    streams: HashMap<u32, Arc<std::sync::RwLock<AudioStream>>>,
    /// Is source running
    running: AtomicBool,
}

impl Source {
    pub fn new(device: AudioDevice, config: Config) -> Result<Self> {
        let format = AudioFormat {
            sample_rate: config.sample_rate,
            sample_format: config.sample_format,
            channels: config.channels,
        };

        Ok(Self {
            name: device.name.clone(),
            device,
            format,
            volume: 100,
            muted: false,
            streams: HashMap::new(),
            running: AtomicBool::new(false),
        })
    }

    /// Connect a stream to this source
    pub fn connect(&mut self, stream: Arc<std::sync::RwLock<AudioStream>>) {
        let id = {
            let s = stream.read().unwrap();
            s.id
        };
        self.streams.insert(id, stream);
    }

    /// Disconnect a stream from this source
    pub fn disconnect(&mut self, stream_id: u32) {
        self.streams.remove(&stream_id);
    }

    /// Get connected stream count
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    /// Set volume (input gain)
    pub fn set_volume(&mut self, volume: u32) {
        self.volume = volume.min(150); // Allow some boost
    }

    /// Set mute
    pub fn set_mute(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Toggle mute
    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        self.muted
    }

    /// Start source capture
    pub fn start(&self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Stop source capture
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if source is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Push captured audio to all connected streams
    pub fn push_audio(&mut self, data: &[u8]) {
        if self.muted {
            return;
        }

        // Apply input volume
        let mut processed = data.to_vec();
        self.apply_volume(&mut processed);

        // Write to all connected streams
        for stream in self.streams.values() {
            let mut stream = stream.write().unwrap();
            stream.write(&processed);
        }
    }

    /// Apply input volume/gain
    fn apply_volume(&self, data: &mut [u8]) {
        if self.volume == 100 {
            return;
        }

        let volume_factor = self.volume as f32 / 100.0;

        // Simple S16LE volume scaling
        for chunk in data.chunks_exact_mut(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            let scaled = ((sample as f32 * volume_factor) as i32)
                .max(i16::MIN as i32)
                .min(i16::MAX as i32) as i16;
            let bytes = scaled.to_le_bytes();
            chunk[0] = bytes[0];
            chunk[1] = bytes[1];
        }
    }
}

/// Source information for IPC
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceInfo {
    pub name: String,
    pub description: String,
    pub volume: u32,
    pub muted: bool,
    pub stream_count: usize,
    pub is_running: bool,
    pub sample_rate: u32,
    pub channels: u32,
}

impl From<&Source> for SourceInfo {
    fn from(source: &Source) -> Self {
        Self {
            name: source.name.clone(),
            description: source.device.description.clone(),
            volume: source.volume,
            muted: source.muted,
            stream_count: source.stream_count(),
            is_running: source.is_running(),
            sample_rate: source.format.sample_rate,
            channels: source.format.channels,
        }
    }
}
