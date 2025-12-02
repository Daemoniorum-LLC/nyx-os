//! Time functions
//!
//! Functions for getting the current time and working with durations.

use crate::syscall::{self, nr, Error};

/// Get current time in nanoseconds since boot
///
/// This is a monotonic clock that never goes backwards.
///
/// # Example
/// ```no_run
/// let start = now_ns()?;
/// // ... do work ...
/// let elapsed = now_ns()? - start;
/// println!("Elapsed: {} ns", elapsed);
/// ```
pub fn now_ns() -> Result<u64, Error> {
    let result = unsafe { syscall::syscall0(nr::GET_TIME) };
    Error::from_raw(result)
}

/// Get current time in microseconds since boot
pub fn now_us() -> Result<u64, Error> {
    now_ns().map(|ns| ns / 1_000)
}

/// Get current time in milliseconds since boot
pub fn now_ms() -> Result<u64, Error> {
    now_ns().map(|ns| ns / 1_000_000)
}

/// Get current time in seconds since boot (with fractional part)
pub fn now_secs_f64() -> Result<f64, Error> {
    now_ns().map(|ns| ns as f64 / 1_000_000_000.0)
}

/// Duration measurement helper
#[derive(Clone, Copy, Debug)]
pub struct Instant {
    ns: u64,
}

impl Instant {
    /// Capture the current instant
    pub fn now() -> Result<Self, Error> {
        now_ns().map(|ns| Self { ns })
    }

    /// Get elapsed time since this instant in nanoseconds
    pub fn elapsed_ns(&self) -> Result<u64, Error> {
        now_ns().map(|now| now.saturating_sub(self.ns))
    }

    /// Get elapsed time in microseconds
    pub fn elapsed_us(&self) -> Result<u64, Error> {
        self.elapsed_ns().map(|ns| ns / 1_000)
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> Result<u64, Error> {
        self.elapsed_ns().map(|ns| ns / 1_000_000)
    }

    /// Get raw nanosecond value
    pub fn as_ns(&self) -> u64 {
        self.ns
    }
}
