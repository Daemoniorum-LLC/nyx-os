//! Audio stream management

use crate::config::AudioFormat;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};

/// Stream ID counter
static STREAM_ID: AtomicU32 = AtomicU32::new(1);

/// Audio stream direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDirection {
    /// Playback stream (to sink)
    Playback,
    /// Capture stream (from source)
    Capture,
}

/// Stream state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamState {
    /// Stream created but not started
    Created,
    /// Stream is running
    Running,
    /// Stream is corked (paused)
    Corked,
    /// Stream is draining
    Draining,
    /// Stream has finished
    Finished,
}

/// Audio stream
pub struct AudioStream {
    /// Unique stream ID
    pub id: u32,
    /// Stream name
    pub name: String,
    /// Application name
    pub app_name: String,
    /// Process ID of client
    pub pid: Option<u32>,
    /// Stream direction
    pub direction: StreamDirection,
    /// Audio format
    pub format: AudioFormat,
    /// Current state
    pub state: StreamState,
    /// Volume (0-100)
    pub volume: u32,
    /// Muted
    pub muted: bool,
    /// Target sink/source name
    pub target: String,
    /// Audio buffer
    buffer: VecDeque<u8>,
    /// Buffer high watermark (bytes)
    buffer_high: usize,
    /// Buffer low watermark (bytes)
    buffer_low: usize,
}

impl AudioStream {
    pub fn new(
        name: &str,
        app_name: &str,
        direction: StreamDirection,
        format: AudioFormat,
        target: &str,
    ) -> Self {
        let id = STREAM_ID.fetch_add(1, Ordering::SeqCst);

        // Calculate buffer sizes based on format
        // Target ~100ms of buffer
        let target_bytes = format.byte_rate() / 10;
        let buffer_high = target_bytes * 2;
        let buffer_low = target_bytes / 2;

        Self {
            id,
            name: name.to_string(),
            app_name: app_name.to_string(),
            pid: None,
            direction,
            format,
            state: StreamState::Created,
            volume: 100,
            muted: false,
            target: target.to_string(),
            buffer: VecDeque::with_capacity(buffer_high),
            buffer_high,
            buffer_low,
        }
    }

    /// Start the stream
    pub fn start(&mut self) {
        self.state = StreamState::Running;
    }

    /// Cork (pause) the stream
    pub fn cork(&mut self) {
        self.state = StreamState::Corked;
    }

    /// Uncork (resume) the stream
    pub fn uncork(&mut self) {
        self.state = StreamState::Running;
    }

    /// Drain the stream
    pub fn drain(&mut self) {
        self.state = StreamState::Draining;
    }

    /// Write audio data to buffer
    pub fn write(&mut self, data: &[u8]) -> usize {
        let available = self.buffer_high - self.buffer.len();
        let to_write = data.len().min(available);

        for &byte in &data[..to_write] {
            self.buffer.push_back(byte);
        }

        to_write
    }

    /// Read audio data from buffer
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let to_read = buf.len().min(self.buffer.len());

        for i in 0..to_read {
            buf[i] = self.buffer.pop_front().unwrap_or(0);
        }

        // If draining and buffer empty, finish
        if self.state == StreamState::Draining && self.buffer.is_empty() {
            self.state = StreamState::Finished;
        }

        to_read
    }

    /// Get bytes available in buffer
    pub fn bytes_available(&self) -> usize {
        self.buffer.len()
    }

    /// Get free space in buffer
    pub fn bytes_free(&self) -> usize {
        self.buffer_high.saturating_sub(self.buffer.len())
    }

    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.buffer_high
    }

    /// Check if buffer is low
    pub fn is_low(&self) -> bool {
        self.buffer.len() <= self.buffer_low
    }

    /// Apply volume to audio data
    pub fn apply_volume(&self, data: &mut [u8]) {
        if self.muted {
            // Zero the buffer
            data.fill(0);
            return;
        }

        if self.volume == 100 {
            return;
        }

        // Simple volume scaling for S16LE
        // In practice, would handle different formats
        let volume_factor = self.volume as f32 / 100.0;

        for chunk in data.chunks_exact_mut(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            let scaled = (sample as f32 * volume_factor) as i16;
            let bytes = scaled.to_le_bytes();
            chunk[0] = bytes[0];
            chunk[1] = bytes[1];
        }
    }

    /// Set volume
    pub fn set_volume(&mut self, volume: u32) {
        self.volume = volume.min(150); // Allow slight boost
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

    /// Get effective volume (considering mute)
    pub fn effective_volume(&self) -> u32 {
        if self.muted { 0 } else { self.volume }
    }
}

/// Stream info for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub id: u32,
    pub name: String,
    pub app_name: String,
    pub pid: Option<u32>,
    pub direction: String,
    pub state: String,
    pub volume: u32,
    pub muted: bool,
    pub sink: String,
}

impl From<&AudioStream> for StreamInfo {
    fn from(stream: &AudioStream) -> Self {
        Self {
            id: stream.id,
            name: stream.name.clone(),
            app_name: stream.app_name.clone(),
            pid: stream.pid,
            direction: match stream.direction {
                StreamDirection::Playback => "playback".to_string(),
                StreamDirection::Capture => "capture".to_string(),
            },
            state: format!("{:?}", stream.state).to_lowercase(),
            volume: stream.volume,
            muted: stream.muted,
            sink: stream.target.clone(),
        }
    }
}
