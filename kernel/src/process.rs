//! Process management
//!
//! Processes are containers for threads, address spaces, and capabilities.
//! Unlike threads, processes have their own capability space and address space.

use crate::cap::{CSpace, Capability, ObjectId, ObjectType, Rights, create_cspace};
use crate::mem::{AddressSpace, VirtAddr, PhysAddr, PAGE_SIZE};
use crate::sched::{Thread, ThreadId, ThreadState, SchedClass};
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
    let mut proc = Process::new(&args.path, Some(parent_pid));

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

            // Zero the frame first
            unsafe {
                let ptr = frame.as_u64() as *mut u8;
                core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
            }

            // Copy file data if applicable
            let page_offset = page_vaddr.as_u64() - vaddr.as_u64();
            if page_offset < filesz {
                let copy_start = offset + page_offset as usize;
                let copy_len = core::cmp::min(PAGE_SIZE, filesz - page_offset) as usize;
                if copy_start + copy_len <= data.len() {
                    unsafe {
                        let dst = frame.as_u64() as *mut u8;
                        let src = data.as_ptr().add(copy_start);
                        core::ptr::copy_nonoverlapping(src, dst, copy_len);
                    }
                }
            }

            // Map the page
            proc.address_space.map(
                page_vaddr,
                PAGE_SIZE,
                prot,
                crate::mem::virt::VmaBacking::Anonymous,
            ).map_err(|_| SpawnError::OutOfMemory)?;
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

        // Zero the frame
        unsafe {
            let ptr = frame.as_u64() as *mut u8;
            core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
        }

        proc.address_space.map(
            page_vaddr,
            PAGE_SIZE,
            prot,
            crate::mem::virt::VmaBacking::Anonymous,
        ).map_err(|_| SpawnError::OutOfMemory)?;
    }

    proc.mem_stats.vm_size += stack_size;
    proc.mem_stats.data += stack_size;

    Ok(())
}

/// Exit the current process
pub fn exit(exit_code: i32) {
    let pid = current_process_id().expect("No current process");

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

    // Wake parent if waiting
    if let Some(parent_pid) = get_process(pid).and_then(|p| p.parent) {
        // Signal parent that child exited
        // TODO: Send SIGCHLD or wake from waitpid
    }

    // Trigger reschedule
    crate::sched::schedule();
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
