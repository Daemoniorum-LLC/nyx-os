//! Audio sinks (playback devices)

use crate::config::{AudioFormat, Config, SampleFormat};
use crate::device::AudioDevice;
use crate::stream::AudioStream;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Audio sink (output device wrapper)
pub struct Sink {
    /// Sink name
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
    /// Is sink running
    running: AtomicBool,
}

impl Sink {
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
            volume: config.default_volume,
            muted: false,
            streams: HashMap::new(),
            running: AtomicBool::new(false),
        })
    }

    /// Connect a stream to this sink
    pub fn connect(&mut self, stream: Arc<std::sync::RwLock<AudioStream>>) {
        let id = {
            let s = stream.read().unwrap();
            s.id
        };
        self.streams.insert(id, stream);
    }

    /// Disconnect a stream from this sink
    pub fn disconnect(&mut self, stream_id: u32) {
        self.streams.remove(&stream_id);
    }

    /// Get connected stream count
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    /// Set volume
    pub fn set_volume(&mut self, volume: u32) {
        self.volume = volume.min(100);
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

    /// Get effective volume
    pub fn effective_volume(&self) -> u32 {
        if self.muted { 0 } else { self.volume }
    }

    /// Start sink playback
    pub fn start(&self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Stop sink playback
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if sink is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get audio from all connected streams (mixed)
    pub fn get_audio(&mut self, buffer: &mut [u8]) -> usize {
        if self.muted || self.streams.is_empty() {
            buffer.fill(0);
            return buffer.len();
        }

        // Simple mixing - in practice would use proper mixer
        buffer.fill(0);

        let volume_factor = self.volume as f32 / 100.0;

        for stream in self.streams.values() {
            let mut stream = stream.write().unwrap();

            // Temporary buffer for this stream
            let mut stream_buf = vec![0u8; buffer.len()];
            let bytes_read = stream.read(&mut stream_buf);

            if bytes_read == 0 {
                continue;
            }

            // Apply stream volume
            stream.apply_volume(&mut stream_buf[..bytes_read]);

            // Mix into output (simple addition with clipping)
            for (i, chunk) in stream_buf[..bytes_read].chunks_exact(2).enumerate() {
                if i * 2 + 1 < buffer.len() {
                    let existing = i16::from_le_bytes([buffer[i * 2], buffer[i * 2 + 1]]);
                    let new = i16::from_le_bytes([chunk[0], chunk[1]]);

                    // Mix with volume
                    let mixed = ((existing as f32 + new as f32 * volume_factor) as i32)
                        .max(i16::MIN as i32)
                        .min(i16::MAX as i32) as i16;

                    let bytes = mixed.to_le_bytes();
                    buffer[i * 2] = bytes[0];
                    buffer[i * 2 + 1] = bytes[1];
                }
            }
        }

        buffer.len()
    }
}

/// Sink information for IPC
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SinkInfo {
    pub name: String,
    pub description: String,
    pub volume: u32,
    pub muted: bool,
    pub stream_count: usize,
    pub is_running: bool,
    pub sample_rate: u32,
    pub channels: u32,
}

impl From<&Sink> for SinkInfo {
    fn from(sink: &Sink) -> Self {
        Self {
            name: sink.name.clone(),
            description: sink.device.description.clone(),
            volume: sink.volume,
            muted: sink.muted,
            stream_count: sink.stream_count(),
            is_running: sink.is_running(),
            sample_rate: sink.format.sample_rate,
            channels: sink.format.channels,
        }
    }
}
