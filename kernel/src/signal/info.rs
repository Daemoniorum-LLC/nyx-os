//! Signal information (siginfo_t equivalent)

use super::Signal;
use crate::process::ProcessId;

/// Signal information structure
///
/// Equivalent to POSIX siginfo_t
#[derive(Clone, Debug)]
pub struct SigInfo {
    /// Signal number
    pub signo: u8,
    /// Error number (if applicable)
    pub errno: i32,
    /// Signal code (how the signal was generated)
    pub code: SigCode,
    /// Sender's process ID
    pub sender_pid: Option<ProcessId>,
    /// Sender's user ID
    pub sender_uid: Option<u32>,
    /// Exit status or signal (for SIGCHLD)
    pub status: i32,
    /// User time consumed (for SIGCHLD)
    pub utime: u64,
    /// System time consumed (for SIGCHLD)
    pub stime: u64,
    /// Signal value (for sigqueue)
    pub value: SigVal,
    /// Fault address (for SIGSEGV, SIGBUS, etc.)
    pub addr: Option<u64>,
    /// Address LSB (for SIGBUS)
    pub addr_lsb: u16,
    /// Timer ID (for SIGALRM, etc.)
    pub timerid: i32,
    /// Timer overrun count
    pub overrun: i32,
    /// File descriptor (for SIGIO)
    pub fd: Option<i32>,
    /// Band event (for SIGIO)
    pub band: i64,
}

impl SigInfo {
    /// Create a new SigInfo for a signal
    pub fn new(signal: Signal) -> Self {
        Self {
            signo: signal.as_raw(),
            errno: 0,
            code: SigCode::User,
            sender_pid: None,
            sender_uid: None,
            status: 0,
            utime: 0,
            stime: 0,
            value: SigVal::Int(0),
            addr: None,
            addr_lsb: 0,
            timerid: 0,
            overrun: 0,
            fd: None,
            band: 0,
        }
    }

    /// Create for a kernel-generated signal
    pub fn kernel(signal: Signal, code: SigCode) -> Self {
        Self {
            signo: signal.as_raw(),
            code,
            ..Self::new(signal)
        }
    }

    /// Create for a fault signal
    pub fn fault(signal: Signal, addr: u64) -> Self {
        let code = match signal {
            Signal::SIGSEGV => SigCode::SegvMapErr,
            Signal::SIGBUS => SigCode::BusAddrErr,
            Signal::SIGFPE => SigCode::FpeDivZero,
            Signal::SIGILL => SigCode::IllOp,
            _ => SigCode::Kernel,
        };

        Self {
            signo: signal.as_raw(),
            code,
            addr: Some(addr),
            ..Self::new(signal)
        }
    }

    /// Builder: set sender
    pub fn with_sender(mut self, pid: Option<ProcessId>) -> Self {
        self.sender_pid = pid;
        self.code = SigCode::User;
        self
    }

    /// Builder: set value
    pub fn with_value(mut self, value: i64) -> Self {
        self.value = SigVal::Int(value);
        self.code = SigCode::Queue;
        self
    }

    /// Builder: set fault address
    pub fn with_addr(mut self, addr: u64) -> Self {
        self.addr = Some(addr);
        self
    }

    /// Builder: set error number
    pub fn with_errno(mut self, errno: i32) -> Self {
        self.errno = errno;
        self
    }

    /// Get the signal
    pub fn signal(&self) -> Option<Signal> {
        Signal::from_raw(self.signo)
    }
}

/// Signal code (how the signal was generated)
///
/// Note: In POSIX, signal codes are specific to each signal type,
/// but we use unique values here to satisfy Rust's enum requirements.
/// The raw POSIX values can be obtained via `posix_value()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum SigCode {
    /// Sent by user (kill, raise, etc.)
    #[default]
    User = 0,
    /// Sent by kernel
    Kernel = 0x80,
    /// Sent by sigqueue
    Queue = -1,
    /// Sent by timer expiration
    Timer = -2,
    /// Sent by message queue
    MesgQ = -3,
    /// Sent by async I/O completion
    AsyncIO = -4,
    /// Sent by SIGIO
    SigIO = -5,

    // SIGCHLD codes (offset 0x100)
    /// Child exited
    ChldExited = 0x101,
    /// Child killed
    ChldKilled = 0x102,
    /// Child dumped core
    ChldDumped = 0x103,
    /// Child trapped
    ChldTrapped = 0x104,
    /// Child stopped
    ChldStopped = 0x105,
    /// Child continued
    ChldContinued = 0x106,

    // SIGSEGV codes (offset 0x200)
    /// Address not mapped
    SegvMapErr = 0x201,
    /// Invalid permissions
    SegvAccErr = 0x202,

    // SIGBUS codes (offset 0x300)
    /// Invalid address alignment
    BusAddrErr = 0x301,
    /// Non-existent physical address
    BusObjErr = 0x302,
    /// Object-specific hardware error
    BusMcErr = 0x303,

    // SIGILL codes (offset 0x400)
    /// Illegal opcode
    IllOp = 0x401,
    /// Illegal operand
    IllOperand = 0x402,
    /// Illegal addressing mode
    IllAddr = 0x403,
    /// Illegal trap
    IllTrap = 0x404,
    /// Privileged opcode
    IllPriv = 0x405,
    /// Coprocessor error
    IllCoproc = 0x406,

    // SIGFPE codes (offset 0x500)
    /// Integer divide by zero
    FpeDivZero = 0x501,
    /// Integer overflow
    FpeIntOvf = 0x502,
    /// Floating-point divide by zero
    FpeFltDiv = 0x503,
    /// Floating-point overflow
    FpeFltOvf = 0x504,
    /// Floating-point underflow
    FpeFltUnd = 0x505,
    /// Floating-point inexact result
    FpeFltRes = 0x506,
    /// Invalid floating-point operation
    FpeFltInv = 0x507,
    /// Subscript out of range
    FpeFltSub = 0x508,

    // SIGTRAP codes (offset 0x600)
    /// Process breakpoint
    TrapBrkpt = 0x601,
    /// Process trace trap
    TrapTrace = 0x602,
}

impl SigCode {
    /// Get the POSIX si_code value (1-based within signal type)
    pub fn posix_value(&self) -> i32 {
        match *self as i32 {
            v if v < 0 => v,
            v if v < 0x100 => v,
            v => (v & 0xFF) as i32,
        }
    }
}

/// Signal value (for sigqueue)
#[derive(Clone, Copy, Debug)]
pub enum SigVal {
    /// Integer value
    Int(i64),
    /// Pointer value
    Ptr(u64),
}

impl SigVal {
    /// Get as integer
    pub fn as_int(&self) -> i64 {
        match self {
            SigVal::Int(v) => *v,
            SigVal::Ptr(v) => *v as i64,
        }
    }

    /// Get as pointer
    pub fn as_ptr(&self) -> u64 {
        match self {
            SigVal::Int(v) => *v as u64,
            SigVal::Ptr(v) => *v,
        }
    }
}

impl Default for SigVal {
    fn default() -> Self {
        SigVal::Int(0)
    }
}
