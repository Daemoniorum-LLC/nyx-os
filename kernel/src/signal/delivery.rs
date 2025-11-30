//! Signal delivery to user-space
//!
//! Handles the mechanics of interrupting a thread to run a signal handler.

use super::{
    action::{SigAction, SigActionFlags, SigHandler, SignalDisposition},
    info::SigInfo,
    set::SigSet,
    DefaultAction, ProcessSignalState, Signal, SignalError, ThreadSignalState,
    PROCESS_SIGNALS, THREAD_SIGNALS,
};
use crate::mem::VirtAddr;
use crate::process::{ProcessId, ThreadId};

/// Signal frame pushed on user stack before calling handler
#[repr(C)]
#[derive(Clone, Debug)]
pub struct SignalFrame {
    /// Return address (points to sigreturn trampoline)
    pub retaddr: u64,
    /// Signal number
    pub signum: u32,
    /// Padding
    pub _pad: u32,
    /// Signal info (if SA_SIGINFO)
    pub info: SigInfoFrame,
    /// User context (saved registers)
    pub context: UContext,
    /// Saved signal mask
    pub saved_mask: u64,
    /// Restorer address
    pub restorer: u64,
}

/// siginfo_t structure for user-space
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct SigInfoFrame {
    pub si_signo: i32,
    pub si_errno: i32,
    pub si_code: i32,
    pub _pad: i32,
    pub si_pid: u32,
    pub si_uid: u32,
    pub si_status: i32,
    pub _pad2: i32,
    pub si_utime: u64,
    pub si_stime: u64,
    pub si_value: i64,
    pub si_addr: u64,
    pub si_band: i64,
    pub si_fd: i32,
    pub _reserved: [u8; 48],
}

impl From<&SigInfo> for SigInfoFrame {
    fn from(info: &SigInfo) -> Self {
        Self {
            si_signo: info.signo as i32,
            si_errno: info.errno,
            si_code: info.code as i32,
            si_pid: info.sender_pid.map(|p| p.raw() as u32).unwrap_or(0),
            si_uid: info.sender_uid.unwrap_or(0),
            si_status: info.status,
            si_utime: info.utime,
            si_stime: info.stime,
            si_value: info.value.as_int(),
            si_addr: info.addr.unwrap_or(0),
            si_band: info.band,
            si_fd: info.fd.unwrap_or(-1),
            ..Default::default()
        }
    }
}

/// User context (saved state for sigreturn)
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct UContext {
    pub uc_flags: u64,
    pub uc_link: u64,
    pub uc_stack: StackT,
    pub uc_mcontext: MContext,
    pub uc_sigmask: u64,
}

/// Stack info
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct StackT {
    pub ss_sp: u64,
    pub ss_flags: i32,
    pub _pad: i32,
    pub ss_size: u64,
}

/// Machine context (saved registers)
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct MContext {
    // x86_64 registers
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rax: u64,
    pub rcx: u64,
    pub rsp: u64,
    pub rip: u64,
    pub rflags: u64,
    // Segment registers
    pub cs: u16,
    pub gs: u16,
    pub fs: u16,
    pub ss: u16,
    pub _pad: u32,
    // Error info
    pub err: u64,
    pub trapno: u64,
    pub oldmask: u64,
    pub cr2: u64,
    // FPU state pointer
    pub fpstate: u64,
    pub _reserved: [u64; 8],
}

/// Check for pending signals and deliver if possible
///
/// Called on:
/// - Return from syscall
/// - Return from interrupt
/// - After unblocking signals
pub fn check_pending_signals(tid: ThreadId, pid: ProcessId) -> Result<Option<u8>, SignalError> {
    // Get thread and process signal state
    let threads = THREAD_SIGNALS.read();
    let thread_state = threads.get(&tid).ok_or(SignalError::ThreadNotFound)?;

    let processes = PROCESS_SIGNALS.read();
    let process_state = processes.get(&pid).ok_or(SignalError::ProcessNotFound)?;

    // Check for unblocked pending signals
    // First check thread-specific, then process-wide
    let mask = &thread_state.mask;

    // Find first unblocked pending signal
    for signum in 1..64u8 {
        if mask.contains(signum) {
            continue;
        }

        let pending = thread_state.pending.is_pending(signum)
            || process_state.pending.is_pending(signum);

        if pending {
            return Ok(Some(signum));
        }
    }

    Ok(None)
}

/// Deliver a signal to a thread
///
/// This sets up the signal frame on the user stack and modifies the thread's
/// saved context to jump to the signal handler.
pub fn deliver_signal(
    tid: ThreadId,
    pid: ProcessId,
    signum: u8,
) -> Result<(), SignalError> {
    let signal = Signal::from_raw(signum).ok_or(SignalError::InvalidSignal)?;

    // Get signal action
    let action = {
        let processes = PROCESS_SIGNALS.read();
        let state = processes.get(&pid).ok_or(SignalError::ProcessNotFound)?;
        state.actions.get(signum as usize).cloned().unwrap_or_default()
    };

    // Dequeue the signal info
    let info = dequeue_signal(tid, pid, signum)?;

    // Determine disposition
    let disposition = SignalDisposition::from(&action);

    match disposition {
        SignalDisposition::Ignore => {
            log::trace!("Ignoring signal {} for thread {:?}", signum, tid);
            Ok(())
        }

        SignalDisposition::Default => {
            execute_default_action(tid, pid, signal, &info)
        }

        SignalDisposition::Handle => {
            setup_signal_frame(tid, pid, signal, &info, &action)
        }
    }
}

/// Dequeue a signal from thread or process queue
fn dequeue_signal(
    tid: ThreadId,
    pid: ProcessId,
    signum: u8,
) -> Result<SigInfo, SignalError> {
    // Try thread queue first
    {
        let mut threads = THREAD_SIGNALS.write();
        if let Some(state) = threads.get_mut(&tid) {
            if let Some(info) = state.pending.dequeue(signum) {
                return Ok(info);
            }
        }
    }

    // Try process queue
    {
        let mut processes = PROCESS_SIGNALS.write();
        if let Some(state) = processes.get_mut(&pid) {
            if let Some(info) = state.pending.dequeue(signum) {
                return Ok(info);
            }
        }
    }

    // No signal found - create default info
    Ok(SigInfo::new(Signal::from_raw(signum).unwrap()))
}

/// Execute default action for a signal
fn execute_default_action(
    tid: ThreadId,
    pid: ProcessId,
    signal: Signal,
    info: &SigInfo,
) -> Result<(), SignalError> {
    match signal.default_action() {
        DefaultAction::Terminate => {
            log::info!("Terminating process {:?} due to signal {:?}", pid, signal);
            crate::process::terminate(pid, 128 + signal.as_raw() as i32);
            Ok(())
        }

        DefaultAction::CoreDump => {
            log::info!(
                "Terminating process {:?} with core dump due to signal {:?}",
                pid, signal
            );
            // TODO: Generate core dump
            crate::process::terminate(pid, 128 + signal.as_raw() as i32);
            Ok(())
        }

        DefaultAction::Stop => {
            log::info!("Stopping process {:?} due to signal {:?}", pid, signal);
            crate::process::stop(pid);
            Ok(())
        }

        DefaultAction::Continue => {
            log::info!("Continuing process {:?} due to signal {:?}", pid, signal);
            crate::process::resume(pid);
            Ok(())
        }

        DefaultAction::Ignore => {
            log::trace!("Ignoring signal {:?} for process {:?} (default)", signal, pid);
            Ok(())
        }
    }
}

/// Set up signal frame and redirect execution to handler
fn setup_signal_frame(
    tid: ThreadId,
    pid: ProcessId,
    signal: Signal,
    info: &SigInfo,
    action: &SigAction,
) -> Result<(), SignalError> {
    let handler_addr = action.handler.address()
        .ok_or(SignalError::InvalidSignal)?;

    log::debug!(
        "Setting up signal frame for {:?} on thread {:?}, handler at {:016x}",
        signal, tid, handler_addr
    );

    // Update thread signal state
    {
        let mut threads = THREAD_SIGNALS.write();
        let state = threads.get_mut(&tid).ok_or(SignalError::ThreadNotFound)?;

        // Save current mask
        state.saved_mask = Some(state.mask.clone());

        // Block signals during handler
        state.mask = state.mask.union(&action.mask);

        // Block the signal itself unless SA_NODEFER
        if !action.flags.contains(SigActionFlags::NODEFER) {
            state.mask.add(signal.as_raw());
        }

        // Mark as handling
        state.handling = Some(signal.as_raw());
    }

    // Reset handler to default if SA_RESETHAND
    if action.flags.contains(SigActionFlags::RESETHAND) {
        let mut processes = PROCESS_SIGNALS.write();
        if let Some(state) = processes.get_mut(&pid) {
            if let Some(act) = state.actions.get_mut(signal.as_raw() as usize) {
                act.handler = SigHandler::Default;
            }
        }
    }

    // In a real implementation:
    // 1. Save current register context
    // 2. Push SignalFrame to user stack
    // 3. Set up registers for handler call:
    //    - RIP = handler_addr
    //    - RDI = signal number
    //    - RSI = &siginfo (if SA_SIGINFO)
    //    - RDX = &ucontext (if SA_SIGINFO)
    //    - RSP = top of signal frame

    Ok(())
}

/// Handle sigreturn syscall
///
/// Restores the saved context from the signal frame.
pub fn sigreturn(tid: ThreadId) -> Result<(), SignalError> {
    log::debug!("Sigreturn for thread {:?}", tid);

    // Restore signal mask
    let mut threads = THREAD_SIGNALS.write();
    let state = threads.get_mut(&tid).ok_or(SignalError::ThreadNotFound)?;

    if let Some(saved_mask) = state.saved_mask.take() {
        state.mask = saved_mask;
    }
    state.handling = None;

    // In a real implementation:
    // 1. Read saved context from user stack
    // 2. Validate the context
    // 3. Restore registers from context
    // 4. Resume execution at saved RIP

    Ok(())
}

/// Wake a thread to handle pending signals
pub fn wake_for_signal(pid: ProcessId) -> Result<(), SignalError> {
    // Find a thread that can handle the signal
    // (not blocked, preferably main thread)

    // In a real implementation, this would:
    // 1. Find threads in the process
    // 2. Check signal masks
    // 3. Wake an appropriate thread

    log::trace!("Waking thread for signal delivery to process {:?}", pid);
    Ok(())
}

/// Wake a specific thread for signal delivery
pub fn wake_thread_for_signal(tid: ThreadId) -> Result<(), SignalError> {
    // In a real implementation, this would make the thread runnable
    // if it's blocked in a syscall

    log::trace!("Waking thread {:?} for signal delivery", tid);
    Ok(())
}
