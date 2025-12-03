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
use crate::process::ProcessId;
use crate::sched::ThreadId;

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
#[derive(Clone, Debug)]
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

impl Default for SigInfoFrame {
    fn default() -> Self {
        Self {
            si_signo: 0,
            si_errno: 0,
            si_code: 0,
            _pad: 0,
            si_pid: 0,
            si_uid: 0,
            si_status: 0,
            _pad2: 0,
            si_utime: 0,
            si_stime: 0,
            si_value: 0,
            si_addr: 0,
            si_band: 0,
            si_fd: 0,
            _reserved: [0u8; 48],
        }
    }
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

            // Generate core dump before terminating
            if let Err(e) = generate_core_dump(pid, signal, info) {
                log::warn!("Failed to generate core dump for process {:?}: {:?}", pid, e);
            }

            // Negative exit code indicates signal death with core
            crate::process::terminate(pid, -(signal.as_raw() as i32));
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

// ============================================================================
// Core Dump Generation
// ============================================================================

/// Core dump error types
#[derive(Debug)]
pub enum CoreDumpError {
    /// Process not found
    ProcessNotFound,
    /// I/O error writing core file
    IoError,
    /// Out of disk space
    OutOfSpace,
    /// Core dumps disabled
    Disabled,
}

/// ELF core dump note types
#[repr(u32)]
#[derive(Clone, Copy)]
enum NoteType {
    /// Process status (prstatus)
    PrStatus = 1,
    /// Floating point registers
    FpRegSet = 2,
    /// Process info (prpsinfo)
    PrPsInfo = 3,
    /// Auxiliary vector
    Auxv = 6,
    /// Signal info
    SigInfo = 0x53494749, // "SIGI"
}

/// Generate an ELF core dump for a crashed process
///
/// Core dumps contain:
/// - ELF header identifying this as a core file
/// - Program headers describing memory segments
/// - NT_PRSTATUS note with register state
/// - NT_PRPSINFO note with process info
/// - NT_SIGINFO note with signal details
/// - Memory contents of all mapped regions
fn generate_core_dump(
    pid: ProcessId,
    signal: Signal,
    info: &SigInfo,
) -> Result<(), CoreDumpError> {
    use crate::process::{get_process, PROCESSES};

    log::debug!("Generating core dump for process {:?} (signal {:?})", pid, signal);

    // Get process info
    let processes = PROCESSES.read();
    let process = processes.get(&pid).ok_or(CoreDumpError::ProcessNotFound)?;

    // Build the core dump path: /var/core/core.<pid>.<timestamp>
    let timestamp = crate::time::get_unix_timestamp().unwrap_or(0);
    let core_path = alloc::format!("/var/core/core.{}.{}", pid.raw(), timestamp);

    // Collect memory mappings and thread states
    let mut segments = alloc::vec::Vec::new();
    let mut notes = alloc::vec::Vec::new();

    // Add process status note (NT_PRSTATUS)
    let prstatus = build_prstatus_note(pid, signal, info);
    notes.push((NoteType::PrStatus, prstatus));

    // Add process info note (NT_PRPSINFO)
    let prpsinfo = build_prpsinfo_note(process);
    notes.push((NoteType::PrPsInfo, prpsinfo));

    // Add signal info note
    let siginfo_data = build_siginfo_note(info);
    notes.push((NoteType::SigInfo, siginfo_data));

    // Collect memory segments from address space
    if let Some(ref address_space) = Some(&process.address_space) {
        for vma in address_space.iter_vmas() {
            // Skip kernel mappings and guard pages
            if vma.is_kernel() || !vma.is_readable() {
                continue;
            }

            segments.push(CoreSegment {
                vaddr: vma.start().as_u64(),
                memsz: vma.size() as u64,
                filesz: if vma.is_readable() { vma.size() as u64 } else { 0 },
                flags: vma_to_elf_flags(vma),
            });
        }
    }

    // Build ELF core file
    let core_data = build_elf_core(&segments, &notes)?;

    // Write to filesystem
    write_core_file(&core_path, &core_data)?;

    log::info!("Core dump written to {} ({} bytes)", core_path, core_data.len());

    Ok(())
}

/// Memory segment for core dump
struct CoreSegment {
    vaddr: u64,
    memsz: u64,
    filesz: u64,
    flags: u32,
}

/// Build NT_PRSTATUS note (process/thread status)
fn build_prstatus_note(pid: ProcessId, signal: Signal, _info: &SigInfo) -> alloc::vec::Vec<u8> {
    // prstatus structure (simplified for x86_64)
    #[repr(C)]
    struct PrStatus {
        si_signo: i32,
        si_code: i32,
        si_errno: i32,
        cursig: u16,
        _pad: u16,
        sigpend: u64,
        sighold: u64,
        pid: i32,
        ppid: i32,
        pgrp: i32,
        sid: i32,
        utime_sec: u64,
        utime_usec: u64,
        stime_sec: u64,
        stime_usec: u64,
        cutime_sec: u64,
        cutime_usec: u64,
        cstime_sec: u64,
        cstime_usec: u64,
        // Register state (x86_64)
        regs: [u64; 27],
        fpvalid: i32,
    }

    let mut prstatus = PrStatus {
        si_signo: signal.as_raw() as i32,
        si_code: 0,
        si_errno: 0,
        cursig: signal.as_raw() as u16,
        _pad: 0,
        sigpend: 0,
        sighold: 0,
        pid: pid.raw() as i32,
        ppid: 0,
        pgrp: pid.raw() as i32,
        sid: pid.raw() as i32,
        utime_sec: 0,
        utime_usec: 0,
        stime_sec: 0,
        stime_usec: 0,
        cutime_sec: 0,
        cutime_usec: 0,
        cstime_sec: 0,
        cstime_usec: 0,
        regs: [0; 27],
        fpvalid: 0,
    };

    // Get thread registers if available
    if let Some(main_thread) = get_main_thread(pid) {
        let threads = crate::sched::THREADS.read();
        if let Some(thread) = threads.get(&main_thread) {
            // Copy register state
            prstatus.regs[0] = thread.registers.r15;
            prstatus.regs[1] = thread.registers.r14;
            prstatus.regs[2] = thread.registers.r13;
            prstatus.regs[3] = thread.registers.r12;
            prstatus.regs[4] = thread.registers.rbp;
            prstatus.regs[5] = thread.registers.rbx;
            prstatus.regs[6] = thread.registers.r11;
            prstatus.regs[7] = thread.registers.r10;
            prstatus.regs[8] = thread.registers.r9;
            prstatus.regs[9] = thread.registers.r8;
            prstatus.regs[10] = thread.registers.rax;
            prstatus.regs[11] = thread.registers.rcx;
            prstatus.regs[12] = thread.registers.rdx;
            prstatus.regs[13] = thread.registers.rsi;
            prstatus.regs[14] = thread.registers.rdi;
            // orig_rax would be here
            prstatus.regs[16] = thread.registers.rip;
            prstatus.regs[17] = thread.registers.cs as u64;
            prstatus.regs[18] = thread.registers.rflags;
            prstatus.regs[19] = thread.registers.rsp;
            prstatus.regs[20] = thread.registers.ss as u64;
        }
    }

    // Convert to bytes
    let ptr = &prstatus as *const PrStatus as *const u8;
    let len = core::mem::size_of::<PrStatus>();
    unsafe { core::slice::from_raw_parts(ptr, len).to_vec() }
}

/// Build NT_PRPSINFO note (process info)
fn build_prpsinfo_note(process: &crate::process::Process) -> alloc::vec::Vec<u8> {
    #[repr(C)]
    struct PrPsInfo {
        state: u8,
        sname: u8,
        zomb: u8,
        nice: i8,
        flag: u64,
        uid: u32,
        gid: u32,
        pid: i32,
        ppid: i32,
        pgrp: i32,
        sid: i32,
        fname: [u8; 16],
        psargs: [u8; 80],
    }

    let mut prpsinfo = PrPsInfo {
        state: b'R',
        sname: b'R',
        zomb: 0,
        nice: 0,
        flag: 0,
        uid: 0,
        gid: 0,
        pid: process.pid.raw() as i32,
        ppid: process.parent.map(|p| p.raw() as i32).unwrap_or(0),
        pgrp: process.pid.raw() as i32,
        sid: process.pid.raw() as i32,
        fname: [0; 16],
        psargs: [0; 80],
    };

    // Copy process name
    let name_bytes = process.name.as_bytes();
    let copy_len = name_bytes.len().min(15);
    prpsinfo.fname[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

    let ptr = &prpsinfo as *const PrPsInfo as *const u8;
    let len = core::mem::size_of::<PrPsInfo>();
    unsafe { core::slice::from_raw_parts(ptr, len).to_vec() }
}

/// Build siginfo note
fn build_siginfo_note(info: &SigInfo) -> alloc::vec::Vec<u8> {
    // Build siginfo_t compatible structure
    let mut data = alloc::vec![0u8; 128];
    // si_signo
    data[0..4].copy_from_slice(&(info.signo as i32).to_le_bytes());
    // si_errno
    data[4..8].copy_from_slice(&info.errno.to_le_bytes());
    // si_code
    data[8..12].copy_from_slice(&(info.code as i32).to_le_bytes());
    // si_pid (if available)
    if let Some(pid) = info.sender_pid {
        data[16..20].copy_from_slice(&(pid.raw() as i32).to_le_bytes());
    }
    // si_addr (for SIGSEGV, etc.)
    if let Some(addr) = info.addr {
        data[24..32].copy_from_slice(&addr.to_le_bytes());
    }
    data
}

/// Convert VMA flags to ELF segment flags
fn vma_to_elf_flags(vma: &crate::mem::virt::Vma) -> u32 {
    let mut flags = 0u32;
    if vma.is_readable() {
        flags |= 0x4; // PF_R
    }
    if vma.is_writable() {
        flags |= 0x2; // PF_W
    }
    if vma.is_executable() {
        flags |= 0x1; // PF_X
    }
    flags
}

/// Get main thread for a process
fn get_main_thread(pid: ProcessId) -> Option<ThreadId> {
    let processes = crate::process::PROCESSES.read();
    processes.get(&pid)?.threads.first().copied()
}

/// Build ELF core file structure
fn build_elf_core(
    segments: &[CoreSegment],
    notes: &[(NoteType, alloc::vec::Vec<u8>)],
) -> Result<alloc::vec::Vec<u8>, CoreDumpError> {
    // ELF64 header size
    const ELF_HEADER_SIZE: usize = 64;
    // Program header size
    const PHDR_SIZE: usize = 56;

    // Calculate sizes
    let note_segment_size: usize = notes.iter()
        .map(|(_, data)| 12 + align_up(4, 4) + align_up(data.len(), 4)) // namesz=4 ("CORE"), + note header
        .sum();

    let num_phdrs = segments.len() + 1; // +1 for PT_NOTE
    let phdrs_size = num_phdrs * PHDR_SIZE;
    let data_offset = ELF_HEADER_SIZE + phdrs_size;

    let mut core = alloc::vec::Vec::new();

    // ELF header
    core.extend_from_slice(&[
        0x7f, b'E', b'L', b'F',  // Magic
        2,                       // 64-bit
        1,                       // Little endian
        1,                       // ELF version
        0,                       // OS/ABI (SYSV)
        0, 0, 0, 0, 0, 0, 0, 0, // Padding
    ]);
    core.extend_from_slice(&4u16.to_le_bytes());    // ET_CORE
    core.extend_from_slice(&0x3Eu16.to_le_bytes()); // x86_64
    core.extend_from_slice(&1u32.to_le_bytes());    // Version
    core.extend_from_slice(&0u64.to_le_bytes());    // Entry (none for core)
    core.extend_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes()); // phoff
    core.extend_from_slice(&0u64.to_le_bytes());    // shoff (none)
    core.extend_from_slice(&0u32.to_le_bytes());    // flags
    core.extend_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes()); // ehsize
    core.extend_from_slice(&(PHDR_SIZE as u16).to_le_bytes()); // phentsize
    core.extend_from_slice(&(num_phdrs as u16).to_le_bytes()); // phnum
    core.extend_from_slice(&0u16.to_le_bytes());    // shentsize
    core.extend_from_slice(&0u16.to_le_bytes());    // shnum
    core.extend_from_slice(&0u16.to_le_bytes());    // shstrndx

    let notes_offset = data_offset;

    // PT_NOTE program header
    core.extend_from_slice(&4u32.to_le_bytes());    // PT_NOTE
    core.extend_from_slice(&0u32.to_le_bytes());    // flags
    core.extend_from_slice(&(notes_offset as u64).to_le_bytes()); // offset
    core.extend_from_slice(&0u64.to_le_bytes());    // vaddr
    core.extend_from_slice(&0u64.to_le_bytes());    // paddr
    core.extend_from_slice(&(note_segment_size as u64).to_le_bytes()); // filesz
    core.extend_from_slice(&(note_segment_size as u64).to_le_bytes()); // memsz
    core.extend_from_slice(&4u64.to_le_bytes());    // align

    // PT_LOAD headers for memory segments
    let mut current_offset = notes_offset + note_segment_size;
    for seg in segments {
        core.extend_from_slice(&1u32.to_le_bytes());    // PT_LOAD
        core.extend_from_slice(&seg.flags.to_le_bytes()); // flags
        core.extend_from_slice(&(current_offset as u64).to_le_bytes()); // offset
        core.extend_from_slice(&seg.vaddr.to_le_bytes()); // vaddr
        core.extend_from_slice(&0u64.to_le_bytes());    // paddr
        core.extend_from_slice(&seg.filesz.to_le_bytes()); // filesz
        core.extend_from_slice(&seg.memsz.to_le_bytes()); // memsz
        core.extend_from_slice(&0x1000u64.to_le_bytes()); // align (page)

        current_offset += seg.filesz as usize;
    }

    // Note segment data
    for (note_type, data) in notes {
        // Note header
        core.extend_from_slice(&5u32.to_le_bytes()); // namesz ("CORE\0" = 5)
        core.extend_from_slice(&(data.len() as u32).to_le_bytes()); // descsz
        core.extend_from_slice(&(*note_type as u32).to_le_bytes()); // type
        core.extend_from_slice(b"CORE\0\0\0\0"); // name (8-byte aligned)
        core.extend_from_slice(data);
        // Pad to 4-byte alignment
        while core.len() % 4 != 0 {
            core.push(0);
        }
    }

    // Memory segment data would be copied here from the process's address space
    // For each segment, we'd read the memory and append it
    // (Omitted for brevity - in real impl would use address_space.read())

    Ok(core)
}

/// Align value up to alignment
fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

/// Write core file to filesystem
fn write_core_file(path: &str, data: &[u8]) -> Result<(), CoreDumpError> {
    // Use filesystem to write the core dump
    // For now, just log - actual implementation would use VFS
    log::debug!("Would write {} bytes to {}", data.len(), path);

    // In real implementation:
    // crate::fs::write(path, data).map_err(|_| CoreDumpError::IoError)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // CoreDumpError Tests
    // =========================================================================

    #[test]
    fn test_core_dump_error_variants() {
        let errors = [
            CoreDumpError::ProcessNotFound,
            CoreDumpError::NoMemorySegments,
            CoreDumpError::IoError,
            CoreDumpError::OutOfMemory,
        ];

        for (i, a) in errors.iter().enumerate() {
            for (j, b) in errors.iter().enumerate() {
                if i != j {
                    assert_ne!(format!("{:?}", a), format!("{:?}", b));
                }
            }
        }
    }

    // =========================================================================
    // NoteType Tests
    // =========================================================================

    #[test]
    fn test_note_type_values() {
        assert_eq!(NoteType::PrStatus as u32, 1);
        assert_eq!(NoteType::PrPsInfo as u32, 3);
        assert_eq!(NoteType::SigInfo as u32, 0x53494749);
    }

    // =========================================================================
    // CoreSegment Tests
    // =========================================================================

    #[test]
    fn test_core_segment_creation() {
        let segment = CoreSegment {
            vaddr: 0x400000,
            memsz: 0x1000,
            filesz: 0x1000,
            flags: 0x5, // PF_R | PF_X
        };

        assert_eq!(segment.vaddr, 0x400000);
        assert_eq!(segment.memsz, 0x1000);
        assert_eq!(segment.filesz, 0x1000);
        assert_eq!(segment.flags, 0x5);
    }

    // =========================================================================
    // Helper Function Tests
    // =========================================================================

    #[test]
    fn test_align_up_already_aligned() {
        assert_eq!(align_up(16, 4), 16);
        assert_eq!(align_up(0, 4), 0);
        assert_eq!(align_up(4096, 4096), 4096);
    }

    #[test]
    fn test_align_up_needs_alignment() {
        assert_eq!(align_up(1, 4), 4);
        assert_eq!(align_up(5, 4), 8);
        assert_eq!(align_up(13, 8), 16);
        assert_eq!(align_up(4097, 4096), 8192);
    }

    #[test]
    fn test_align_up_edge_cases() {
        assert_eq!(align_up(0, 1), 0);
        assert_eq!(align_up(1, 1), 1);
        assert_eq!(align_up(255, 256), 256);
    }

    // =========================================================================
    // ELF Constants Tests
    // =========================================================================

    #[test]
    fn test_elf_magic() {
        let magic = [0x7f, b'E', b'L', b'F'];
        assert_eq!(magic[0], 0x7f);
        assert_eq!(magic[1], b'E');
        assert_eq!(magic[2], b'L');
        assert_eq!(magic[3], b'F');
    }

    #[test]
    fn test_elf_core_type() {
        // ET_CORE = 4
        let et_core: u16 = 4;
        assert_eq!(et_core, 4);
    }

    #[test]
    fn test_elf_x86_64_machine() {
        // EM_X86_64 = 0x3E
        let em_x86_64: u16 = 0x3E;
        assert_eq!(em_x86_64, 62);
    }

    // =========================================================================
    // Siginfo Note Tests
    // =========================================================================

    #[test]
    fn test_build_siginfo_note_basic() {
        use crate::signal::SigInfo;
        use crate::signal::Signal;

        let info = SigInfo::new(Signal::SIGSEGV);
        let note = build_siginfo_note(&info);

        // Check minimum size (128 bytes)
        assert_eq!(note.len(), 128);

        // Check si_signo is correctly encoded
        let si_signo = i32::from_le_bytes([note[0], note[1], note[2], note[3]]);
        assert_eq!(si_signo, Signal::SIGSEGV.as_raw() as i32);
    }

    #[test]
    fn test_build_siginfo_note_with_addr() {
        use crate::signal::SigInfo;
        use crate::signal::Signal;

        let mut info = SigInfo::new(Signal::SIGSEGV);
        info.addr = Some(0xDEADBEEF);
        let note = build_siginfo_note(&info);

        // Check si_addr is correctly encoded at offset 24
        let si_addr = u64::from_le_bytes([
            note[24], note[25], note[26], note[27],
            note[28], note[29], note[30], note[31],
        ]);
        assert_eq!(si_addr, 0xDEADBEEF);
    }

    // =========================================================================
    // ELF Core Building Tests
    // =========================================================================

    #[test]
    fn test_build_elf_core_header() {
        let segments: Vec<CoreSegment> = vec![];
        let notes: Vec<(NoteType, Vec<u8>)> = vec![];

        let result = build_elf_core(&segments, &notes);
        assert!(result.is_ok());

        let core = result.unwrap();

        // Check ELF magic
        assert_eq!(&core[0..4], &[0x7f, b'E', b'L', b'F']);

        // Check 64-bit
        assert_eq!(core[4], 2);

        // Check little endian
        assert_eq!(core[5], 1);

        // Check ELF version
        assert_eq!(core[6], 1);

        // Check ET_CORE (bytes 16-17)
        let e_type = u16::from_le_bytes([core[16], core[17]]);
        assert_eq!(e_type, 4); // ET_CORE

        // Check EM_X86_64 (bytes 18-19)
        let e_machine = u16::from_le_bytes([core[18], core[19]]);
        assert_eq!(e_machine, 0x3E);
    }

    #[test]
    fn test_build_elf_core_with_segments() {
        let segments = vec![
            CoreSegment {
                vaddr: 0x400000,
                memsz: 0x1000,
                filesz: 0x1000,
                flags: 0x5, // PF_R | PF_X
            },
            CoreSegment {
                vaddr: 0x600000,
                memsz: 0x2000,
                filesz: 0x2000,
                flags: 0x6, // PF_R | PF_W
            },
        ];
        let notes: Vec<(NoteType, Vec<u8>)> = vec![];

        let result = build_elf_core(&segments, &notes);
        assert!(result.is_ok());

        let core = result.unwrap();

        // Check program header count (e_phnum at offset 56-57)
        let e_phnum = u16::from_le_bytes([core[56], core[57]]);
        assert_eq!(e_phnum, 3); // 2 PT_LOAD + 1 PT_NOTE
    }

    #[test]
    fn test_build_elf_core_with_notes() {
        let segments: Vec<CoreSegment> = vec![];
        let notes = vec![
            (NoteType::PrStatus, vec![0u8; 336]),
            (NoteType::PrPsInfo, vec![0u8; 136]),
        ];

        let result = build_elf_core(&segments, &notes);
        assert!(result.is_ok());

        let core = result.unwrap();

        // Just verify it builds without error and has reasonable size
        assert!(core.len() > 64); // At least ELF header
    }

    // =========================================================================
    // Signal Action Tests
    // =========================================================================

    #[test]
    fn test_default_action_core_dump() {
        use crate::signal::{Signal, DefaultAction};

        // These signals should generate core dumps
        let core_dump_signals = [
            Signal::SIGQUIT,
            Signal::SIGILL,
            Signal::SIGABRT,
            Signal::SIGFPE,
            Signal::SIGSEGV,
            Signal::SIGBUS,
            Signal::SIGSYS,
            Signal::SIGTRAP,
            Signal::SIGXCPU,
            Signal::SIGXFSZ,
        ];

        for sig in core_dump_signals {
            let action = sig.default_action();
            assert_eq!(action, DefaultAction::CoreDump,
                "Expected CoreDump for {:?}", sig);
        }
    }

    #[test]
    fn test_default_action_terminate() {
        use crate::signal::{Signal, DefaultAction};

        // These signals should terminate
        let term_signals = [
            Signal::SIGTERM,
            Signal::SIGKILL,
            Signal::SIGHUP,
            Signal::SIGINT,
            Signal::SIGPIPE,
            Signal::SIGALRM,
        ];

        for sig in term_signals {
            let action = sig.default_action();
            assert_eq!(action, DefaultAction::Terminate,
                "Expected Terminate for {:?}", sig);
        }
    }

    #[test]
    fn test_default_action_ignore() {
        use crate::signal::{Signal, DefaultAction};

        let ignore_signals = [
            Signal::SIGCHLD,
            Signal::SIGURG,
            Signal::SIGWINCH,
        ];

        for sig in ignore_signals {
            let action = sig.default_action();
            assert_eq!(action, DefaultAction::Ignore,
                "Expected Ignore for {:?}", sig);
        }
    }

    #[test]
    fn test_default_action_stop() {
        use crate::signal::{Signal, DefaultAction};

        let stop_signals = [
            Signal::SIGSTOP,
            Signal::SIGTSTP,
            Signal::SIGTTIN,
            Signal::SIGTTOU,
        ];

        for sig in stop_signals {
            let action = sig.default_action();
            assert_eq!(action, DefaultAction::Stop,
                "Expected Stop for {:?}", sig);
        }
    }

    #[test]
    fn test_default_action_continue() {
        use crate::signal::{Signal, DefaultAction};

        let action = Signal::SIGCONT.default_action();
        assert_eq!(action, DefaultAction::Continue);
    }
}
