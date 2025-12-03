//! Time management subsystem
//!
//! Provides time-related functionality including:
//! - System uptime tracking
//! - Wall clock time (via RTC)
//! - Timer management for sleep and alarms

use core::sync::atomic::{AtomicU64, Ordering};

/// System boot timestamp (set from RTC during init)
static BOOT_TIMESTAMP: AtomicU64 = AtomicU64::new(0);

/// Ticks since boot (incremented by timer interrupt)
static TICKS_SINCE_BOOT: AtomicU64 = AtomicU64::new(0);

/// Timer frequency in Hz
const TIMER_FREQ_HZ: u64 = 1000;

/// Initialize the time subsystem
pub fn init() {
    log::info!("Initializing time subsystem");

    // Read initial time from RTC
    if let Some(rtc_time) = read_rtc() {
        BOOT_TIMESTAMP.store(rtc_time, Ordering::SeqCst);
        log::info!("RTC time: {} (Unix timestamp)", rtc_time);
    } else {
        log::warn!("Could not read RTC, using epoch as boot time");
    }
}

/// Get current Unix timestamp (seconds since 1970-01-01 00:00:00 UTC)
pub fn get_unix_timestamp() -> Option<u64> {
    let boot_ts = BOOT_TIMESTAMP.load(Ordering::Relaxed);
    let ticks = TICKS_SINCE_BOOT.load(Ordering::Relaxed);
    let elapsed_secs = ticks / TIMER_FREQ_HZ;

    Some(boot_ts + elapsed_secs)
}

/// Get system uptime in milliseconds
pub fn uptime_ms() -> u64 {
    let ticks = TICKS_SINCE_BOOT.load(Ordering::Relaxed);
    ticks * 1000 / TIMER_FREQ_HZ
}

/// Get system uptime in seconds
pub fn uptime_secs() -> u64 {
    TICKS_SINCE_BOOT.load(Ordering::Relaxed) / TIMER_FREQ_HZ
}

/// Called by timer interrupt handler to increment tick count
pub fn tick() {
    TICKS_SINCE_BOOT.fetch_add(1, Ordering::Relaxed);
}

/// Set the boot timestamp (called during RTC initialization)
pub fn set_boot_timestamp(timestamp: u64) {
    BOOT_TIMESTAMP.store(timestamp, Ordering::SeqCst);
}

/// Read time from RTC hardware
fn read_rtc() -> Option<u64> {
    // Read CMOS RTC registers
    // Port 0x70 = index port, 0x71 = data port

    unsafe {
        // Wait for RTC update to complete
        loop {
            outb(0x70, 0x0A);
            if (inb(0x71) & 0x80) == 0 {
                break;
            }
        }

        // Read time registers
        let second = read_cmos_reg(0x00);
        let minute = read_cmos_reg(0x02);
        let hour = read_cmos_reg(0x04);
        let day = read_cmos_reg(0x07);
        let month = read_cmos_reg(0x08);
        let year = read_cmos_reg(0x09);

        // Check if BCD mode (bit 2 of status register B)
        outb(0x70, 0x0B);
        let status_b = inb(0x71);
        let is_binary = (status_b & 0x04) != 0;

        // Convert from BCD if needed
        let (second, minute, hour, day, month, year) = if is_binary {
            (second, minute, hour, day, month, year)
        } else {
            (
                bcd_to_binary(second),
                bcd_to_binary(minute),
                bcd_to_binary(hour),
                bcd_to_binary(day),
                bcd_to_binary(month),
                bcd_to_binary(year),
            )
        };

        // Assume 21st century
        let full_year = 2000 + year as u32;

        // Convert to Unix timestamp
        Some(datetime_to_unix(full_year, month, day, hour, minute, second))
    }
}

/// Read a CMOS register
unsafe fn read_cmos_reg(reg: u8) -> u8 {
    unsafe {
        outb(0x70, reg);
        inb(0x71)
    }
}

/// Convert BCD to binary
fn bcd_to_binary(bcd: u8) -> u8 {
    ((bcd >> 4) * 10) + (bcd & 0x0F)
}

/// Convert date/time to Unix timestamp
fn datetime_to_unix(year: u32, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> u64 {
    // Days from year 1970 to start of each month (non-leap year)
    const DAYS_BEFORE_MONTH: [u16; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

    let mut days: u64 = 0;

    // Add days for years since 1970
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Add days for months
    if month > 0 && month <= 12 {
        days += DAYS_BEFORE_MONTH[(month - 1) as usize] as u64;

        // Add leap day if past February in a leap year
        if month > 2 && is_leap_year(year) {
            days += 1;
        }
    }

    // Add days (1-indexed)
    if day > 0 {
        days += (day - 1) as u64;
    }

    // Convert to seconds
    let hours_secs = hour as u64 * 3600;
    let mins_secs = minute as u64 * 60;

    days * 86400 + hours_secs + mins_secs + second as u64
}

/// Check if year is a leap year
fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// I/O port operations
unsafe fn outb(port: u16, value: u8) {
    // SAFETY: Caller ensures valid I/O port access
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nostack, preserves_flags)
        );
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: Caller ensures valid I/O port access
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nostack, preserves_flags)
        );
    }
    value
}
