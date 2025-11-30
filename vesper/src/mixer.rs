//! Audio mixing

use crate::config::{AudioFormat, Config, SampleFormat};
use crate::stream::AudioStream;
use std::collections::HashMap;

/// Audio mixer - combines multiple streams
pub struct Mixer {
    config: Config,
    master_volume: u32,
    master_muted: bool,
}

impl Mixer {
    pub fn new(config: Config) -> Self {
        Self {
            master_volume: config.default_volume,
            master_muted: false,
            config,
        }
    }

    /// Mix multiple streams into output buffer
    pub fn mix(&self, streams: &mut [&mut AudioStream], output: &mut [u8], format: &AudioFormat) {
        // Clear output
        output.fill(0);

        if streams.is_empty() || self.master_muted {
            return;
        }

        // Temporary mixing buffer (float for precision)
        let sample_count = output.len() / format.frame_size();
        let channel_count = format.channels as usize;
        let mut mix_buffer = vec![0.0f32; sample_count * channel_count];

        // Mix each stream
        for stream in streams.iter_mut() {
            if stream.state != crate::stream::StreamState::Running {
                continue;
            }

            // Read from stream
            let mut stream_data = vec![0u8; output.len()];
            let bytes_read = stream.read(&mut stream_data);

            if bytes_read == 0 {
                continue;
            }

            // Apply stream volume
            stream.apply_volume(&mut stream_data[..bytes_read]);

            // Convert to float and add to mix
            self.add_to_mix(&stream_data[..bytes_read], &mut mix_buffer, format);
        }

        // Apply master volume
        let master_factor = self.master_volume as f32 / 100.0;
        for sample in &mut mix_buffer {
            *sample *= master_factor;
        }

        // Convert back to output format
        self.float_to_output(&mix_buffer, output, format);
    }

    /// Add samples to mix buffer
    fn add_to_mix(&self, input: &[u8], mix: &mut [f32], format: &AudioFormat) {
        match format.sample_format {
            SampleFormat::S16Le => {
                for (i, chunk) in input.chunks_exact(2).enumerate() {
                    if i < mix.len() {
                        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                        mix[i] += sample as f32 / 32768.0;
                    }
                }
            }
            SampleFormat::S32Le => {
                for (i, chunk) in input.chunks_exact(4).enumerate() {
                    if i < mix.len() {
                        let sample = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        mix[i] += sample as f32 / 2147483648.0;
                    }
                }
            }
            SampleFormat::Float32Le => {
                for (i, chunk) in input.chunks_exact(4).enumerate() {
                    if i < mix.len() {
                        let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        mix[i] += sample;
                    }
                }
            }
            _ => {}
        }
    }

    /// Convert float mix to output format
    fn float_to_output(&self, mix: &[f32], output: &mut [u8], format: &AudioFormat) {
        match format.sample_format {
            SampleFormat::S16Le => {
                for (i, &sample) in mix.iter().enumerate() {
                    let offset = i * 2;
                    if offset + 1 < output.len() {
                        // Soft clipping
                        let clamped = soft_clip(sample);
                        let int_sample = (clamped * 32767.0) as i16;
                        let bytes = int_sample.to_le_bytes();
                        output[offset] = bytes[0];
                        output[offset + 1] = bytes[1];
                    }
                }
            }
            SampleFormat::S32Le => {
                for (i, &sample) in mix.iter().enumerate() {
                    let offset = i * 4;
                    if offset + 3 < output.len() {
                        let clamped = soft_clip(sample);
                        let int_sample = (clamped * 2147483647.0) as i32;
                        let bytes = int_sample.to_le_bytes();
                        output[offset..offset + 4].copy_from_slice(&bytes);
                    }
                }
            }
            SampleFormat::Float32Le => {
                for (i, &sample) in mix.iter().enumerate() {
                    let offset = i * 4;
                    if offset + 3 < output.len() {
                        let clamped = soft_clip(sample);
                        let bytes = clamped.to_le_bytes();
                        output[offset..offset + 4].copy_from_slice(&bytes);
                    }
                }
            }
            _ => {}
        }
    }

    /// Get master volume
    pub fn master_volume(&self) -> u32 {
        self.master_volume
    }

    /// Set master volume
    pub fn set_master_volume(&mut self, volume: u32) {
        self.master_volume = volume.min(100);
    }

    /// Get master mute state
    pub fn is_muted(&self) -> bool {
        self.master_muted
    }

    /// Set master mute
    pub fn set_muted(&mut self, muted: bool) {
        self.master_muted = muted;
    }

    /// Toggle master mute
    pub fn toggle_mute(&mut self) -> bool {
        self.master_muted = !self.master_muted;
        self.master_muted
    }

    /// Adjust volume by relative amount
    pub fn adjust_volume(&mut self, delta: i32) {
        let new_volume = (self.master_volume as i32 + delta).max(0).min(100) as u32;
        self.master_volume = new_volume;
    }
}

/// Soft clipping to avoid harsh distortion
fn soft_clip(sample: f32) -> f32 {
    if sample > 1.0 {
        1.0 - (-(sample - 1.0)).exp() * 0.5
    } else if sample < -1.0 {
        -1.0 + (-(-sample - 1.0)).exp() * 0.5
    } else {
        sample
    }
}

/// Volume curve types
#[derive(Debug, Clone, Copy)]
pub enum VolumeCurve {
    /// Linear volume (not recommended)
    Linear,
    /// Logarithmic (perceptually linear)
    Logarithmic,
    /// Cubic curve
    Cubic,
}

impl VolumeCurve {
    /// Apply curve to volume percentage
    pub fn apply(&self, volume: u32) -> f32 {
        let normalized = volume as f32 / 100.0;

        match self {
            VolumeCurve::Linear => normalized,
            VolumeCurve::Logarithmic => {
                if normalized == 0.0 {
                    0.0
                } else {
                    // dB scale
                    10.0f32.powf((normalized - 1.0) * 2.0)
                }
            }
            VolumeCurve::Cubic => normalized * normalized * normalized,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_clip() {
        assert!((soft_clip(0.5) - 0.5).abs() < 0.001);
        assert!(soft_clip(1.5) < 1.0);
        assert!(soft_clip(-1.5) > -1.0);
    }
}
