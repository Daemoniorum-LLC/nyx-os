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
    pub fn with_sender(mut self, pid: ProcessId) -> Self {
        self.sender_pid = Some(pid);
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum SigCode {
    /// Sent by user (kill, raise, etc.)
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

    // SIGCHLD codes
    /// Child exited
    ChldExited = 1,
    /// Child killed
    ChldKilled = 2,
    /// Child dumped core
    ChldDumped = 3,
    /// Child trapped
    ChldTrapped = 4,
    /// Child stopped
    ChldStopped = 5,
    /// Child continued
    ChldContinued = 6,

    // SIGSEGV codes
    /// Address not mapped
    SegvMapErr = 1,
    /// Invalid permissions
    SegvAccErr = 2,

    // SIGBUS codes
    /// Invalid address alignment
    BusAddrErr = 1,
    /// Non-existent physical address
    BusObjErr = 2,
    /// Object-specific hardware error
    BusMcErr = 3,

    // SIGILL codes
    /// Illegal opcode
    IllOp = 1,
    /// Illegal operand
    IllOperand = 2,
    /// Illegal addressing mode
    IllAddr = 3,
    /// Illegal trap
    IllTrap = 4,
    /// Privileged opcode
    IllPriv = 5,
    /// Coprocessor error
    IllCoproc = 6,

    // SIGFPE codes
    /// Integer divide by zero
    FpeDivZero = 1,
    /// Integer overflow
    FpeIntOvf = 2,
    /// Floating-point divide by zero
    FpeFltDiv = 3,
    /// Floating-point overflow
    FpeFltOvf = 4,
    /// Floating-point underflow
    FpeFltUnd = 5,
    /// Floating-point inexact result
    FpeFltRes = 6,
    /// Invalid floating-point operation
    FpeFltInv = 7,
    /// Subscript out of range
    FpeFltSub = 8,

    // SIGTRAP codes
    /// Process breakpoint
    TrapBrkpt = 1,
    /// Process trace trap
    TrapTrace = 2,
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
