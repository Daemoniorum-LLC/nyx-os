//! Audio configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Vesper configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default sample rate
    pub sample_rate: u32,
    /// Default sample format
    pub sample_format: SampleFormat,
    /// Default channel count
    pub channels: u32,
    /// Buffer size in frames
    pub buffer_size: u32,
    /// Periods per buffer
    pub periods: u32,
    /// Default volume (0-100)
    pub default_volume: u32,
    /// Default sink name
    pub default_sink: Option<String>,
    /// Default source name
    pub default_source: Option<String>,
    /// Enable Bluetooth audio
    pub bluetooth_enabled: bool,
    /// Enable network audio (RTP)
    pub network_enabled: bool,
    /// Resampler quality (1-10)
    pub resampler_quality: u32,
    /// Enable flat volume (all streams same volume)
    pub flat_volume: bool,
    /// Auto-switch to new devices
    pub auto_switch: bool,
    /// Saved stream volumes
    pub stream_volumes: std::collections::HashMap<String, u32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            sample_format: SampleFormat::S16Le,
            channels: 2,
            buffer_size: 1024,
            periods: 4,
            default_volume: 70,
            default_sink: None,
            default_source: None,
            bluetooth_enabled: true,
            network_enabled: false,
            resampler_quality: 4,
            flat_volume: false,
            auto_switch: true,
            stream_volumes: std::collections::HashMap::new(),
        }
    }
}

/// Sample format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleFormat {
    U8,
    S16Le,
    S16Be,
    S24Le,
    S24Be,
    S32Le,
    S32Be,
    Float32Le,
    Float32Be,
}

impl SampleFormat {
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            SampleFormat::U8 => 1,
            SampleFormat::S16Le | SampleFormat::S16Be => 2,
            SampleFormat::S24Le | SampleFormat::S24Be => 3,
            SampleFormat::S32Le | SampleFormat::S32Be |
            SampleFormat::Float32Le | SampleFormat::Float32Be => 4,
        }
    }

    pub fn is_float(&self) -> bool {
        matches!(self, SampleFormat::Float32Le | SampleFormat::Float32Be)
    }
}

impl std::fmt::Display for SampleFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SampleFormat::U8 => "u8",
            SampleFormat::S16Le => "s16le",
            SampleFormat::S16Be => "s16be",
            SampleFormat::S24Le => "s24le",
            SampleFormat::S24Be => "s24be",
            SampleFormat::S32Le => "s32le",
            SampleFormat::S32Be => "s32be",
            SampleFormat::Float32Le => "f32le",
            SampleFormat::Float32Be => "f32be",
        };
        write!(f, "{}", s)
    }
}

/// Audio format specification
#[derive(Debug, Clone)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub sample_format: SampleFormat,
    pub channels: u32,
}

impl AudioFormat {
    pub fn new(sample_rate: u32, sample_format: SampleFormat, channels: u32) -> Self {
        Self {
            sample_rate,
            sample_format,
            channels,
        }
    }

    /// Bytes per frame (all channels)
    pub fn frame_size(&self) -> usize {
        self.sample_format.bytes_per_sample() * self.channels as usize
    }

    /// Bytes per second
    pub fn byte_rate(&self) -> usize {
        self.frame_size() * self.sample_rate as usize
    }

    /// Microseconds per frame
    pub fn usec_per_frame(&self) -> u64 {
        1_000_000 / self.sample_rate as u64
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            sample_format: SampleFormat::S16Le,
            channels: 2,
        }
    }
}

/// Load configuration from file
pub fn load_config(path: &Path) -> Result<Config> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    } else {
        Ok(Config::default())
    }
}

/// Channel map
#[derive(Debug, Clone)]
pub struct ChannelMap {
    pub channels: Vec<ChannelPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelPosition {
    Mono,
    FrontLeft,
    FrontRight,
    FrontCenter,
    RearLeft,
    RearRight,
    RearCenter,
    Lfe,
    SideLeft,
    SideRight,
    TopFrontLeft,
    TopFrontRight,
    TopRearLeft,
    TopRearRight,
}

impl ChannelMap {
    pub fn mono() -> Self {
        Self {
            channels: vec![ChannelPosition::Mono],
        }
    }

    pub fn stereo() -> Self {
        Self {
            channels: vec![ChannelPosition::FrontLeft, ChannelPosition::FrontRight],
        }
    }

    pub fn surround51() -> Self {
        Self {
            channels: vec![
                ChannelPosition::FrontLeft,
                ChannelPosition::FrontRight,
                ChannelPosition::FrontCenter,
                ChannelPosition::Lfe,
                ChannelPosition::RearLeft,
                ChannelPosition::RearRight,
            ],
        }
    }
}
