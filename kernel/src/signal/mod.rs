//! # Signal Delivery Subsystem
//!
//! POSIX-compatible signal handling for the Nyx microkernel.
//!
//! ## Design
//!
//! Unlike traditional Unix kernels where signals interrupt execution directly,
//! Nyx uses a capability-based signal delivery model:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      Signal Sources                          │
//! │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────────────┐ │
//! │  │Terminal │  │ Timer   │  │ Fault   │  │ Process (kill)  │ │
//! │  └────┬────┘  └────┬────┘  └────┬────┘  └────────┬────────┘ │
//! ├───────┴────────────┴───────────┴────────────────┴───────────┤
//! │                    Signal Queue (per-thread)                 │
//! ├─────────────────────────────────────────────────────────────┤
//! │                    Signal Handler Dispatch                   │
//! │  ┌─────────────────────────────────────────────────────┐    │
//! │  │  1. Check signal mask                                │    │
//! │  │  2. Look up handler (default/ignore/custom)          │    │
//! │  │  3. If custom: push signal frame, transfer control   │    │
//! │  │  4. On sigreturn: restore context                    │    │
//! │  └─────────────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────────────┘
//! ```

mod action;
mod delivery;
mod info;
mod queue;
mod set;

pub use action::{SigAction, SigHandler};
pub use delivery::{deliver_signal, check_pending_signals};
pub use info::SigInfo;
pub use queue::SignalQueue;
pub use set::SigSet;

use crate::process::ProcessId;
use crate::sched::ThreadId;
use alloc::collections::BTreeMap;
use spin::RwLock;

/// Maximum number of real-time signals
pub const SIGRTMAX: u8 = 64;
/// First real-time signal number
pub const SIGRTMIN: u8 = 32;

/// Standard signal numbers (POSIX)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Signal {
    /// Hangup
    SIGHUP = 1,
    /// Interrupt (Ctrl+C)
    SIGINT = 2,
    /// Quit (Ctrl+\)
    SIGQUIT = 3,
    /// Illegal instruction
    SIGILL = 4,
    /// Trace/breakpoint trap
    SIGTRAP = 5,
    /// Abort
    SIGABRT = 6,
    /// Bus error
    SIGBUS = 7,
    /// Floating point exception
    SIGFPE = 8,
    /// Kill (cannot be caught)
    SIGKILL = 9,
    /// User-defined signal 1
    SIGUSR1 = 10,
    /// Segmentation fault
    SIGSEGV = 11,
    /// User-defined signal 2
    SIGUSR2 = 12,
    /// Broken pipe
    SIGPIPE = 13,
    /// Alarm clock
    SIGALRM = 14,
    /// Termination
    SIGTERM = 15,
    /// Stack fault
    SIGSTKFLT = 16,
    /// Child status changed
    SIGCHLD = 17,
    /// Continue
    SIGCONT = 18,
    /// Stop (cannot be caught)
    SIGSTOP = 19,
    /// Terminal stop (Ctrl+Z)
    SIGTSTP = 20,
    /// Background read from tty
    SIGTTIN = 21,
    /// Background write to tty
    SIGTTOU = 22,
    /// Urgent data available
    SIGURG = 23,
    /// CPU time limit exceeded
    SIGXCPU = 24,
    /// File size limit exceeded
    SIGXFSZ = 25,
    /// Virtual timer expired
    SIGVTALRM = 26,
    /// Profiling timer expired
    SIGPROF = 27,
    /// Window size changed
    SIGWINCH = 28,
    /// I/O possible
    SIGIO = 29,
    /// Power failure
    SIGPWR = 30,
    /// Bad system call
    SIGSYS = 31,
}

impl Signal {
    /// Create from raw signal number
    pub fn from_raw(signum: u8) -> Option<Self> {
        match signum {
            1 => Some(Signal::SIGHUP),
            2 => Some(Signal::SIGINT),
            3 => Some(Signal::SIGQUIT),
            4 => Some(Signal::SIGILL),
            5 => Some(Signal::SIGTRAP),
            6 => Some(Signal::SIGABRT),
            7 => Some(Signal::SIGBUS),
            8 => Some(Signal::SIGFPE),
            9 => Some(Signal::SIGKILL),
            10 => Some(Signal::SIGUSR1),
            11 => Some(Signal::SIGSEGV),
            12 => Some(Signal::SIGUSR2),
            13 => Some(Signal::SIGPIPE),
            14 => Some(Signal::SIGALRM),
            15 => Some(Signal::SIGTERM),
            16 => Some(Signal::SIGSTKFLT),
            17 => Some(Signal::SIGCHLD),
            18 => Some(Signal::SIGCONT),
            19 => Some(Signal::SIGSTOP),
            20 => Some(Signal::SIGTSTP),
            21 => Some(Signal::SIGTTIN),
            22 => Some(Signal::SIGTTOU),
            23 => Some(Signal::SIGURG),
            24 => Some(Signal::SIGXCPU),
            25 => Some(Signal::SIGXFSZ),
            26 => Some(Signal::SIGVTALRM),
            27 => Some(Signal::SIGPROF),
            28 => Some(Signal::SIGWINCH),
            29 => Some(Signal::SIGIO),
            30 => Some(Signal::SIGPWR),
            31 => Some(Signal::SIGSYS),
            _ => None,
        }
    }

    /// Get raw signal number
    pub fn as_raw(self) -> u8 {
        self as u8
    }

    /// Get default action for this signal
    pub fn default_action(self) -> DefaultAction {
        match self {
            // Terminate with core dump
            Signal::SIGQUIT | Signal::SIGILL | Signal::SIGTRAP |
            Signal::SIGABRT | Signal::SIGBUS | Signal::SIGFPE |
            Signal::SIGSEGV | Signal::SIGSTKFLT | Signal::SIGXCPU |
            Signal::SIGXFSZ | Signal::SIGSYS => DefaultAction::CoreDump,

            // Terminate
            Signal::SIGHUP | Signal::SIGINT | Signal::SIGKILL |
            Signal::SIGPIPE | Signal::SIGALRM | Signal::SIGTERM |
            Signal::SIGUSR1 | Signal::SIGUSR2 | Signal::SIGPWR |
            Signal::SIGIO => DefaultAction::Terminate,

            // Stop
            Signal::SIGSTOP | Signal::SIGTSTP | Signal::SIGTTIN |
            Signal::SIGTTOU => DefaultAction::Stop,

            // Continue
            Signal::SIGCONT => DefaultAction::Continue,

            // Ignore
            Signal::SIGCHLD | Signal::SIGURG | Signal::SIGWINCH |
            Signal::SIGVTALRM | Signal::SIGPROF => DefaultAction::Ignore,
        }
    }

    /// Check if signal can be caught or ignored
    pub fn is_catchable(self) -> bool {
        !matches!(self, Signal::SIGKILL | Signal::SIGSTOP)
    }

    /// Check if this is a synchronous signal (caused by process itself)
    pub fn is_synchronous(self) -> bool {
        matches!(
            self,
            Signal::SIGILL | Signal::SIGTRAP | Signal::SIGBUS |
            Signal::SIGFPE | Signal::SIGSEGV | Signal::SIGSYS
        )
    }
}

/// Default signal action
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultAction {
    /// Terminate the process
    Terminate,
    /// Terminate with core dump
    CoreDump,
    /// Ignore the signal
    Ignore,
    /// Stop the process
    Stop,
    /// Continue the process
    Continue,
}

/// Signal error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalError {
    /// Invalid signal number
    InvalidSignal,
    /// Process not found
    ProcessNotFound,
    /// Thread not found
    ThreadNotFound,
    /// Permission denied
    PermissionDenied,
    /// Signal cannot be caught or ignored
    Uncatchable,
    /// Queue full
    QueueFull,
}

/// Per-process signal state
static PROCESS_SIGNALS: RwLock<BTreeMap<ProcessId, ProcessSignalState>> =
    RwLock::new(BTreeMap::new());

/// Per-thread signal state
static THREAD_SIGNALS: RwLock<BTreeMap<ThreadId, ThreadSignalState>> =
    RwLock::new(BTreeMap::new());

/// Process-level signal state
#[derive(Clone, Debug)]
pub struct ProcessSignalState {
    /// Signal actions (handlers)
    pub actions: [SigAction; 64],
    /// Pending signals (process-wide)
    pub pending: SignalQueue,
}

impl Default for ProcessSignalState {
    fn default() -> Self {
        Self {
            actions: core::array::from_fn(|_| SigAction::default()),
            pending: SignalQueue::new(),
        }
    }
}

/// Thread-level signal state
#[derive(Clone, Debug)]
pub struct ThreadSignalState {
    /// Signal mask (blocked signals)
    pub mask: SigSet,
    /// Pending signals (thread-specific)
    pub pending: SignalQueue,
    /// Alternate signal stack
    pub alt_stack: Option<AltStack>,
    /// Currently being handled signal
    pub handling: Option<u8>,
    /// Saved signal mask during handler
    pub saved_mask: Option<SigSet>,
}

impl Default for ThreadSignalState {
    fn default() -> Self {
        Self {
            mask: SigSet::empty(),
            pending: SignalQueue::new(),
            alt_stack: None,
            handling: None,
            saved_mask: None,
        }
    }
}

/// Alternate signal stack configuration
#[derive(Clone, Debug)]
pub struct AltStack {
    /// Stack base address
    pub base: u64,
    /// Stack size
    pub size: u64,
    /// Stack flags
    pub flags: AltStackFlags,
}

bitflags::bitflags! {
    /// Alternate stack flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct AltStackFlags: u32 {
        /// Currently executing on alt stack
        const ONSTACK = 1 << 0;
        /// Disable alt stack
        const DISABLE = 1 << 1;
        /// Auto-disarm after signal delivery
        const AUTODISARM = 1 << 2;
    }
}

/// Initialize signal subsystem
pub fn init() {
    log::info!("Initializing signal subsystem");
}

/// Initialize signal state for a new process
pub fn init_process(pid: ProcessId) {
    PROCESS_SIGNALS.write().insert(pid, ProcessSignalState::default());
}

/// Initialize signal state for a new thread
pub fn init_thread(tid: ThreadId) {
    THREAD_SIGNALS.write().insert(tid, ThreadSignalState::default());
}

/// Clean up signal state when process exits
pub fn cleanup_process(pid: ProcessId) {
    PROCESS_SIGNALS.write().remove(&pid);
}

/// Clean up signal state when thread exits
pub fn cleanup_thread(tid: ThreadId) {
    THREAD_SIGNALS.write().remove(&tid);
}

// ============================================================================
// Signal Sending
// ============================================================================

/// Send a signal to a process
pub fn kill(pid: ProcessId, signal: Signal) -> Result<(), SignalError> {
    let mut processes = PROCESS_SIGNALS.write();
    let state = processes
        .get_mut(&pid)
        .ok_or(SignalError::ProcessNotFound)?;

    let info = SigInfo::new(signal)
        .with_sender(crate::process::current_pid());

    state.pending.enqueue(signal.as_raw(), info)?;

    log::debug!("Queued signal {:?} for process {:?}", signal, pid);

    // Wake up a thread to handle the signal
    delivery::wake_for_signal(pid)?;

    Ok(())
}

/// Send a signal to a specific thread
pub fn tkill(tid: ThreadId, signal: Signal) -> Result<(), SignalError> {
    let mut threads = THREAD_SIGNALS.write();
    let state = threads
        .get_mut(&tid)
        .ok_or(SignalError::ThreadNotFound)?;

    let info = SigInfo::new(signal)
        .with_sender(crate::process::current_pid());

    state.pending.enqueue(signal.as_raw(), info)?;

    log::debug!("Queued signal {:?} for thread {:?}", signal, tid);

    // Wake the specific thread
    delivery::wake_thread_for_signal(tid)?;

    Ok(())
}

/// Send a signal with additional info
pub fn sigqueue(pid: ProcessId, signal: Signal, value: i64) -> Result<(), SignalError> {
    let mut processes = PROCESS_SIGNALS.write();
    let state = processes
        .get_mut(&pid)
        .ok_or(SignalError::ProcessNotFound)?;

    let info = SigInfo::new(signal)
        .with_sender(crate::process::current_pid())
        .with_value(value);

    state.pending.enqueue(signal.as_raw(), info)?;

    delivery::wake_for_signal(pid)?;

    Ok(())
}

// ============================================================================
// Signal Actions
// ============================================================================

/// Set signal action
pub fn sigaction(
    pid: ProcessId,
    signal: Signal,
    action: SigAction,
    old_action: Option<&mut SigAction>,
) -> Result<(), SignalError> {
    if !signal.is_catchable() && !matches!(action.handler, SigHandler::Default) {
        return Err(SignalError::Uncatchable);
    }

    let mut processes = PROCESS_SIGNALS.write();
    let state = processes
        .get_mut(&pid)
        .ok_or(SignalError::ProcessNotFound)?;

    let idx = signal.as_raw() as usize;
    if idx >= state.actions.len() {
        return Err(SignalError::InvalidSignal);
    }

    if let Some(old) = old_action {
        *old = state.actions[idx].clone();
    }

    state.actions[idx] = action;

    Ok(())
}

/// Get current signal action
pub fn get_sigaction(pid: ProcessId, signal: Signal) -> Result<SigAction, SignalError> {
    let processes = PROCESS_SIGNALS.read();
    let state = processes
        .get(&pid)
        .ok_or(SignalError::ProcessNotFound)?;

    let idx = signal.as_raw() as usize;
    if idx >= state.actions.len() {
        return Err(SignalError::InvalidSignal);
    }

    Ok(state.actions[idx].clone())
}

// ============================================================================
// Signal Masking
// ============================================================================

/// Get/set signal mask for a thread
pub fn sigmask(
    tid: ThreadId,
    how: SigMaskHow,
    set: Option<&SigSet>,
    old_set: Option<&mut SigSet>,
) -> Result<(), SignalError> {
    let mut threads = THREAD_SIGNALS.write();
    let state = threads
        .get_mut(&tid)
        .ok_or(SignalError::ThreadNotFound)?;

    if let Some(old) = old_set {
        *old = state.mask.clone();
    }

    if let Some(new_set) = set {
        match how {
            SigMaskHow::Block => {
                state.mask = state.mask.union(new_set);
            }
            SigMaskHow::Unblock => {
                state.mask = state.mask.difference(new_set);
            }
            SigMaskHow::SetMask => {
                state.mask = new_set.clone();
            }
        }

        // SIGKILL and SIGSTOP cannot be blocked
        state.mask.remove(Signal::SIGKILL.as_raw());
        state.mask.remove(Signal::SIGSTOP.as_raw());
    }

    Ok(())
}

/// Signal mask operation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigMaskHow {
    /// Block signals in set
    Block,
    /// Unblock signals in set
    Unblock,
    /// Set mask to set
    SetMask,
}

/// Get pending signals for a thread
pub fn sigpending(tid: ThreadId, pid: ProcessId) -> Result<SigSet, SignalError> {
    let threads = THREAD_SIGNALS.read();
    let thread_state = threads
        .get(&tid)
        .ok_or(SignalError::ThreadNotFound)?;

    let processes = PROCESS_SIGNALS.read();
    let process_state = processes
        .get(&pid)
        .ok_or(SignalError::ProcessNotFound)?;

    // Combine thread and process pending signals
    let mut pending = thread_state.pending.pending_set();
    pending = pending.union(&process_state.pending.pending_set());

    Ok(pending)
}

// ============================================================================
// Signal Waiting
// ============================================================================

/// Wait for a signal from the specified set
pub fn sigsuspend(tid: ThreadId, mask: &SigSet) -> Result<Signal, SignalError> {
    // Save current mask
    let mut threads = THREAD_SIGNALS.write();
    let state = threads
        .get_mut(&tid)
        .ok_or(SignalError::ThreadNotFound)?;

    let old_mask = state.mask.clone();
    state.mask = mask.clone();
    state.mask.remove(Signal::SIGKILL.as_raw());
    state.mask.remove(Signal::SIGSTOP.as_raw());
    state.saved_mask = Some(old_mask);

    drop(threads);

    // Block until signal arrives
    // (In real implementation, would yield to scheduler)
    Err(SignalError::InvalidSignal)
}

/// Wait for and dequeue a signal from the specified set
pub fn sigwait(tid: ThreadId, set: &SigSet) -> Result<SigInfo, SignalError> {
    // Check thread pending signals
    let mut threads = THREAD_SIGNALS.write();
    let state = threads
        .get_mut(&tid)
        .ok_or(SignalError::ThreadNotFound)?;

    // Try to dequeue a signal from the set
    for signum in 1..64u8 {
        if set.contains(signum) {
            if let Some(info) = state.pending.dequeue(signum) {
                return Ok(info);
            }
        }
    }

    // No signal available, would block
    Err(SignalError::InvalidSignal)
}
