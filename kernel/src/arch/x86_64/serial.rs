//! Serial console driver (UART 16550)
//!
//! Provides basic serial I/O for debugging and console output.
//! Implements the standard 16550 UART found in x86 systems.

use core::arch::asm;
use core::fmt::{self, Write};
use spin::Mutex;

/// Standard COM port addresses
pub mod ports {
    pub const COM1: u16 = 0x3F8;
    pub const COM2: u16 = 0x2F8;
    pub const COM3: u16 = 0x3E8;
    pub const COM4: u16 = 0x2E8;
}

/// UART register offsets
mod regs {
    /// Data register (R/W) - also transmit/receive buffer
    pub const DATA: u16 = 0;
    /// Interrupt enable register
    pub const IER: u16 = 1;
    /// FIFO control register (write) / interrupt ID (read)
    pub const FCR_IIR: u16 = 2;
    /// Line control register
    pub const LCR: u16 = 3;
    /// Modem control register
    pub const MCR: u16 = 4;
    /// Line status register
    pub const LSR: u16 = 5;
    /// Modem status register
    pub const MSR: u16 = 6;
    /// Scratch register
    pub const SCRATCH: u16 = 7;

    // When DLAB (Divisor Latch Access Bit) is set:
    /// Divisor latch low byte (same offset as DATA)
    pub const DLL: u16 = 0;
    /// Divisor latch high byte (same offset as IER)
    pub const DLH: u16 = 1;
}

/// Line status register bits
mod lsr {
    /// Data ready (receiver buffer has data)
    pub const DATA_READY: u8 = 1 << 0;
    /// Overrun error
    pub const OVERRUN_ERR: u8 = 1 << 1;
    /// Parity error
    pub const PARITY_ERR: u8 = 1 << 2;
    /// Framing error
    pub const FRAMING_ERR: u8 = 1 << 3;
    /// Break indicator
    pub const BREAK_IND: u8 = 1 << 4;
    /// Transmitter holding register empty (can send)
    pub const THR_EMPTY: u8 = 1 << 5;
    /// Transmitter empty (all data sent)
    pub const TX_EMPTY: u8 = 1 << 6;
    /// FIFO error
    pub const FIFO_ERR: u8 = 1 << 7;
}

/// Line control register bits
mod lcr {
    /// 5 data bits
    pub const DATA_5: u8 = 0b00;
    /// 6 data bits
    pub const DATA_6: u8 = 0b01;
    /// 7 data bits
    pub const DATA_7: u8 = 0b10;
    /// 8 data bits
    pub const DATA_8: u8 = 0b11;
    /// 2 stop bits (or 1.5 for 5 data bits)
    pub const STOP_2: u8 = 1 << 2;
    /// Parity enable
    pub const PARITY_EN: u8 = 1 << 3;
    /// Even parity
    pub const PARITY_EVEN: u8 = 1 << 4;
    /// Stick parity
    pub const PARITY_STICK: u8 = 1 << 5;
    /// Set break
    pub const SET_BREAK: u8 = 1 << 6;
    /// Divisor latch access bit
    pub const DLAB: u8 = 1 << 7;
}

/// FIFO control register bits
mod fcr {
    /// Enable FIFOs
    pub const ENABLE: u8 = 1 << 0;
    /// Clear receive FIFO
    pub const CLR_RX: u8 = 1 << 1;
    /// Clear transmit FIFO
    pub const CLR_TX: u8 = 1 << 2;
    /// DMA mode select
    pub const DMA_MODE: u8 = 1 << 3;
    /// 64-byte FIFO enable (16750)
    pub const FIFO_64: u8 = 1 << 5;
    /// Trigger level: 1 byte
    pub const TRIGGER_1: u8 = 0b00 << 6;
    /// Trigger level: 4 bytes
    pub const TRIGGER_4: u8 = 0b01 << 6;
    /// Trigger level: 8 bytes
    pub const TRIGGER_8: u8 = 0b10 << 6;
    /// Trigger level: 14 bytes
    pub const TRIGGER_14: u8 = 0b11 << 6;
}

/// Modem control register bits
mod mcr {
    /// Data terminal ready
    pub const DTR: u8 = 1 << 0;
    /// Request to send
    pub const RTS: u8 = 1 << 1;
    /// Auxiliary output 1
    pub const OUT1: u8 = 1 << 2;
    /// Auxiliary output 2 (enables IRQ)
    pub const OUT2: u8 = 1 << 3;
    /// Loopback mode
    pub const LOOPBACK: u8 = 1 << 4;
}

/// Baud rate divisors (for 115200 base clock)
mod baud {
    pub const B115200: u16 = 1;
    pub const B57600: u16 = 2;
    pub const B38400: u16 = 3;
    pub const B19200: u16 = 6;
    pub const B9600: u16 = 12;
    pub const B4800: u16 = 24;
    pub const B2400: u16 = 48;
    pub const B1200: u16 = 96;
}

/// Serial port driver
pub struct SerialPort {
    /// Base I/O port address
    base: u16,
    /// Port initialized flag
    initialized: bool,
}

impl SerialPort {
    /// Create a new serial port (uninitialized)
    pub const fn new(base: u16) -> Self {
        Self {
            base,
            initialized: false,
        }
    }

    /// Initialize the serial port
    pub fn init(&mut self) -> Result<(), SerialError> {
        unsafe {
            // Disable interrupts
            self.write_reg(regs::IER, 0x00);

            // Enable DLAB to set baud rate
            self.write_reg(regs::LCR, lcr::DLAB);

            // Set baud rate to 115200
            self.write_reg(regs::DLL, (baud::B115200 & 0xFF) as u8);
            self.write_reg(regs::DLH, (baud::B115200 >> 8) as u8);

            // 8 data bits, 1 stop bit, no parity
            self.write_reg(regs::LCR, lcr::DATA_8);

            // Enable FIFO, clear buffers, 14-byte trigger
            self.write_reg(
                regs::FCR_IIR,
                fcr::ENABLE | fcr::CLR_RX | fcr::CLR_TX | fcr::TRIGGER_14,
            );

            // DTR + RTS + OUT2 (enables IRQ)
            self.write_reg(regs::MCR, mcr::DTR | mcr::RTS | mcr::OUT2);

            // Test serial chip (loopback mode)
            self.write_reg(regs::MCR, mcr::LOOPBACK | mcr::DTR | mcr::RTS | mcr::OUT1 | mcr::OUT2);

            // Send test byte
            self.write_reg(regs::DATA, 0xAE);

            // Check if we receive the same byte
            if self.read_reg(regs::DATA) != 0xAE {
                return Err(SerialError::LoopbackFailed);
            }

            // Exit loopback mode, set normal operation
            self.write_reg(regs::MCR, mcr::DTR | mcr::RTS | mcr::OUT2);

            self.initialized = true;
        }

        Ok(())
    }

    /// Check if serial port is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Write a byte to the serial port (blocking)
    pub fn write_byte(&self, byte: u8) {
        if !self.initialized {
            return;
        }

        unsafe {
            // Wait for transmitter to be ready
            while (self.read_reg(regs::LSR) & lsr::THR_EMPTY) == 0 {
                core::hint::spin_loop();
            }

            self.write_reg(regs::DATA, byte);
        }
    }

    /// Read a byte from the serial port (blocking)
    pub fn read_byte(&self) -> u8 {
        if !self.initialized {
            return 0;
        }

        unsafe {
            // Wait for data to be available
            while (self.read_reg(regs::LSR) & lsr::DATA_READY) == 0 {
                core::hint::spin_loop();
            }

            self.read_reg(regs::DATA)
        }
    }

    /// Try to read a byte without blocking
    pub fn try_read_byte(&self) -> Option<u8> {
        if !self.initialized {
            return None;
        }

        unsafe {
            if (self.read_reg(regs::LSR) & lsr::DATA_READY) != 0 {
                Some(self.read_reg(regs::DATA))
            } else {
                None
            }
        }
    }

    /// Check if data is available to read
    pub fn data_available(&self) -> bool {
        if !self.initialized {
            return false;
        }

        unsafe { (self.read_reg(regs::LSR) & lsr::DATA_READY) != 0 }
    }

    /// Check if transmitter is ready to send
    pub fn can_send(&self) -> bool {
        if !self.initialized {
            return false;
        }

        unsafe { (self.read_reg(regs::LSR) & lsr::THR_EMPTY) != 0 }
    }

    /// Write a string to the serial port
    pub fn write_str(&self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
    }

    /// Enable receive interrupt
    pub fn enable_rx_interrupt(&self) {
        if !self.initialized {
            return;
        }

        unsafe {
            let ier = self.read_reg(regs::IER);
            self.write_reg(regs::IER, ier | 0x01);
        }
    }

    /// Disable receive interrupt
    pub fn disable_rx_interrupt(&self) {
        if !self.initialized {
            return;
        }

        unsafe {
            let ier = self.read_reg(regs::IER);
            self.write_reg(regs::IER, ier & !0x01);
        }
    }

    /// Read from a register
    #[inline]
    unsafe fn read_reg(&self, offset: u16) -> u8 {
        // SAFETY: Caller ensures valid port offset
        unsafe { inb(self.base + offset) }
    }

    /// Write to a register
    #[inline]
    unsafe fn write_reg(&self, offset: u16, value: u8) {
        // SAFETY: Caller ensures valid port offset and value
        unsafe { outb(self.base + offset, value); }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        SerialPort::write_str(self, s);
        Ok(())
    }
}

/// Serial port errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialError {
    /// Loopback test failed (port not present)
    LoopbackFailed,
    /// Port not initialized
    NotInitialized,
}

/// Global serial console (COM1)
static SERIAL1: Mutex<SerialPort> = Mutex::new(SerialPort::new(ports::COM1));

/// Initialize the serial console
pub fn init() {
    let mut serial = SERIAL1.lock();
    if let Err(e) = serial.init() {
        // Can't log here since logging isn't set up yet
        // But at least we tried
        let _ = e;
    }
}

/// Write a string to the serial console
pub fn write_str(s: &str) {
    SERIAL1.lock().write_str(s);
}

/// Write a formatted string to the serial console
pub fn write_fmt(args: fmt::Arguments) {
    use core::fmt::Write;
    let _ = SERIAL1.lock().write_fmt(args);
}

/// Read a byte from the serial console (blocking)
pub fn read_byte() -> u8 {
    SERIAL1.lock().read_byte()
}

/// Try to read a byte from the serial console (non-blocking)
pub fn try_read_byte() -> Option<u8> {
    SERIAL1.lock().try_read_byte()
}

/// Print macro for serial output
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::arch::x86_64::serial::write_fmt(format_args!($($arg)*))
    };
}

/// Println macro for serial output
#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::serial_print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::serial_print!("{}\n", format_args!($($arg)*))
    };
}

// ============================================================================
// Log backend implementation
// ============================================================================

/// Logger that outputs to serial console
struct SerialLogger;

impl log::Log for SerialLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Trace
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let level_str = match record.level() {
                log::Level::Error => "\x1b[31mERROR\x1b[0m",
                log::Level::Warn => "\x1b[33mWARN \x1b[0m",
                log::Level::Info => "\x1b[32mINFO \x1b[0m",
                log::Level::Debug => "\x1b[34mDEBUG\x1b[0m",
                log::Level::Trace => "\x1b[90mTRACE\x1b[0m",
            };

            write_fmt(format_args!(
                "[{}] {}: {}\n",
                level_str,
                record.target(),
                record.args()
            ));
        }
    }

    fn flush(&self) {}
}

static LOGGER: SerialLogger = SerialLogger;

/// Initialize logging to serial console
pub fn init_logging() {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(log::LevelFilter::Trace))
        .expect("Failed to set logger");
}

// ============================================================================
// Port I/O helpers
// ============================================================================

/// Read byte from I/O port
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: Caller ensures valid port address
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nostack, preserves_flags)
        );
    }
    value
}

/// Write byte to I/O port
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    // SAFETY: Caller ensures valid port address
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nostack, preserves_flags)
        );
    }
}
