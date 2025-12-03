//! Process management
//!
//! Processes are containers for threads, address spaces, and capabilities.
//! Unlike threads, processes have their own capability space and address space.

use crate::cap::{CSpace, Capability, ObjectId, ObjectType, Rights, create_cspace};
use crate::mem::{AddressSpace, VirtAddr, PhysAddr, PAGE_SIZE, virt::Protection};
use crate::sched::{Thread, ThreadState, SchedClass};
pub use crate::sched::ThreadId;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

/// Process ID counter
static NEXT_PID: AtomicU64 = AtomicU64::new(1);

/// Global process table
static PROCESSES: RwLock<BTreeMap<ProcessId, Process>> = RwLock::new(BTreeMap::new());

/// Process identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessId(pub u64);

impl ProcessId {
    /// Allocate a new process ID
    pub fn new() -> Self {
        Self(NEXT_PID.fetch_add(1, Ordering::Relaxed))
    }

    /// Get raw value
    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl Default for ProcessId {
    fn default() -> Self {
        Self::new()
    }
}

/// Process state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is being created
    Creating,
    /// Process is running (has runnable threads)
    Running,
    /// Process is stopped (all threads stopped)
    Stopped,
    /// Process has exited but not yet reaped
    Zombie(i32),
    /// Process has been fully cleaned up
    Dead,
}

/// Tracked memory allocation
#[derive(Clone, Debug)]
pub struct TrackedAllocation {
    /// Start virtual address
    pub start: VirtAddr,
    /// Size in bytes (page-aligned)
    pub size: u64,
    /// Whether this is physically contiguous (for DMA)
    pub contiguous: bool,
}

/// Process control block
pub struct Process {
    /// Process ID
    pub pid: ProcessId,
    /// Object ID for capability system
    pub object_id: ObjectId,
    /// Parent process ID
    pub parent: Option<ProcessId>,
    /// Child process IDs
    pub children: Vec<ProcessId>,
    /// Current state
    pub state: ProcessState,
    /// Process name (for debugging)
    pub name: String,
    /// Address space
    pub address_space: AddressSpace,
    /// Capability space
    pub cspace: CSpace,
    /// Thread IDs owned by this process
    pub threads: Vec<ThreadId>,
    /// Main thread ID
    pub main_thread: Option<ThreadId>,
    /// Exit code (if zombie)
    pub exit_code: i32,
    /// User ID (for permission checks)
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Environment variables
    pub env: BTreeMap<String, String>,
    /// Working directory capability
    pub cwd: Option<Capability>,
    /// File descriptor table (cap slots for open files)
    pub fd_table: BTreeMap<i32, u32>,
    /// Next file descriptor
    pub next_fd: i32,
    /// Memory statistics
    pub mem_stats: MemoryStats,
    /// Tracked memory allocations (for ownership verification)
    allocations: BTreeMap<u64, TrackedAllocation>,
    /// Threads waiting to join on this process's threads
    pub join_waiters: BTreeMap<ThreadId, Vec<ThreadId>>,
}

/// Memory usage statistics
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryStats {
    /// Virtual memory size
    pub vm_size: u64,
    /// Resident set size (physical)
    pub rss: u64,
    /// Shared memory
    pub shared: u64,
    /// Code (text) size
    pub text: u64,
    /// Data + stack size
    pub data: u64,
}

impl Process {
    /// Create a new process
    pub fn new(name: impl Into<String>, parent: Option<ProcessId>) -> Self {
        Self {
            pid: ProcessId::new(),
            object_id: ObjectId::new(ObjectType::Process),
            parent,
            children: Vec::new(),
            state: ProcessState::Creating,
            name: name.into(),
            address_space: AddressSpace::new(),
            cspace: create_cspace(),
            threads: Vec::new(),
            main_thread: None,
            exit_code: 0,
            uid: 0,
            gid: 0,
            env: BTreeMap::new(),
            cwd: None,
            fd_table: BTreeMap::new(),
            next_fd: 3, // 0=stdin, 1=stdout, 2=stderr
            mem_stats: MemoryStats::default(),
            allocations: BTreeMap::new(),
            join_waiters: BTreeMap::new(),
        }
    }

    /// Create the init process (PID 1)
    pub fn new_init() -> Self {
        let mut proc = Self::new("init", None);
        // Force PID 1 for init
        proc.pid = ProcessId(1);
        proc
    }

    /// Add a thread to this process
    pub fn add_thread(&mut self, thread_id: ThreadId) {
        self.threads.push(thread_id);
        if self.main_thread.is_none() {
            self.main_thread = Some(thread_id);
        }
    }

    /// Remove a thread from this process
    pub fn remove_thread(&mut self, thread_id: ThreadId) {
        self.threads.retain(|&id| id != thread_id);
    }

    /// Check if process has any running threads
    pub fn has_running_threads(&self) -> bool {
        !self.threads.is_empty()
    }

    /// Allocate a file descriptor
    pub fn alloc_fd(&mut self, cap_slot: u32) -> i32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.fd_table.insert(fd, cap_slot);
        fd
    }

    /// Get capability slot for file descriptor
    pub fn get_fd(&self, fd: i32) -> Option<u32> {
        self.fd_table.get(&fd).copied()
    }

    /// Close a file descriptor
    pub fn close_fd(&mut self, fd: i32) -> Option<u32> {
        self.fd_table.remove(&fd)
    }

    /// Insert a capability and return its slot
    pub fn insert_cap(&mut self, cap: Capability) -> u32 {
        self.cspace.insert_next(cap).unwrap_or(0)
    }

    /// Get a capability by slot
    pub fn get_cap(&self, slot: u32) -> Option<&Capability> {
        self.cspace.get(slot)
    }

    /// Track a memory allocation for ownership verification
    ///
    /// This must be called after successfully allocating memory to enable
    /// verification during free operations.
    pub fn track_allocation(&mut self, start: VirtAddr, size: u64, contiguous: bool) {
        self.allocations.insert(
            start.as_u64(),
            TrackedAllocation {
                start,
                size,
                contiguous,
            },
        );
    }

    /// Verify that this process owns an allocation at the given address
    ///
    /// Returns true if the process has a tracked allocation that exactly matches
    /// the start address and size, false otherwise.
    pub fn verify_allocation(&self, start: VirtAddr, size: u64) -> bool {
        self.allocations
            .get(&start.as_u64())
            .is_some_and(|alloc| alloc.size == size)
    }

    /// Remove tracking for an allocation (called during free)
    pub fn untrack_allocation(&mut self, start: VirtAddr) -> Option<TrackedAllocation> {
        self.allocations.remove(&start.as_u64())
    }

    /// Get all allocations (for debugging/cleanup)
    pub fn allocations(&self) -> impl Iterator<Item = &TrackedAllocation> {
        self.allocations.values()
    }

    /// Clean up all allocations (called during process exit)
    pub fn cleanup_allocations(&mut self) {
        // Free all tracked allocations
        for alloc in self.allocations.values() {
            let num_pages = (alloc.size / crate::mem::PAGE_SIZE) as usize;
            for i in 0..num_pages {
                let page_virt = VirtAddr::new(alloc.start.as_u64() + (i as u64) * crate::mem::PAGE_SIZE);
                if let Some(phys_addr) = self.address_space.translate(page_virt) {
                    let _ = self.address_space.unmap(page_virt, crate::mem::PAGE_SIZE);
                    crate::mem::free_frame(phys_addr);
                }
            }
        }
        self.allocations.clear();
    }

    /// Register a thread to wait for another thread to exit (for join)
    pub fn add_join_waiter(&mut self, target_thread: ThreadId, waiting_thread: ThreadId) {
        self.join_waiters
            .entry(target_thread)
            .or_insert_with(Vec::new)
            .push(waiting_thread);
    }

    /// Get and clear waiters for a thread that exited
    pub fn take_join_waiters(&mut self, thread: ThreadId) -> Vec<ThreadId> {
        self.join_waiters.remove(&thread).unwrap_or_default()
    }

    /// Get the raw process ID value
    pub fn raw_pid(&self) -> u64 {
        self.pid.0
    }
}

/// Spawn arguments for creating a new process
#[derive(Clone, Debug)]
pub struct SpawnArgs {
    /// Path to executable
    pub path: String,
    /// Command line arguments
    pub args: Vec<String>,
    /// Environment variables
    pub env: Vec<(String, String)>,
    /// Capabilities to grant
    pub caps: Vec<Capability>,
    /// Scheduling class
    pub sched_class: SchedClass,
    /// Initial priority
    pub priority: i32,
    /// Working directory
    pub cwd: Option<String>,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
}

impl Default for SpawnArgs {
    fn default() -> Self {
        Self {
            path: String::new(),
            args: Vec::new(),
            env: Vec::new(),
            caps: Vec::new(),
            sched_class: SchedClass::Normal,
            priority: 0,
            cwd: None,
            uid: 0,
            gid: 0,
        }
    }
}

/// Process spawn error
#[derive(Debug, Clone)]
pub enum SpawnError {
    /// File not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// Invalid executable format
    InvalidFormat,
    /// Out of memory
    OutOfMemory,
    /// Too many processes
    TooManyProcesses,
    /// Invalid argument
    InvalidArgument,
    /// I/O error
    IoError,
}

// ============================================================================
// Process Management Functions
// ============================================================================

/// Initialize the process subsystem
pub fn init() {
    log::debug!("Initializing process subsystem");
    // Nothing to do yet - PROCESSES is statically initialized
    log::debug!("Process subsystem initialized");
}

/// Spawn a new process
pub fn spawn(args: SpawnArgs) -> Result<ProcessId, SpawnError> {
    // Get current process as parent
    let parent_pid = current_process_id();

    // Create new process
    let mut proc = Process::new(&args.path, parent_pid);

    // Set up environment
    for (key, value) in args.env {
        proc.env.insert(key, value);
    }

    // Set credentials
    proc.uid = args.uid;
    proc.gid = args.gid;

    // Load executable
    let entry_point = load_executable(&args.path, &mut proc)?;

    // Grant initial capabilities
    for cap in args.caps {
        proc.insert_cap(cap);
    }

    // Set up user stack
    let stack_base = VirtAddr::new(0x0000_7FFF_FFFF_0000); // Below kernel
    let stack_size = 8 * PAGE_SIZE; // 32KB stack
    setup_user_stack(&mut proc, stack_base, stack_size, &args.args)?;

    // Create main thread
    let stack_top = stack_base.as_u64() + stack_size;
    let thread = Thread::new_user(
        entry_point,
        stack_top,
        proc.address_space.clone(),
        proc.pid,
    );
    let thread_id = thread.id;

    // Register thread
    crate::sched::THREADS.write().insert(thread_id, thread);
    proc.add_thread(thread_id);

    // Mark process as running
    proc.state = ProcessState::Running;
    let pid = proc.pid;

    // Add to parent's children list
    if let Some(parent_pid) = parent_pid {
        if let Some(parent) = PROCESSES.write().get_mut(&parent_pid) {
            parent.children.push(pid);
        }
    }

    // Register process
    PROCESSES.write().insert(pid, proc);

    // Enqueue main thread for scheduling
    let cpu_id = crate::sched::current_thread_id().0 as u32 % 1; // Simplified
    {
        let mut per_cpu = crate::sched::PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
            cpu_sched.enqueue(thread_id);
        }
    }

    log::info!("Spawned process {} ({})", pid.0, args.path);
    Ok(pid)
}

/// Load an executable into a process address space
fn load_executable(path: &str, proc: &mut Process) -> Result<u64, SpawnError> {
    // Try to load from initrd or filesystem
    let data = crate::fs::read_file(path).map_err(|_| SpawnError::NotFound)?;

    // Parse ELF
    let elf = Elf::parse(&data).map_err(|_| SpawnError::InvalidFormat)?;

    // Load program headers
    for phdr in elf.program_headers() {
        if phdr.p_type != PT_LOAD {
            continue;
        }

        let vaddr = VirtAddr::new(phdr.p_vaddr);
        let memsz = phdr.p_memsz;
        let filesz = phdr.p_filesz;
        let offset = phdr.p_offset as usize;

        // Determine protection flags
        let mut prot = crate::mem::virt::Protection::empty();
        if phdr.p_flags & PF_R != 0 {
            prot |= crate::mem::virt::Protection::READ;
        }
        if phdr.p_flags & PF_W != 0 {
            prot |= crate::mem::virt::Protection::WRITE;
        }
        if phdr.p_flags & PF_X != 0 {
            prot |= crate::mem::virt::Protection::EXECUTE;
        }
        prot |= crate::mem::virt::Protection::USER;

        // Map pages
        let start_page = vaddr.align_down(PAGE_SIZE);
        let end_page = VirtAddr::new(vaddr.as_u64() + memsz).align_up(PAGE_SIZE);
        let page_count = ((end_page.as_u64() - start_page.as_u64()) / PAGE_SIZE) as usize;

        for i in 0..page_count {
            let page_vaddr = VirtAddr::new(start_page.as_u64() + i as u64 * PAGE_SIZE);
            let frame = crate::mem::alloc_frame().ok_or(SpawnError::OutOfMemory)?;

            // Get the kernel-mapped virtual address for this physical frame
            // This is safe because the kernel has all physical memory mapped
            let kernel_vaddr = crate::mem::phys_to_virt(frame);

            // Zero the frame first using the kernel virtual address
            // SAFETY: kernel_vaddr points to valid kernel-mapped memory for this frame
            unsafe {
                let ptr = kernel_vaddr as *mut u8;
                core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
            }

            // Copy file data if applicable
            let page_start_in_segment = if page_vaddr.as_u64() >= vaddr.as_u64() {
                0u64
            } else {
                vaddr.as_u64() - page_vaddr.as_u64()
            };
            let page_end_in_segment = PAGE_SIZE;

            // Calculate file offset for this page
            let segment_offset_start = page_vaddr.as_u64().saturating_sub(vaddr.as_u64());
            if segment_offset_start < filesz {
                let file_start = offset + segment_offset_start as usize;
                let copy_len = core::cmp::min(
                    PAGE_SIZE - page_start_in_segment,
                    filesz.saturating_sub(segment_offset_start),
                ) as usize;

                if file_start + copy_len <= data.len() && copy_len > 0 {
                    // SAFETY: We're copying into kernel-mapped memory from a valid slice
                    unsafe {
                        let dst = (kernel_vaddr + page_start_in_segment) as *mut u8;
                        let src = data.as_ptr().add(file_start);
                        core::ptr::copy_nonoverlapping(src, dst, copy_len);
                    }
                }
            }

            // Map the physical frame into the process's address space
            proc.address_space
                .map_page(page_vaddr, frame, prot)
                .map_err(|_| SpawnError::OutOfMemory)?;
        }

        // Update memory stats
        proc.mem_stats.vm_size += memsz;
        if phdr.p_flags & PF_X != 0 {
            proc.mem_stats.text += memsz;
        } else {
            proc.mem_stats.data += memsz;
        }
    }

    Ok(elf.entry())
}

/// Set up the user stack with arguments
fn setup_user_stack(
    proc: &mut Process,
    stack_base: VirtAddr,
    stack_size: u64,
    args: &[String],
) -> Result<(), SpawnError> {
    // Map stack pages
    let page_count = (stack_size / PAGE_SIZE) as usize;
    let prot = crate::mem::virt::Protection::READ
        | crate::mem::virt::Protection::WRITE
        | crate::mem::virt::Protection::USER;

    for i in 0..page_count {
        let page_vaddr = VirtAddr::new(stack_base.as_u64() + i as u64 * PAGE_SIZE);
        let frame = crate::mem::alloc_frame().ok_or(SpawnError::OutOfMemory)?;

        // Get the kernel-mapped virtual address for this physical frame
        let kernel_vaddr = crate::mem::phys_to_virt(frame);

        // Zero the frame using the kernel virtual address
        // SAFETY: kernel_vaddr points to valid kernel-mapped memory for this frame
        unsafe {
            let ptr = kernel_vaddr as *mut u8;
            core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
        }

        // Map the physical frame into the process's address space
        proc.address_space
            .map_page(page_vaddr, frame, prot)
            .map_err(|_| SpawnError::OutOfMemory)?;
    }

    proc.mem_stats.vm_size += stack_size;
    proc.mem_stats.data += stack_size;

    Ok(())
}

/// Exit the current process
pub fn exit(exit_code: i32) {
    let pid = match current_process_id() {
        Some(pid) => pid,
        None => {
            log::error!("exit() called with no current process");
            return;
        }
    };

    log::info!("Process {} exiting with code {}", pid.0, exit_code);

    // Mark process as zombie
    {
        let mut processes = PROCESSES.write();
        if let Some(proc) = processes.get_mut(&pid) {
            proc.state = ProcessState::Zombie(exit_code);
            proc.exit_code = exit_code;

            // Terminate all threads
            for thread_id in &proc.threads {
                let mut threads = crate::sched::THREADS.write();
                if let Some(thread) = threads.get_mut(thread_id) {
                    thread.state = ThreadState::Terminated;
                }
            }

            // Reparent children to init (PID 1)
            let children = proc.children.clone();
            for child_pid in children {
                if let Some(child) = processes.get_mut(&child_pid) {
                    child.parent = Some(ProcessId(1));
                }
                if let Some(init) = processes.get_mut(&ProcessId(1)) {
                    init.children.push(child_pid);
                }
            }
        }
    }

    // Signal parent that child exited (SIGCHLD)
    if let Some(parent_pid) = get_process(pid).and_then(|p| p.parent) {
        send_sigchld_to_parent(parent_pid, pid, exit_code, false);
    }

    // Trigger reschedule
    crate::sched::schedule();
}

/// Send SIGCHLD to parent process when child exits/stops/continues
///
/// This function:
/// 1. Creates a properly formatted SigInfo for SIGCHLD
/// 2. Queues the signal to the parent process
/// 3. Wakes any parent threads blocked in waitpid
fn send_sigchld_to_parent(parent_pid: ProcessId, child_pid: ProcessId, exit_code: i32, dumped_core: bool) {
    use crate::signal::{info::SigInfo, Signal, PROCESS_SIGNALS};

    log::debug!(
        "Sending SIGCHLD to parent {:?} for child {:?} (exit_code={})",
        parent_pid, child_pid, exit_code
    );

    // Create SIGCHLD info with proper status encoding
    let info = SigInfo::child_exited(child_pid, exit_code, dumped_core);

    // Queue signal to parent
    {
        let mut signals = PROCESS_SIGNALS.write();
        if let Some(parent_state) = signals.get_mut(&parent_pid) {
            if let Err(e) = parent_state.pending.enqueue(Signal::SIGCHLD.as_raw(), info) {
                log::warn!("Failed to queue SIGCHLD to parent {:?}: {:?}", parent_pid, e);
            }
        }
    }

    // Wake parent threads that might be blocked in waitpid
    wake_waiting_parent(parent_pid);
}

/// Wake parent threads blocked in waitpid
fn wake_waiting_parent(parent_pid: ProcessId) {
    // Find threads belonging to the parent process that are blocked waiting for children
    let processes = PROCESSES.read();
    if let Some(parent) = processes.get(&parent_pid) {
        for thread_id in &parent.threads {
            let mut threads = crate::sched::THREADS.write();
            if let Some(thread) = threads.get_mut(thread_id) {
                if matches!(thread.state, ThreadState::Blocked(crate::sched::BlockReason::WaitChild)) {
                    // Wake this thread - it was waiting for a child
                    thread.state = ThreadState::Ready;

                    // Re-enqueue for scheduling
                    let cpu_id = 0u32; // Simplified - ideally use parent's preferred CPU
                    let mut per_cpu = crate::sched::PER_CPU.write();
                    if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
                        cpu_sched.enqueue(*thread_id);
                    }

                    log::trace!("Woke parent thread {:?} from waitpid", thread_id);
                }
            }
        }
    }
}

/// Wait for a child process to exit
pub fn waitpid(pid: Option<ProcessId>) -> Result<(ProcessId, i32), WaitError> {
    let current_pid = current_process_id().expect("No current process");

    loop {
        let mut processes = PROCESSES.write();

        // Find a zombie child
        let zombie = {
            let current = processes.get(&current_pid).ok_or(WaitError::NoChild)?;

            if pid.is_some() {
                // Wait for specific child
                let target = pid.unwrap();
                if !current.children.contains(&target) {
                    return Err(WaitError::NoChild);
                }
                processes.get(&target)
                    .filter(|p| matches!(p.state, ProcessState::Zombie(_)))
                    .map(|p| (p.pid, p.exit_code))
            } else {
                // Wait for any child
                current.children.iter()
                    .filter_map(|&child_pid| processes.get(&child_pid))
                    .find(|p| matches!(p.state, ProcessState::Zombie(_)))
                    .map(|p| (p.pid, p.exit_code))
            }
        };

        if let Some((child_pid, exit_code)) = zombie {
            // Reap the zombie
            if let Some(current) = processes.get_mut(&current_pid) {
                current.children.retain(|&c| c != child_pid);
            }
            processes.remove(&child_pid);

            return Ok((child_pid, exit_code));
        }

        drop(processes);

        // No zombie found, block and wait
        crate::sched::block(crate::sched::BlockReason::WaitChild);
    }
}

/// Wait error
#[derive(Debug, Clone, Copy)]
pub enum WaitError {
    /// No child processes
    NoChild,
    /// Interrupted
    Interrupted,
}

/// Get current process ID
pub fn current_process_id() -> Option<ProcessId> {
    let thread_id = crate::sched::current_thread_id();

    // Find process owning this thread
    let processes = PROCESSES.read();
    for (pid, proc) in processes.iter() {
        if proc.threads.contains(&thread_id) {
            return Some(*pid);
        }
    }

    None
}

/// Get a process by ID
pub fn get_process(pid: ProcessId) -> Option<Process> {
    PROCESSES.read().get(&pid).cloned()
}

/// Get a mutable reference to a process (for timetravel restore)
pub fn get_process_mut(pid: ProcessId) -> Option<ProcessGuard> {
    let processes = PROCESSES.write();
    if processes.contains_key(&pid) {
        Some(ProcessGuard { processes, pid })
    } else {
        None
    }
}

/// Guard for mutable process access
pub struct ProcessGuard {
    processes: spin::RwLockWriteGuard<'static, alloc::collections::BTreeMap<ProcessId, Process>>,
    pid: ProcessId,
}

impl core::ops::Deref for ProcessGuard {
    type Target = Process;
    fn deref(&self) -> &Self::Target {
        self.processes.get(&self.pid).unwrap()
    }
}

impl core::ops::DerefMut for ProcessGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.processes.get_mut(&self.pid).unwrap()
    }
}

impl Clone for Process {
    fn clone(&self) -> Self {
        Self {
            pid: self.pid,
            object_id: self.object_id,
            parent: self.parent,
            children: self.children.clone(),
            state: self.state,
            name: self.name.clone(),
            address_space: AddressSpace::new(), // New address space
            cspace: self.cspace.clone(),
            threads: Vec::new(), // Threads are not cloned
            main_thread: None,
            exit_code: self.exit_code,
            uid: self.uid,
            gid: self.gid,
            env: self.env.clone(),
            cwd: self.cwd,
            fd_table: self.fd_table.clone(),
            next_fd: self.next_fd,
            mem_stats: self.mem_stats,
        }
    }
}

// ============================================================================
// Minimal ELF Parser
// ============================================================================

/// ELF magic number
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// Program header types
const PT_LOAD: u32 = 1;

/// Program header flags
const PF_X: u32 = 1; // Execute
const PF_W: u32 = 2; // Write
const PF_R: u32 = 4; // Read

/// Minimal ELF64 header parser
struct Elf<'a> {
    data: &'a [u8],
    entry: u64,
    phoff: u64,
    phnum: u16,
    phentsize: u16,
}

impl<'a> Elf<'a> {
    fn parse(data: &'a [u8]) -> Result<Self, ()> {
        if data.len() < 64 {
            return Err(());
        }

        // Check magic
        if data[0..4] != ELF_MAGIC {
            return Err(());
        }

        // Check class (64-bit)
        if data[4] != 2 {
            return Err(());
        }

        // Check endianness (little)
        if data[5] != 1 {
            return Err(());
        }

        // Parse header fields
        let entry = u64::from_le_bytes(data[24..32].try_into().unwrap());
        let phoff = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let phentsize = u16::from_le_bytes(data[54..56].try_into().unwrap());
        let phnum = u16::from_le_bytes(data[56..58].try_into().unwrap());

        Ok(Self {
            data,
            entry,
            phoff,
            phnum,
            phentsize,
        })
    }

    fn entry(&self) -> u64 {
        self.entry
    }

    fn program_headers(&self) -> impl Iterator<Item = ProgramHeader> + '_ {
        (0..self.phnum).filter_map(move |i| {
            let offset = self.phoff as usize + i as usize * self.phentsize as usize;
            ProgramHeader::parse(&self.data[offset..])
        })
    }
}

/// ELF64 Program header
#[derive(Debug)]
struct ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
    p_memsz: u64,
}

impl ProgramHeader {
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 56 {
            return None;
        }

        Some(Self {
            p_type: u32::from_le_bytes(data[0..4].try_into().ok()?),
            p_flags: u32::from_le_bytes(data[4..8].try_into().ok()?),
            p_offset: u64::from_le_bytes(data[8..16].try_into().ok()?),
            p_vaddr: u64::from_le_bytes(data[16..24].try_into().ok()?),
            // p_paddr at 24..32
            p_filesz: u64::from_le_bytes(data[32..40].try_into().ok()?),
            p_memsz: u64::from_le_bytes(data[40..48].try_into().ok()?),
        })
    }
}

// ============================================================================
// Signal-related process control
// ============================================================================

/// Get current PID (alias for signal module)
pub fn current_pid() -> Option<ProcessId> {
    current_process_id()
}

/// Terminate a process (for signal delivery)
pub fn terminate(pid: ProcessId, exit_code: i32) {
    log::info!("Terminating process {:?} with exit code {}", pid, exit_code);

    // Determine if this is a core dump (negative exit codes indicate signal death)
    let dumped_core = exit_code < 0 && matches!(
        exit_code.abs() as u8,
        3 | 4 | 6 | 7 | 8 | 11 | 24 | 25 | 31 // SIGQUIT, SIGILL, SIGABRT, etc.
    );

    let parent_pid = {
        let mut processes = PROCESSES.write();
        let parent = if let Some(proc) = processes.get_mut(&pid) {
            proc.state = ProcessState::Zombie(exit_code);
            proc.exit_code = exit_code;

            // Terminate all threads
            for thread_id in &proc.threads {
                let mut threads = crate::sched::THREADS.write();
                if let Some(thread) = threads.get_mut(thread_id) {
                    thread.state = ThreadState::Terminated;
                }
            }

            proc.parent
        } else {
            None
        };
        parent
    };

    // Send SIGCHLD to parent
    if let Some(parent_pid) = parent_pid {
        send_sigchld_to_parent(parent_pid, pid, exit_code, dumped_core);
    }

    // Trigger reschedule if we killed the current process
    if current_process_id() == Some(pid) {
        crate::sched::schedule();
    }
}

/// Stop a process (SIGSTOP/SIGTSTP)
pub fn stop(pid: ProcessId) {
    stop_with_signal(pid, 19) // Default to SIGSTOP
}

/// Stop a process with a specific stop signal (SIGSTOP=19, SIGTSTP=20, etc.)
pub fn stop_with_signal(pid: ProcessId, stop_signal: u8) {
    log::info!("Stopping process {:?} with signal {}", pid, stop_signal);

    let parent_pid = {
        let mut processes = PROCESSES.write();
        let parent = if let Some(proc) = processes.get_mut(&pid) {
            proc.state = ProcessState::Stopped;

            // Stop all threads
            for thread_id in &proc.threads {
                let mut threads = crate::sched::THREADS.write();
                if let Some(thread) = threads.get_mut(thread_id) {
                    thread.state = ThreadState::Blocked(crate::sched::BlockReason::Sleep);
                }
            }

            proc.parent
        } else {
            None
        };
        parent
    };

    // Send SIGCHLD to parent with CLD_STOPPED code
    if let Some(parent_pid) = parent_pid {
        send_sigchld_stopped(parent_pid, pid, stop_signal);
    }

    // Trigger reschedule if we stopped the current process
    if current_process_id() == Some(pid) {
        crate::sched::schedule();
    }
}

/// Send SIGCHLD to parent for stopped child
fn send_sigchld_stopped(parent_pid: ProcessId, child_pid: ProcessId, stop_signal: u8) {
    use crate::signal::{info::SigInfo, Signal, PROCESS_SIGNALS};

    log::debug!(
        "Sending SIGCHLD (stopped) to parent {:?} for child {:?}",
        parent_pid, child_pid
    );

    let info = SigInfo::child_stopped(child_pid, stop_signal);

    let mut signals = PROCESS_SIGNALS.write();
    if let Some(parent_state) = signals.get_mut(&parent_pid) {
        if let Err(e) = parent_state.pending.enqueue(Signal::SIGCHLD.as_raw(), info) {
            log::warn!("Failed to queue SIGCHLD (stopped) to parent {:?}: {:?}", parent_pid, e);
        }
    }
}

/// Resume a stopped process (SIGCONT)
pub fn resume(pid: ProcessId) {
    log::info!("Resuming process {:?}", pid);

    let parent_pid = {
        let mut processes = PROCESSES.write();
        let parent = if let Some(proc) = processes.get_mut(&pid) {
            if proc.state == ProcessState::Stopped {
                proc.state = ProcessState::Running;

                // Make threads runnable again
                for thread_id in &proc.threads {
                    let mut threads = crate::sched::THREADS.write();
                    if let Some(thread) = threads.get_mut(thread_id) {
                        thread.state = ThreadState::Ready;

                        // Re-enqueue thread
                        let cpu_id = 0u32; // Simplified
                        let mut per_cpu = crate::sched::PER_CPU.write();
                        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
                            cpu_sched.enqueue(*thread_id);
                        }
                    }
                }

                proc.parent
            } else {
                None
            }
        } else {
            None
        };
        parent
    };

    // Send SIGCHLD to parent with CLD_CONTINUED code
    if let Some(parent_pid) = parent_pid {
        send_sigchld_continued(parent_pid, pid);
    }
}

/// Send SIGCHLD to parent for continued child
fn send_sigchld_continued(parent_pid: ProcessId, child_pid: ProcessId) {
    use crate::signal::{info::SigInfo, Signal, PROCESS_SIGNALS};

    log::debug!(
        "Sending SIGCHLD (continued) to parent {:?} for child {:?}",
        parent_pid, child_pid
    );

    let info = SigInfo::child_continued(child_pid);

    let mut signals = PROCESS_SIGNALS.write();
    if let Some(parent_state) = signals.get_mut(&parent_pid) {
        if let Err(e) = parent_state.pending.enqueue(Signal::SIGCHLD.as_raw(), info) {
            log::warn!("Failed to queue SIGCHLD (continued) to parent {:?}: {:?}", parent_pid, e);
        }
    }
}
